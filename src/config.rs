use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub project: ProjectConfig,
    pub git: GitConfig,
    pub docker: DockerConfig,
    pub kubernetes: KubernetesConfig,
    pub aws: AwsConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github: Option<GitHubConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notifications: Option<NotificationsConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_check: Option<HealthCheckConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectConfig {
    pub name: String,
    pub language: Language,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Rust,
    Node,
    Python,
    Go,
    Java,
}

impl Language {
    pub fn version_file(&self) -> &str {
        match self {
            Language::Rust => "Cargo.toml",
            Language::Node => "package.json",
            Language::Python => "pyproject.toml",
            Language::Go => "go.mod",
            Language::Java => "pom.xml",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitConfig {
    pub main_branch: String,
    pub tag_format: String,
    #[serde(default = "default_true")]
    pub changelog: bool,
    pub commit_message: String,
    #[serde(default = "default_remote")]
    pub remote: String,
    #[serde(default = "default_true")]
    pub require_clean: bool,
    #[serde(default = "default_true")]
    pub require_main_branch: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DockerConfig {
    pub registry: DockerRegistry,
    pub repository: String,
    #[serde(default = "default_dockerfile")]
    pub dockerfile: String,
    #[serde(default = "default_context")]
    pub context: String,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_args: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DockerRegistry {
    AwsEcr,
    DockerHub,
    Ghcr,
    Custom,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KubernetesConfig {
    pub context: String,
    pub namespace: String,
    pub deployment: String,
    pub manifest_path: String,
    pub image_field: String,
    #[serde(default = "default_rollout_timeout")]
    pub rollout_timeout: u64,
    #[serde(default = "default_min_ready_percent")]
    pub min_ready_percent: u8,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AwsConfig {
    pub region: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitHubConfig {
    pub repository: String,
    pub token: String,
    #[serde(default = "default_true")]
    pub create_release: bool,
    #[serde(default = "default_false")]
    pub prerelease: bool,
    #[serde(default = "default_false")]
    pub draft: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotificationsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slack: Option<SlackConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook: Option<WebhookConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SlackConfig {
    pub webhook_url: String,
    pub message: String,
    #[serde(default = "default_notify_on")]
    pub notify_on: NotifyOn,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebhookConfig {
    pub url: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    pub body: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NotifyOn {
    Success,
    Failure,
    Both,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HealthCheckConfig {
    pub url: String,
    #[serde(default = "default_expected_status")]
    pub expected_status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_body_field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_body_value: Option<String>,
    #[serde(default = "default_health_timeout")]
    pub timeout: u64,
    #[serde(default = "default_health_interval")]
    pub interval: u64,
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_remote() -> String {
    "origin".to_string()
}

fn default_dockerfile() -> String {
    "Dockerfile".to_string()
}

fn default_context() -> String {
    ".".to_string()
}

fn default_rollout_timeout() -> u64 {
    300
}

fn default_min_ready_percent() -> u8 {
    100
}

fn default_notify_on() -> NotifyOn {
    NotifyOn::Both
}

fn default_method() -> String {
    "POST".to_string()
}

fn default_expected_status() -> u16 {
    200
}

fn default_health_timeout() -> u64 {
    60
}

fn default_health_interval() -> u64 {
    5
}

impl Config {
    pub fn from_file(path: &PathBuf) -> crate::error::Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            crate::error::ApiForgError::Config(format!("Failed to read config file: {}", e))
        })?;

        let config: Config = toml::from_str(&content).map_err(|e| {
            crate::error::ApiForgError::Config(format!("Failed to parse config: {}", e))
        })?;

        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> crate::error::Result<()> {
        // Git validations
        if !self.git.tag_format.contains("{version}") {
            return Err(crate::error::ApiForgError::Config(
                "git.tag_format must contain {version} placeholder".to_string(),
            ));
        }

        // Docker validations
        if self.docker.repository.is_empty() {
            return Err(crate::error::ApiForgError::Config(
                "docker.repository cannot be empty".to_string(),
            ));
        }

        if self.docker.tags.is_empty() {
            return Err(crate::error::ApiForgError::Config(
                "docker.tags must have at least one tag pattern".to_string(),
            ));
        }

        // Kubernetes validations
        if self.kubernetes.namespace.is_empty() {
            return Err(crate::error::ApiForgError::Config(
                "kubernetes.namespace cannot be empty".to_string(),
            ));
        }

        if self.kubernetes.deployment.is_empty() {
            return Err(crate::error::ApiForgError::Config(
                "kubernetes.deployment cannot be empty".to_string(),
            ));
        }

        if self.kubernetes.context.is_empty() {
            return Err(crate::error::ApiForgError::Config(
                "kubernetes.context cannot be empty".to_string(),
            ));
        }

        if self.kubernetes.min_ready_percent > 100 {
            return Err(crate::error::ApiForgError::Config(
                "kubernetes.min_ready_percent must be between 0-100".to_string(),
            ));
        }

        if self.kubernetes.rollout_timeout == 0 {
            return Err(crate::error::ApiForgError::Config(
                "kubernetes.rollout_timeout must be greater than 0".to_string(),
            ));
        }

        // AWS region validation (if ECR is used)
        if matches!(self.docker.registry, DockerRegistry::AwsEcr) && self.aws.region.is_empty() {
            return Err(crate::error::ApiForgError::Config(
                "aws.region is required when using ECR registry".to_string(),
            ));
        }

        // Health check validations
        if let Some(ref hc) = self.health_check {
            if hc.url.is_empty() {
                return Err(crate::error::ApiForgError::Config(
                    "health_check.url cannot be empty".to_string(),
                ));
            }
            
            if hc.timeout == 0 {
                return Err(crate::error::ApiForgError::Config(
                    "health_check.timeout must be greater than 0".to_string(),
                ));
            }
        }

        // Notification validations
        if let Some(ref notify) = self.notifications {
            if let Some(ref slack) = notify.slack {
                if slack.webhook_url.is_empty() {
                    return Err(crate::error::ApiForgError::Config(
                        "notifications.slack.webhook_url cannot be empty".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }

    pub fn save(&self, path: &PathBuf) -> crate::error::Result<()> {
        let content = toml::to_string_pretty(self).map_err(|e| {
            crate::error::ApiForgError::Config(format!("Failed to serialize config: {}", e))
        })?;

        std::fs::write(path, content).map_err(|e| {
            crate::error::ApiForgError::Config(format!("Failed to write config file: {}", e))
        })?;

        Ok(())
    }
}
