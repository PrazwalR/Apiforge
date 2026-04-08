//! Utilities for reading and writing version information from various project types.

use crate::config::Language;
use crate::error::{ApiForgError, Result};
use std::fs;
use std::path::Path;

/// Read the current version from a project file based on language.
pub fn read_version(language: Language, path: &Path) -> Result<String> {
    match language {
        Language::Rust => read_rust_version(path),
        Language::Node => read_node_version(path),
        Language::Python => read_python_version(path),
        Language::Go => read_go_version(path),
        Language::Java => read_java_version(path),
    }
}

/// Read version from Cargo.toml (Rust projects).
pub fn read_rust_version(path: &Path) -> Result<String> {
    let content = fs::read_to_string(path)?;
    let doc: toml::Value = toml::from_str(&content)
        .map_err(|e| ApiForgError::Config(format!("Failed to parse Cargo.toml: {}", e)))?;

    doc.get("package")
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| ApiForgError::Config("No version field in Cargo.toml".to_string()))
}

/// Read version from package.json (Node.js projects).
pub fn read_node_version(path: &Path) -> Result<String> {
    let content = fs::read_to_string(path)?;
    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| ApiForgError::Config(format!("Failed to parse package.json: {}", e)))?;

    json.get("version")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| ApiForgError::Config("No version field in package.json".to_string()))
}

/// Read version from pyproject.toml (Python projects).
/// Supports both Poetry (tool.poetry.version) and PEP 621 (project.version) formats.
pub fn read_python_version(path: &Path) -> Result<String> {
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
        .ok_or_else(|| ApiForgError::Config(
            "No version field in pyproject.toml (expected tool.poetry.version or project.version)".to_string()
        ))
}

/// Read version from go.mod or version.go (Go projects).
/// Checks for version.go first, then falls back to go.mod comments.
pub fn read_go_version(path: &Path) -> Result<String> {
    // First try to find a version.go pattern (common convention)
    let version_file = path
        .parent()
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
    let content = fs::read_to_string(path)?;
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

/// Read version from pom.xml (Java/Maven projects).
pub fn read_java_version(path: &Path) -> Result<String> {
    let content = fs::read_to_string(path)?;
    let lines: Vec<&str> = content.lines().collect();
    let mut in_project = false;
    let mut result = None;

    for line in &lines {
        let trimmed = line.trim();

        // Track when we enter/exit the root <project> element
        if trimmed.contains("<project") && !trimmed.contains("</project>") {
            in_project = true;
            continue;
        }

        if trimmed == "</project>" {
            in_project = false;
            continue;
        }

        // Only look for version at the project level (not in dependencies)
        if in_project && trimmed.starts_with("<version>") && trimmed.ends_with("</version>") {
            let start = trimmed.find("<version>").unwrap() + "<version>".len();
            let end = trimmed.find("</version>").unwrap();
            let version = &trimmed[start..end];

            // Skip placeholder versions like ${parent.version}
            if !version.starts_with("${") {
                result = Some(version.to_string());
                break;
            }
        }
    }

    result.ok_or_else(|| ApiForgError::Config("No version field in pom.xml".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_read_rust_version() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"[package]
name = "test"
version = "1.2.3""#
        )
        .unwrap();

        let result = read_rust_version(file.path()).unwrap();
        assert_eq!(result, "1.2.3");
    }

    #[test]
    fn test_read_node_version() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"{{"name": "test", "version": "2.3.4"}}"#).unwrap();

        let result = read_node_version(file.path()).unwrap();
        assert_eq!(result, "2.3.4");
    }

    #[test]
    fn test_read_python_version_poetry() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"[tool.poetry]
name = "test"
version = "3.4.5""#
        )
        .unwrap();

        let result = read_python_version(file.path()).unwrap();
        assert_eq!(result, "3.4.5");
    }

    #[test]
    fn test_read_python_version_pep621() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"[project]
name = "test"
version = "4.5.6""#
        )
        .unwrap();

        let result = read_python_version(file.path()).unwrap();
        assert_eq!(result, "4.5.6");
    }

    #[test]
    fn test_read_java_version() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"<?xml version="1.0"?>
<project>
    <groupId>com.example</groupId>
    <artifactId>test</artifactId>
    <version>5.6.7</version>
</project>"#
        )
        .unwrap();

        let result = read_java_version(file.path()).unwrap();
        assert_eq!(result, "5.6.7");
    }
}
