use anyhow::Result;

use k8s_openapi::api::core::v1::ConfigMap;
use kube::{Api, Client};

use crate::errors::KubeError;

pub async fn get_configmap(
    client: Client,
    namespace: &str,
    name: &str,
) -> Result<ConfigMap, KubeError> {
    let api: Api<ConfigMap> = Api::namespaced(client, namespace);

    let configmap = api.get(name).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve resource: {}", e))
    })?;

    Ok(configmap)
}

pub async fn get_all_configmaps(
    client: Client,
    namespace: &str,
) -> Result<Vec<ConfigMap>, KubeError> {
    let api: Api<ConfigMap> = Api::namespaced(client, namespace);

    let configmaps = api.list(&Default::default()).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve resource: {}", e))
    })?;

    Ok(configmaps.items)
}

pub async fn delete_configmap(
    client: Client,
    namespace: &str,
    name: &str,
) -> Result<(), KubeError> {
    let api: Api<ConfigMap> = Api::namespaced(client, namespace);

    api.delete(name, &Default::default()).await.map_err(|e| {
        KubeError::ResourceDeletionFailed(format!("Failed to delete resource: {}", e))
    })?;

    Ok(())
}

pub async fn delete_all_configmaps(client: Client, namespace: &str) -> Result<(), KubeError> {
    let api: Api<ConfigMap> = Api::namespaced(client, namespace);

    api.delete_collection(&Default::default(), &Default::default())
        .await
        .map_err(|e| {
            KubeError::ResourceDeletionFailed(format!("Failed to delete resource: {}", e))
        })?;
    Ok(())
}
