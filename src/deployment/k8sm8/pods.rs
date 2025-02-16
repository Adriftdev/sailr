use anyhow::Result;

use k8s_openapi::api::core::v1::Pod;

use kube::{Api, Client};

use crate::errors::KubeError;

pub async fn delete_pod(client: Client, namespace: &str, name: &str) -> Result<(), KubeError> {
    let api: Api<Pod> = Api::namespaced(client, namespace);

    api.delete(name, &Default::default()).await.map_err(|e| {
        KubeError::ResourceDeletionFailed(format!("Failed to delete resource: {}", e))
    })?;
    Ok(())
}

pub async fn delete_all_pods(client: Client, namespace: &str) -> Result<(), KubeError> {
    let api: Api<Pod> = Api::namespaced(client, namespace);

    api.delete_collection(&Default::default(), &Default::default())
        .await
        .map_err(|e| {
            KubeError::ResourceDeletionFailed(format!("Failed to delete resource: {}", e))
        })?;
    Ok(())
}

pub async fn get_pod(client: Client, namespace: &str, name: &str) -> Result<Pod, KubeError> {
    let api: Api<Pod> = Api::namespaced(client, namespace);

    let pod = api.get(name).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve resource: {}", e))
    })?;

    Ok(pod)
}

pub async fn get_all_pods(client: Client, namespace: &str) -> Result<Vec<Pod>, KubeError> {
    let api: Api<Pod> = Api::namespaced(client, namespace);

    let pods = api.list(&Default::default()).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve resource: {}", e))
    })?;

    Ok(pods.items)
}
