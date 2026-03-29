use crate::config::DockerRegistry;
use crate::error::Result;
use crate::integrations::aws::AwsClient;
use crate::integrations::kubernetes::K8sClient;
use crate::steps::{Step, StepContext, StepOutput};
use async_trait::async_trait;
use semver::Version;

pub struct K8sUpdateStep {
    version: Version,
}

impl K8sUpdateStep {
    pub fn new(version: Version) -> Self {
        Self { version }
    }

    async fn get_full_image(&self, ctx: &StepContext) -> Result<String> {
        let repo = &ctx.config.docker.repository;
        let tag = self.version.to_string();

        let image_base = match ctx.config.docker.registry {
            DockerRegistry::AwsEcr => {
                let aws = if let Some(ref profile) = ctx.config.aws.profile {
                    AwsClient::with_profile(&ctx.config.aws.region, profile).await?
                } else {
                    AwsClient::new(&ctx.config.aws.region).await?
                };

                let (account_id, _) = aws.get_caller_identity().await?;
                let registry_url = aws.get_ecr_registry_url(&account_id);
                format!("{}/{}", registry_url, repo)
            }
            DockerRegistry::DockerHub => repo.clone(),
            DockerRegistry::Ghcr => format!("ghcr.io/{}", repo),
            DockerRegistry::Custom => repo.clone(),
        };

        Ok(format!("{}:{}", image_base, tag))
    }
}

#[async_trait]
impl Step for K8sUpdateStep {
    fn name(&self) -> &str {
        "k8s-update"
    }

    fn description(&self) -> &str {
        "Update Kubernetes deployment image"
    }

    async fn validate(&self, ctx: &StepContext) -> Result<()> {
        let k8s = K8sClient::new(&ctx.config.kubernetes.context).await?;

        // Verify namespace exists
        if !k8s.namespace_exists(&ctx.config.kubernetes.namespace).await? {
            return Err(crate::error::K8sError::NamespaceNotFound(
                ctx.config.kubernetes.namespace.clone(),
            )
            .into());
        }

        // Verify deployment exists
        k8s.get_deployment(
            &ctx.config.kubernetes.namespace,
            &ctx.config.kubernetes.deployment,
        )
        .await?;

        Ok(())
    }

    async fn execute(&self, ctx: &StepContext) -> Result<StepOutput> {
        let k8s = K8sClient::new(&ctx.config.kubernetes.context).await?;
        let new_image = self.get_full_image(ctx).await?;

        k8s.update_deployment_image(
            &ctx.config.kubernetes.namespace,
            &ctx.config.kubernetes.deployment,
            0, // First container
            &new_image,
        )
        .await?;

        Ok(StepOutput::ok(format!(
            "Updated deployment {} to {}",
            ctx.config.kubernetes.deployment, new_image
        )))
    }

    async fn dry_run(&self, ctx: &StepContext) -> Result<StepOutput> {
        let new_image = self.get_full_image(ctx).await?;

        Ok(StepOutput::ok(format!(
            "Would update deployment {} in {} to {}",
            ctx.config.kubernetes.deployment,
            ctx.config.kubernetes.namespace,
            new_image
        )))
    }
}
