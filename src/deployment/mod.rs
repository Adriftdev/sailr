pub mod k8sm8;
use crate::deployment::k8sm8::deployments::delete_deployment;
use crate::deployment::k8sm8::multidoc_deserialize;
use crate::environment::{CommandSpec, Environment, Service};
use crate::{cli::DeploymentStrategy, deployment::k8sm8::daemonsets::delete_daemonset};
use anyhow::Result;
use kube::api::{DeleteParams, Patch, PatchParams};
use kube::core::DynamicObject;
use kube::core::GroupVersionKind;
use kube::discovery::Discovery;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use walkdir::WalkDir;

use crate::{errors::DeployError, LOGGER};

#[derive(Clone)]
struct DeploymentSnapshot {
    desired: DynamicObject,
    previous: Option<DynamicObject>,
}

#[derive(Clone, Default)]
pub struct DeploymentJournal {
    context: String,
    entries: Vec<DeploymentSnapshot>,
}

pub type SharedDeploymentJournal = Arc<Mutex<DeploymentJournal>>;

#[async_trait::async_trait]
trait RollbackBackend: Sync {
    async fn restore(&self, snapshot: &DeploymentSnapshot) -> Result<(), String>;
    async fn delete(&self, snapshot: &DeploymentSnapshot) -> Result<(), String>;
}

struct KubernetesRollbackBackend {
    client: kube::Client,
    discovery: Discovery,
}

impl KubernetesRollbackBackend {
    fn resolve(
        &self,
        snapshot: &DeploymentSnapshot,
    ) -> Result<(kube::Api<DynamicObject>, String, String), String> {
        let types = snapshot
            .desired
            .types
            .as_ref()
            .ok_or_else(|| "Rollback object has no apiVersion/kind".to_string())?;
        let gvk = GroupVersionKind::try_from(types).map_err(|error| error.to_string())?;
        let (resource, capabilities) = self
            .discovery
            .resolve_gvk(&gvk)
            .ok_or_else(|| format!("Unable to resolve {} during rollback", gvk.kind))?;
        let namespace = snapshot
            .desired
            .metadata
            .namespace
            .as_deref()
            .unwrap_or("default");
        let name = snapshot
            .desired
            .metadata
            .name
            .clone()
            .ok_or_else(|| format!("Rollback {} has no metadata.name", gvk.kind))?;
        let api = k8sm8::dynamic_api(
            resource,
            capabilities,
            self.client.clone(),
            Some(namespace),
            false,
        );
        Ok((api, gvk.kind, name))
    }
}

#[async_trait::async_trait]
impl RollbackBackend for KubernetesRollbackBackend {
    async fn restore(&self, snapshot: &DeploymentSnapshot) -> Result<(), String> {
        let (api, kind, name) = self.resolve(snapshot)?;
        let previous = snapshot
            .previous
            .as_ref()
            .ok_or_else(|| format!("{kind} {name} has no prior snapshot"))?;
        let mut value = serde_json::to_value(previous).map_err(|error| error.to_string())?;
        sanitize_snapshot_for_apply(&mut value);
        api.patch(
            &name,
            &PatchParams::apply("sailr-rollback").force(),
            &Patch::Apply(value),
        )
        .await
        .map_err(|error| format!("{kind} {name}: {error}"))?;
        LOGGER.info(&format!("[ROLLBACK] restored {kind} {name}"));
        Ok(())
    }

    async fn delete(&self, snapshot: &DeploymentSnapshot) -> Result<(), String> {
        let (api, kind, name) = self.resolve(snapshot)?;
        api.delete(&name, &DeleteParams::default())
            .await
            .map_err(|error| format!("{kind} {name}: {error}"))?;
        LOGGER.info(&format!("[ROLLBACK] deleted newly created {kind} {name}"));
        Ok(())
    }
}

pub fn new_deployment_journal() -> SharedDeploymentJournal {
    Arc::new(Mutex::new(DeploymentJournal::default()))
}

pub async fn deploy_transactional(
    context: String,
    env_name: &str,
    strategy: DeploymentStrategy,
    journal: SharedDeploymentJournal,
) -> Result<(), DeployError> {
    let prepared = snapshot_deployment_targets(&context, env_name).await?;
    {
        let mut state = journal.lock().map_err(|_| {
            DeployError::EnvironmentDeploymentFailed(
                "Deployment journal lock is poisoned".to_string(),
            )
        })?;
        *state = prepared;
    }

    if let Err(deploy_error) = deploy(context, env_name, strategy).await {
        let rollback_error = rollback_transaction(&journal).await.err();
        return Err(DeployError::EnvironmentDeploymentFailed(
            match rollback_error {
                Some(rollback_error) => format!(
                    "{}; automatic rollback also failed: {}",
                    deploy_error, rollback_error
                ),
                None => format!("{}; managed resources were restored", deploy_error),
            },
        ));
    }
    Ok(())
}

pub async fn rollback_transaction(journal: &SharedDeploymentJournal) -> Result<(), DeployError> {
    let state = journal
        .lock()
        .map_err(|_| {
            DeployError::EnvironmentDeploymentFailed(
                "Deployment journal lock is poisoned".to_string(),
            )
        })?
        .clone();
    if state.entries.is_empty() {
        return Ok(());
    }

    LOGGER.info("[ROLLBACK] restoring managed Kubernetes resources");
    let client = k8sm8::create_client(state.context.clone()).await?;
    let discovery = Discovery::new(client.clone())
        .run()
        .await
        .map_err(|error| {
            DeployError::DiscoveryInitializationFailed(format!(
                "Failed to initialize Kubernetes Discovery for rollback: {error}"
            ))
        })?;
    let backend = KubernetesRollbackBackend { client, discovery };
    let errors = execute_rollback(&state.entries, &backend).await;

    if errors.is_empty() {
        Ok(())
    } else {
        Err(DeployError::EnvironmentDeploymentFailed(format!(
            "Rollback failed for: {}",
            errors.join("; ")
        )))
    }
}

async fn execute_rollback<B: RollbackBackend>(
    entries: &[DeploymentSnapshot],
    backend: &B,
) -> Vec<String> {
    let mut errors = Vec::new();
    for snapshot in entries.iter().rev() {
        let result = if snapshot.previous.is_some() {
            backend.restore(snapshot).await
        } else {
            backend.delete(snapshot).await
        };
        if let Err(error) = result {
            errors.push(error);
        }
    }
    errors
}

async fn snapshot_deployment_targets(
    context: &str,
    env_name: &str,
) -> Result<DeploymentJournal, DeployError> {
    let client = k8sm8::create_client(context.to_string()).await?;
    let discovery = Discovery::new(client.clone())
        .run()
        .await
        .map_err(|error| {
            DeployError::DiscoveryInitializationFailed(format!(
                "Failed to initialize Kubernetes Discovery for deployment snapshot: {error}"
            ))
        })?;
    let root = Path::new("k8s").join("generated").join(env_name);
    let mut files = WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| matches!(extension, "yaml" | "yml"))
        })
        .collect::<Vec<_>>();
    files.sort();

    let mut entries = Vec::new();
    for file in files {
        let yaml = std::fs::read_to_string(&file).map_err(|error| {
            DeployError::ManifestApplicationFailed(format!(
                "Failed to read {} for deployment snapshot: {}",
                file.display(),
                error
            ))
        })?;
        for document in k8sm8::multidoc_deserialize(&yaml).await? {
            let desired: DynamicObject = serde_yaml::from_value(document).map_err(|error| {
                DeployError::ManifestApplicationFailed(format!(
                    "Failed to parse {} for deployment snapshot: {}",
                    file.display(),
                    error
                ))
            })?;
            let types = desired.types.as_ref().ok_or_else(|| {
                DeployError::ManifestApplicationFailed(format!(
                    "{} contains an object without apiVersion/kind",
                    file.display()
                ))
            })?;
            let gvk = GroupVersionKind::try_from(types).map_err(|error| {
                DeployError::ManifestApplicationFailed(format!(
                    "Invalid object type in {}: {}",
                    file.display(),
                    error
                ))
            })?;
            let (resource, capabilities) = discovery.resolve_gvk(&gvk).ok_or_else(|| {
                DeployError::ManifestApplicationFailed(format!(
                    "Resource type {} is not available in the target cluster",
                    gvk.kind
                ))
            })?;
            let namespace = desired.metadata.namespace.as_deref().unwrap_or("default");
            let name = desired.metadata.name.as_deref().ok_or_else(|| {
                DeployError::ManifestApplicationFailed(format!(
                    "{} contains a {} without metadata.name",
                    file.display(),
                    gvk.kind
                ))
            })?;
            let api = k8sm8::dynamic_api(
                resource,
                capabilities,
                client.clone(),
                Some(namespace),
                false,
            );
            let previous = api.get_opt(name).await.map_err(|error| {
                DeployError::ManifestApplicationFailed(format!(
                    "Failed to snapshot {} {}: {}",
                    gvk.kind, name, error
                ))
            })?;
            entries.push(DeploymentSnapshot { desired, previous });
        }
    }

    Ok(DeploymentJournal {
        context: context.to_string(),
        entries,
    })
}

fn sanitize_snapshot_for_apply(value: &mut serde_json::Value) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    object.remove("status");
    if let Some(metadata) = object
        .get_mut("metadata")
        .and_then(serde_json::Value::as_object_mut)
    {
        for field in [
            "creationTimestamp",
            "generation",
            "managedFields",
            "resourceVersion",
            "selfLink",
            "uid",
        ] {
            metadata.remove(field);
        }
    }
}

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
    LOGGER.header(
        "Deploy",
        &format!("{} → {} ({:?})", env_name, ctx, strategy),
    );

    let env = Environment::load_from_file(env_name).map_err(|e| {
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

    LOGGER.status(
        "Finished",
        &format!(
            "deployed successfully! Applied {} manifests.",
            applied_total
        ),
        "green",
    );

    Ok(())
}

#[cfg(test)]
mod transactional_tests {
    use super::*;

    #[derive(Default)]
    struct FakeRollbackBackend {
        operations: Mutex<Vec<String>>,
        fail_name: Option<String>,
    }

    #[async_trait::async_trait]
    impl RollbackBackend for FakeRollbackBackend {
        async fn restore(&self, snapshot: &DeploymentSnapshot) -> Result<(), String> {
            self.record("restore", snapshot)
        }

        async fn delete(&self, snapshot: &DeploymentSnapshot) -> Result<(), String> {
            self.record("delete", snapshot)
        }
    }

    impl FakeRollbackBackend {
        fn record(&self, operation: &str, snapshot: &DeploymentSnapshot) -> Result<(), String> {
            let name = snapshot.desired.metadata.name.clone().unwrap();
            self.operations
                .lock()
                .unwrap()
                .push(format!("{operation}:{name}"));
            if self.fail_name.as_deref() == Some(name.as_str()) {
                Err(format!("{operation} failed for {name}"))
            } else {
                Ok(())
            }
        }
    }

    fn object(name: &str) -> DynamicObject {
        serde_json::from_value(serde_json::json!({
            "apiVersion": "v1",
            "kind": "ConfigMap",
            "metadata": {"name": name, "namespace": "default"},
            "data": {"key": "value"}
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn rollback_uses_reverse_order_and_restores_or_deletes() {
        let entries = vec![
            DeploymentSnapshot {
                desired: object("existing"),
                previous: Some(object("existing")),
            },
            DeploymentSnapshot {
                desired: object("created"),
                previous: None,
            },
        ];
        let backend = FakeRollbackBackend::default();
        assert!(execute_rollback(&entries, &backend).await.is_empty());
        assert_eq!(
            *backend.operations.lock().unwrap(),
            vec!["delete:created", "restore:existing"]
        );
    }

    #[tokio::test]
    async fn rollback_collects_backend_failures_without_stopping() {
        let entries = vec![
            DeploymentSnapshot {
                desired: object("first"),
                previous: Some(object("first")),
            },
            DeploymentSnapshot {
                desired: object("second"),
                previous: None,
            },
        ];
        let backend = FakeRollbackBackend {
            fail_name: Some("second".to_string()),
            ..Default::default()
        };
        let errors = execute_rollback(&entries, &backend).await;
        assert_eq!(errors, vec!["delete failed for second"]);
        assert_eq!(backend.operations.lock().unwrap().len(), 2);
    }
}
