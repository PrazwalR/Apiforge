use crate::error::Result;
use crate::integrations::github::{GitHubClient, ReleaseConfig};
use crate::steps::{Step, StepContext, StepOutput};
use crate::utils::format_version;
use async_trait::async_trait;
use semver::Version;

pub struct GitHubReleaseStep {
    version: Version,
    previous_tag: Option<String>,
    changelog: Option<String>,
}

impl GitHubReleaseStep {
    pub fn new(version: Version) -> Self {
        Self {
            version,
            previous_tag: None,
            changelog: None,
        }
    }

    pub fn with_previous_tag(mut self, tag: Option<String>) -> Self {
        self.previous_tag = tag;
        self
    }

    pub fn with_changelog(mut self, changelog: Option<String>) -> Self {
        self.changelog = changelog;
        self
    }

    fn get_release_body(&self, ctx: &StepContext) -> String {
        if let Some(ref changelog) = self.changelog {
            return changelog.clone();
        }

        // Generate basic release notes
        let mut body = format!("## Release {}\n\n", self.version);

        if let Some(ref prev) = self.previous_tag {
            body.push_str(&format!(
                "**Full Changelog**: https://github.com/{}/compare/{}...{}\n",
                ctx.config.github.as_ref().map(|g| &g.repository).unwrap_or(&String::new()),
                prev,
                format_version(&self.version, &ctx.config.git.tag_format)
            ));
        }

        body
    }
}

#[async_trait]
impl Step for GitHubReleaseStep {
    fn name(&self) -> &str {
        "github-release"
    }

    fn description(&self) -> &str {
        "Create GitHub release"
    }

    async fn validate(&self, ctx: &StepContext) -> Result<()> {
        let github_config = ctx
            .config
            .github
            .as_ref()
            .ok_or(crate::error::GitHubError::TokenInvalid)?;

        // Verify we can connect to GitHub
        let _client = GitHubClient::new(&github_config.token, &github_config.repository).await?;

        Ok(())
    }

    async fn execute(&self, ctx: &StepContext) -> Result<StepOutput> {
        let github_config = ctx
            .config
            .github
            .as_ref()
            .ok_or(crate::error::GitHubError::TokenInvalid)?;

        let client = GitHubClient::new(&github_config.token, &github_config.repository).await?;

        let tag_name = format_version(&self.version, &ctx.config.git.tag_format);
        let release_name = format!("v{}", self.version);

        // Try to generate release notes from GitHub if no changelog provided
        let body = if self.changelog.is_some() {
            self.get_release_body(ctx)
        } else {
            match client
                .generate_release_notes(&tag_name, self.previous_tag.as_deref())
                .await
            {
                Ok(notes) => notes,
                Err(_) => self.get_release_body(ctx),
            }
        };

        let config = ReleaseConfig {
            tag_name: tag_name.clone(),
            name: release_name,
            body,
            draft: github_config.draft,
            prerelease: github_config.prerelease,
        };

        let release = client.create_release(&config).await?;

        Ok(StepOutput::ok(format!(
            "Created GitHub release {} ({})",
            tag_name,
            release.html_url
        )))
    }

    async fn dry_run(&self, ctx: &StepContext) -> Result<StepOutput> {
        let github_config = ctx.config.github.as_ref();
        let tag_name = format_version(&self.version, &ctx.config.git.tag_format);

        let status = match github_config {
            Some(config) => format!(
                "Would create GitHub release {} on {}",
                tag_name, config.repository
            ),
            None => format!("Would create GitHub release {} (no config)", tag_name),
        };

        Ok(StepOutput::ok(status))
    }
}
