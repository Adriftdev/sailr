use anyhow::Result;

use k8s_openapi::api::core::v1::Secret;
use kube::{Api, Client};

use crate::errors::KubeError;

pub async fn create_secret(
    client: Client,
    namespace: &str,
    secret: Secret,
) -> Result<Secret, KubeError> {
    let api: Api<Secret> = Api::namespaced(client, namespace);

    let secret = api
        .create(&Default::default(), &secret)
        .await
        .map_err(|e| KubeError::UnexpectedError(format!("Failed to create resource: {}", e)))?;

    Ok(secret)
}

pub async fn get_secret(client: Client, namespace: &str, name: &str) -> Result<Secret, KubeError> {
    let api: Api<Secret> = Api::namespaced(client, namespace);

    let secret = api.get(name).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve resource: {}", e))
    })?;

    Ok(secret)
}

pub async fn get_all_secrets(client: Client, namespace: &str) -> Result<Vec<Secret>, KubeError> {
    let api: Api<Secret> = Api::namespaced(client, namespace);

    let secrets = api.list(&Default::default()).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve resource: {}", e))
    })?;

    Ok(secrets.items)
}

pub async fn delete_secret(client: Client, namespace: &str, name: &str) -> Result<(), KubeError> {
    let api: Api<Secret> = Api::namespaced(client, namespace);
    api.delete(name, &Default::default()).await.map_err(|e| {
        KubeError::ResourceDeletionFailed(format!("Failed to delete resource: {}", e))
    })?;

    Ok(())
}

pub async fn delete_all_secrets(client: Client, namespace: &str) -> Result<(), KubeError> {
    let api: Api<Secret> = Api::namespaced(client, namespace);

    api.delete_collection(&Default::default(), &Default::default())
        .await
        .map_err(|e| {
            KubeError::ResourceDeletionFailed(format!("Failed to delete resource: {}", e))
        })?;
    Ok(())
}
