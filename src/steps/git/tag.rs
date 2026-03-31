use crate::error::{GitError, Result};
use crate::integrations::git::GitRepo;
use crate::steps::{Step, StepContext, StepOutput};
use crate::utils::format_version;
use async_trait::async_trait;
use semver::Version;

pub struct GitTagStep {
    version: Version,
}

impl GitTagStep {
    pub fn new(version: Version) -> Self {
        Self { version }
    }

    fn get_tag_name(&self, ctx: &StepContext) -> String {
        format_version(&self.version, &ctx.config.git.tag_format)
    }
}

#[async_trait]
impl Step for GitTagStep {
    fn name(&self) -> &str {
        "git-tag"
    }

    fn description(&self) -> &str {
        "Create Git tag"
    }

    async fn validate(&self, ctx: &StepContext) -> Result<()> {
        let repo = GitRepo::open()?;
        let tag_name = self.get_tag_name(ctx);

        // Check if tag already exists
        if repo.tag_exists(&tag_name)? {
            return Err(GitError::TagFailed(format!(
                "Tag '{}' already exists. Use a different version or delete the existing tag.",
                tag_name
            )).into());
        }

        Ok(())
    }

    async fn execute(&self, ctx: &StepContext) -> Result<StepOutput> {
        let repo = GitRepo::open()?;
        let tag_name = self.get_tag_name(ctx);
        let message = format!("Release {}", self.version);

        repo.create_tag(&tag_name, &message)?;

        Ok(StepOutput::ok(format!("Created tag {}", tag_name)))
    }

    async fn dry_run(&self, ctx: &StepContext) -> Result<StepOutput> {
        let tag_name = self.get_tag_name(ctx);
        Ok(StepOutput::ok(format!("Would create tag {}", tag_name)))
    }

    async fn rollback(&self, ctx: &StepContext) -> Result<()> {
        let repo = GitRepo::open()?;
        let tag_name = self.get_tag_name(ctx);

        // Delete the tag we created
        if repo.tag_exists(&tag_name)? {
            repo.delete_tag(&tag_name)?;
            tracing::info!("Deleted tag {}", tag_name);
        }

        Ok(())
    }
}
