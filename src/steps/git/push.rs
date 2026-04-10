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
            return Err(GitError::RemoteNotFound(ctx.config.git.remote.clone()).into());
        }
        Ok(())
    }

    async fn execute(&self, ctx: &StepContext) -> Result<StepOutput> {
        let repo = GitRepo::open()?;
        let remote = ctx.config.git.remote.clone();
        let branch = repo.current_branch()?;

        // Create timeout config from git config
        let timeout_config = crate::integrations::git::GitTimeoutConfig::from_config(
            ctx.config.git.fetch_timeout_secs,
            ctx.config.git.push_timeout_secs,
            ctx.config.git.operation_timeout_secs,
        );

        // Store the commit sha before pushing (for potential rollback)
        let commit_sha = repo.current_commit_sha()?;
        tracing::debug!("GitPushStep: Current commit before push: {}", commit_sha);

        // Clone repo root path for use in async block
        let repo_path = repo.root_path();
        let tag_name = self.get_tag_name(ctx);

        // Push branch with timeout - using free function to avoid borrowing repo across await
        let repo_path1 = repo_path.clone();
        let remote1 = remote.clone();
        let branch1 = branch.clone();
        crate::integrations::git::push_with_timeout(
            move || {
                let repo = GitRepo::open_at(&repo_path1)?;
                repo.push(&remote1, &format!("refs/heads/{}", branch1))?;
                Ok(())
            },
            &timeout_config,
        )
        .await?;

        // Push tag with timeout
        let repo_path2 = repo_path.clone();
        let remote2 = remote.clone();
        crate::integrations::git::push_with_timeout(
            move || {
                let repo = GitRepo::open_at(&repo_path2)?;
                repo.push(&remote2, &format!("refs/tags/{}", tag_name))?;
                Ok(())
            },
            &timeout_config,
        )
        .await?;

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
        let remote = ctx.config.git.remote.clone();
        let tag_name = self.get_tag_name(ctx);

        // Create timeout config from git config
        let timeout_config = crate::integrations::git::GitTimeoutConfig::from_config(
            ctx.config.git.fetch_timeout_secs,
            ctx.config.git.push_timeout_secs,
            ctx.config.git.operation_timeout_secs,
        );

        tracing::info!("Rolling back git push: deleting remote tag {}", tag_name);

        // Clone repo root path for use in async block
        let repo_path = repo.root_path();

        // Delete the remote tag first with timeout (this is the most important part)
        let repo_path2 = repo_path.clone();
        let remote2 = remote.clone();
        let tag_name2 = tag_name.clone();
        match crate::integrations::git::push_with_timeout(
            move || {
                let repo = GitRepo::open_at(&repo_path2)?;
                repo.delete_remote_tag(&remote2, &tag_name2)?;
                Ok(())
            },
            &timeout_config,
        )
        .await
        {
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

        // ROLLBACK STRATEGY DOCUMENTATION:
        // ================================
        // We intentionally do NOT try to revert or delete the pushed commit.
        //
        // Why this is the correct approach:
        // ---------------------------------
        // 1. SAFETY: Once a commit is pushed to a shared remote, force-deleting it
        //    (git push --force) is dangerous and can cause significant issues:
        //    - Other developers may have already fetched/pulled the commit
        //    - CI/CD pipelines may have already processed it
        //    - It violates the principle of immutable history in shared repos
        //
        // 2. HARMLESSNESS: The version bump commit is essentially benign:
        //    - It only changes version numbers in manifest files
        //    - It doesn't affect runtime behavior
        //    - It can coexist with subsequent releases
        //
        // 3. TAG DELETION IS SUFFICIENT:
        //    - Without a tag, the commit isn't marked as a release
        //    - No GitHub release will reference it
        //    - No Docker images will be pushed with that version
        //    - No Kubernetes deployments will use that version
        //
        // 4. RECOVERY PATH:
        //    - The next successful release will create a new commit with the
        //      correct version and a proper tag
        //    - The orphaned commit becomes just another commit in history
        //
        // Alternative considered but rejected:
        // ------------------------------------
        // Creating a revert commit (git revert) was considered but rejected because:
        // - It adds noise to the commit history
        // - It requires another push, which could also fail
        // - The version file would flip-flop between versions
        // - It complicates the git history without real benefit

        tracing::info!(
            "Git rollback complete. Tag deleted, commit preserved at remote. \
             See code comments in src/steps/git/push.rs for rationale."
        );

        Ok(())
    }
}
