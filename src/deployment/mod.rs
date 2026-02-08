pub mod k8sm8;
use crate::deployment::k8sm8::multidoc_deserialize;
use crate::{
    cli::DeploymentStrategy,
    deployment::k8sm8::{daemonsets::restart_daemonset, deployments::restart_deployment},
};
use anyhow::Result;
use kube::core::DynamicObject;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

use crate::{errors::DeployError, LOGGER};

/// Applies all valid Kubernetes YAML manifests found recursively in a given path.
///
/// This function is non-recursive and uses `walkdir` for efficient traversal.
async fn apply_manifests_from_path(
    path: &Path,
    client: kube::Client,
    discovery: &kube::discovery::Discovery,
) -> Result<Vec<(String, String)>, DeployError> {
    let mut applied_manifests = vec![];
    let walker = WalkDir::new(path).into_iter().filter_map(|e| e.ok());

    for entry in walker {
        let file_path = entry.path();
        if file_path.is_file()
            && (file_path
                .extension()
                .map_or(false, |ext| ext == "yaml" || ext == "yml"))
        {
            LOGGER.debug(&format!("Applying manifest: {:?}", file_path));
            let res =
                k8sm8::apply(Some(file_path.to_path_buf()), client.clone(), discovery).await?;
            applied_manifests.push(res);
        }
    }

    Ok(applied_manifests)
}

/// Helper function to deserialize a YAML document and restart the resource if it's a target kind.
async fn restart_workload_if_matches(
    doc: serde_yaml::Value,
    client: &kube::Client,
    target_kinds: &[&str],
) -> Result<()> {
    if let Ok(obj) = serde_yaml::from_value::<DynamicObject>(doc) {
        if let Some(tm) = obj.types.as_ref() {
            if target_kinds.contains(&tm.kind.as_str()) {
                if let Some(name) = obj.metadata.name.as_ref() {
                    let namespace = obj.metadata.namespace.as_deref().unwrap_or("default");
                    LOGGER.info(&format!(
                        "Attempting to restart {}: {} in namespace: {}",
                        tm.kind, name, namespace
                    ));

                    if tm.kind == "Deployment" {
                        match restart_deployment(client.clone(), namespace, name).await {
                            Ok(_) => LOGGER.info(&format!(
                                "Successfully restarted Deployment: {} in namespace: {}",
                                name, namespace
                            )),
                            Err(e) => LOGGER.warn(&format!(
                                "Failed to restart Deployment: {} in namespace: {}. Error: {:?}",
                                name, namespace, e
                            )),
                        }
                    } else if tm.kind == "DaemonSet" {
                        match restart_daemonset(client.clone(), namespace, name).await {
                            Ok(_) => LOGGER.info(&format!(
                                "Successfully restarted DaemonSet: {} in namespace: {}",
                                name, namespace
                            )),
                            Err(e) => LOGGER.warn(&format!(
                                "Failed to restart DaemonSet: {} in namespace: {}. Error: {:?}",
                                name, namespace, e
                            )),
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Main entry point for deploying resources to a Kubernetes cluster.
pub async fn deploy(
    ctx: String,
    env_name: &str,
    strategy: DeploymentStrategy,
) -> Result<(), DeployError> {
    LOGGER.info(&format!(
        "Deploying to {} for {} with strategy {:?}",
        ctx, env_name, strategy
    ));

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

    // 1. Apply all manifests first (Create/Update)
    let _manifest = apply_manifests_from_path(path.as_path(), client.clone(), &discovery).await?;

    // 2. If strategy is Restart, trigger rollout restarts for Deployments and DaemonSets
    if strategy == DeploymentStrategy::Restart {
        LOGGER.info(&format!(
            "Restart strategy selected. Triggering rollout restart for Deployments and DaemonSets in environment: {}",
            env_name
        ));

        let walker = WalkDir::new(&path).into_iter().filter_map(|e| e.ok());

        for entry in walker {
            let file_path = entry.path();
            if file_path.is_file()
                && (file_path
                    .extension()
                    .map_or(false, |ext| ext == "yaml" || ext == "yml"))
            {
                LOGGER.debug(&format!(
                    "Processing file for restart check: {:?}",
                    file_path
                ));
                if let Ok(yaml_content) = fs::read_to_string(file_path) {
                    if let Ok(docs) = multidoc_deserialize(&yaml_content).await {
                        for doc in docs {
                            restart_workload_if_matches(
                                doc,
                                &client,
                                &["Deployment", "DaemonSet"],
                            )
                            .await
                            .map_err(|e| {
                                DeployError::ManifestApplicationFailed(format!(
                                    "Failed during restart step: {}",
                                    e
                                ))
                            })?;
                        }
                    }
                }
            }
        }
    }

    LOGGER.info("Deployed successfully!");

    Ok(())
}
