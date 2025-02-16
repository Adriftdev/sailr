use anyhow::Result;

use k8s_openapi::api::core::v1::Namespace;
use kube::{Api, Client};

use crate::errors::KubeError;

pub async fn get_all_namespaces(client: Client) -> Result<Vec<Namespace>, KubeError> {
    let api: Api<Namespace> = Api::all(client);

    let namespaces = api.list(&Default::default()).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve resource: {}", e))
    })?;

    Ok(namespaces.items)
}

pub async fn delete_namespace(client: Client, name: &str) -> Result<(), KubeError> {
    let api: Api<Namespace> = Api::all(client);

    api.delete(name, &Default::default()).await.map_err(|e| {
        KubeError::ResourceDeletionFailed(format!("Failed to delete resource: {}", e))
    })?;

    Ok(())
}

pub async fn delete_all_namespaces(client: Client) -> Result<(), KubeError> {
    let api: Api<Namespace> = Api::all(client);

    api.delete_collection(&Default::default(), &Default::default())
        .await
        .map_err(|e| {
            KubeError::ResourceDeletionFailed(format!("Failed to delete resource: {}", e))
        })?;
    Ok(())
}
