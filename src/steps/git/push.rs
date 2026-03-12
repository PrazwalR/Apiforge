use crate::error::Result;
use crate::integrations::git::GitRepo;
use crate::steps::{Step, StepContext, StepOutput};
use crate::utils::format_version;
use async_trait::async_trait;
use semver::Version;

pub struct GitPushStep {
    version: Version,
}

impl GitPushStep {
    pub fn new(version: Version) -> Self {
        Self { version }
    }
}

#[async_trait]
impl Step for GitPushStep {
    fn name(&self) -> &str {
        "git-push"
    }

    fn description(&self) -> &str {
        "Push commits and tags to remote"
    }

    async fn validate(&self, ctx: &StepContext) -> Result<()> {
        let repo = GitRepo::open()?;
        if !repo.remote_exists(&ctx.config.git.remote) {
            return Err(crate::error::GitError::RemoteNotFound(
                ctx.config.git.remote.clone(),
            )
            .into());
        }
        Ok(())
    }

    async fn execute(&self, ctx: &StepContext) -> Result<StepOutput> {
        let repo = GitRepo::open()?;
        let remote = &ctx.config.git.remote;
        let branch = repo.current_branch()?;
        
        repo.push(remote, &format!("refs/heads/{}", branch))?;

        let tag_name = format_version(&self.version, &ctx.config.git.tag_format);
        repo.push(remote, &format!("refs/tags/{}", tag_name))?;

        Ok(StepOutput::ok(format!(
            "Pushed to {} (branch + tag)",
            remote
        )))
    }

    async fn dry_run(&self, ctx: &StepContext) -> Result<StepOutput> {
        let repo = GitRepo::open()?;
        let branch = repo.current_branch()?;
        let tag_name = format_version(&self.version, &ctx.config.git.tag_format);

        Ok(StepOutput::ok(format!(
            "Would push branch '{}' and tag '{}' to '{}'",
            branch, tag_name, ctx.config.git.remote
        )))
    }
}
