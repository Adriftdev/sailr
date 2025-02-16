use anyhow::Result;

use k8s_openapi::api::batch::v1::CronJob;
use kube::{Api, Client};

use crate::errors::KubeError;

pub async fn get_all_cronjobs(client: Client, namespace: &str) -> Result<Vec<CronJob>, KubeError> {
    let api: Api<CronJob> = Api::namespaced(client, namespace);

    let cronjobs = api.list(&Default::default()).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve resource: {}", e))
    })?;

    Ok(cronjobs.items)
}

pub async fn delete_cronjob(client: Client, namespace: &str, name: &str) -> Result<(), KubeError> {
    let api: Api<CronJob> = Api::namespaced(client, namespace);

    api.delete(name, &Default::default()).await.map_err(|e| {
        KubeError::ResourceDeletionFailed(format!("Failed to delete resource: {}", e))
    })?;

    Ok(())
}

pub async fn delete_all_cronjobs(client: Client, namespace: &str) -> Result<(), KubeError> {
    let api: Api<CronJob> = Api::namespaced(client, namespace);

    api.delete_collection(&Default::default(), &Default::default())
        .await
        .map_err(|e| {
            KubeError::ResourceDeletionFailed(format!("Failed to delete resource: {}", e))
        })?;
    Ok(())
}
