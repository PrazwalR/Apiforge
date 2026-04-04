use crate::error::{GitError, Result};
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

    fn get_tag_name(&self, ctx: &StepContext) -> String {
        format_version(&self.version, &ctx.config.git.tag_format)
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
            return Err(
                GitError::RemoteNotFound(ctx.config.git.remote.clone()).into(),
            );
        }
        Ok(())
    }

    async fn execute(&self, ctx: &StepContext) -> Result<StepOutput> {
        let repo = GitRepo::open()?;
        let remote = &ctx.config.git.remote;
        let branch = repo.current_branch()?;

        // Store the commit sha before pushing (for potential rollback)
        let commit_sha = repo.current_commit_sha()?;
        tracing::debug!("GitPushStep: Current commit before push: {}", commit_sha);

        repo.push(remote, &format!("refs/heads/{}", branch))?;

        let tag_name = self.get_tag_name(ctx);
        repo.push(remote, &format!("refs/tags/{}", tag_name))?;

        Ok(StepOutput::ok(format!(
            "Pushed to {} (branch + tag)",
            remote
        )))
    }

    async fn dry_run(&self, ctx: &StepContext) -> Result<StepOutput> {
        let repo = GitRepo::open()?;
        let branch = repo.current_branch()?;
        let tag_name = self.get_tag_name(ctx);

        Ok(StepOutput::ok(format!(
            "Would push branch '{}' and tag '{}' to '{}'",
            branch, tag_name, ctx.config.git.remote
        )))
    }

    async fn rollback(&self, ctx: &StepContext) -> Result<()> {
        let repo = GitRepo::open()?;
        let remote = &ctx.config.git.remote;
        let tag_name = self.get_tag_name(ctx);

        tracing::info!("Rolling back git push: deleting remote tag {}", tag_name);

        // Delete the remote tag first (this is the most important part)
        match repo.delete_remote_tag(remote, &tag_name) {
            Ok(()) => {
                tracing::info!("Successfully deleted remote tag {}", tag_name);
            }
            Err(e) => {
                tracing::warn!("Failed to delete remote tag {}: {}", tag_name, e);
                // Continue anyway - the tag might not exist or already be deleted
            }
        }

        // Also delete local tag if it exists
        if let Ok(true) = repo.tag_exists(&tag_name) {
            if let Err(e) = repo.delete_tag(&tag_name) {
                tracing::warn!("Failed to delete local tag {}: {}", tag_name, e);
            }
        }

        // Note: We intentionally do NOT try to revert or delete the pushed commit.
        // Once a commit is pushed to a shared remote, deleting it is dangerous
        // and can cause issues for other developers. Instead:
        // 1. The version bump commit remains (it's harmless)
        // 2. The tag is deleted, so the release isn't marked
        // 3. A subsequent successful release will create a new tag on a new commit

        tracing::info!(
            "Git rollback complete. Tag deleted, commit preserved at remote."
        );

        Ok(())
    }
}
