use std::collections::HashMap;

use async_trait::async_trait;

use crate::config::Config;
use crate::error::Result;

pub struct StepContext {
    pub config: Config,
    pub dry_run: bool,
    pub state: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct StepOutput {
    pub status: StepStatus,
    pub message: String,
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

#[async_trait]
pub trait Step: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn validate(&self, ctx: &StepContext) -> Result<()>;
    async fn execute(&self, ctx: &mut StepContext) -> Result<StepOutput>;
    fn dry_run(&self, ctx: &StepContext) -> Result<StepOutput>;

    async fn rollback(&self, _ctx: &mut StepContext) -> Result<()> {
        Ok(())
    }
}
