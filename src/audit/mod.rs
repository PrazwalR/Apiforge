use std::collections::HashMap;
use std::path::Path;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

pub mod retry;

#[cfg(test)]
const MAX_AUDIT_RECORDS: usize = 50;
#[cfg(not(test))]
const MAX_AUDIT_RECORDS: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseRecord {
    pub id: String,
    pub version: String,
    pub bump_type: String,
    pub timestamp: String,
    pub status: ReleaseStatus,
    pub steps: Vec<StepRecord>,
    pub duration_ms: u64,
    pub dry_run: bool,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReleaseStatus {
    Success,
    Failed,
    RolledBack,
}

impl std::fmt::Display for ReleaseStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReleaseStatus::Success => write!(f, "success"),
            ReleaseStatus::Failed => write!(f, "failed"),
            ReleaseStatus::RolledBack => write!(f, "rolled_back"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRecord {
    pub name: String,
    pub status: StepStatus,
    pub duration_ms: u64,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    Success,
    Failed,
    Skipped,
}

pub struct AuditStore {
    db: sled::Db,
    retry_config: retry::AuditRetryConfig,
}

impl AuditStore {
    pub fn open(path: &Path) -> crate::error::Result<Self> {
        Self::open_with_config(path, retry::AuditRetryConfig::default())
    }

    pub fn open_with_config(
        path: &Path,
        retry_config: retry::AuditRetryConfig,
    ) -> crate::error::Result<Self> {
        // Ensure audit path exists before opening sled.
        if !path.exists() {
            std::fs::create_dir_all(path).map_err(|e| {
                crate::error::ApiForgError::Audit(format!(
                    "Failed to create audit directory {:?}: {}",
                    path, e
                ))
            })?;
        }

        let db = retry::with_sled_retry(&retry_config, "Open audit database", || sled::open(path))?;

        info!("Audit store opened at {:?}", path);
        Ok(Self { db, retry_config })
    }

    pub fn record(&self, record: &ReleaseRecord) -> crate::error::Result<()> {
        let key = format!("{}_{}", record.timestamp, record.id);
        let value = serde_json::to_vec(record).map_err(|e| {
            crate::error::ApiForgError::Audit(format!("Failed to serialize record: {}", e))
        })?;

        retry::with_sled_retry(&self.retry_config, "Write audit record", || {
            self.db.insert(key.as_bytes(), value.clone())
        })?;

        // Flush after each write to ensure data persistence
        retry::with_sled_retry(&self.retry_config, "Flush audit database", || {
            self.db.flush()
        })?;

        let pruned = self.prune_excess_records(MAX_AUDIT_RECORDS)?;
        if pruned > 0 {
            info!(
                "Audit record retention enforced: pruned {} old record(s), retained latest {}",
                pruned, MAX_AUDIT_RECORDS
            );
        }

        debug!("Audit record written: {}", record.id);
        Ok(())
    }

    pub fn list(&self, limit: usize) -> crate::error::Result<Vec<ReleaseRecord>> {
        let mut records: Vec<ReleaseRecord> = Vec::new();

        let entries: Vec<(sled::IVec, sled::IVec)> =
            retry::with_sled_retry(&self.retry_config, "Iterate audit database", || {
                self.db.iter().rev().collect::<Result<Vec<_>, _>>()
            })?;

        for (key, value) in entries {
            if records.len() >= limit {
                break;
            }
            let record: ReleaseRecord = serde_json::from_slice(&value).map_err(|e| {
                crate::error::ApiForgError::Audit(format!(
                    "Failed to deserialize record for key {:?}: {}",
                    key, e
                ))
            })?;
            records.push(record);
        }
        Ok(records)
    }

    /// Flush the database to disk explicitly
    pub fn flush(&self) -> crate::error::Result<()> {
        retry::with_sled_retry(&self.retry_config, "Flush audit database", || {
            self.db.flush()
        })?;
        Ok(())
    }

    /// Get the approximate size of the database on disk
    pub fn size_on_disk(&self) -> crate::error::Result<u64> {
        let size = retry::with_sled_retry(&self.retry_config, "Get database size", || {
            self.db.size_on_disk()
        })?;
        Ok(size)
    }

    /// Compact the database to reclaim space
    ///
    /// This operation removes stale data and rewrites the database
    /// to reclaim disk space from deleted/updated entries.
    /// Should be called periodically (e.g., weekly) to prevent
    /// unbounded growth.
    pub fn compact(&self) -> crate::error::Result<()> {
        info!("Starting audit database compaction...");

        let size_before = self.size_on_disk()?;
        debug!("Database size before compaction: {} bytes", size_before);

        let pruned = self.prune_excess_records(MAX_AUDIT_RECORDS)?;
        retry::with_sled_retry(&self.retry_config, "Compact audit database", || {
            self.db.flush()
        })?;

        let size_after = self.size_on_disk()?;
        let saved = size_before.saturating_sub(size_after);

        if saved > 0 {
            let reduction_percent = if size_before > 0 {
                saved
                    .checked_mul(100)
                    .and_then(|product| product.checked_div(size_before))
                    .unwrap_or_default()
            } else {
                0
            };
            info!(
                "Compaction completed: freed {} bytes ({} MB) ({}% reduction)",
                saved,
                saved / 1_048_576,
                reduction_percent
            );
        } else {
            info!(
                "Compaction completed: no space to reclaim (size: {} bytes)",
                size_after
            );
        }

        if pruned > 0 {
            info!(
                "Compaction also pruned {} old record(s) to enforce retention limit ({})",
                pruned, MAX_AUDIT_RECORDS
            );
        }

        Ok(())
    }

    /// Compact the database if it exceeds the given size threshold
    ///
    /// Returns true if compaction was performed
    pub fn compact_if_needed(&self, threshold_bytes: u64) -> crate::error::Result<bool> {
        let size = self.size_on_disk()?;
        if size > threshold_bytes {
            warn!(
                "Audit database size ({} bytes) exceeds threshold ({} bytes), compacting...",
                size, threshold_bytes
            );
            self.compact()?;
            Ok(true)
        } else {
            debug!(
                "Audit database size ({} bytes) below threshold ({} bytes), skipping compaction",
                size, threshold_bytes
            );
            Ok(false)
        }
    }

    fn prune_excess_records(&self, max_records: usize) -> crate::error::Result<usize> {
        let total = self.db.len();
        if total <= max_records {
            return Ok(0);
        }

        let to_delete = total - max_records;
        let keys_to_delete: Vec<Vec<u8>> =
            retry::with_sled_retry(&self.retry_config, "Collect records for pruning", || {
                let mut keys = Vec::with_capacity(to_delete);
                for entry in self.db.iter().take(to_delete) {
                    let (key, _) = entry?;
                    keys.push(key.to_vec());
                }
                Ok::<_, sled::Error>(keys)
            })?;

        let mut deleted = 0usize;
        for key in keys_to_delete {
            retry::with_sled_retry(&self.retry_config, "Prune old record", || {
                self.db.remove(&key)
            })?;
            deleted += 1;
        }

        if deleted > 0 {
            retry::with_sled_retry(&self.retry_config, "Flush pruned records", || {
                self.db.flush()
            })?;
        }

        Ok(deleted)
    }

    /// Get the number of records in the database
    pub fn len(&self) -> crate::error::Result<usize> {
        // sled::Db::len() returns usize directly, not Result
        let count = self.db.len();
        Ok(count)
    }

    /// Check if the database is empty
    pub fn is_empty(&self) -> crate::error::Result<bool> {
        Ok(self.len()? == 0)
    }

    /// Delete old records older than the given retention period
    ///
    /// Returns the number of records deleted
    pub fn prune_old_records(&self, retention_days: u32) -> crate::error::Result<usize> {
        if retention_days == 0 {
            return Ok(0);
        }

        let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
        let cutoff_str = cutoff.to_rfc3339();

        let mut deleted = 0usize;

        let keys_to_delete: Vec<Vec<u8>> =
            retry::with_sled_retry(&self.retry_config, "Scan for old records", || {
                let mut keys = Vec::new();
                for entry in self.db.iter() {
                    let (key, _) = entry?;
                    let key_str = String::from_utf8_lossy(&key);
                    // Keys are formatted as "{timestamp}_{uuid}"
                    if let Some(timestamp) = key_str.split('_').next() {
                        if *timestamp < *cutoff_str {
                            keys.push(key.to_vec());
                        }
                    }
                }
                Ok::<_, sled::Error>(keys)
            })
            .map_err(|e| {
                crate::error::ApiForgError::Audit(format!("Failed to scan records: {}", e))
            })?;

        for key in keys_to_delete {
            retry::with_sled_retry(&self.retry_config, "Delete old record", || {
                self.db.remove(&key)
            })?;
            deleted += 1;
        }

        if deleted > 0 {
            info!(
                "Pruned {} records older than {} days",
                deleted, retention_days
            );
            self.flush()?;
        }

        Ok(deleted)
    }
}

impl Drop for AuditStore {
    fn drop(&mut self) {
        // Ensure all data is written to disk when the store is dropped
        // Note: We can't use retry logic here since we're in Drop
        if let Err(e) = self.db.flush() {
            error!("Failed to flush audit database on drop: {}", e);
        }
    }
}

impl AuditStore {
    pub fn new_record(version: &str, bump_type: &str, dry_run: bool) -> ReleaseRecord {
        ReleaseRecord {
            id: Uuid::new_v4().to_string(),
            version: version.to_string(),
            bump_type: bump_type.to_string(),
            timestamp: Utc::now().to_rfc3339(),
            status: ReleaseStatus::Success,
            steps: Vec::new(),
            duration_ms: 0,
            dry_run,
            metadata: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;
    use std::time::Duration;
    use tempfile::TempDir;

    fn create_test_store() -> (AuditStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_audit");
        let store = AuditStore::open(&db_path).unwrap();
        (store, temp_dir)
    }

    #[test]
    fn test_audit_store_open_close() {
        let (store, _temp) = create_test_store();
        // Just verify it opens without error
        drop(store);
    }

    #[test]
    fn test_audit_store_record_and_list() {
        let (store, _temp) = create_test_store();

        let record = AuditStore::new_record("1.0.0", "minor", false);
        store.record(&record).unwrap();

        let records = store.list(10).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].version, "1.0.0");
    }

    #[test]
    fn test_audit_store_list_limit() {
        let (store, _temp) = create_test_store();

        // Insert 5 records
        for i in 0..5 {
            let record = AuditStore::new_record(&format!("1.0.{}", i), "patch", false);
            store.record(&record).unwrap();
        }

        // List only 3
        let records = store.list(3).unwrap();
        assert_eq!(records.len(), 3);
    }

    #[test]
    fn test_audit_store_is_empty() {
        let (store, _temp) = create_test_store();

        assert!(store.is_empty().unwrap());

        let record = AuditStore::new_record("1.0.0", "minor", false);
        store.record(&record).unwrap();

        assert!(!store.is_empty().unwrap());
    }

    #[test]
    fn test_audit_store_len() {
        let (store, _temp) = create_test_store();

        assert_eq!(store.len().unwrap(), 0);

        for i in 0..3 {
            let record = AuditStore::new_record(&format!("1.0.{}", i), "patch", false);
            store.record(&record).unwrap();
        }

        assert_eq!(store.len().unwrap(), 3);
    }

    #[test]
    fn test_audit_store_size_on_disk() {
        let (store, _temp) = create_test_store();

        // Insert some data to ensure database has content
        let record = AuditStore::new_record("1.0.0", "minor", false);
        store.record(&record).unwrap();
        store.flush().unwrap();

        // size_on_disk returns a u64, should not panic
        let _size = store.size_on_disk().unwrap();
        // sled database size is valid (u64 is always >= 0)
    }

    #[test]
    fn test_audit_store_compact() {
        let (store, _temp) = create_test_store();

        // Insert some records
        for i in 0..10 {
            let record = AuditStore::new_record(&format!("1.0.{}", i), "patch", false);
            store.record(&record).unwrap();
        }

        // Compact should succeed
        store.compact().unwrap();

        // Records should still be there
        let records = store.list(20).unwrap();
        assert_eq!(records.len(), 10);
    }

    #[test]
    fn test_audit_store_compact_if_needed() {
        let (store, _temp) = create_test_store();

        // Insert some records
        for i in 0..5 {
            let record = AuditStore::new_record(&format!("1.0.{}", i), "patch", false);
            store.record(&record).unwrap();
        }

        // With a very high threshold, compaction should not happen
        let compacted = store.compact_if_needed(u64::MAX).unwrap();
        assert!(!compacted);

        // With a very low threshold, compaction should happen
        let compacted = store.compact_if_needed(1).unwrap();
        assert!(compacted);
    }

    #[test]
    fn test_audit_store_prune_old_records() {
        let (store, _temp) = create_test_store();

        // Insert a record with a very old timestamp
        let mut old_record = AuditStore::new_record("0.1.0", "minor", false);
        old_record.timestamp = "2020-01-01T00:00:00+00:00".to_string();
        store.record(&old_record).unwrap();

        // Insert a recent record
        let new_record = AuditStore::new_record("1.0.0", "minor", false);
        store.record(&new_record).unwrap();

        assert_eq!(store.len().unwrap(), 2);

        // Prune records older than 365 days
        let deleted = store.prune_old_records(365).unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(store.len().unwrap(), 1);
    }

    #[test]
    fn test_audit_store_prune_zero_days() {
        let (store, _temp) = create_test_store();

        let record = AuditStore::new_record("1.0.0", "minor", false);
        store.record(&record).unwrap();

        // Pruning with 0 days should delete nothing
        let deleted = store.prune_old_records(0).unwrap();
        assert_eq!(deleted, 0);
        assert_eq!(store.len().unwrap(), 1);
    }

    #[test]
    fn test_audit_store_auto_prunes_excess_records() {
        let (store, _temp) = create_test_store();
        let base_ts = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let total_records = MAX_AUDIT_RECORDS + 5;

        for i in 0..total_records {
            let mut record = AuditStore::new_record(&format!("1.0.{}", i), "patch", false);
            record.id = format!("id-{:04}", i);
            record.timestamp = (base_ts + ChronoDuration::seconds(i as i64)).to_rfc3339();
            store.record(&record).unwrap();
        }

        assert_eq!(store.len().unwrap(), MAX_AUDIT_RECORDS);
        let records = store.list(MAX_AUDIT_RECORDS + 10).unwrap();
        assert_eq!(records.first().unwrap().version, "1.0.54");
        assert_eq!(records.last().unwrap().version, "1.0.5");
    }

    #[test]
    fn test_audit_store_with_retry_config() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_audit");

        let retry_config = retry::AuditRetryConfig {
            max_retries: 5,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            backoff_multiplier: 1.5,
        };

        let store = AuditStore::open_with_config(&db_path, retry_config).unwrap();

        let record = AuditStore::new_record("1.0.0", "minor", false);
        store.record(&record).unwrap();

        let records = store.list(10).unwrap();
        assert_eq!(records.len(), 1);
    }

    #[test]
    fn test_release_status_display() {
        assert_eq!(format!("{}", ReleaseStatus::Success), "success");
        assert_eq!(format!("{}", ReleaseStatus::Failed), "failed");
        assert_eq!(format!("{}", ReleaseStatus::RolledBack), "rolled_back");
    }

    #[test]
    fn test_release_record_serialization() {
        let record = ReleaseRecord {
            id: "test-id".to_string(),
            version: "1.0.0".to_string(),
            bump_type: "minor".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            status: ReleaseStatus::Success,
            steps: vec![StepRecord {
                name: "test-step".to_string(),
                status: StepStatus::Success,
                duration_ms: 100,
                message: None,
            }],
            duration_ms: 1000,
            dry_run: false,
            metadata: {
                let mut m = HashMap::new();
                m.insert("key".to_string(), "value".to_string());
                m
            },
        };

        let json = serde_json::to_string(&record).unwrap();
        let deserialized: ReleaseRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.version, "1.0.0");
        assert_eq!(deserialized.steps.len(), 1);
    }
}
