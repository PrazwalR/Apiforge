use crate::config::Language;
use crate::error::{ApiForgError, Result};
use crate::integrations::git::GitRepo;
use crate::steps::{Step, StepContext, StepOutput};
use crate::utils::{bump_version, BumpType};
use async_trait::async_trait;
use std::fs;
use std::path::PathBuf;

pub struct VersionBumpStep {
    bump_type: BumpType,
}

impl VersionBumpStep {
    pub fn new(bump_type: BumpType) -> Self {
        Self { bump_type }
    }

    fn read_rust_version(path: &PathBuf) -> Result<String> {
        let content = fs::read_to_string(path)?;
        let doc: toml::Value = toml::from_str(&content)
            .map_err(|e| ApiForgError::Config(format!("Failed to parse Cargo.toml: {}", e)))?;

        doc.get("package")
            .and_then(|p| p.get("version"))
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| ApiForgError::Config("No version field in Cargo.toml".to_string()))
    }

    fn write_rust_version(path: &PathBuf, new_version: &str) -> Result<()> {
        let content = fs::read_to_string(path)?;
        let mut doc: toml_edit::DocumentMut = content
            .parse()
            .map_err(|e| ApiForgError::Config(format!("Failed to parse Cargo.toml: {}", e)))?;

        doc["package"]["version"] = toml_edit::value(new_version);

        fs::write(path, doc.to_string())?;
        Ok(())
    }

    fn read_node_version(path: &PathBuf) -> Result<String> {
        let content = fs::read_to_string(path)?;
        let json: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| ApiForgError::Config(format!("Failed to parse package.json: {}", e)))?;

        json.get("version")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| ApiForgError::Config("No version field in package.json".to_string()))
    }

    fn write_node_version(path: &PathBuf, new_version: &str) -> Result<()> {
        let content = fs::read_to_string(path)?;
        let mut json: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| ApiForgError::Config(format!("Failed to parse package.json: {}", e)))?;

        json["version"] = serde_json::Value::String(new_version.to_string());

        let pretty = serde_json::to_string_pretty(&json).map_err(|e| {
            ApiForgError::Serialization(format!("Failed to serialize package.json: {}", e))
        })?;
        fs::write(path, format!("{}\n", pretty))?;
        Ok(())
    }

    fn get_version_file_path(&self, ctx: &StepContext) -> Result<PathBuf> {
        let repo = GitRepo::open()?;
        let root = repo.root_path();
        let version_file = ctx.config.project.language.version_file();
        Ok(root.join(version_file))
    }

    fn read_version(&self, ctx: &StepContext, path: &PathBuf) -> Result<String> {
        match ctx.config.project.language {
            Language::Rust => Self::read_rust_version(path),
            Language::Node => Self::read_node_version(path),
            _ => Err(ApiForgError::Config(format!(
                "Version bumping not yet supported for {:?}",
                ctx.config.project.language
            ))),
        }
    }

    fn write_version(&self, ctx: &StepContext, path: &PathBuf, version: &str) -> Result<()> {
        match ctx.config.project.language {
            Language::Rust => Self::write_rust_version(path, version),
            Language::Node => Self::write_node_version(path, version),
            _ => Err(ApiForgError::Config(format!(
                "Version bumping not yet supported for {:?}",
                ctx.config.project.language
            ))),
        }
    }
}

#[async_trait]
impl Step for VersionBumpStep {
    fn name(&self) -> &str {
        "version-bump"
    }

    fn description(&self) -> &str {
        "Bump project version"
    }

    async fn validate(&self, ctx: &StepContext) -> Result<()> {
        let path = self.get_version_file_path(ctx)?;
        if !path.exists() {
            return Err(ApiForgError::Config(format!(
                "Version file not found: {}",
                path.display()
            )));
        }
        self.read_version(ctx, &path)?;
        Ok(())
    }

    async fn execute(&self, ctx: &StepContext) -> Result<StepOutput> {
        let path = self.get_version_file_path(ctx)?;
        let current = self.read_version(ctx, &path)?;
        let new_version = bump_version(&current, self.bump_type)?;

        self.write_version(ctx, &path, &new_version.to_string())?;

        Ok(StepOutput::ok(format!(
            "Bumped version from {} to {}",
            current, new_version
        )))
    }

    async fn dry_run(&self, ctx: &StepContext) -> Result<StepOutput> {
        let path = self.get_version_file_path(ctx)?;
        let current = self.read_version(ctx, &path)?;
        let new_version = bump_version(&current, self.bump_type)?;

        Ok(StepOutput::ok(format!(
            "Would bump version from {} to {}",
            current, new_version
        )))
    }

    async fn rollback(&self, ctx: &StepContext) -> Result<()> {
        let repo = GitRepo::open()?;
        let path = self.get_version_file_path(ctx)?;
        let rel_path = path
            .strip_prefix(repo.root_path())
            .map_err(|_| ApiForgError::Config("Invalid path".to_string()))?;

        repo.add(rel_path)?;
        Ok(())
    }
}
