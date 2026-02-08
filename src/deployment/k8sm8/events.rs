use anyhow::Result;

use k8s_openapi::api::core::v1::Event;

use kube::{Api, Client};

use crate::errors::KubeError;

pub async fn get_event(client: Client, namespace: &str, name: &str) -> Result<Event, KubeError> {
    let api: Api<Event> = Api::namespaced(client, namespace);

    let event = api.get(name).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve resource: {}", e))
    })?;

    Ok(event)
}

pub async fn get_all_events(client: Client, namespace: &str) -> Result<Vec<Event>, KubeError> {
    let api: Api<Event> = Api::namespaced(client, namespace);

    let events = api.list(&Default::default()).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve resource: {}", e))
    })?;

    let mut items = events.items;
    
    // Sort by last_timestamp, falling back to event_time, then creation_timestamp
    // Ascending order (oldest -> newest) so newest are at the bottom
    items.sort_by(|a, b| {
        let get_time = |event: &Event| {
            event.last_timestamp.as_ref().map(|t| t.0)
                .or_else(|| event.event_time.as_ref().map(|t| t.0))
                .or_else(|| event.metadata.creation_timestamp.as_ref().map(|t| t.0))
        };
        
        get_time(a).cmp(&get_time(b))
    });

    Ok(items)
}
