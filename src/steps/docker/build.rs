use crate::config::DockerRegistry;
use crate::error::Result;
use crate::integrations::aws::AwsClient;
use crate::integrations::docker::{BuildConfig, DockerClient};
use crate::integrations::git::GitRepo;
use crate::steps::{Step, StepContext, StepOutput};
use async_trait::async_trait;
use semver::Version;

pub struct DockerBuildStep {
    version: Version,
}

impl DockerBuildStep {
    pub fn new(version: Version) -> Self {
        Self { version }
    }

    fn get_image_tags(&self, ctx: &StepContext) -> Vec<String> {
        let version_str = self.version.to_string();
        let git_sha_full = GitRepo::open()
            .ok()
            .and_then(|repo| repo.current_commit_sha().ok())
            .unwrap_or_else(|| "unknown".to_string());
        let git_sha = git_sha_full.chars().take(7).collect::<String>();

        ctx.config
            .docker
            .tags
            .iter()
            .map(|t| {
                t.replace("{version}", &version_str)
                    .replace("{major}", &self.version.major.to_string())
                    .replace("{minor}", &self.version.minor.to_string())
                    .replace("{patch}", &self.version.patch.to_string())
                    .replace("{git_sha}", &git_sha)
                    .replace("{git_sha_full}", &git_sha_full)
            })
            .collect()
    }

    async fn get_full_image_name(&self, ctx: &StepContext) -> Result<String> {
        let repo = &ctx.config.docker.repository;

        match ctx.config.docker.registry {
            DockerRegistry::AwsEcr => {
                let aws = if let Some(ref profile) = ctx.config.aws.profile {
                    AwsClient::with_profile(&ctx.config.aws.region, profile).await?
                } else {
                    AwsClient::new(&ctx.config.aws.region).await?
                };

                let (account_id, _) = aws.get_caller_identity().await?;
                let registry_url = aws.get_ecr_registry_url(&account_id);
                Ok(format!("{}/{}", registry_url, repo))
            }
            DockerRegistry::DockerHub => Ok(repo.clone()),
            DockerRegistry::Ghcr => Ok(format!("ghcr.io/{}", repo)),
            DockerRegistry::Custom => Ok(repo.clone()),
        }
    }

    fn get_full_image_name_dry_run(&self, ctx: &StepContext) -> String {
        let repo = &ctx.config.docker.repository;

        match ctx.config.docker.registry {
            DockerRegistry::AwsEcr => format!(
                "<aws-account-id>.dkr.ecr.{}.amazonaws.com/{}",
                ctx.config.aws.region, repo
            ),
            DockerRegistry::DockerHub => repo.clone(),
            DockerRegistry::Ghcr => format!("ghcr.io/{}", repo),
            DockerRegistry::Custom => repo.clone(),
        }
    }
}

#[async_trait]
impl Step for DockerBuildStep {
    fn name(&self) -> &str {
        "docker-build"
    }

    fn description(&self) -> &str {
        "Build Docker image"
    }

    async fn validate(&self, ctx: &StepContext) -> Result<()> {
        // Check Dockerfile exists
        let dockerfile_path =
            std::path::Path::new(&ctx.config.docker.context).join(&ctx.config.docker.dockerfile);

        if !dockerfile_path.exists() {
            return Err(crate::error::DockerError::BuildFailed(format!(
                "Dockerfile not found: {}",
                dockerfile_path.display()
            ))
            .into());
        }

        // In dry-run mode we don't require a running Docker daemon.
        if ctx.dry_run {
            return Ok(());
        }

        // Check Docker daemon is accessible for real execution.
        let docker = DockerClient::new().await?;
        docker.version().await?;

        Ok(())
    }

    async fn execute(&self, ctx: &StepContext) -> Result<StepOutput> {
        let docker = DockerClient::new().await?;
        let full_image_name = self.get_full_image_name(ctx).await?;

        let tags: Vec<String> = self
            .get_image_tags(ctx)
            .iter()
            .map(|t| format!("{}:{}", full_image_name, t))
            .collect();

        let build_args = ctx.config.docker.build_args.clone().unwrap_or_default();

        let config = BuildConfig {
            dockerfile: ctx.config.docker.dockerfile.clone(),
            context: ctx.config.docker.context.clone(),
            tags: tags.clone(),
            build_args,
        };

        let image_id = docker
            .build_image(&config, |msg| {
                tracing::debug!("{}", msg);
            })
            .await?;

        let tag_list = tags.join(", ");
        Ok(StepOutput::ok(format!(
            "Built image {} with tags: {}",
            &image_id[..12.min(image_id.len())],
            tag_list
        )))
    }

    async fn dry_run(&self, ctx: &StepContext) -> Result<StepOutput> {
        let full_image_name = self.get_full_image_name_dry_run(ctx);
        let tags = self.get_image_tags(ctx);

        // Calculate estimated layers
        let dockerfile_path =
            std::path::Path::new(&ctx.config.docker.context).join(&ctx.config.docker.dockerfile);
        let layers_estimate = if dockerfile_path.exists() {
            std::fs::read_to_string(&dockerfile_path)
                .ok()
                .map(|content| {
                    content
                        .lines()
                        .filter(|l| l.starts_with("FROM") || l.starts_with("RUN"))
                        .count()
                })
        } else {
            None
        };

        let docker_preview = crate::steps::DockerPreview {
            image_name: full_image_name.clone(),
            tags: tags.clone(),
            build_args: ctx
                .config
                .docker
                .build_args
                .clone()
                .unwrap_or_default()
                .into_iter()
                .collect(),
            layers_estimate,
        };

        let notes = vec![
            format!("Dockerfile: {}", dockerfile_path.display()),
            format!("Build context: {}", ctx.config.docker.context),
            format!(
                "Estimated layers: {}",
                layers_estimate
                    .map(|l| l.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            ),
        ];

        let details = crate::steps::DryRunDetails {
            file_changes: vec![],
            docker_preview: Some(docker_preview),
            notes,
        };

        Ok(StepOutput::ok(format!(
            "Would build {} with tags: {}",
            full_image_name,
            tags.join(", ")
        ))
        .with_dry_run_details(details))
    }
}
