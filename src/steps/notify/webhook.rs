use crate::error::{ApiForgError, Result};
use crate::steps::{Step, StepContext, StepOutput};
use crate::utils::TemplateEngine;
use async_trait::async_trait;
use semver::Version;
use std::collections::HashMap;

pub struct WebhookNotifyStep {
    version: Version,
    success: bool,
    error_message: Option<String>,
}

impl WebhookNotifyStep {
    pub fn new(version: Version, success: bool) -> Self {
        Self {
            version,
            success,
            error_message: None,
        }
    }

    pub fn with_error(mut self, error: String) -> Self {
        self.error_message = Some(error);
        self
    }

    async fn send_webhook(&self, ctx: &StepContext) -> Result<()> {
        let webhook_config = ctx
            .config
            .notifications
            .as_ref()
            .and_then(|n| n.webhook.as_ref())
            .ok_or_else(|| ApiForgError::Config("Webhook configuration missing".to_string()))?;

        // Build template context
        let mut template_ctx = HashMap::new();
        template_ctx.insert("version".to_string(), self.version.to_string());
        template_ctx.insert("project".to_string(), ctx.config.project.name.clone());
        template_ctx.insert(
            "status".to_string(),
            if self.success { "success" } else { "failed" }.to_string(),
        );
        if let Some(ref error) = self.error_message {
            template_ctx.insert("error".to_string(), error.clone());
        }

        let mut engine = TemplateEngine::new();
        let body = engine.render(&webhook_config.body, &template_ctx)?;

        let client = reqwest::Client::new();
        let mut request = match webhook_config.method.to_uppercase().as_str() {
            "POST" => client.post(&webhook_config.url),
            "PUT" => client.put(&webhook_config.url),
            "PATCH" => client.patch(&webhook_config.url),
            _ => client.post(&webhook_config.url),
        };

        // Add custom headers
        if let Some(ref headers) = webhook_config.headers {
            for (key, value) in headers {
                let resolved_value = engine.render(value, &template_ctx)?;
                request = request.header(key, resolved_value);
            }
        }

        // Try to parse body as JSON, otherwise send as plain text
        let response = if let Ok(json_body) = serde_json::from_str::<serde_json::Value>(&body) {
            request.json(&json_body).send().await
        } else {
            request
                .header("Content-Type", "text/plain")
                .body(body)
                .send()
                .await
        }
        .map_err(|e| ApiForgError::StepFailed(format!("Webhook request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ApiForgError::StepFailed(format!(
                "Webhook returned error {}: {}",
                status, body
            )));
        }

        Ok(())
    }
}

#[async_trait]
impl Step for WebhookNotifyStep {
    fn name(&self) -> &str {
        "webhook-notify"
    }

    fn description(&self) -> &str {
        "Send webhook notification"
    }

    async fn validate(&self, ctx: &StepContext) -> Result<()> {
        let webhook = ctx
            .config
            .notifications
            .as_ref()
            .and_then(|n| n.webhook.as_ref());

        if webhook.is_none() {
            return Err(ApiForgError::Config("Webhook configuration missing".to_string()));
        }

        Ok(())
    }

    async fn execute(&self, ctx: &StepContext) -> Result<StepOutput> {
        self.send_webhook(ctx).await?;
        Ok(StepOutput::ok("Webhook notification sent"))
    }

    async fn dry_run(&self, ctx: &StepContext) -> Result<StepOutput> {
        let webhook_config = ctx
            .config
            .notifications
            .as_ref()
            .and_then(|n| n.webhook.as_ref());

        match webhook_config {
            Some(config) => Ok(StepOutput::ok(format!(
                "Would send {} request to {}",
                config.method, config.url
            ))),
            None => Ok(StepOutput::skipped("No webhook configuration")),
        }
    }
}
