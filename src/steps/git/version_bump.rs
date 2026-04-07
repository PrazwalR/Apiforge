use crate::config::Language;
use crate::error::{ApiForgError, Result};
use crate::integrations::git::GitRepo;
use crate::steps::{Step, StepContext, StepOutput};
use crate::utils::{bump_version, BumpType};
use crate::utils::version::read_version;
use async_trait::async_trait;
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;

pub struct VersionBumpStep {
    bump_type: BumpType,
    /// Stores original file content before modification for safe rollback.
    /// Using RwLock to allow interior mutability in async context.
    original_content: RwLock<Option<(PathBuf, String)>>,
}

impl VersionBumpStep {
    pub fn new(bump_type: BumpType) -> Self {
        Self { 
            bump_type,
            original_content: RwLock::new(None),
        }
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

    fn write_python_version(path: &PathBuf, new_version: &str) -> Result<()> {
        let content = fs::read_to_string(path)?;
        let mut doc: toml_edit::DocumentMut = content
            .parse()
            .map_err(|e| ApiForgError::Config(format!("Failed to parse pyproject.toml: {}", e)))?;

        // Try to set in tool.poetry.version first
        if doc.get("tool").and_then(|t| t.get("poetry")).is_some() {
            doc["tool"]["poetry"]["version"] = toml_edit::value(new_version);
        } else if doc.get("project").is_some() {
            // Otherwise try project.version (PEP 621)
            doc["project"]["version"] = toml_edit::value(new_version);
        } else {
            return Err(ApiForgError::Config(
                "Could not find version location in pyproject.toml (expected tool.poetry or project section)".to_string()
            ));
        }

        fs::write(path, doc.to_string())?;
        Ok(())
    }

    fn write_go_version(path: &PathBuf, new_version: &str) -> Result<()> {
        // Try to write to version.go if it exists
        let version_file = path.parent()
            .map(|p| p.join("version.go"))
            .filter(|p| p.exists());

        if let Some(vf) = version_file {
            let content = fs::read_to_string(&vf)?;
            let mut new_content = String::new();
            let mut found = false;

            for line in content.lines() {
                if line.contains("Version") && line.contains('=') && !found {
                    // Replace the version string
                    if let Some(quote_start) = line.find('"') {
                        if let Some(quote_end) = line[quote_start + 1..].find('"') {
                            let new_line = format!("{}\"{}\"{}",
                                &line[..quote_start + 1],
                                new_version,
                                &line[quote_start + 1 + quote_end..]
                            );
                            new_content.push_str(&new_line);
                            new_content.push('\n');
                            found = true;
                            continue;
                        }
                    }
                }
                new_content.push_str(line);
                new_content.push('\n');
            }

            if found {
                fs::write(&vf, new_content)?;
                return Ok(());
            }
        }

        // Fallback: add a comment to go.mod
        let content = fs::read_to_string(path)?;
        let new_content = format!("{}\n// {}\n", content.trim_end(), new_version);
        fs::write(path, new_content)?;

        tracing::warn!("Wrote version to go.mod comment. Consider creating a version.go file for better version management.");
        Ok(())
    }

    fn write_java_version(path: &PathBuf, new_version: &str) -> Result<()> {
        let content = fs::read_to_string(path)?;
        let mut new_content = String::new();
        let mut found = false;
        let mut in_project = false;

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed.contains("<project") && !trimmed.contains("</project>") {
                in_project = true;
            }

            if trimmed == "</project>" {
                in_project = false;
            }

            if in_project && !found && trimmed.starts_with("<version>") && trimmed.ends_with("</version>") {
                // Check this isn't a property reference
                let start = trimmed.find("<version>").unwrap() + "<version>".len();
                let end = trimmed.find("</version>").unwrap();
                let current = &trimmed[start..end];

                if !current.starts_with("${") {
                    // Replace this version
                    let indent = line.len() - line.trim_start().len();
                    let spaces = " ".repeat(indent);
                    new_content.push_str(&format!("{}<version>{}</version>\n", spaces, new_version));
                    found = true;
                    continue;
                }
            }

            new_content.push_str(line);
            new_content.push('\n');
        }

        if !found {
            return Err(ApiForgError::Config(
                "Could not find project version to update in pom.xml".to_string()
            ));
        }

        fs::write(path, new_content)?;
        Ok(())
    }

    fn get_version_file_path(&self, ctx: &StepContext) -> Result<PathBuf> {
        let repo = GitRepo::open()?;
        let root = repo.root_path();
        let version_file = ctx.config.project.language.version_file();
        Ok(root.join(version_file))
    }

    fn write_version(&self, ctx: &StepContext, path: &PathBuf, version: &str) -> Result<()> {
        match ctx.config.project.language {
            Language::Rust => Self::write_rust_version(path, version),
            Language::Node => Self::write_node_version(path, version),
            Language::Python => Self::write_python_version(path, version),
            Language::Go => Self::write_go_version(path, version),
            Language::Java => Self::write_java_version(path, version),
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
        read_version(ctx.config.project.language, &path)?;
        Ok(())
    }

    async fn execute(&self, ctx: &StepContext) -> Result<StepOutput> {
        let path = self.get_version_file_path(ctx)?;
        
        // Store original content before modification for safe rollback
        // This preserves any uncommitted changes that the user might have
        let original = fs::read_to_string(&path)?;
        {
            let mut guard = self.original_content.write()
                .map_err(|_| ApiForgError::StepFailed("Failed to acquire lock".to_string()))?;
            *guard = Some((path.clone(), original));
        }
        
        let current = read_version(ctx.config.project.language, &path)?;
        let new_version = bump_version(&current, self.bump_type)?;

        self.write_version(ctx, &path, &new_version.to_string())?;

        Ok(StepOutput::ok(format!(
            "Bumped version from {} to {}",
            current, new_version
        )))
    }

    async fn dry_run(&self, ctx: &StepContext) -> Result<StepOutput> {
        let path = self.get_version_file_path(ctx)?;
        let current = read_version(ctx.config.project.language, &path)?;
        let new_version = bump_version(&current, self.bump_type)?;

        Ok(StepOutput::ok(format!(
            "Would bump version from {} to {}",
            current, new_version
        )))
    }

    async fn rollback(&self, _ctx: &StepContext) -> Result<()> {
        // Try to restore from our saved original content first
        // This is safer than checkout_file because it preserves any uncommitted changes
        // that existed before the release was started
        let restored = {
            let guard = self.original_content.read()
                .map_err(|_| ApiForgError::StepFailed("Failed to acquire lock".to_string()))?;
            
            if let Some((ref path, ref content)) = *guard {
                fs::write(path, content)?;
                tracing::info!("Restored {} from saved original content", path.display());
                true
            } else {
                false
            }
        };
        
        if !restored {
            // Fallback: if we don't have the original content (shouldn't happen),
            // log a warning. We can't safely restore without knowing the original state.
            tracing::warn!(
                "No original content saved for version file rollback. \
                 The file may need to be manually restored if there were uncommitted changes."
            );
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::version::{read_python_version, read_java_version, read_go_version};
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_read_python_version_poetry() {
        let content = r#"
[tool.poetry]
name = "my-app"
version = "1.2.3"
description = "Test app"
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        let result = read_python_version(file.path());
        assert_eq!(result.unwrap(), "1.2.3");
    }

    #[test]
    fn test_read_python_version_pep621() {
        let content = r#"
[project]
name = "my-app"
version = "2.0.0"
description = "Test app"
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        let result = read_python_version(file.path());
        assert_eq!(result.unwrap(), "2.0.0");
    }

    #[test]
    fn test_write_python_version_poetry() {
        let content = r#"[tool.poetry]
name = "my-app"
version = "1.0.0"
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        VersionBumpStep::write_python_version(&file.path().to_path_buf(), "1.1.0").unwrap();
        let new_content = fs::read_to_string(file.path()).unwrap();
        assert!(new_content.contains("version = \"1.1.0\""));
    }

    #[test]
    fn test_read_java_version() {
        let content = r#"<?xml version="1.0" encoding="UTF-8"?>
<project>
    <modelVersion>4.0.0</modelVersion>
    <groupId>com.example</groupId>
    <artifactId>my-app</artifactId>
    <version>3.4.5</version>
</project>
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        let result = read_java_version(file.path());
        assert_eq!(result.unwrap(), "3.4.5");
    }

    #[test]
    fn test_write_java_version() {
        let content = r#"<?xml version="1.0" encoding="UTF-8"?>
<project>
    <version>1.0.0</version>
</project>
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        VersionBumpStep::write_java_version(&file.path().to_path_buf(), "2.0.0").unwrap();
        let new_content = fs::read_to_string(file.path()).unwrap();
        assert!(new_content.contains("<version>2.0.0</version>"));
    }

    #[test]
    fn test_read_go_version_from_mod() {
        let content = r#"
module github.com/example/app

go 1.21
// v1.2.3
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        let result = read_go_version(file.path());
        assert_eq!(result.unwrap(), "v1.2.3");
    }

    #[test]
    fn test_write_go_version_to_mod() {
        let content = r#"module github.com/example/app

go 1.21
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        VersionBumpStep::write_go_version(&file.path().to_path_buf(), "1.2.4").unwrap();
        let new_content = fs::read_to_string(file.path()).unwrap();
        assert!(new_content.contains("// 1.2.4"));
    }
}
