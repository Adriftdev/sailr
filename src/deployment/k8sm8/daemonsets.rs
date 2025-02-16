use anyhow::Result;

use k8s_openapi::api::apps::v1::DaemonSet;
use kube::{Api, Client};

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
