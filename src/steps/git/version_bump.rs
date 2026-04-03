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

    fn read_python_version(path: &PathBuf) -> Result<String> {
        let content = fs::read_to_string(path)?;
        let doc: toml::Value = toml::from_str(&content)
            .map_err(|e| ApiForgError::Config(format!("Failed to parse pyproject.toml: {}", e)))?;

        // Try poetry/tool.poetry.version first, then project.version
        doc.get("tool")
            .and_then(|t| t.get("poetry"))
            .and_then(|p| p.get("version"))
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| {
                doc.get("project")
                    .and_then(|p| p.get("version"))
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
            .ok_or_else(|| ApiForgError::Config("No version field in pyproject.toml (expected tool.poetry.version or project.version)".to_string()))
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

    fn read_go_version(path: &PathBuf) -> Result<String> {
        // Go modules don't have a standard version field in go.mod
        // We look for a version.go file or try to extract from module path
        let content = fs::read_to_string(path)?;

        // First try to find a version.go pattern (common convention)
        let version_file = path.parent()
            .map(|p| p.join("version.go"))
            .filter(|p| p.exists());

        if let Some(vf) = version_file {
            let vf_content = fs::read_to_string(&vf)?;
            // Look for Version variable definition
            for line in vf_content.lines() {
                if line.contains("Version") && line.contains('=') {
                    if let Some(quote_start) = line.find('"') {
                        if let Some(quote_end) = line[quote_start + 1..].find('"') {
                            let version = &line[quote_start + 1..quote_start + 1 + quote_end];
                            if !version.is_empty() {
                                return Ok(version.to_string());
                            }
                        }
                    }
                }
            }
        }

        // Fallback: try to find version in module path comment
        // go.mod files sometimes have: // v1.2.3
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("// v") || trimmed.starts_with("// ") {
                let potential = trimmed.trim_start_matches("// ").trim();
                if semver::Version::parse(potential.trim_start_matches('v')).is_ok() {
                    return Ok(potential.to_string());
                }
            }
        }

        Err(ApiForgError::Config(
            "Could not find version in go.mod or version.go. Consider creating a version.go file with a Version constant.".to_string()
        ))
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

    fn read_java_version(path: &PathBuf) -> Result<String> {
        let content = fs::read_to_string(path)?;

        // Parse pom.xml for version
        // Look for <version>X.Y.Z</version> that's a direct child of <project>
        // We need to be careful not to match dependency versions

        // First, try to find the project version (not in dependencies/plugins)
        let lines: Vec<&str> = content.lines().collect();
        let mut in_project = false;

        for line in &lines {
            let trimmed = line.trim();

            if trimmed.contains("<project") && !trimmed.contains("</project>") {
                in_project = true;
                continue;
            }

            if trimmed == "</project>" {
                in_project = false;
                continue;
            }

            if in_project && trimmed.starts_with("<version>") && trimmed.ends_with("</version>") {
                // Extract version content
                let start = trimmed.find("<version>").unwrap() + "<version>".len();
                let end = trimmed.find("</version>").unwrap();
                let version = &trimmed[start..end];

                // Skip property references like ${version}
                if !version.starts_with("${") {
                    return Ok(version.to_string());
                }
            }
        }

        // Fallback: try to find any version tag at project level using regex-like approach
        if let Some(start) = content.find("<version>") {
            if let Some(end) = content[start..].find("</version>") {
                let version = &content[start + 9..start + end];
                if !version.starts_with("${") && !version.contains('<') {
                    return Ok(version.to_string());
                }
            }
        }

        Err(ApiForgError::Config(
            "No version field found in pom.xml".to_string()
        ))
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

    fn read_version(&self, ctx: &StepContext, path: &PathBuf) -> Result<String> {
        match ctx.config.project.language {
            Language::Rust => Self::read_rust_version(path),
            Language::Node => Self::read_node_version(path),
            Language::Python => Self::read_python_version(path),
            Language::Go => Self::read_go_version(path),
            Language::Java => Self::read_java_version(path),
        }
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

        // Restore the original file from HEAD
        repo.checkout_file(rel_path)?;
        
        tracing::info!("Restored {} to previous version", rel_path.display());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let result = VersionBumpStep::read_python_version(&file.path().to_path_buf());
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
        let result = VersionBumpStep::read_python_version(&file.path().to_path_buf());
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
        let result = VersionBumpStep::read_java_version(&file.path().to_path_buf());
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
        let result = VersionBumpStep::read_go_version(&file.path().to_path_buf());
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
