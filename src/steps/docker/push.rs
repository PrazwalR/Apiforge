use crate::config::DockerRegistry;
use crate::error::Result;
use crate::integrations::aws::AwsClient;
use crate::integrations::docker::{DockerClient, PushConfig};
use crate::steps::{Step, StepContext, StepOutput};
use async_trait::async_trait;
use bollard::auth::DockerCredentials;
use semver::Version;

pub struct DockerPushStep {
    version: Version,
}

impl DockerPushStep {
    pub fn new(version: Version) -> Self {
        Self { version }
    }

    fn get_image_tags(&self, ctx: &StepContext) -> Vec<String> {
        let version_str = self.version.to_string();

        ctx.config
            .docker
            .tags
            .iter()
            .map(|t| {
                t.replace("{version}", &version_str)
                    .replace("{major}", &self.version.major.to_string())
                    .replace("{minor}", &self.version.minor.to_string())
            })
            .collect()
    }

    async fn get_registry_info(&self, ctx: &StepContext) -> Result<(String, Option<DockerCredentials>)> {
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
                let credentials = aws.get_ecr_authorization().await?;

                Ok((format!("{}/{}", registry_url, repo), Some(credentials)))
            }
            DockerRegistry::DockerHub => Ok((repo.clone(), None)),
            DockerRegistry::Ghcr => Ok((format!("ghcr.io/{}", repo), None)),
            DockerRegistry::Custom => Ok((repo.clone(), None)),
        }
    }
}

#[async_trait]
impl Step for DockerPushStep {
    fn name(&self) -> &str {
        "docker-push"
    }

    fn description(&self) -> &str {
        "Push Docker image to registry"
    }

    async fn validate(&self, ctx: &StepContext) -> Result<()> {
        let _docker = DockerClient::new().await?;

        // For ECR, verify we can get auth token
        if matches!(ctx.config.docker.registry, DockerRegistry::AwsEcr) {
            let aws = if let Some(ref profile) = ctx.config.aws.profile {
                AwsClient::with_profile(&ctx.config.aws.region, profile).await?
            } else {
                AwsClient::new(&ctx.config.aws.region).await?
            };

            // Verify credentials work
            aws.get_caller_identity().await?;
        }

        Ok(())
    }

    async fn execute(&self, ctx: &StepContext) -> Result<StepOutput> {
        let docker = DockerClient::new().await?;
        let (full_image_name, credentials) = self.get_registry_info(ctx).await?;
        let tags = self.get_image_tags(ctx);

        let mut pushed_tags = Vec::new();

        for tag in &tags {
            let config = PushConfig {
                image: full_image_name.clone(),
                tag: tag.clone(),
                registry: None,
                credentials: credentials.clone(),
            };

            docker
                .push_image(&config, |msg| {
                    tracing::debug!("{}", msg);
                })
                .await?;

            pushed_tags.push(tag.clone());
        }

        Ok(StepOutput::ok(format!(
            "Pushed {} tags to {}",
            pushed_tags.len(),
            match ctx.config.docker.registry {
                DockerRegistry::AwsEcr => "AWS ECR",
                DockerRegistry::DockerHub => "Docker Hub",
                DockerRegistry::Ghcr => "GitHub Container Registry",
                DockerRegistry::Custom => "custom registry",
            }
        )))
    }

    async fn dry_run(&self, ctx: &StepContext) -> Result<StepOutput> {
        let (full_image_name, _) = self.get_registry_info(ctx).await?;
        let tags = self.get_image_tags(ctx);

        Ok(StepOutput::ok(format!(
            "Would push {} with tags {} to {:?}",
            full_image_name,
            tags.join(", "),
            ctx.config.docker.registry
        )))
    }
}
