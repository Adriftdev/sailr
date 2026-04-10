pub mod k8sm8;
use crate::deployment::k8sm8::deployments::delete_deployment;
use crate::deployment::k8sm8::multidoc_deserialize;
use crate::environment::{CommandSpec, Environment, Service};
use crate::{cli::DeploymentStrategy, deployment::k8sm8::daemonsets::delete_daemonset};
use anyhow::Result;
use kube::core::DynamicObject;
use std::fs;
use std::path::Path;
use std::process::Command;
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
                .is_some_and(|ext| ext == "yaml" || ext == "yml"))
        {
            LOGGER.debug(&format!("Applying manifest: {:?}", file_path));
            let res =
                k8sm8::apply(Some(file_path.to_path_buf()), client.clone(), discovery).await?;
            applied_manifests.push(res);
        }
    }

    Ok(applied_manifests)
}

fn replace_template_var(input: &str, key: &str, value: &str) -> String {
    input
        .replace(&format!("{{{{ {} }}}}", key), value)
        .replace(&format!("{{{{{}}}}}", key), value)
}

fn render_service_hook(hook: &str, env: &Environment, service: &Service) -> String {
    let namespace = service.namespace_or(&env.name);
    let rendered = replace_template_var(hook, "name", &service.name);
    let rendered =
        replace_template_var(&rendered, "platform", env.platform.as_deref().unwrap_or(""));
    let rendered = replace_template_var(&rendered, "version", &service.version);
    replace_template_var(&rendered, "namespace", namespace)
}

fn run_service_hooks(
    stage: &str,
    hook_spec: &CommandSpec,
    env: &Environment,
    service: &Service,
) -> Result<(), DeployError> {
    for hook in hook_spec.as_vec() {
        let rendered_hook = render_service_hook(&hook, env, service);
        LOGGER.info(&format!(
            "Running {} hook for service '{}': {}",
            stage, service.name, rendered_hook
        ));

        let output = Command::new("sh")
            .arg("-c")
            .arg(&rendered_hook)
            .output()
            .map_err(|e| {
                DeployError::ManifestApplicationFailed(format!(
                    "Failed to execute {} hook for service '{}': {}",
                    stage, service.name, e
                ))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DeployError::ManifestApplicationFailed(format!(
                "{} hook failed for service '{}': {}",
                stage, service.name, stderr
            )));
        }
    }

    Ok(())
}

/// Helper function to deserialize a YAML document and delete the resource if it's a target kind.
///
/// This avoids code duplication for deleting Deployments, DaemonSets, etc.
async fn delete_workload_if_matches(
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
                        "Attempting to delete {}: {} in namespace: {}",
                        tm.kind, name, namespace
                    ));
                    match delete_deployment(client.clone(), namespace, name).await {
                        Ok(_) => LOGGER.info(&format!(
                            "Successfully deleted {}: {} in namespace: {}",
                            tm.kind, name, namespace
                        )),
                        Err(e) => LOGGER.warn(&format!(
                            "Failed to delete {}: {} in namespace: {}. Error: {:?}",
                            tm.kind, name, namespace, e
                        )),
                    }

                    match delete_daemonset(client.clone(), namespace, name).await {
                        Ok(_) => LOGGER.info(&format!(
                            "Successfully deleted {}: {} in namespace: {}",
                            tm.kind, name, namespace
                        )),
                        Err(e) => LOGGER.warn(&format!(
                            "Failed to delete {}: {} in namespace: {}. Error: {:?}",
                            tm.kind, name, namespace, e
                        )),
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

    let env = Environment::load_from_file(&env_name.to_string()).map_err(|e| {
        DeployError::EnvironmentDeploymentFailed(format!(
            "Failed to load environment '{}': {}",
            env_name, e
        ))
    })?;

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
        LOGGER.info(&format!(
            "Restart strategy selected. Deleting existing Deployments or Daemonsets in environment: {}",
            env_name
        ));

        let walker = WalkDir::new(&path).into_iter().filter_map(|e| e.ok());

        for entry in walker {
            let file_path = entry.path();
            if file_path.is_file()
                && (file_path
                    .extension()
                    .is_some_and(|ext| ext == "yaml" || ext == "yml"))
            {
                LOGGER.debug(&format!(
                    "Processing file for pre-deletion: {:?}",
                    file_path
                ));
                if let Ok(yaml_content) = fs::read_to_string(file_path) {
                    if let Ok(docs) = multidoc_deserialize(&yaml_content).await {
                        for doc in docs {
                            delete_workload_if_matches(doc, &client, &["Deployment", "DaemonSet"])
                                .await
                                .map_err(|e| {
                                    DeployError::ManifestApplicationFailed(format!(
                                        "Failed during pre-deletion step: {}",
                                        e
                                    ))
                                })?;
                        }
                    }
                }
            }
        }
    }

    let mut applied_total = 0usize;

    for service in &env.services {
        let service_path = path.join(service.get_path());
        if !service_path.exists() {
            LOGGER.warn(&format!(
                "Generated manifests not found for service '{}': {:?}",
                service.name, service_path
            ));
            continue;
        }

        if let Some(hooks) = &service.hooks {
            if let Some(pre_deploy) = &hooks.pre_deploy {
                run_service_hooks("pre_deploy", pre_deploy, &env, service)?;
            }
        }

        let applied =
            apply_manifests_from_path(service_path.as_path(), client.clone(), &discovery).await?;
        applied_total += applied.len();

        if let Some(hooks) = &service.hooks {
            if let Some(post_deploy) = &hooks.post_deploy {
                run_service_hooks("post_deploy", post_deploy, &env, service)?;
            }
        }
    }

    // Fallback for legacy/generated layouts where manifests are not grouped by service directory.
    if applied_total == 0 {
        let applied = apply_manifests_from_path(path.as_path(), client.clone(), &discovery).await?;
        applied_total += applied.len();
    }

    LOGGER.info(&format!(
        "Deployed successfully! Applied {} manifests.",
        applied_total
    ));

    Ok(())
}
