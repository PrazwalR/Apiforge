//! Retry logic for audit store operations
//!
//! This module provides retry mechanisms for sled database operations
//! which can occasionally fail due to file system or I/O issues.

use std::time::Duration;
use tracing::{debug, error, warn};

/// Configuration for audit store retry behavior
#[derive(Debug, Clone)]
pub struct AuditRetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Initial delay before first retry
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Backoff multiplier
    pub backoff_multiplier: f64,
}

impl Default for AuditRetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            backoff_multiplier: 2.0,
        }
    }
}

impl AuditRetryConfig {
    /// Create a new config with specified max retries
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// Calculate delay for a given attempt
    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        let base_delay =
            self.initial_delay.as_millis() as f64 * self.backoff_multiplier.powi(attempt as i32);
        let delay_ms = base_delay.min(self.max_delay.as_millis() as f64);
        Duration::from_millis(delay_ms as u64)
    }
}

/// Check if a sled error is retryable
pub fn is_sled_error_retryable(err: &sled::Error) -> bool {
    match err {
        // I/O errors might be transient
        sled::Error::Io(io_err) => {
            let kind = io_err.kind();
            let retryable_kind = matches!(
                kind,
                std::io::ErrorKind::Interrupted
                    | std::io::ErrorKind::WouldBlock
                    | std::io::ErrorKind::TimedOut
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::ConnectionRefused
                    | std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::UnexpectedEof
            );
            let message = io_err.to_string().to_lowercase();
            retryable_kind
                || message.contains("resource busy")
                || message.contains("temporarily unavailable")
        }
        // Collection errors might resolve on retry
        sled::Error::CollectionNotFound(_) => true,
        // Snapshot-related errors might be transient
        sled::Error::ReportableBug(_) => false,
        // Other errors are not retryable
        _ => false,
    }
}

/// Execute a sled operation with retry logic
pub fn with_retry<T, E, F>(
    config: &AuditRetryConfig,
    operation_name: &str,
    mut operation: F,
) -> Result<T, E>
where
    F: FnMut() -> Result<T, E>,
    E: std::fmt::Display,
{
    let mut last_error: Option<E> = None;

    for attempt in 0..=config.max_retries {
        match operation() {
            Ok(result) => {
                if attempt > 0 {
                    debug!("{} succeeded on attempt {}", operation_name, attempt + 1);
                }
                return Ok(result);
            }
            Err(e) => {
                if attempt >= config.max_retries {
                    error!(
                        "{} failed after {} attempts: {}",
                        operation_name,
                        attempt + 1,
                        e
                    );
                    return Err(e);
                }

                let delay = config.calculate_delay(attempt);
                warn!(
                    "{} failed (attempt {}/{}): {}. Retrying in {:?}...",
                    operation_name,
                    attempt + 1,
                    config.max_retries + 1,
                    e,
                    delay
                );

                last_error = Some(e);
                std::thread::sleep(delay);
            }
        }
    }

    // This should be unreachable
    Err(last_error.expect("Retry loop should have set last_error"))
}

/// Execute a sled operation with retry logic for sled-specific errors
pub fn with_sled_retry<T, F>(
    config: &AuditRetryConfig,
    operation_name: &str,
    mut operation: F,
) -> Result<T, crate::error::ApiForgError>
where
    F: FnMut() -> Result<T, sled::Error>,
{
    let mut last_error: Option<sled::Error> = None;

    for attempt in 0..=config.max_retries {
        match operation() {
            Ok(result) => {
                if attempt > 0 {
                    debug!("{} succeeded on attempt {}", operation_name, attempt + 1);
                }
                return Ok(result);
            }
            Err(e) => {
                // Check if error is retryable
                if !is_sled_error_retryable(&e) || attempt >= config.max_retries {
                    if attempt >= config.max_retries {
                        error!(
                            "{} failed after {} attempts: {}",
                            operation_name,
                            attempt + 1,
                            e
                        );
                    }
                    return Err(crate::error::ApiForgError::Audit(format!(
                        "{} failed: {}",
                        operation_name, e
                    )));
                }

                let delay = config.calculate_delay(attempt);
                warn!(
                    "{} failed with retryable error (attempt {}/{}): {}. Retrying in {:?}...",
                    operation_name,
                    attempt + 1,
                    config.max_retries + 1,
                    e,
                    delay
                );

                last_error = Some(e);
                std::thread::sleep(delay);
            }
        }
    }

    Err(crate::error::ApiForgError::Audit(format!(
        "{} failed: {}",
        operation_name,
        last_error.expect("Retry loop should have set last_error")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_retry_config_calculations() {
        let config = AuditRetryConfig::default();
        assert_eq!(config.calculate_delay(0), Duration::from_millis(100));
        assert_eq!(config.calculate_delay(1), Duration::from_millis(200));
        assert_eq!(config.calculate_delay(2), Duration::from_millis(400));
    }

    #[test]
    fn test_retry_succeeds_first_attempt() {
        let config = AuditRetryConfig::default();
        let result: Result<&str, &str> = with_retry(&config, "test", || Ok("success"));
        assert_eq!(result.unwrap(), "success");
    }

    #[test]
    fn test_retry_succeeds_after_failures() {
        let config = AuditRetryConfig {
            max_retries: 3,
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            backoff_multiplier: 1.0,
        };

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result: Result<i32, &str> = with_retry(&config, "test", || {
            let attempt = attempts_clone.fetch_add(1, Ordering::SeqCst);
            if attempt < 2 {
                Err("transient")
            } else {
                Ok(42)
            }
        });

        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn test_retry_exhausts_all_attempts() {
        let config = AuditRetryConfig {
            max_retries: 2,
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            backoff_multiplier: 1.0,
        };

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result: Result<(), &str> = with_retry(&config, "test", || {
            attempts_clone.fetch_add(1, Ordering::SeqCst);
            Err("permanent")
        });

        assert!(result.is_err());
        // initial attempt + 2 retries = 3 total
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }
}
