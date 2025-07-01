use anyhow::Result;
use k8s_openapi::{self, api::apps::v1::Deployment};
use kube::{Api, Client};

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
