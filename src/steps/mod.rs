use std::collections::HashMap;

use async_trait::async_trait;

use crate::config::Config;
use crate::error::Result;

/// Shared context available to every step during validation/execution/rollback.
pub struct StepContext {
    /// Fully resolved release configuration.
    pub config: Config,
    /// Whether the current pipeline run is a dry-run.
    pub dry_run: bool,
    /// Cross-step state bag for lightweight value passing.
    pub state: HashMap<String, String>,
}

#[derive(Debug, Clone)]
/// Result payload emitted by each successful step execution.
pub struct StepOutput {
    /// Final step status.
    pub status: StepStatus,
    /// Human-readable message rendered in CLI output.
    pub message: String,
    /// Execution duration in milliseconds.
    pub duration_ms: u64,
}

impl StepOutput {
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            status: StepStatus::Success,
            message: message.into(),
            duration_ms: 0,
        }
    }

    pub fn skipped(message: impl Into<String>) -> Self {
        Self {
            status: StepStatus::Skipped,
            message: message.into(),
            duration_ms: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Unified step status values used in run summaries.
pub enum StepStatus {
    Success,
    Skipped,
    Failed,
}

impl std::fmt::Display for StepStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StepStatus::Success => write!(f, "✓"),
            StepStatus::Skipped => write!(f, "⊘"),
            StepStatus::Failed => write!(f, "✗"),
        }
    }
}

pub mod docker;
pub mod git;
pub mod github;
pub mod health;
pub mod kubernetes;
pub mod notify;

#[async_trait]
/// Contract implemented by all release pipeline steps.
pub trait Step: Send + Sync {
    /// Stable step name used in output and logs.
    fn name(&self) -> &str;
    /// Human-readable summary of what the step does.
    fn description(&self) -> &str;
    /// Validate prerequisites before execution starts.
    async fn validate(&self, ctx: &StepContext) -> Result<()>;
    /// Execute the step in normal mode.
    async fn execute(&self, ctx: &StepContext) -> Result<StepOutput>;
    /// Simulate execution without changing external systems.
    async fn dry_run(&self, ctx: &StepContext) -> Result<StepOutput>;

    /// Attempt to rollback effects of a previously successful execution.
    ///
    /// Default implementation is a no-op for steps that are read-only.
    async fn rollback(&self, _ctx: &StepContext) -> Result<()> {
        Ok(())
    }
}
