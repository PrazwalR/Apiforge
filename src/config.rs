use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize)]
/// Root configuration loaded from `apiforge.toml`.
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
/// Project identity and language metadata.
pub struct ProjectConfig {
    pub name: String,
    pub language: Language,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
/// Supported project language types for version file detection.
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
/// Git settings for branch checks, tagging, and commit behavior.
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
    /// Timeout in seconds for git fetch operations (default: 60)
    #[serde(default = "default_git_fetch_timeout")]
    pub fetch_timeout_secs: u64,
    /// Timeout in seconds for git push operations (default: 120)
    #[serde(default = "default_git_push_timeout")]
    pub push_timeout_secs: u64,
    /// Timeout in seconds for other git operations (default: 30)
    #[serde(default = "default_git_operation_timeout")]
    pub operation_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
/// Docker build and push settings.
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
/// Supported container registry backends.
pub enum DockerRegistry {
    AwsEcr,
    DockerHub,
    Ghcr,
    Custom,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
/// Kubernetes deployment rollout settings.
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
/// AWS configuration used by cloud integrations.
pub struct AwsConfig {
    pub region: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
/// GitHub release settings.
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
/// Outbound notification configuration.
pub struct NotificationsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slack: Option<SlackConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook: Option<WebhookConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
/// Slack notification settings.
pub struct SlackConfig {
    pub webhook_url: String,
    pub message: String,
    #[serde(default = "default_notify_on")]
    pub notify_on: NotifyOn,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
/// Generic webhook notification settings.
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
/// Notification trigger mode.
pub enum NotifyOn {
    Success,
    Failure,
    Both,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "UPPERCASE")]
/// HTTP methods supported by health checks.
pub enum HttpMethod {
    #[default]
    GET,
    POST,
    HEAD,
    PUT,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
/// Health endpoint polling configuration.
pub struct HealthCheckConfig {
    pub url: String,
    #[serde(default = "default_http_method")]
    pub method: HttpMethod,
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

fn default_http_method() -> HttpMethod {
    HttpMethod::GET
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

fn default_git_fetch_timeout() -> u64 {
    60
}

fn default_git_push_timeout() -> u64 {
    120
}

fn default_git_operation_timeout() -> u64 {
    30
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

        if self.git.fetch_timeout_secs == 0 {
            return Err(crate::error::ApiForgError::Config(
                "git.fetch_timeout_secs must be greater than 0".to_string(),
            ));
        }

        if self.git.push_timeout_secs == 0 {
            return Err(crate::error::ApiForgError::Config(
                "git.push_timeout_secs must be greater than 0".to_string(),
            ));
        }

        if self.git.operation_timeout_secs == 0 {
            return Err(crate::error::ApiForgError::Config(
                "git.operation_timeout_secs must be greater than 0".to_string(),
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

        // Docker tag format validation
        // Compile regex once outside the loop
        let tag_regex = regex::Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9_.-]*$").unwrap();
        for tag in &self.docker.tags {
            // Docker tags must be <= 128 chars, start with letter/number,
            // and only contain letters, numbers, periods, underscores, dashes
            if tag.is_empty() {
                return Err(crate::error::ApiForgError::Config(
                    "docker.tags cannot contain empty strings".to_string(),
                ));
            }

            // Resolve supported placeholders to validate final docker tag syntax.
            // Supported placeholders mirror docker tag expansion in steps.
            let resolved_tag = tag
                .replace("{version}", "1.2.3")
                .replace("{major}", "1")
                .replace("{minor}", "2")
                .replace("{patch}", "3")
                .replace("{git_sha}", "abcdef0")
                .replace("{git_sha_full}", "abcdef0123456789");

            if resolved_tag.contains('{') || resolved_tag.contains('}') {
                return Err(crate::error::ApiForgError::Config(format!(
                    "docker tag '{}' contains unsupported placeholder(s). Supported placeholders: {{version}}, {{major}}, {{minor}}, {{patch}}, {{git_sha}}, {{git_sha_full}}",
                    tag
                )));
            }

            if resolved_tag.len() > 128 {
                return Err(crate::error::ApiForgError::Config(format!(
                    "docker tag '{}' exceeds 128 character limit after template resolution",
                    tag
                )));
            }
            if !tag_regex.is_match(&resolved_tag) {
                return Err(crate::error::ApiForgError::Config(format!(
                    "docker tag '{}' has invalid format after template resolution ('{}'). Tags must start with alphanumeric and contain only [a-zA-Z0-9_.-]",
                    tag, resolved_tag
                )));
            }
        }

        // Kubernetes image field validation
        if !self.kubernetes.manifest_path.is_empty() {
            // The image field in kubernetes config should be a valid reference format
            // Pattern: [registry/]repository[:tag][@digest]
            // For now just ensure it's not empty when we expect to update images
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

            if hc.interval == 0 {
                return Err(crate::error::ApiForgError::Config(
                    "health_check.interval must be greater than 0".to_string(),
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
