use crate::error::{ApiForgError, Result};
use crate::steps::{Step, StepContext, StepOutput};
use crate::utils::TemplateEngine;
use async_trait::async_trait;
use semver::Version;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;

pub struct HealthCheckStep {
    version: Version,
}

impl HealthCheckStep {
    pub fn new(version: Version) -> Self {
        Self { version }
    }

    async fn check_health(&self, ctx: &StepContext) -> Result<bool> {
        let health_config = ctx
            .config
            .health_check
            .as_ref()
            .ok_or_else(|| ApiForgError::Config("Health check configuration missing".to_string()))?;

        // Build template context for URL
        let mut template_ctx = HashMap::new();
        template_ctx.insert("version".to_string(), self.version.to_string());
        template_ctx.insert("project".to_string(), ctx.config.project.name.clone());

        let mut engine = TemplateEngine::new();
        let url = engine.render(&health_config.url, &template_ctx)?;

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| ApiForgError::StepFailed(format!("Failed to create HTTP client: {}", e)))?;

        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| ApiForgError::StepFailed(format!("Health check request failed: {}", e)))?;

        // Check status code
        if response.status().as_u16() != health_config.expected_status {
            tracing::debug!(
                "Health check failed: expected status {}, got {}",
                health_config.expected_status,
                response.status()
            );
            return Ok(false);
        }

        // Check response body if configured
        if let (Some(field), Some(expected_value)) = (
            &health_config.expected_body_field,
            &health_config.expected_body_value,
        ) {
            let body: serde_json::Value = response.json().await.map_err(|e| {
                ApiForgError::StepFailed(format!("Failed to parse health check response: {}", e))
            })?;

            let actual_value = body
                .pointer(field)
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // Support template in expected value
            let resolved_expected = engine.render(expected_value, &template_ctx)?;

            if actual_value != resolved_expected {
                tracing::debug!(
                    "Health check failed: expected {} = '{}', got '{}'",
                    field,
                    resolved_expected,
                    actual_value
                );
                return Ok(false);
            }
        }

        Ok(true)
    }
}

#[async_trait]
impl Step for HealthCheckStep {
    fn name(&self) -> &str {
        "health-check"
    }

    fn description(&self) -> &str {
        "Verify deployed service health"
    }

    async fn validate(&self, ctx: &StepContext) -> Result<()> {
        if ctx.config.health_check.is_none() {
            return Err(ApiForgError::Config("Health check configuration missing".to_string()).into());
        }
        Ok(())
    }

    async fn execute(&self, ctx: &StepContext) -> Result<StepOutput> {
        let health_config = ctx
            .config
            .health_check
            .as_ref()
            .ok_or_else(|| ApiForgError::Config("Health check configuration missing".to_string()))?;

        let timeout = Duration::from_secs(health_config.timeout);
        let interval = Duration::from_secs(health_config.interval);
        let start = std::time::Instant::now();

        let mut attempts = 0;
        loop {
            attempts += 1;
            tracing::debug!("Health check attempt {}", attempts);

            match self.check_health(ctx).await {
                Ok(true) => {
                    return Ok(StepOutput::ok(format!(
                        "Health check passed after {} attempts",
                        attempts
                    )));
                }
                Ok(false) => {
                    tracing::debug!("Health check failed, retrying...");
                }
                Err(e) => {
                    tracing::debug!("Health check error: {}", e);
                }
            }

            if start.elapsed() >= timeout {
                return Err(ApiForgError::StepFailed(format!(
                    "Health check failed after {} attempts over {}s",
                    attempts,
                    start.elapsed().as_secs()
                ))
                .into());
            }

            sleep(interval).await;
        }
    }

    async fn dry_run(&self, ctx: &StepContext) -> Result<StepOutput> {
        let health_config = ctx.config.health_check.as_ref();

        match health_config {
            Some(config) => Ok(StepOutput::ok(format!(
                "Would check health at {} (expect status {})",
                config.url, config.expected_status
            ))),
            None => Ok(StepOutput::skipped("No health check configuration")),
        }
    }
}
