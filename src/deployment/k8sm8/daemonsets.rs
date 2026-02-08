use anyhow::Result;

use k8s_openapi::api::apps::v1::DaemonSet;
use kube::{Api, Client};
use chrono;

use crate::errors::KubeError;

pub async fn get_daemonset(
    client: Client,
    namespace: &str,
    name: &str,
) -> Result<DaemonSet, KubeError> {
    let api: Api<DaemonSet> = Api::namespaced(client, namespace);

    let daemonset = api.get(name).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve resource: {}", e))
    })?;

    Ok(daemonset)
}

pub async fn get_all_daemonsets(
    client: Client,
    namespace: &str,
) -> Result<Vec<DaemonSet>, KubeError> {
    let api: Api<DaemonSet> = Api::namespaced(client, namespace);

    let daemonsets = api.list(&Default::default()).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve resource: {}", e))
    })?;

    Ok(daemonsets.items)
}

pub async fn delete_daemonset(
    client: Client,
    namespace: &str,
    name: &str,
) -> Result<(), KubeError> {
    let api: Api<DaemonSet> = Api::namespaced(client, namespace);

    api.delete(name, &Default::default()).await.map_err(|e| {
        KubeError::ResourceDeletionFailed(format!("Failed to delete resource: {}", e))
    })?;

    Ok(())
}

pub async fn delete_all_daemonsets(client: Client, namespace: &str) -> Result<(), KubeError> {
    let api: Api<DaemonSet> = Api::namespaced(client, namespace);

    api.delete_collection(&Default::default(), &Default::default())
        .await
        .map_err(|e| {
            KubeError::ResourceDeletionFailed(format!("Failed to delete resource: {}", e))
        })?;
    Ok(())
}

pub async fn restart_daemonset(
    client: Client,
    namespace: &str,
    name: &str,
) -> Result<(), KubeError> {
    let daemonsets: Api<DaemonSet> = Api::namespaced(client, namespace);

    // Fetch the daemonset
    let mut daemonset = daemonsets.get(name).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve resource: {}", e))
    })?;

    // Update the pod template annotations to trigger a rollout
    if let Some(spec) = &mut daemonset.spec {
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
    let patch = kube::api::Patch::Merge(&daemonset);
    daemonsets
        .patch(name, &kube::api::PatchParams::default(), &patch)
        .await
        .map_err(|e| {
            KubeError::ResourceUpdateFailed(format!("Failed to patch resource: {}", e))
        })?;

    Ok(())
}
