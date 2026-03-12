use crate::error::Result;
use crate::integrations::git::GitRepo;
use crate::steps::{Step, StepContext, StepOutput};
use crate::utils::TemplateEngine;
use async_trait::async_trait;
use std::collections::HashMap;

pub struct GitCommitStep {
    version: String,
}

impl GitCommitStep {
    pub fn new(version: String) -> Self {
        Self { version }
    }
}

#[async_trait]
impl Step for GitCommitStep {
    fn name(&self) -> &str {
        "git-commit"
    }

    fn description(&self) -> &str {
        "Commit version changes"
    }

    async fn validate(&self, _ctx: &StepContext) -> Result<()> {
        GitRepo::open()?;
        Ok(())
    }

    async fn execute(&self, ctx: &StepContext) -> Result<StepOutput> {
        let repo = GitRepo::open()?;
        
        let version_file = ctx.config.project.language.version_file();
        repo.add(std::path::Path::new(version_file))?;

        if ctx.config.git.changelog {
            repo.add(std::path::Path::new("CHANGELOG.md"))?;
        }

        let mut template_ctx = HashMap::new();
        template_ctx.insert("version".to_string(), self.version.clone());
        template_ctx.insert("project".to_string(), ctx.config.project.name.clone());

        let mut engine = TemplateEngine::new();
        let message = engine.render(&ctx.config.git.commit_message, &template_ctx)?;

        let sha = repo.commit(&message)?;

        Ok(StepOutput::ok(format!("Created commit {}", &sha[..8])))
    }

    async fn dry_run(&self, ctx: &StepContext) -> Result<StepOutput> {
        let mut template_ctx = HashMap::new();
        template_ctx.insert("version".to_string(), self.version.clone());
        template_ctx.insert("project".to_string(), ctx.config.project.name.clone());

        let mut engine = TemplateEngine::new();
        let message = engine.render(&ctx.config.git.commit_message, &template_ctx)?;

        Ok(StepOutput::ok(format!("Would commit with message: {}", message)))
    }
}
