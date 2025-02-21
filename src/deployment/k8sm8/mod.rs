pub mod configmaps;
pub mod cronjobs;
pub mod daemonsets;
pub mod deployments;
pub mod jobs;
pub mod namespaces;
pub mod pods;
pub mod secrets;
pub mod services;
pub mod statefulsets;

use std::path::PathBuf;

use anyhow::Result;

use diffy::{create_patch, PatchFormatter};
use k8s_openapi::serde_json;
use k8s_openapi::{self};
use kube::api::{Patch, PatchParams};
use kube::core::{DynamicObject, GroupVersionKind};
use kube::discovery::{ApiCapabilities, ApiResource, Scope};
use kube::{Api, Client, Discovery};

use serde::Deserialize;
use serde_json::Value;

pub use deployments::delete_all_deployments;
pub use deployments::delete_deployment;
pub use deployments::get_all_deployments;
use crate::errors::KubeError;
use crate::LOGGER;
use kube::config::KubeConfigOptions;

pub async fn create_client(context: String) -> Result<kube::Client, KubeError> {
    let options = KubeConfigOptions {
        context: Some(context.clone()),
        cluster: None,
        user: None,
    };
    let config = kube::Config::from_kubeconfig(&options)
        .await
        .map_err(|e| KubeError::UnexpectedError(format!("Failed to create client: {}", e)))?;
    let client = Client::try_from(config)
        .map_err(|e| KubeError::UnexpectedError(format!("Failed to create client: {}", e)))?;

    Ok(client)
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
