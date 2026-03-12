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
    output: OutputManager,
}

impl ReleaseOrchestrator {
    pub fn new(config: Config, dry_run: bool) -> Self {
        Self {
            steps: Vec::new(),
            config,
            dry_run,
            output: OutputManager::new(),
        }
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
        for step in &self.steps {
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
                }
                Err(e) => {
                    self.output.step_fail(step.name(), &e.to_string());
                    return Err(e);
                }
            }
        }

        self.output.blank_line();
        Ok(outputs)
    }
}
