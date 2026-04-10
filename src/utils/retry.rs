//! Retry utility with exponential backoff for network operations.
//!
//! This module provides a configurable retry mechanism for handling transient
//! network failures in AWS, GitHub, and Kubernetes operations.

use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (not including the initial attempt)
    pub max_retries: u32,
    /// Initial delay before the first retry
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Multiplier for exponential backoff (e.g., 2.0 doubles the delay each retry)
    pub backoff_multiplier: f64,
    /// Whether to add jitter to prevent thundering herd
    pub add_jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            add_jitter: true,
        }
    }
}

impl RetryConfig {
    /// Create a new retry config with specified max retries
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// Create a new retry config with specified initial delay
    pub fn with_initial_delay(mut self, delay: Duration) -> Self {
        self.initial_delay = delay;
        self
    }

    /// Create a config suitable for fast network operations
    pub fn fast() -> Self {
        Self {
            max_retries: 2,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(2),
            backoff_multiplier: 2.0,
            add_jitter: true,
        }
    }

    /// Create a config suitable for slow/large operations
    pub fn slow() -> Self {
        Self {
            max_retries: 5,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            add_jitter: true,
        }
    }

    /// Calculate delay for a given attempt number (0-indexed)
    fn calculate_delay(&self, attempt: u32) -> Duration {
        let base_delay =
            self.initial_delay.as_millis() as f64 * self.backoff_multiplier.powi(attempt as i32);

        let delay_ms = base_delay.min(self.max_delay.as_millis() as f64);

        let final_delay_ms = if self.add_jitter {
            // Add up to 25% jitter
            let jitter = delay_ms * 0.25 * rand_jitter();
            delay_ms + jitter
        } else {
            delay_ms
        };

        Duration::from_millis(final_delay_ms as u64)
    }
}

/// Simple pseudo-random jitter (0.0 to 1.0) without external dependencies
fn rand_jitter() -> f64 {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos % 1000) as f64 / 1000.0
}

/// Determines if an error is retryable
pub trait RetryableError {
    /// Returns true if this error is transient and the operation should be retried
    fn is_retryable(&self) -> bool;
}

/// Execute an async operation with retry logic
///
/// # Arguments
/// * `config` - Retry configuration
/// * `operation_name` - Name of the operation for logging
/// * `operation` - Async closure that returns Result<T, E> where E: RetryableError
///
/// # Returns
/// * Ok(T) - If the operation succeeds within the retry limit
/// * Err(E) - The last error if all retries are exhausted
pub async fn with_retry<F, Fut, T, E>(
    config: &RetryConfig,
    operation_name: &str,
    mut operation: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: RetryableError + std::fmt::Display,
{
    let mut last_error: Option<E> = None;

    for attempt in 0..=config.max_retries {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!(
                        "{} succeeded on attempt {} of {}",
                        operation_name,
                        attempt + 1,
                        config.max_retries + 1
                    );
                }
                return Ok(result);
            }
            Err(e) => {
                if !e.is_retryable() || attempt >= config.max_retries {
                    if attempt >= config.max_retries {
                        warn!(
                            "{} failed after {} attempts: {}",
                            operation_name,
                            attempt + 1,
                            e
                        );
                    }
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
                sleep(delay).await;
            }
        }
    }

    // This should be unreachable, but just in case
    Err(last_error.expect("Retry loop should have set last_error"))
}

/// Execute an async operation with default retry config
pub async fn retry<F, Fut, T, E>(operation_name: &str, operation: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: RetryableError + std::fmt::Display,
{
    with_retry(&RetryConfig::default(), operation_name, operation).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[derive(Debug)]
    struct TestError {
        retryable: bool,
        message: String,
    }

    impl std::fmt::Display for TestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.message)
        }
    }

    impl RetryableError for TestError {
        fn is_retryable(&self) -> bool {
            self.retryable
        }
    }

    #[tokio::test]
    async fn test_retry_succeeds_first_attempt() {
        let result: Result<&str, TestError> = retry("test", || async { Ok("success") }).await;
        assert_eq!(result.unwrap(), "success");
    }

    #[tokio::test]
    async fn test_retry_succeeds_after_failures() {
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let config = RetryConfig::default().with_initial_delay(Duration::from_millis(10));

        let result = with_retry(&config, "test", || {
            let attempts = attempts_clone.clone();
            async move {
                let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                if attempt < 2 {
                    Err(TestError {
                        retryable: true,
                        message: "transient".to_string(),
                    })
                } else {
                    Ok("success")
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), "success");
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_fails_non_retryable() {
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let config = RetryConfig::default().with_initial_delay(Duration::from_millis(10));

        let result: Result<&str, TestError> = with_retry(&config, "test", || {
            let attempts = attempts_clone.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err(TestError {
                    retryable: false,
                    message: "permanent".to_string(),
                })
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 1); // No retries for non-retryable
    }

    #[tokio::test]
    async fn test_retry_exhausts_retryable_errors() {
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let config = RetryConfig {
            max_retries: 2,
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            backoff_multiplier: 2.0,
            add_jitter: false,
        };

        let result: Result<&str, TestError> = with_retry(&config, "test", || {
            let attempts = attempts_clone.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err(TestError {
                    retryable: true,
                    message: "still transient".to_string(),
                })
            }
        })
        .await;

        assert!(result.is_err());
        // Initial attempt + 2 retries = 3 total attempts
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_with_zero_retries_attempts_once() {
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let config = RetryConfig {
            max_retries: 0,
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            backoff_multiplier: 2.0,
            add_jitter: false,
        };

        let result: Result<&str, TestError> = with_retry(&config, "test", || {
            let attempts = attempts_clone.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err(TestError {
                    retryable: true,
                    message: "transient".to_string(),
                })
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_calculate_delay_with_backoff() {
        let config = RetryConfig {
            max_retries: 5,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
            add_jitter: false,
        };

        assert_eq!(config.calculate_delay(0), Duration::from_millis(100));
        assert_eq!(config.calculate_delay(1), Duration::from_millis(200));
        assert_eq!(config.calculate_delay(2), Duration::from_millis(400));
        assert_eq!(config.calculate_delay(3), Duration::from_millis(800));
    }

    #[test]
    fn test_calculate_delay_respects_max() {
        let config = RetryConfig {
            max_retries: 10,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(5),
            backoff_multiplier: 2.0,
            add_jitter: false,
        };

        // After several retries, should cap at max_delay
        assert_eq!(config.calculate_delay(10), Duration::from_secs(5));
    }

    #[test]
    fn test_fast_and_slow_profiles() {
        let fast = RetryConfig::fast();
        assert_eq!(fast.max_retries, 2);
        assert_eq!(fast.initial_delay, Duration::from_millis(100));
        assert_eq!(fast.max_delay, Duration::from_secs(2));

        let slow = RetryConfig::slow();
        assert_eq!(slow.max_retries, 5);
        assert_eq!(slow.initial_delay, Duration::from_secs(1));
        assert_eq!(slow.max_delay, Duration::from_secs(60));
    }
}
