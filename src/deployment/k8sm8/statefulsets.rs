use anyhow::Result;

use k8s_openapi::api::apps::v1::StatefulSet;
use kube::{Api, Client};

use crate::errors::KubeError;

pub async fn get_statefulset(
    client: Client,
    namespace: &str,
    name: &str,
) -> Result<StatefulSet, KubeError> {
    let api: Api<StatefulSet> = Api::namespaced(client, namespace);

    let statefulset = api.get(name).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve resource: {}", e))
    })?;

    Ok(statefulset)
}

pub async fn get_all_statefulsets(
    client: Client,
    namespace: &str,
) -> Result<Vec<StatefulSet>, KubeError> {
    let api: Api<StatefulSet> = Api::namespaced(client, namespace);

    let statefulsets = api.list(&Default::default()).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve resource: {}", e))
    })?;

    Ok(statefulsets.items)
}

pub async fn delete_statefulset(
    client: Client,
    namespace: &str,
    name: &str,
) -> Result<(), KubeError> {
    let api: Api<StatefulSet> = Api::namespaced(client, namespace);

    api.delete(name, &Default::default()).await.map_err(|e| {
        KubeError::ResourceDeletionFailed(format!("Failed to delete resource: {}", e))
    })?;

    Ok(())
}

pub async fn delete_all_statefulsets(client: Client, namespace: &str) -> Result<(), KubeError> {
    let api: Api<StatefulSet> = Api::namespaced(client, namespace);

    api.delete_collection(&Default::default(), &Default::default())
        .await
        .map_err(|e| {
            KubeError::ResourceDeletionFailed(format!("Failed to delete resource: {}", e))
        })?;
    Ok(())
}
