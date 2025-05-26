pub mod k8sm8;
use anyhow::Result;
use crate::cli::DeploymentStrategy;
use crate::deployment::k8sm8::deployments::delete_deployment;
use crate::deployment::k8sm8::multidoc_deserialize;
use kube::core::DynamicObject;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

use async_recursion::async_recursion;

use crate::{errors::DeployError, LOGGER};

#[async_recursion]
pub async fn apply_path_recursive(
    path: &Path,
    client: kube::Client,
    discovery: &kube::discovery::Discovery,
    manifest: &mut Option<Vec<(String, String)>>,
) -> Result<Vec<(String, String)>, DeployError> {
    let mut manifest: Vec<(String, String)> = match manifest {
        Some(m) => m.clone(),
        None => vec![],
    };

    if path.is_dir() {
        for entry in std::fs::read_dir(path).map_err(|e| {
            DeployError::ManifestApplicationFailed(format!(
                "Failed to read or apply Kubernetes manifest: {}",
                e
            ))
        })? {
            let entry = entry.map_err(|e| {
                DeployError::ManifestApplicationFailed(format!(
                    "Failed to read or apply Kubernetes manifest: {}",
                    e
                ))
            })?;
            let path = entry.path();
            manifest = apply_path_recursive(
                &path,
                client.clone(),
                discovery,
                &mut Some(manifest.clone()),
            )
            .await?;
        }
    } else {
        let path = path.to_path_buf();
        let res = k8sm8::apply(Some(path), client, discovery).await?;
        manifest.push(res);
    }

    Ok(manifest)
}

pub async fn deploy(ctx: String, env_name: &str, strategy: DeploymentStrategy) -> Result<(), DeployError> {
    LOGGER.info(&format!("Deploying to {} for {} with strategy {:?}", ctx, env_name, strategy));
    let client = k8sm8::create_client(ctx).await?;
    let discovery = kube::Discovery::new(client.clone())
        .run()
        .await
        .map_err(|e| {
            DeployError::DiscoveryInitializationFailed(format!(
                "Failed to initialize Kubernetes Discovery: {}",
                e
            ))
        })?;
    let path = Path::new("./k8s/generated").join(env_name);

    if strategy == DeploymentStrategy::Restart {
        LOGGER.info(&format!("Restart strategy selected. Deleting existing Deployments in environment: {}", env_name));
        let env_path = Path::new("./k8s/generated").join(env_name);
        if env_path.is_dir() {
            for entry in WalkDir::new(env_path).into_iter().filter_map(Result::ok) {
                let file_path = entry.path();
                if file_path.is_file() && (file_path.extension().map_or(false, |ext| ext == "yaml" || ext == "yml")) {
                    LOGGER.debug(&format!("Processing file for pre-deletion: {:?}", file_path));
                    match fs::read_to_string(file_path) {
                        Ok(yaml_content) => {
                            match multidoc_deserialize(&yaml_content).await {
                                Ok(docs) => {
                                    for doc in docs {
                                        match serde_yaml::from_value::<DynamicObject>(doc.clone()) {
                                            Ok(obj) => {
                                                if obj.types.as_ref().map_or(false, |tm| tm.kind == "Deployment") {
                                                    if let Some(name) = obj.metadata.name.as_ref() {
                                                        let namespace = obj.metadata.namespace.as_deref().unwrap_or("default");
                                                        LOGGER.info(&format!("Attempting to delete Deployment: {} in namespace: {}", name, namespace));
                                                        match delete_deployment(client.clone(), namespace, name).await {
                                                            Ok(_) => LOGGER.info(&format!("Successfully deleted Deployment: {} in namespace: {}", name, namespace)),
                                                            Err(e) => {
                                                                // Log non-critical errors (e.g., NotFound) differently if possible,
                                                                // but for now, just log the error.
                                                                LOGGER.warn(&format!("Failed to delete Deployment: {} in namespace: {}. Error: {:?}", name, namespace, e));
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                LOGGER.warn(&format!("Failed to parse document in {:?} as DynamicObject: {:?}", file_path, e));
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    LOGGER.warn(&format!("Failed to deserialize YAML from {:?}: {:?}", file_path, e));
                                }
                            }
                        }
                        Err(e) => {
                            LOGGER.warn(&format!("Failed to read file {:?}: {:?}", file_path, e));
                        }
                    }
                }
            }
        }
    }

    // TODO: https://linear.app/adriftdev/issue/ADR-40/environment-version-management
    let _manifest = apply_path_recursive(path.as_path(), client.clone(), &discovery, &mut None).await?;
    LOGGER.info("Deployed successfully!");

    Ok(())
}
