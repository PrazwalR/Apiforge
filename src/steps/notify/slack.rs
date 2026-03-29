use crate::error::{ApiForgError, Result};
use crate::steps::{Step, StepContext, StepOutput};
use crate::utils::TemplateEngine;
use async_trait::async_trait;
use semver::Version;
use std::collections::HashMap;

pub struct SlackNotifyStep {
    version: Version,
    success: bool,
    error_message: Option<String>,
}

impl SlackNotifyStep {
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

    async fn send_slack_message(&self, webhook_url: &str, message: &str) -> Result<()> {
        let client = reqwest::Client::new();

        let payload = serde_json::json!({
            "text": message,
            "unfurl_links": false,
            "unfurl_media": false
        });

        let response = client
            .post(webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| ApiForgError::StepFailed(format!("Slack request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ApiForgError::StepFailed(format!(
                "Slack returned error {}: {}",
                status, body
            ))
            .into());
        }

        Ok(())
    }
}

#[async_trait]
impl Step for SlackNotifyStep {
    fn name(&self) -> &str {
        "slack-notify"
    }

    fn description(&self) -> &str {
        "Send Slack notification"
    }

    async fn validate(&self, ctx: &StepContext) -> Result<()> {
        let notifications = ctx.config.notifications.as_ref();
        let slack = notifications.and_then(|n| n.slack.as_ref());

        if slack.is_none() {
            return Err(ApiForgError::Config("Slack configuration missing".to_string()).into());
        }

        Ok(())
    }

    async fn execute(&self, ctx: &StepContext) -> Result<StepOutput> {
        let slack_config = ctx
            .config
            .notifications
            .as_ref()
            .and_then(|n| n.slack.as_ref())
            .ok_or_else(|| ApiForgError::Config("Slack configuration missing".to_string()))?;

        // Check if we should notify based on success/failure
        let should_notify = match slack_config.notify_on {
            crate::config::NotifyOn::Success => self.success,
            crate::config::NotifyOn::Failure => !self.success,
            crate::config::NotifyOn::Both => true,
        };

        if !should_notify {
            return Ok(StepOutput::skipped("Notification not configured for this status"));
        }

        // Build template context
        let mut template_ctx = HashMap::new();
        template_ctx.insert("version".to_string(), self.version.to_string());
        template_ctx.insert("project".to_string(), ctx.config.project.name.clone());
        template_ctx.insert(
            "status".to_string(),
            if self.success { "success" } else { "failed" }.to_string(),
        );
        template_ctx.insert(
            "status_emoji".to_string(),
            if self.success { "✅" } else { "❌" }.to_string(),
        );
        if let Some(ref error) = self.error_message {
            template_ctx.insert("error".to_string(), error.clone());
        }

        let mut engine = TemplateEngine::new();
        let message = engine.render(&slack_config.message, &template_ctx)?;

        self.send_slack_message(&slack_config.webhook_url, &message)
            .await?;

        Ok(StepOutput::ok("Slack notification sent"))
    }

    async fn dry_run(&self, ctx: &StepContext) -> Result<StepOutput> {
        let slack_config = ctx
            .config
            .notifications
            .as_ref()
            .and_then(|n| n.slack.as_ref());

        match slack_config {
            Some(_) => Ok(StepOutput::ok(format!(
                "Would send Slack notification for {} release",
                if self.success { "successful" } else { "failed" }
            ))),
            None => Ok(StepOutput::skipped("No Slack configuration")),
        }
    }
}
