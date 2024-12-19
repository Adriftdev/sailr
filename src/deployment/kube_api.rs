use std::path::PathBuf;

use anyhow::Result;

use k8s_openapi::serde_json;
use k8s_openapi::{self, api::apps::v1::Deployment};
use kube::api::{Patch, PatchParams};
use kube::client::ConfigExt;
use kube::core::{DynamicObject, GroupVersionKind};
use kube::discovery::{ApiCapabilities, ApiResource, Scope};
use kube::{Api, Client, Discovery};

use serde::Deserialize;
use tower::ServiceBuilder;

use crate::errors::KubeError;
use crate::LOGGER;
use kube::config::Config;

pub async fn get_all_deployments() -> Result<(), KubeError> {
    let client = Client::try_default().await.map_err(|e| {
        KubeError::ClientCreationFailed(format!("Failed to create Kubernetes client: {}", e))
    })?;

    let deployments: Api<Deployment> = Api::all(client);

    let list = deployments.list(&Default::default()).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve Kubernetes resource: {}", e))
    })?;

    for deployment in list.items {
        println!(
            "Deployment {} in {} has {} replicas",
            deployment.metadata.name.unwrap(),
            deployment.metadata.namespace.unwrap(),
            deployment.spec.unwrap().replicas.unwrap()
        );
    }

    Ok(())
}

pub async fn get_all_deployments_with_config() -> Result<(), KubeError> {
    let config = Config::infer().await.map_err(|e| {
        KubeError::ClientCreationFailed(format!("Failed to create Kubernetes client: {}", e))
    })?;
    let https = config.openssl_https_connector().map_err(|e| {
        KubeError::ClientCreationFailed(format!("Failed to create Kubernetes client: {}", e))
    })?;
    let service = ServiceBuilder::new()
        .layer(config.base_uri_layer())
        .service(hyper::Client::builder().build(https));
    let client = Client::new(service, "default"); // TODO: make namespace configurable

    let deployments: Api<Deployment> = Api::all(client);
    let list = deployments.list(&Default::default()).await.map_err(|e| {
        KubeError::ResourceRetrievalFailed(format!("Failed to retrieve Kubernetes resource: {}", e))
    })?;

    for deployment in list.items {
        println!(
            "Deployment {} in {} has {} replicas",
            deployment.metadata.name.unwrap(),
            deployment.metadata.namespace.unwrap(),
            deployment.spec.unwrap().replicas.unwrap()
        );
    }

    Ok(())
}

pub async fn create_client(context: String) -> Result<kube::Client, KubeError> {
    let mut kubeconfig = kube::config::KubeConfigOptions::default();
    kubeconfig.context = Some(context);

    let config = Config::from_kubeconfig(&kubeconfig).await.map_err(|e| {
        KubeError::ClientCreationFailed(format!("Failed to create Kubernetes client: {}", e))
    })?;
    let https = config.openssl_https_connector().map_err(|e| {
        KubeError::ClientCreationFailed(format!("Failed to create Kubernetes client: {}", e))
    })?;
    let service = ServiceBuilder::new()
        .layer(config.base_uri_layer())
        .service(hyper::Client::builder().build(https));
    let client = Client::new(service, "default"); // TODO: make namespace configurable (or infer from kubeconfig)

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
