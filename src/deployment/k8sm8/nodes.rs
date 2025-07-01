use anyhow::Result;

use k8s_openapi::api::core::v1::Node;

use kube::{Api, Client};

use crate::errors::KubeError;

pub async fn get_node(client: Client, name: &str) -> Result<Node, KubeError> {
    let api: Api<Node> = Api::all(client);

    let node = api.get(name).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve resource: {}", e))
    })?;

    Ok(node)
}

pub async fn get_all_nodes(client: Client) -> Result<Vec<Node>, KubeError> {
    let api: Api<Node> = Api::all(client);

    let nodes = api.list(&Default::default()).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve resource: {}", e))
    })?;

    Ok(nodes.items)
}

pub async fn delete_node(client: Client, name: &str) -> Result<(), KubeError> {
    let api: Api<Node> = Api::all(client);

    api.delete(name, &Default::default()).await.map_err(|e| {
        KubeError::ResourceDeletionFailed(format!("Failed to delete resource: {}", e))
    })?;

    Ok(())
}
