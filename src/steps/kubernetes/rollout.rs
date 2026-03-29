use crate::error::Result;
use crate::integrations::kubernetes::K8sClient;
use crate::steps::{Step, StepContext, StepOutput};
use async_trait::async_trait;

pub struct K8sRolloutStep {
    timeout: Option<u64>,
}

impl K8sRolloutStep {
    pub fn new() -> Self {
        Self { timeout: None }
    }

    pub fn with_timeout(mut self, timeout: u64) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

impl Default for K8sRolloutStep {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Step for K8sRolloutStep {
    fn name(&self) -> &str {
        "k8s-rollout"
    }

    fn description(&self) -> &str {
        "Wait for Kubernetes rollout to complete"
    }

    async fn validate(&self, ctx: &StepContext) -> Result<()> {
        let k8s = K8sClient::new(&ctx.config.kubernetes.context).await?;

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
        let timeout = self.timeout.unwrap_or(ctx.config.kubernetes.rollout_timeout);

        let status = k8s
            .wait_for_rollout(
                &ctx.config.kubernetes.namespace,
                &ctx.config.kubernetes.deployment,
                timeout,
                |status| {
                    tracing::debug!(
                        "Rollout progress: {}/{} ready",
                        status.ready_replicas,
                        status.desired_replicas
                    );
                },
            )
            .await?;

        // Check minimum ready percent
        let ready_percent =
            (status.ready_replicas as f64 / status.desired_replicas as f64 * 100.0) as u8;

        if ready_percent < ctx.config.kubernetes.min_ready_percent {
            return Err(crate::error::K8sError::RolloutFailed(format!(
                "Only {}% of replicas ready, minimum is {}%",
                ready_percent, ctx.config.kubernetes.min_ready_percent
            ))
            .into());
        }

        Ok(StepOutput::ok(format!(
            "Rollout complete: {}/{} replicas ready",
            status.ready_replicas, status.desired_replicas
        )))
    }

    async fn dry_run(&self, ctx: &StepContext) -> Result<StepOutput> {
        let timeout = self.timeout.unwrap_or(ctx.config.kubernetes.rollout_timeout);

        Ok(StepOutput::ok(format!(
            "Would wait for rollout of {} with {}s timeout",
            ctx.config.kubernetes.deployment, timeout
        )))
    }

    async fn rollback(&self, ctx: &StepContext) -> Result<()> {
        let k8s = K8sClient::new(&ctx.config.kubernetes.context).await?;

        // Rollback by restarting the deployment (K8s will use previous revision)
        tracing::info!("Rolling back deployment {}", ctx.config.kubernetes.deployment);

        // In a real implementation, we'd use kubectl rollout undo or similar
        // For now, we'll just log that rollback is needed
        tracing::warn!(
            "Manual rollback may be required for deployment {}",
            ctx.config.kubernetes.deployment
        );

        Ok(())
    }
}
