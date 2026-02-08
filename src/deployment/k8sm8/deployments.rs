use anyhow::Result;
use k8s_openapi::{self, api::apps::v1::Deployment};
use kube::{Api, Client};
use chrono;

use crate::errors::KubeError;

pub async fn get_all_deployments(
    client: Client,
    namespace: &str,
) -> Result<Vec<Deployment>, KubeError> {
    let deployments: Api<Deployment> = Api::namespaced(client, namespace);

    let list = deployments.list(&Default::default()).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve Kubernetes resource: {}", e))
    })?;

    Ok(list.items)
}

pub async fn get_all_deployments_with_config(client: Client) -> Result<Vec<Deployment>, KubeError> {
    let deployments: Api<Deployment> = Api::all(client);
    let list = deployments.list(&Default::default()).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve Kubernetes resource: {}", e))
    })?;

    Ok(list.items)
}

pub async fn delete_deployment(
    client: Client,
    namespace: &str,
    name: &str,
) -> Result<(), KubeError> {
    let api: Api<Deployment> = Api::namespaced(client, namespace);

    api.delete(name, &Default::default()).await.map_err(|e| {
        KubeError::ResourceDeletionFailed(format!("Failed to delete resource: {}", e))
    })?;

    Ok(())
}

pub async fn delete_all_deployments(client: Client, namespace: &str) -> Result<(), KubeError> {
    let api: Api<Deployment> = Api::namespaced(client, namespace);

    api.delete_collection(&Default::default(), &Default::default())
        .await
        .map_err(|e| {
            KubeError::ResourceDeletionFailed(format!("Failed to delete resource: {}", e))
        })?;
    Ok(())
}

pub async fn restart_deployment(
    client: Client,
    namespace: &str,
    name: &str,
) -> Result<(), KubeError> {
    let deployments: Api<Deployment> = Api::namespaced(client, namespace);

    // Fetch the deployment
    let mut deployment = deployments.get(name).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve resource: {}", e))
    })?;

    // Update the pod template annotations to trigger a rollout
    if let Some(spec) = &mut deployment.spec {
        let template = &mut spec.template;
        if let Some(metadata) = &mut template.metadata {
            let mut annotations = metadata.annotations.clone().unwrap_or_default();
            annotations.insert(
                "kubectl.kubernetes.io/restartedAt".to_string(),
                chrono::Utc::now().to_rfc3339(),
            );
            metadata.annotations = Some(annotations);
        }
    }

    // Apply the patch
    let patch = kube::api::Patch::Merge(&deployment);
    deployments
        .patch(name, &kube::api::PatchParams::default(), &patch)
        .await
        .map_err(|e| {
            KubeError::ResourceUpdateFailed(format!("Failed to patch resource: {}", e))
        })?;

    Ok(())
}
