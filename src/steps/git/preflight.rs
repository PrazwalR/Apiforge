use crate::error::{GitError, Result};
use crate::integrations::git::GitRepo;
use crate::steps::{Step, StepContext, StepOutput};
use async_trait::async_trait;

pub struct GitPreflightStep;

impl GitPreflightStep {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Step for GitPreflightStep {
    fn name(&self) -> &str {
        "git-preflight"
    }

    fn description(&self) -> &str {
        "Validate Git repository state"
    }

    async fn validate(&self, ctx: &StepContext) -> Result<()> {
        let repo = GitRepo::open()?;
        
        if ctx.config.git.require_clean && !repo.is_working_tree_clean()? {
            let changes = repo.get_uncommitted_changes()?;
            return Err(GitError::DirtyWorkingTree(format!(
                "{} uncommitted file(s): {}",
                changes.len(),
                changes.join(", ")
            ))
            .into());
        }

        if ctx.config.git.require_main_branch {
            let current = repo.current_branch()?;
            if current != ctx.config.git.main_branch {
                return Err(GitError::WrongBranch {
                    current,
                    required: ctx.config.git.main_branch.clone(),
                }
                .into());
            }
        }

        if !repo.remote_exists(&ctx.config.git.remote) {
            return Err(GitError::RemoteNotFound(ctx.config.git.remote.clone()).into());
        }

        let current_branch = repo.current_branch()?;
        let (ahead, behind) = repo.check_remote_sync(&current_branch, &ctx.config.git.remote)?;

        if behind > 0 {
            return Err(GitError::BehindRemote(behind).into());
        }

        if ahead > 0 {
            return Err(GitError::AheadOfRemote(ahead).into());
        }

        Ok(())
    }

    async fn execute(&self, _ctx: &StepContext) -> Result<StepOutput> {
        Ok(StepOutput::ok("Repository state validated"))
    }

    async fn dry_run(&self, _ctx: &StepContext) -> Result<StepOutput> {
        Ok(StepOutput::ok("Would validate repository state"))
    }
}
