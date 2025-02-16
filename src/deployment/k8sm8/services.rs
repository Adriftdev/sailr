use anyhow::Result;

use k8s_openapi::api::core::v1::Service;
use kube::{Api, Client};

use crate::errors::KubeError;

pub async fn get_all_services(client: Client, namespace: &str) -> Result<Vec<Service>, KubeError> {
    let api: Api<Service> = Api::namespaced(client, namespace);

    let services = api.list(&Default::default()).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve resource: {}", e))
    })?;

    Ok(services.items)
}

pub async fn get_service(
    client: Client,
    namespace: &str,
    name: &str,
) -> Result<Service, KubeError> {
    let api: Api<Service> = Api::namespaced(client, namespace);

    let service = api.get(name).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve resource: {}", e))
    })?;

    Ok(service)
}

pub async fn delete_service(client: Client, namespace: &str, name: &str) -> Result<(), KubeError> {
    let api: Api<Service> = Api::namespaced(client, namespace);

    api.delete(name, &Default::default()).await.map_err(|e| {
        KubeError::ResourceDeletionFailed(format!("Failed to delete resource: {}", e))
    })?;
    Ok(())
}

pub async fn delete_all_services(client: Client, namespace: &str) -> Result<(), KubeError> {
    let api: Api<Service> = Api::namespaced(client, namespace);

    api.delete_collection(&Default::default(), &Default::default())
        .await
        .map_err(|e| {
            KubeError::ResourceDeletionFailed(format!("Failed to delete resource: {}", e))
        })?;
    Ok(())
}
