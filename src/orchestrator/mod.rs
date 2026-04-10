use std::collections::HashMap;
use std::time::Instant;

use crate::config::Config;
use crate::error::Result;
use crate::output::OutputManager;
use crate::steps::{Step, StepContext, StepOutput};
use crate::utils::sanitize_message;

pub struct ReleaseOrchestrator {
    steps: Vec<Box<dyn Step>>,
    config: Config,
    dry_run: bool,
    auto_rollback: bool,
    output: OutputManager,
}

impl ReleaseOrchestrator {
    /// Create a new orchestrator with the provided config and mode.
    pub fn new(config: Config, dry_run: bool) -> Self {
        Self {
            steps: Vec::new(),
            config,
            dry_run,
            auto_rollback: true, // Enable by default
            output: OutputManager::new(),
        }
    }

    /// Enable or disable automatic rollback on step failure.
    pub fn with_auto_rollback(mut self, enabled: bool) -> Self {
        self.auto_rollback = enabled;
        self
    }

    /// Append a step to the execution pipeline.
    pub fn add_step(&mut self, step: Box<dyn Step>) {
        self.steps.push(step);
    }

    /// Run preflight validation for each configured step.
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
                    self.output
                        .step_ok(&format!("{} (rolled back)", step.name()));
                }
                Err(e) => {
                    // Log rollback failure but continue with other rollbacks.
                    let safe_error = sanitize_message(&e.to_string());
                    self.output
                        .step_fail(step.name(), &format!("rollback failed: {}", safe_error));
                    tracing::error!("Failed to rollback step '{}': {}", step.name(), safe_error);
                }
            }
        }
    }

    /// Execute the configured step pipeline and return per-step outputs.
    ///
    /// In normal mode, failures trigger rollback of already completed steps
    /// when `auto_rollback` is enabled.
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
                    let safe_error = sanitize_message(&e.to_string());
                    self.output.step_fail(step.name(), &safe_error);

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AwsConfig, Config, DockerConfig, DockerRegistry, GitConfig, KubernetesConfig, Language,
        ProjectConfig,
    };
    use crate::error::ApiForgError;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    struct MockStep {
        name: &'static str,
        fail_on_execute: bool,
        events: Arc<Mutex<Vec<String>>>,
    }

    impl MockStep {
        fn new(name: &'static str, fail_on_execute: bool, events: Arc<Mutex<Vec<String>>>) -> Self {
            Self {
                name,
                fail_on_execute,
                events,
            }
        }
    }

    #[async_trait]
    impl Step for MockStep {
        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            "mock test step"
        }

        async fn validate(&self, _ctx: &StepContext) -> Result<()> {
            Ok(())
        }

        async fn execute(&self, _ctx: &StepContext) -> Result<StepOutput> {
            self.events
                .lock()
                .unwrap()
                .push(format!("execute:{}", self.name));

            if self.fail_on_execute {
                return Err(ApiForgError::StepFailed(format!(
                    "step {} failed",
                    self.name
                )));
            }

            Ok(StepOutput::ok(format!("{} executed", self.name)))
        }

        async fn dry_run(&self, _ctx: &StepContext) -> Result<StepOutput> {
            self.events
                .lock()
                .unwrap()
                .push(format!("dry_run:{}", self.name));
            Ok(StepOutput::ok(format!("{} dry-run", self.name)))
        }

        async fn rollback(&self, _ctx: &StepContext) -> Result<()> {
            self.events
                .lock()
                .unwrap()
                .push(format!("rollback:{}", self.name));
            Ok(())
        }
    }

    fn test_config() -> Config {
        Config {
            project: ProjectConfig {
                name: "test-project".to_string(),
                language: Language::Rust,
            },
            git: GitConfig {
                main_branch: "main".to_string(),
                tag_format: "v{version}".to_string(),
                changelog: true,
                commit_message: "release {{ version }}".to_string(),
                remote: "origin".to_string(),
                require_clean: false,
                require_main_branch: false,
                fetch_timeout_secs: 60,
                push_timeout_secs: 120,
                operation_timeout_secs: 30,
            },
            docker: DockerConfig {
                registry: DockerRegistry::AwsEcr,
                repository: "test-repo".to_string(),
                dockerfile: "Dockerfile".to_string(),
                context: ".".to_string(),
                tags: vec!["{version}".to_string(), "latest".to_string()],
                build_args: Some(HashMap::new()),
            },
            kubernetes: KubernetesConfig {
                context: "test".to_string(),
                namespace: "default".to_string(),
                deployment: "test-project".to_string(),
                manifest_path: "k8s/deployment.yaml".to_string(),
                image_field: ".spec.template.spec.containers[0].image".to_string(),
                rollout_timeout: 300,
                min_ready_percent: 100,
            },
            aws: AwsConfig {
                region: "us-east-1".to_string(),
                profile: None,
            },
            github: None,
            notifications: None,
            health_check: None,
        }
    }

    #[tokio::test]
    async fn test_run_executes_all_steps_successfully() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut orchestrator = ReleaseOrchestrator::new(test_config(), false);
        orchestrator.add_step(Box::new(MockStep::new("step-a", false, events.clone())));
        orchestrator.add_step(Box::new(MockStep::new("step-b", false, events.clone())));

        let outputs = orchestrator.run().await.unwrap();

        assert_eq!(outputs.len(), 2);
        assert!(outputs
            .iter()
            .all(|output| output.status == crate::steps::StepStatus::Success));
        assert_eq!(
            events.lock().unwrap().clone(),
            vec!["execute:step-a", "execute:step-b"]
        );
    }

    #[tokio::test]
    async fn test_run_rolls_back_completed_steps_in_reverse_order() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut orchestrator = ReleaseOrchestrator::new(test_config(), false);
        orchestrator.add_step(Box::new(MockStep::new("step-a", false, events.clone())));
        orchestrator.add_step(Box::new(MockStep::new("step-b", false, events.clone())));
        orchestrator.add_step(Box::new(MockStep::new("step-c", true, events.clone())));

        let result = orchestrator.run().await;
        assert!(result.is_err());

        assert_eq!(
            events.lock().unwrap().clone(),
            vec![
                "execute:step-a",
                "execute:step-b",
                "execute:step-c",
                "rollback:step-b",
                "rollback:step-a"
            ]
        );
    }

    #[tokio::test]
    async fn test_run_does_not_rollback_when_disabled() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut orchestrator =
            ReleaseOrchestrator::new(test_config(), false).with_auto_rollback(false);
        orchestrator.add_step(Box::new(MockStep::new("step-a", false, events.clone())));
        orchestrator.add_step(Box::new(MockStep::new("step-b", true, events.clone())));

        let result = orchestrator.run().await;
        assert!(result.is_err());

        assert_eq!(
            events.lock().unwrap().clone(),
            vec!["execute:step-a", "execute:step-b"]
        );
    }

    #[tokio::test]
    async fn test_dry_run_uses_dry_run_paths() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut orchestrator = ReleaseOrchestrator::new(test_config(), true);
        orchestrator.add_step(Box::new(MockStep::new("step-a", false, events.clone())));
        orchestrator.add_step(Box::new(MockStep::new("step-b", false, events.clone())));

        let outputs = orchestrator.run().await.unwrap();

        assert_eq!(outputs.len(), 2);
        assert_eq!(
            events.lock().unwrap().clone(),
            vec!["dry_run:step-a", "dry_run:step-b"]
        );
    }
}
