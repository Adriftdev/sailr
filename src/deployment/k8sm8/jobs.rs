use anyhow::Result;

use k8s_openapi::api::batch::v1::Job;
use kube::{Api, Client};

use crate::errors::KubeError;

pub async fn get_job(client: Client, namespace: &str, name: &str) -> Result<Job, KubeError> {
    let api: Api<Job> = Api::namespaced(client, namespace);

    let job = api.get(name).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve resource: {}", e))
    })?;

    Ok(job)
}

pub async fn get_all_jobs(client: Client, namespace: &str) -> Result<Vec<Job>, KubeError> {
    let api: Api<Job> = Api::namespaced(client, namespace);

    let jobs = api.list(&Default::default()).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve resource: {}", e))
    })?;

    Ok(jobs.items)
}

pub async fn delete_job(client: Client, namespace: &str, name: &str) -> Result<(), KubeError> {
    let api: Api<Job> = Api::namespaced(client, namespace);

    api.delete(name, &Default::default()).await.map_err(|e| {
        KubeError::ResourceDeletionFailed(format!("Failed to delete resource: {}", e))
    })?;

    Ok(())
}

pub async fn delete_all_jobs(client: Client, namespace: &str) -> Result<(), KubeError> {
    let api: Api<Job> = Api::namespaced(client, namespace);

    api.delete_collection(&Default::default(), &Default::default())
        .await
        .map_err(|e| {
            KubeError::ResourceDeletionFailed(format!("Failed to delete resource: {}", e))
        })?;
    Ok(())
}
