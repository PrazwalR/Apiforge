use crate::error::Result;
use crate::integrations::git::{CommitInfo, GitRepo};
use crate::steps::{Step, StepContext, StepOutput};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::fs;
use std::path::PathBuf;

pub struct ChangelogStep {
    version: String,
    previous_tag: Option<String>,
}

impl ChangelogStep {
    pub fn new(version: String, previous_tag: Option<String>) -> Self {
        Self {
            version,
            previous_tag,
        }
    }

    fn format_changelog(
        version: &str,
        commits: &[CommitInfo],
        previous_tag: Option<&str>,
    ) -> String {
        let mut output = String::new();
        let now: DateTime<Utc> = Utc::now();
        
        output.push_str(&format!("## [{}] - {}\n\n", version, now.format("%Y-%m-%d")));

        if commits.is_empty() {
            output.push_str("No changes recorded.\n\n");
            return output;
        }

        let mut features = Vec::new();
        let mut fixes = Vec::new();
        let mut other = Vec::new();

        for commit in commits {
            let msg = commit.message.lines().next().unwrap_or("").trim();
            if msg.is_empty() {
                continue;
            }

            if msg.starts_with("feat:") || msg.starts_with("feature:") {
                features.push(msg.trim_start_matches("feat:").trim_start_matches("feature:").trim());
            } else if msg.starts_with("fix:") {
                fixes.push(msg.trim_start_matches("fix:").trim());
            } else {
                other.push(msg);
            }
        }

        if !features.is_empty() {
            output.push_str("### Features\n");
            for feature in features {
                output.push_str(&format!("- {}\n", feature));
            }
            output.push('\n');
        }

        if !fixes.is_empty() {
            output.push_str("### Bug Fixes\n");
            for fix in fixes {
                output.push_str(&format!("- {}\n", fix));
            }
            output.push('\n');
        }

        if !other.is_empty() {
            output.push_str("### Other Changes\n");
            for change in other {
                output.push_str(&format!("- {}\n", change));
            }
            output.push('\n');
        }

        if let Some(prev) = previous_tag {
            output.push_str(&format!("**Full Changelog**: {}...{}\n\n", prev, version));
        }

        output
    }

    fn get_changelog_path(&self) -> Result<PathBuf> {
        let repo = GitRepo::open()?;
        Ok(repo.root_path().join("CHANGELOG.md"))
    }

    fn prepend_to_changelog(&self, path: &PathBuf, new_content: &str) -> Result<()> {
        let existing = if path.exists() {
            fs::read_to_string(path)?
        } else {
            "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n".to_string()
        };

        let header_end = existing.find("\n\n").unwrap_or(0);
        let (header, rest) = existing.split_at(header_end + 2);

        let updated = format!("{}{}{}", header, new_content, rest);
        fs::write(path, updated)?;

        Ok(())
    }
}

#[async_trait]
impl Step for ChangelogStep {
    fn name(&self) -> &str {
        "changelog"
    }

    fn description(&self) -> &str {
        "Generate changelog from commits"
    }

    async fn validate(&self, _ctx: &StepContext) -> Result<()> {
        GitRepo::open()?;
        Ok(())
    }

    async fn execute(&self, _ctx: &StepContext) -> Result<StepOutput> {
        let repo = GitRepo::open()?;
        
        let commits = if let Some(ref prev_tag) = self.previous_tag {
            repo.get_commits_between(prev_tag, "HEAD")?
        } else {
            Vec::new()
        };

        let changelog_content = Self::format_changelog(
            &self.version,
            &commits,
            self.previous_tag.as_deref(),
        );

        let path = self.get_changelog_path()?;
        self.prepend_to_changelog(&path, &changelog_content)?;

        Ok(StepOutput::ok(format!(
            "Generated changelog with {} commits",
            commits.len()
        )))
    }

    async fn dry_run(&self, _ctx: &StepContext) -> Result<StepOutput> {
        let repo = GitRepo::open()?;
        
        let commits = if let Some(ref prev_tag) = self.previous_tag {
            repo.get_commits_between(prev_tag, "HEAD")?
        } else {
            Vec::new()
        };

        Ok(StepOutput::ok(format!(
            "Would generate changelog with {} commits",
            commits.len()
        )))
    }
}
