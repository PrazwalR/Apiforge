//! Git operations with timeout support
//!
//! This module provides async wrappers around git2 operations with configurable
//! timeouts to prevent hanging on network operations.

use crate::error::{GitError, Result};
use std::time::Duration;
use tokio::task;
use tokio::time::timeout;
use tracing::{debug, error, warn};

/// Configuration for git operation timeouts
#[derive(Debug, Clone, Copy)]
pub struct GitTimeoutConfig {
    pub fetch_timeout: Duration,
    pub push_timeout: Duration,
    pub operation_timeout: Duration,
}

impl Default for GitTimeoutConfig {
    fn default() -> Self {
        Self {
            fetch_timeout: Duration::from_secs(60),
            push_timeout: Duration::from_secs(120),
            operation_timeout: Duration::from_secs(30),
        }
    }
}

impl GitTimeoutConfig {
    /// Create from git config values
    pub fn from_config(fetch_secs: u64, push_secs: u64, op_secs: u64) -> Self {
        Self {
            fetch_timeout: Duration::from_secs(fetch_secs),
            push_timeout: Duration::from_secs(push_secs),
            operation_timeout: Duration::from_secs(op_secs),
        }
    }
}

/// Error type for timeout operations
#[derive(Debug)]
pub enum TimeoutError {
    Timeout(Duration),
    GitError(crate::error::ApiForgError),
    JoinError(tokio::task::JoinError),
}

impl std::fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimeoutError::Timeout(d) => write!(f, "Operation timed out after {:?}", d),
            TimeoutError::GitError(e) => write!(f, "Git error: {}", e),
            TimeoutError::JoinError(e) => write!(f, "Task join error: {}", e),
        }
    }
}

impl std::error::Error for TimeoutError {}

/// Execute a git operation with timeout
///
/// This spawns the operation in a blocking task and applies a timeout.
/// Git2 operations are blocking, so they need to run in spawn_blocking.
pub async fn with_timeout<F, T>(
    operation: F,
    timeout_duration: Duration,
    operation_name: &str,
) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    debug!(
        "Starting git operation '{}' with timeout {:?}",
        operation_name, timeout_duration
    );

    let handle = task::spawn_blocking(operation);

    match timeout(timeout_duration, handle).await {
        Ok(Ok(result)) => {
            debug!("Git operation '{}' completed successfully", operation_name);
            result
        }
        Ok(Err(e)) => {
            error!("Git operation '{}' panicked: {}", operation_name, e);
            Err(TimeoutError::JoinError(e).into())
        }
        Err(_) => {
            warn!(
                "Git operation '{}' timed out after {:?}",
                operation_name, timeout_duration
            );
            Err(TimeoutError::Timeout(timeout_duration).into())
        }
    }
}

/// Execute a git fetch with timeout
pub async fn fetch_with_timeout<F, T>(operation: F, config: &GitTimeoutConfig) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    with_timeout(operation, config.fetch_timeout, "fetch").await
}

/// Execute a git push with timeout
pub async fn push_with_timeout<F, T>(operation: F, config: &GitTimeoutConfig) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    with_timeout(operation, config.push_timeout, "push").await
}

/// Execute a general git operation with timeout
pub async fn operation_with_timeout<F, T>(
    operation: F,
    config: &GitTimeoutConfig,
    operation_name: &str,
) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    with_timeout(operation, config.operation_timeout, operation_name).await
}

/// Check if a timeout error is retryable (only network timeouts are retryable)
pub fn is_timeout_retryable(err: &crate::error::ApiForgError) -> bool {
    match err {
        crate::error::ApiForgError::Git(git_err) => {
            let msg = git_err.to_string().to_lowercase();
            // Retry on timeout-related errors
            msg.contains("timeout")
                || msg.contains("timed out")
                || msg.contains("connection")
                || msg.contains("network")
                || msg.contains("unreachable")
        }
        _ => false,
    }
}

/// Convert TimeoutError to ApiForgError
impl From<TimeoutError> for crate::error::ApiForgError {
    fn from(err: TimeoutError) -> Self {
        match err {
            TimeoutError::Timeout(d) => {
                GitError::GitOperation(format!("Operation timed out after {:?}", d)).into()
            }
            TimeoutError::GitError(e) => e,
            TimeoutError::JoinError(e) => {
                GitError::GitOperation(format!("Task failed: {}", e)).into()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_timeout_success() {
        let config = GitTimeoutConfig::default();
        let result: Result<&str> =
            operation_with_timeout(|| Ok("success"), &config, "test_op").await;
        assert_eq!(result.unwrap(), "success");
    }

    #[tokio::test]
    async fn test_timeout_actual_timeout() {
        let config = GitTimeoutConfig {
            operation_timeout: Duration::from_millis(50),
            ..Default::default()
        };

        let result: Result<&str> = operation_with_timeout(
            || {
                std::thread::sleep(Duration::from_secs(1));
                Ok("should not reach")
            },
            &config,
            "slow_op",
        )
        .await;

        assert!(result.is_err());
        let err_str = format!("{}", result.unwrap_err());
        assert!(err_str.contains("timed out"));
    }

    #[tokio::test]
    async fn test_config_from_values() {
        let config = GitTimeoutConfig::from_config(60, 120, 30);
        assert_eq!(config.fetch_timeout, Duration::from_secs(60));
        assert_eq!(config.push_timeout, Duration::from_secs(120));
        assert_eq!(config.operation_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_is_timeout_retryable() {
        let timeout_err = GitError::GitOperation("Connection timed out".to_string());
        assert!(is_timeout_retryable(&timeout_err.into()));

        let network_err = GitError::GitOperation("Network unreachable".to_string());
        assert!(is_timeout_retryable(&network_err.into()));

        let other_err = GitError::NotARepository;
        assert!(!is_timeout_retryable(&other_err.into()));
    }
}
