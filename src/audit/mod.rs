use std::collections::HashMap;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
}

impl AuditStore {
    pub fn open(path: &std::path::Path) -> crate::error::Result<Self> {
        let db = sled::open(path).map_err(|e| {
            crate::error::ApiForgError::Audit(format!("Failed to open audit DB: {}", e))
        })?;
        Ok(Self { db })
    }

    pub fn record(&self, record: &ReleaseRecord) -> crate::error::Result<()> {
        let key = format!("{}_{}", record.timestamp, record.id);
        let value = serde_json::to_vec(record).map_err(|e| {
            crate::error::ApiForgError::Audit(format!("Failed to serialize record: {}", e))
        })?;
        self.db.insert(key.as_bytes(), value).map_err(|e| {
            crate::error::ApiForgError::Audit(format!("Failed to write record: {}", e))
        })?;
        // Flush after each write to ensure data persistence
        self.db.flush().map_err(|e| {
            crate::error::ApiForgError::Audit(format!("Failed to flush audit DB: {}", e))
        })?;
        Ok(())
    }

    pub fn list(&self, limit: usize) -> crate::error::Result<Vec<ReleaseRecord>> {
        let mut records: Vec<ReleaseRecord> = Vec::new();
        for entry in self.db.iter().rev() {
            if records.len() >= limit {
                break;
            }
            let (_key, value) = entry.map_err(|e| {
                crate::error::ApiForgError::Audit(format!("Failed to read record: {}", e))
            })?;
            let record: ReleaseRecord = serde_json::from_slice(&value).map_err(|e| {
                crate::error::ApiForgError::Audit(format!("Failed to deserialize record: {}", e))
            })?;
            records.push(record);
        }
        Ok(records)
    }

    /// Flush the database to disk explicitly
    pub fn flush(&self) -> crate::error::Result<()> {
        self.db.flush().map_err(|e| {
            crate::error::ApiForgError::Audit(format!("Failed to flush audit DB: {}", e))
        })?;
        Ok(())
    }
}

impl Drop for AuditStore {
    fn drop(&mut self) {
        // Ensure all data is written to disk when the store is dropped
        if let Err(e) = self.db.flush() {
            tracing::error!("Failed to flush audit database on drop: {}", e);
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
