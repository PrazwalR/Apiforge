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
        let kubeconfig = Kubeconfig::read().map_err(|_e| {
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
        .map_err(|_e| K8sError::ContextNotFound(context.to_string()))?;

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

    /// Update the image of a specific container in a deployment
    /// `container` can be a container name or index (e.g., "app", "0", "sidecar")
    pub async fn update_deployment_image(
        &self,
        namespace: &str,
        deployment_name: &str,
        container: &str,
        new_image: &str,
    ) -> Result<()> {
        let deployments: Api<Deployment> = Api::namespaced(self.client.clone(), namespace);
        
        // Resolve container name (could be a name or an index)
        let container_name = self.resolve_container_name(namespace, deployment_name, container).await?;

        let patch = serde_json::json!({
            "spec": {
                "template": {
                    "spec": {
                        "containers": [{
                            "name": container_name,
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

    /// Resolve a container identifier to a container name
    /// Accepts either a container name or a numeric index
    async fn resolve_container_name(
        &self,
        namespace: &str,
        deployment_name: &str,
        container: &str,
    ) -> Result<String> {
        let deployment = self.get_deployment(namespace, deployment_name).await?;
        let containers = deployment
            .spec
            .as_ref()
            .and_then(|s| s.template.spec.as_ref())
            .map(|s| &s.containers)
            .ok_or_else(|| K8sError::ManifestError("No containers in deployment".to_string()))?;

        // First try to parse as index
        if let Ok(index) = container.parse::<usize>() {
            return containers
                .get(index)
                .map(|c| c.name.clone())
                .ok_or_else(|| K8sError::ManifestError(format!("Container index {} not found", index)).into());
        }

        // Otherwise treat as container name - verify it exists
        if containers.iter().any(|c| c.name == container) {
            Ok(container.to_string())
        } else {
            Err(K8sError::ManifestError(format!(
                "Container '{}' not found. Available containers: {}",
                container,
                containers.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")
            )).into())
        }
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

    /// Rollback a deployment to a previous revision.
    /// If `revision` is None, rolls back to the previous revision.
    /// If `revision` is Some(n), rolls back to revision n.
    pub async fn rollback_deployment(
        &self,
        namespace: &str,
        deployment_name: &str,
        revision: Option<i64>,
    ) -> Result<()> {
        let deployments: Api<Deployment> = Api::namespaced(self.client.clone(), namespace);
        
        // Get the deployment to verify it exists and get current state
        let deployment = self.get_deployment(namespace, deployment_name).await?;
        
        // Get the ReplicaSets to find the revision to rollback to
        use k8s_openapi::api::apps::v1::ReplicaSet;
        let replicasets: Api<ReplicaSet> = Api::namespaced(self.client.clone(), namespace);
        
        // Get selector labels from deployment
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
        
        let list_params = ListParams::default().labels(&label_selector);
        let rs_list = replicasets
            .list(&list_params)
            .await
            .map_err(|e| K8sError::KubeApi(format!("Failed to list ReplicaSets: {}", e)))?;
        
        // Sort ReplicaSets by revision annotation (descending)
        let mut replica_sets: Vec<_> = rs_list.items.into_iter().collect();
        replica_sets.sort_by(|a, b| {
            let rev_a = a.metadata.annotations.as_ref()
                .and_then(|ann| ann.get("deployment.kubernetes.io/revision"))
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(0);
            let rev_b = b.metadata.annotations.as_ref()
                .and_then(|ann| ann.get("deployment.kubernetes.io/revision"))
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(0);
            rev_b.cmp(&rev_a)  // Descending order
        });
        
        // Find the target revision
        let target_rs = if let Some(target_rev) = revision {
            // Find specific revision
            replica_sets.iter().find(|rs| {
                rs.metadata.annotations.as_ref()
                    .and_then(|ann| ann.get("deployment.kubernetes.io/revision"))
                    .and_then(|v| v.parse::<i64>().ok())
                    == Some(target_rev)
            })
        } else {
            // Get the second most recent revision (previous)
            replica_sets.get(1)
        };
        
        let target_rs = target_rs.ok_or_else(|| {
            K8sError::RolloutFailed("No previous revision found to rollback to".to_string())
        })?;
        
        // Extract the pod template spec from the target ReplicaSet
        let target_template = target_rs
            .spec
            .as_ref()
            .map(|s| s.template.clone())
            .ok_or_else(|| K8sError::RolloutFailed("Target ReplicaSet has no template".to_string()))?;
        
        // Patch the deployment with the previous pod template
        // This mimics `kubectl rollout undo`
        let patch = serde_json::json!({
            "spec": {
                "template": target_template
            }
        });
        
        let patch_params = PatchParams::apply("apiforge-rollback");
        deployments
            .patch(deployment_name, &patch_params, &Patch::Strategic(patch))
            .await
            .map_err(|e| K8sError::KubeApi(format!("Failed to rollback deployment: {}", e)))?;
        
        tracing::info!(
            "Rolled back deployment {} to previous revision",
            deployment_name
        );
        
        Ok(())
    }
    
    /// Get the current revision number of a deployment
    pub async fn get_deployment_revision(&self, namespace: &str, deployment_name: &str) -> Result<Option<i64>> {
        let deployment = self.get_deployment(namespace, deployment_name).await?;
        
        Ok(deployment.metadata.annotations.as_ref()
            .and_then(|ann| ann.get("deployment.kubernetes.io/revision"))
            .and_then(|v| v.parse::<i64>().ok()))
    }
    
    /// List available revisions for a deployment
    pub async fn list_deployment_revisions(
        &self,
        namespace: &str,
        deployment_name: &str,
    ) -> Result<Vec<i64>> {
        use k8s_openapi::api::apps::v1::ReplicaSet;
        
        let deployment = self.get_deployment(namespace, deployment_name).await?;
        let replicasets: Api<ReplicaSet> = Api::namespaced(self.client.clone(), namespace);
        
        // Get selector labels from deployment
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
        
        let list_params = ListParams::default().labels(&label_selector);
        let rs_list = replicasets
            .list(&list_params)
            .await
            .map_err(|e| K8sError::KubeApi(format!("Failed to list ReplicaSets: {}", e)))?;
        
        let mut revisions: Vec<i64> = rs_list.items.iter()
            .filter_map(|rs| {
                rs.metadata.annotations.as_ref()
                    .and_then(|ann| ann.get("deployment.kubernetes.io/revision"))
                    .and_then(|v| v.parse::<i64>().ok())
            })
            .collect();
        
        revisions.sort_by(|a, b| b.cmp(a));  // Descending
        Ok(revisions)
    }
}
