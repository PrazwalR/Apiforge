use crate::error::{K8sError, Result};
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::{Namespace, Pod};
use kube::api::{Api, ListParams, Patch, PatchParams};
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Client, Config};
use std::time::Duration;
use tokio::time::sleep;

pub struct K8sClient {
    client: Client,
    context: String,
}

#[derive(Debug, Clone)]
pub struct RolloutStatus {
    pub ready: bool,
    pub ready_replicas: i32,
    pub desired_replicas: i32,
    pub updated_replicas: i32,
    pub available_replicas: i32,
    pub message: String,
}

impl K8sClient {
    pub async fn new(context: &str) -> Result<Self> {
        let kubeconfig = Kubeconfig::read().map_err(|e| {
            K8sError::KubeconfigInvalid
        })?;

        let config = Config::from_custom_kubeconfig(
            kubeconfig,
            &KubeConfigOptions {
                context: Some(context.to_string()),
                ..Default::default()
            },
        )
        .await
        .map_err(|e| K8sError::ContextNotFound(context.to_string()))?;

        let client = Client::try_from(config)
            .map_err(|e| K8sError::ClusterUnreachable(e.to_string()))?;

        Ok(Self {
            client,
            context: context.to_string(),
        })
    }

    pub async fn new_in_cluster() -> Result<Self> {
        let client = Client::try_default()
            .await
            .map_err(|e| K8sError::ClusterUnreachable(e.to_string()))?;

        Ok(Self {
            client,
            context: "in-cluster".to_string(),
        })
    }

    pub async fn verify_connection(&self) -> Result<()> {
        let _: Api<Namespace> = Api::all(self.client.clone());
        Ok(())
    }

    pub async fn namespace_exists(&self, namespace: &str) -> Result<bool> {
        let namespaces: Api<Namespace> = Api::all(self.client.clone());
        match namespaces.get(namespace).await {
            Ok(_) => Ok(true),
            Err(kube::Error::Api(err)) if err.code == 404 => Ok(false),
            Err(e) => Err(K8sError::KubeApi(e.to_string()).into()),
        }
    }

    pub async fn get_deployment(&self, namespace: &str, name: &str) -> Result<Deployment> {
        let deployments: Api<Deployment> = Api::namespaced(self.client.clone(), namespace);
        deployments
            .get(name)
            .await
            .map_err(|e| match e {
                kube::Error::Api(err) if err.code == 404 => {
                    K8sError::DeploymentNotFound(name.to_string(), namespace.to_string()).into()
                }
                _ => K8sError::KubeApi(e.to_string()).into(),
            })
    }

    pub async fn update_deployment_image(
        &self,
        namespace: &str,
        deployment_name: &str,
        container_index: usize,
        new_image: &str,
    ) -> Result<()> {
        let deployments: Api<Deployment> = Api::namespaced(self.client.clone(), namespace);

        let patch = serde_json::json!({
            "spec": {
                "template": {
                    "spec": {
                        "containers": [{
                            "name": self.get_container_name(namespace, deployment_name, container_index).await?,
                            "image": new_image
                        }]
                    }
                }
            }
        });

        let patch_params = PatchParams::apply("apiforge");
        deployments
            .patch(deployment_name, &patch_params, &Patch::Strategic(patch))
            .await
            .map_err(|e| K8sError::KubeApi(format!("Failed to patch deployment: {}", e)))?;

        Ok(())
    }

    async fn get_container_name(
        &self,
        namespace: &str,
        deployment_name: &str,
        container_index: usize,
    ) -> Result<String> {
        let deployment = self.get_deployment(namespace, deployment_name).await?;
        let containers = deployment
            .spec
            .as_ref()
            .and_then(|s| s.template.spec.as_ref())
            .map(|s| &s.containers)
            .ok_or_else(|| K8sError::ManifestError("No containers in deployment".to_string()))?;

        containers
            .get(container_index)
            .map(|c| c.name.clone())
            .ok_or_else(|| K8sError::ManifestError(format!("Container index {} not found", container_index)).into())
    }

    pub async fn get_rollout_status(&self, namespace: &str, deployment_name: &str) -> Result<RolloutStatus> {
        let deployment = self.get_deployment(namespace, deployment_name).await?;

        let spec_replicas = deployment
            .spec
            .as_ref()
            .and_then(|s| s.replicas)
            .unwrap_or(1);

        let status = deployment.status.as_ref();

        let ready_replicas = status.and_then(|s| s.ready_replicas).unwrap_or(0);
        let updated_replicas = status.and_then(|s| s.updated_replicas).unwrap_or(0);
        let available_replicas = status.and_then(|s| s.available_replicas).unwrap_or(0);

        let ready = ready_replicas >= spec_replicas 
            && updated_replicas >= spec_replicas 
            && available_replicas >= spec_replicas;

        let message = if ready {
            format!(
                "Deployment {} successfully rolled out ({}/{} replicas ready)",
                deployment_name, ready_replicas, spec_replicas
            )
        } else {
            format!(
                "Rolling out: {}/{} ready, {}/{} updated",
                ready_replicas, spec_replicas, updated_replicas, spec_replicas
            )
        };

        Ok(RolloutStatus {
            ready,
            ready_replicas,
            desired_replicas: spec_replicas,
            updated_replicas,
            available_replicas,
            message,
        })
    }

    pub async fn wait_for_rollout<F>(
        &self,
        namespace: &str,
        deployment_name: &str,
        timeout_seconds: u64,
        on_progress: F,
    ) -> Result<RolloutStatus>
    where
        F: Fn(&RolloutStatus),
    {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(timeout_seconds);
        let poll_interval = Duration::from_secs(2);

        loop {
            let status = self.get_rollout_status(namespace, deployment_name).await?;
            on_progress(&status);

            if status.ready {
                return Ok(status);
            }

            if start.elapsed() >= timeout {
                return Err(K8sError::RolloutTimeout(timeout_seconds).into());
            }

            sleep(poll_interval).await;
        }
    }

    pub async fn get_pods_for_deployment(
        &self,
        namespace: &str,
        deployment_name: &str,
    ) -> Result<Vec<Pod>> {
        let deployment = self.get_deployment(namespace, deployment_name).await?;

        let match_labels = deployment
            .spec
            .as_ref()
            .and_then(|s| s.selector.match_labels.clone())
            .unwrap_or_default();

        let label_selector = match_labels
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join(",");

        let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace);
        let list_params = ListParams::default().labels(&label_selector);

        let pod_list = pods
            .list(&list_params)
            .await
            .map_err(|e| K8sError::KubeApi(e.to_string()))?;

        Ok(pod_list.items)
    }

    pub async fn restart_deployment(&self, namespace: &str, deployment_name: &str) -> Result<()> {
        let deployments: Api<Deployment> = Api::namespaced(self.client.clone(), namespace);

        let patch = serde_json::json!({
            "spec": {
                "template": {
                    "metadata": {
                        "annotations": {
                            "kubectl.kubernetes.io/restartedAt": chrono::Utc::now().to_rfc3339()
                        }
                    }
                }
            }
        });

        let patch_params = PatchParams::apply("apiforge");
        deployments
            .patch(deployment_name, &patch_params, &Patch::Strategic(patch))
            .await
            .map_err(|e| K8sError::KubeApi(format!("Failed to restart deployment: {}", e)))?;

        Ok(())
    }

    pub async fn scale_deployment(
        &self,
        namespace: &str,
        deployment_name: &str,
        replicas: i32,
    ) -> Result<()> {
        let deployments: Api<Deployment> = Api::namespaced(self.client.clone(), namespace);

        let patch = serde_json::json!({
            "spec": {
                "replicas": replicas
            }
        });

        let patch_params = PatchParams::apply("apiforge");
        deployments
            .patch(deployment_name, &patch_params, &Patch::Strategic(patch))
            .await
            .map_err(|e| K8sError::KubeApi(format!("Failed to scale deployment: {}", e)))?;

        Ok(())
    }

    pub fn context(&self) -> &str {
        &self.context
    }
}
