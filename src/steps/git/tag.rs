use crate::error::Result;
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
}

#[async_trait]
impl Step for GitTagStep {
    fn name(&self) -> &str {
        "git-tag"
    }

    fn description(&self) -> &str {
        "Create Git tag"
    }

    async fn validate(&self, _ctx: &StepContext) -> Result<()> {
        GitRepo::open()?;
        Ok(())
    }

    async fn execute(&self, ctx: &StepContext) -> Result<StepOutput> {
        let repo = GitRepo::open()?;
        let tag_name = format_version(&self.version, &ctx.config.git.tag_format);
        let message = format!("Release {}", self.version);

        repo.create_tag(&tag_name, &message)?;

        Ok(StepOutput::ok(format!("Created tag {}", tag_name)))
    }

    async fn dry_run(&self, ctx: &StepContext) -> Result<StepOutput> {
        let tag_name = format_version(&self.version, &ctx.config.git.tag_format);
        Ok(StepOutput::ok(format!("Would create tag {}", tag_name)))
    }
}
