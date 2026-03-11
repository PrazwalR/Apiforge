use crate::error::{ApiForgError, Result};
use semver::Version;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BumpType {
    Major,
    Minor,
    Patch,
}

impl BumpType {
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "major" => Ok(BumpType::Major),
            "minor" => Ok(BumpType::Minor),
            "patch" => Ok(BumpType::Patch),
            _ => Err(ApiForgError::InvalidVersion(format!(
                "Invalid bump type: {}. Must be one of: major, minor, patch",
                s
            ))),
        }
    }
}

impl std::fmt::Display for BumpType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BumpType::Major => write!(f, "major"),
            BumpType::Minor => write!(f, "minor"),
            BumpType::Patch => write!(f, "patch"),
        }
    }
}

pub fn parse_version(version: &str) -> Result<Version> {
    let clean_version = version.trim_start_matches('v');
    Version::parse(clean_version).map_err(|e| {
        ApiForgError::InvalidVersion(format!("Failed to parse version '{}': {}", version, e))
    })
}

pub fn bump_version(version: &str, bump_type: BumpType) -> Result<Version> {
    let mut ver = parse_version(version)?;

    match bump_type {
        BumpType::Major => {
            ver.major += 1;
            ver.minor = 0;
            ver.patch = 0;
        }
        BumpType::Minor => {
            ver.minor += 1;
            ver.patch = 0;
        }
        BumpType::Patch => {
            ver.patch += 1;
        }
    }

    Ok(ver)
}

pub fn format_version(version: &Version, format: &str) -> String {
    format.replace("{version}", &version.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version() {
        assert_eq!(parse_version("1.2.3").unwrap().to_string(), "1.2.3");
        assert_eq!(parse_version("v1.2.3").unwrap().to_string(), "1.2.3");
    }

    #[test]
    fn test_bump_patch() {
        let result = bump_version("1.2.3", BumpType::Patch).unwrap();
        assert_eq!(result.to_string(), "1.2.4");
    }

    #[test]
    fn test_bump_minor() {
        let result = bump_version("1.2.3", BumpType::Minor).unwrap();
        assert_eq!(result.to_string(), "1.3.0");
    }

    #[test]
    fn test_bump_major() {
        let result = bump_version("1.2.3", BumpType::Major).unwrap();
        assert_eq!(result.to_string(), "2.0.0");
    }

    #[test]
    fn test_format_version() {
        let version = Version::new(1, 2, 3);
        assert_eq!(format_version(&version, "v{version}"), "v1.2.3");
        assert_eq!(format_version(&version, "{version}"), "1.2.3");
    }
}
