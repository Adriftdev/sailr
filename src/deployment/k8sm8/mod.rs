pub mod configmaps;
pub mod cronjobs;
pub mod daemonsets;
pub mod deployments;
pub mod events;
pub mod jobs;
pub mod logs;
pub mod namespaces;
pub mod nodes;
pub mod pods;
pub mod processing;
pub mod secrets;
pub mod services;
pub mod statefulsets;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;

use diffy::{create_patch, PatchFormatter};
use k8s_openapi::serde_json;
use k8s_openapi::{self};
use kube::api::{ListParams, Patch, PatchParams};
use kube::core::{DynamicObject, GroupVersionKind};
use kube::discovery::{ApiCapabilities, ApiResource, Scope};
use kube::{Api, Client, Discovery};

use serde::Deserialize;
use serde_json::Value;

use crate::errors::KubeError;
use crate::LOGGER;
pub use deployments::delete_all_deployments;
pub use deployments::delete_deployment;
pub use deployments::get_all_deployments;
use kube::config::KubeConfigOptions;

pub async fn create_client(context: String) -> Result<kube::Client, KubeError> {
    let options = KubeConfigOptions {
        context: Some(context.clone()),
        cluster: None,
        user: None,
    };
    let mut config = kube::Config::from_kubeconfig(&options)
        .await
        .map_err(|e| KubeError::UnexpectedError(format!("Failed to create client: {}", e)))?;

    config.connect_timeout = Some(Duration::from_secs(10));
    config.read_timeout = Some(Duration::from_secs(30));
    config.write_timeout = Some(Duration::from_secs(30));


    let client = Client::try_from(config)
        .map_err(|e| KubeError::UnexpectedError(format!("Failed to create client: {}", e)))?;

    Ok(client)
}

pub async fn get_cluster_resources(
    context: &str,
    namespace: &str,
    resource_type: &DynamicObject,
) -> Result<Vec<Value>> {
    // Use the existing client creation logic from this module.
    let client = create_client(context.to_string()).await?;

    // Discover the available API resources from the cluster.
    let discovery = Discovery::new(client.clone());

    let discovery = match tokio::time::timeout(Duration::from_secs(10), discovery.run()).await {
        Ok(Ok(d)) => d,
        Ok(Err(e)) => return Err(anyhow::anyhow!("Failed to discover API resources: {}", e)),
        Err(_) => return Err(anyhow::anyhow!("Failed to discover API resources: timed out")),
    };

    let gvk = if let Some(tm) = resource_type.types.clone() {
        GroupVersionKind::try_from(tm).map_err(|e| {
            KubeError::ManifestApplicationFailed(format!(
                "Failed to read or apply Kubernetes manifest: {}",
                e
            ))
        })?
    } else {
        return Err(anyhow::anyhow!(
            "Resource type must have valid TypeMeta (apiVersion and kind)"
        ));
    };

    // Find the specific ApiResource and its capabilities based on the plural name provided.
    // For example, "deployments" will resolve to the ApiResource for apps/v1/Deployment.
    let (ar, caps) = match discovery.resolve_gvk(&gvk) {
        Some((ar, caps)) => (ar, caps),
        None => {
            return Err(anyhow::anyhow!(
                "Resource type '{}' not found in the cluster",
                resource_type.metadata.name.as_deref().unwrap_or("unknown")
            ));
        }
    };
    // Use the existing dynamic_api helper to create an API client.
    // By passing `all = true`, this will fetch resources from all namespaces
    // if the resource type is namespaced, which is a common requirement.
    let api = dynamic_api(ar, caps, client, Some(namespace), false);

    // List all resources of the specified type using default parameters.
    let list = api
        .list(&ListParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to list resources: {}", e))?;

    // The result of the list operation is an `ObjectList<DynamicObject>`.
    // We iterate through the `items`, serialize each `DynamicObject` into a `serde_json::Value`,
    // and collect them into a Vec.
    let resources: Result<Vec<Value>> = list
        .items
        .into_iter()
        .map(|item| {
            serde_json::to_value(item)
                .map_err(|e| anyhow::anyhow!("Failed to serialize resource to JSON: {}", e))
        })
        .collect();

    resources
}

pub async fn apply(
    path: Option<PathBuf>,
    client: Client,
    discovery: &Discovery,
) -> Result<(String, String), KubeError> {
    let ssapply = PatchParams::apply("sailr").force();
    let pth = path.clone().expect("path is required");
    let mut name: String = "".to_string();
    let mut namespace: String = "".to_string();
    let yaml = std::fs::read_to_string(&pth)
        .map_err(|e| KubeError::UnexpectedError(format!("Failed reading from file: {}", e)))?;
    for doc in multidoc_deserialize(&yaml).await.map_err(|e| {
        KubeError::UnexpectedError(format!("Multidoc Deserialization failed : {}", e))
    })? {
        let obj: DynamicObject = serde_yaml::from_value(doc).map_err(|e| {
            KubeError::UnexpectedError(format!("Yaml Deserialization failed: {}", e))
        })?;
        namespace = obj
            .metadata
            .namespace
            .as_deref()
            .or(Some("default"))
            .unwrap()
            .to_string();
        name = obj.metadata.name.clone().unwrap_or_default();
        let gvk = if let Some(tm) = &obj.types {
            GroupVersionKind::try_from(tm).map_err(|e| {
                KubeError::ManifestApplicationFailed(format!(
                    "Failed to read or apply Kubernetes manifest: {}",
                    e
                ))
            })?
        } else {
            LOGGER.error(&format!(
                "cannot apply object without valid TypeMeta {:?}",
                &obj
            ));
            LOGGER.error(&format!("please add apiVersion and kind to the object"));
            continue;
        };
        let name = &obj.metadata.name;
        let res = discovery.resolve_gvk(&gvk);
        if let Some((ar, caps)) = res {
            let api = dynamic_api(ar, caps, client.clone(), Some(&namespace), false);
            let data: serde_json::Value = serde_json::to_value(&obj).map_err(|e| {
                KubeError::UnexpectedError(format!("Json Serialization failed: {}", e))
            })?;
            let _ = api
                .patch(&name.clone().unwrap(), &ssapply, &Patch::Apply(data))
                .await
                .map_err(|e| {
                    KubeError::ResourceUpdateFailed(format!("Resource patch failed: {}", e))
                })?;
            LOGGER.info(&format!(
                "Applied {} {}",
                gvk.kind,
                name.clone().unwrap_or_default()
            ));
        } else {
            LOGGER.error(&format!("Cannot apply document for unknown {:?}", gvk));
        }
    }

    Ok((namespace, name))
}

/// Compares two JSON representations of Kubernetes resources and returns a diff string if they differ.
/// Returns `None` if there are no differences.
pub fn diff_resources(current: &Value, new: &Value) -> Option<String> {
    // Convert both JSON values into pretty-printed strings
    let current_str = serde_json::to_string_pretty(current).ok()?;
    let new_str = serde_json::to_string_pretty(new).ok()?;

    // If both representations are identical, no diff is needed
    if current_str == new_str {
        return None;
    }

    // Create a diff patch between the current and new states
    let patch = create_patch(&current_str, &new_str);
    let diff = PatchFormatter::new();

    let res = diff.fmt_patch(&patch).to_string();

    Some(res)
}

fn dynamic_api(
    ar: ApiResource,
    caps: ApiCapabilities,
    client: Client,
    ns: Option<&str>,
    all: bool,
) -> Api<DynamicObject> {
    if caps.scope == Scope::Cluster || all {
        Api::all_with(client, &ar)
    } else if let Some(namespace) = ns {
        Api::namespaced_with(client, namespace, &ar)
    } else {
        Api::default_namespaced_with(client, &ar)
    }
}

pub async fn multidoc_deserialize(data: &str) -> Result<Vec<serde_yaml::Value>, KubeError> {
    let mut docs = vec![];
    for de in serde_yaml::Deserializer::from_str(data) {
        docs.push(serde_yaml::Value::deserialize(de).map_err(|e| {
            KubeError::ManifestApplicationFailed(format!(
                "Failed to read or apply Kubernetes manifest: {}",
                e
            ))
        })?);
    }
    Ok(docs)
}

pub fn is_send<T: Sync>(_t: T) {}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn test_api_is_send() {
        is_send(multidoc_deserialize(""));
    }
}
