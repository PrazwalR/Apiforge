use std::collections::HashMap;
use std::time::Instant;

use crate::config::Config;
use crate::error::Result;
use crate::output::OutputManager;
use crate::steps::{Step, StepContext, StepOutput};

pub struct ReleaseOrchestrator {
    steps: Vec<Box<dyn Step>>,
    config: Config,
    dry_run: bool,
    auto_rollback: bool,
    output: OutputManager,
}

impl ReleaseOrchestrator {
    pub fn new(config: Config, dry_run: bool) -> Self {
        Self {
            steps: Vec::new(),
            config,
            dry_run,
            auto_rollback: true,  // Enable by default
            output: OutputManager::new(),
        }
    }

    pub fn with_auto_rollback(mut self, enabled: bool) -> Self {
        self.auto_rollback = enabled;
        self
    }

    pub fn add_step(&mut self, step: Box<dyn Step>) {
        self.steps.push(step);
    }

    pub async fn preflight(&self, ctx: &StepContext) -> Result<()> {
        self.output.section("Pre-flight checks");
        for step in &self.steps {
            self.output.step_status(step.name(), "validating...");
            step.validate(ctx).await?;
            self.output.step_ok(step.name());
        }
        self.output.blank_line();
        Ok(())
    }

    /// Rollback completed steps in reverse order
    async fn rollback_steps(&self, ctx: &StepContext, completed_indices: &[usize]) {
        if completed_indices.is_empty() {
            return;
        }

        self.output.blank_line();
        self.output.section("Rolling back completed steps");

        // Rollback in reverse order
        for &idx in completed_indices.iter().rev() {
            let step = &self.steps[idx];
            self.output.step_status(step.name(), "rolling back...");
            
            match step.rollback(ctx).await {
                Ok(()) => {
                    self.output.step_ok(&format!("{} (rolled back)", step.name()));
                }
                Err(e) => {
                    // Log rollback failure but continue with other rollbacks
                    self.output.step_fail(
                        step.name(),
                        &format!("rollback failed: {}", e),
                    );
                    tracing::error!(
                        "Failed to rollback step '{}': {}",
                        step.name(),
                        e
                    );
                }
            }
        }
    }

    pub async fn run(&self) -> Result<Vec<StepOutput>> {
        let ctx = StepContext {
            config: self.config.clone(),
            dry_run: self.dry_run,
            state: HashMap::new(),
        };

        self.preflight(&ctx).await?;

        let mode = if self.dry_run { "Dry-run" } else { "Executing" };
        self.output.section(&format!("{} release pipeline", mode));

        let mut outputs = Vec::new();
        let mut completed_indices: Vec<usize> = Vec::new();

        for (idx, step) in self.steps.iter().enumerate() {
            let step_start = Instant::now();
            self.output.step_status(step.name(), "running...");

            let result = if self.dry_run {
                step.dry_run(&ctx).await
            } else {
                step.execute(&ctx).await
            };

            let elapsed = step_start.elapsed();

            match result {
                Ok(mut out) => {
                    out.duration_ms = elapsed.as_millis() as u64;
                    self.output.step_done(step.name(), &out);
                    outputs.push(out);
                    completed_indices.push(idx);
                }
                Err(e) => {
                    self.output.step_fail(step.name(), &e.to_string());

                    // Perform automatic rollback if enabled and not in dry-run mode
                    if self.auto_rollback && !self.dry_run && !completed_indices.is_empty() {
                        self.output.blank_line();
                        self.output.warn(&format!(
                            "Step '{}' failed, initiating automatic rollback of {} completed step(s)...",
                            step.name(),
                            completed_indices.len()
                        ));
                        self.rollback_steps(&ctx, &completed_indices).await;
                    }

                    return Err(e);
                }
            }
        }

        self.output.blank_line();
        Ok(outputs)
    }
}
