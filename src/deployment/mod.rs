pub mod bundle;
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

#[derive(Debug, Clone)]
pub struct AppliedMutation {
    pub sequence: usize,
    pub identity: bundle::ResourceIdentity,
    pub previous: Option<DynamicObject>,
    pub applied: DynamicObject,
    pub source_path: String,
    pub document_index: usize,
    pub sha256: String,
}

#[derive(Debug, Clone, Default)]
pub struct DeploymentJournal {
    pub entries: Vec<AppliedMutation>,
    pub rollback_attempted: bool,
    pub rollback_errors: Vec<String>,
}

pub type SharedDeploymentJournal = Arc<Mutex<DeploymentJournal>>;

#[async_trait::async_trait]
pub trait DeploymentBackend: Send + Sync {
    async fn get(
        &self,
        identity: &bundle::ResourceIdentity,
    ) -> Result<Option<DynamicObject>, DeployError>;

    async fn apply(
        &self,
        resource: &bundle::DeploymentResource,
    ) -> Result<DynamicObject, DeployError>;

    async fn restore(
        &self,
        identity: &bundle::ResourceIdentity,
        previous: &DynamicObject,
    ) -> Result<(), DeployError>;

    async fn delete(&self, identity: &bundle::ResourceIdentity) -> Result<(), DeployError>;
}

pub struct KubernetesDeploymentBackend {
    client: kube::Client,
    discovery: Discovery,
}

impl KubernetesDeploymentBackend {
    pub async fn new(context: String) -> Result<Self, DeployError> {
        let client = k8sm8::create_client(context).await?;
        let discovery = Discovery::new(client.clone())
            .run()
            .await
            .map_err(|error| {
                DeployError::DiscoveryInitializationFailed(format!(
                    "Failed to initialize Kubernetes Discovery: {error}"
                ))
            })?;
        Ok(Self { client, discovery })
    }

    fn resolve(
        &self,
        identity: &bundle::ResourceIdentity,
    ) -> Result<(kube::Api<DynamicObject>, String, String), String> {
        let (group, version) = identity
            .api_version
            .split_once('/')
            .map_or(("", identity.api_version.as_str()), |(group, version)| {
                (group, version)
            });
        let gvk = GroupVersionKind::gvk(group, version, &identity.kind);
        let (resource, capabilities) = self
            .discovery
            .resolve_gvk(&gvk)
            .ok_or_else(|| format!("Unable to resolve {} during rollback", gvk.kind))?;

        let api = k8sm8::dynamic_api(
            resource,
            capabilities,
            self.client.clone(),
            identity.namespace.as_deref(),
            false,
        );
        Ok((api, gvk.kind, identity.name.clone()))
    }
}

#[async_trait::async_trait]
impl DeploymentBackend for KubernetesDeploymentBackend {
    async fn get(
        &self,
        identity: &bundle::ResourceIdentity,
    ) -> Result<Option<DynamicObject>, DeployError> {
        let (api, kind, name) = self
            .resolve(identity)
            .map_err(DeployError::ManifestApplicationFailed)?;
        api.get_opt(&name).await.map_err(|error| {
            DeployError::ManifestApplicationFailed(format!("Failed to read {kind} {name}: {error}"))
        })
    }

    async fn apply(
        &self,
        resource: &bundle::DeploymentResource,
    ) -> Result<DynamicObject, DeployError> {
        let (api, kind, name) = self
            .resolve(&resource.identity)
            .map_err(DeployError::ManifestApplicationFailed)?;
        let value: serde_json::Value =
            serde_json::from_slice(&resource.raw_bytes).map_err(|error| {
                DeployError::ManifestApplicationFailed(format!(
                    "Invalid canonical bundle bytes for {kind} {name}: {error}"
                ))
            })?;
        let applied = api
            .patch(
                &name,
                &PatchParams::apply("sailr").force(),
                &Patch::Apply(value),
            )
            .await
            .map_err(|error| {
                DeployError::ManifestApplicationFailed(format!(
                    "Failed to apply {kind} {name}: {error}"
                ))
            })?;
        LOGGER.info(&format!("Applied {kind} {name}"));
        Ok(applied)
    }

    async fn restore(
        &self,
        identity: &bundle::ResourceIdentity,
        previous: &DynamicObject,
    ) -> Result<(), DeployError> {
        let (api, kind, name) = self
            .resolve(identity)
            .map_err(DeployError::ManifestApplicationFailed)?;
        let mut value = serde_json::to_value(previous).map_err(|error| {
            DeployError::ManifestApplicationFailed(format!(
                "Failed to serialize rollback snapshot for {kind} {name}: {error}"
            ))
        })?;

        sanitize_snapshot_for_apply(&mut value);
        api.patch(
            &name,
            &PatchParams::apply("sailr-rollback").force(),
            &Patch::Apply(value),
        )
        .await
        .map_err(|error| {
            DeployError::ManifestApplicationFailed(format!("{kind} {name}: {error}"))
        })?;

        LOGGER.info(&format!("[ROLLBACK] restored {kind} {name}"));
        Ok(())
    }

    async fn delete(&self, identity: &bundle::ResourceIdentity) -> Result<(), DeployError> {
        let (api, kind, name) = self
            .resolve(identity)
            .map_err(DeployError::ManifestApplicationFailed)?;
        match api.delete(&name, &DeleteParams::default()).await {
            Ok(_) => {}
            Err(kube::Error::Api(response)) if response.code == 404 => {}
            Err(error) => {
                return Err(DeployError::ManifestApplicationFailed(format!(
                    "{kind} {name}: {error}"
                )));
            }
        }

        LOGGER.info(&format!("[ROLLBACK] deleted newly created {kind} {name}"));
        Ok(())
    }
}

pub fn new_deployment_journal() -> SharedDeploymentJournal {
    Arc::new(Mutex::new(DeploymentJournal::default()))
}

pub async fn deploy_bundle(
    deployment_bundle: &bundle::DeploymentBundle,
    backend: &dyn DeploymentBackend,
    journal: SharedDeploymentJournal,
) -> Result<(), DeployError> {
    {
        let mut state = journal.lock().map_err(|_| {
            DeployError::EnvironmentDeploymentFailed(
                "Deployment journal lock is poisoned".to_string(),
            )
        })?;
        *state = DeploymentJournal::default();
    }

    for resource in &deployment_bundle.resources {
        let previous = match backend.get(&resource.identity).await {
            Ok(previous) => previous,
            Err(error) => {
                return fail_with_rollback(error, backend, &journal).await;
            }
        };
        let applied = match backend.apply(resource).await {
            Ok(applied) => applied,
            Err(error) => {
                return fail_with_rollback(error, backend, &journal).await;
            }
        };
        let mut state = journal.lock().map_err(|_| {
            DeployError::EnvironmentDeploymentFailed(
                "Deployment journal lock is poisoned".to_string(),
            )
        })?;
        let sequence = state.entries.len();
        state.entries.push(AppliedMutation {
            sequence,
            identity: resource.identity.clone(),
            previous,
            applied,
            source_path: resource.source_path.clone(),
            document_index: resource.document_index,
            sha256: resource.sha256.clone(),
        });
    }
    Ok(())
}

async fn fail_with_rollback<T>(
    deploy_error: DeployError,
    backend: &dyn DeploymentBackend,
    journal: &SharedDeploymentJournal,
) -> Result<T, DeployError> {
    match rollback_transaction(journal, backend).await {
        Ok(()) => Err(DeployError::EnvironmentDeploymentFailed(format!(
            "{deploy_error}; successfully applied resources were restored"
        ))),
        Err(rollback_error) => Err(DeployError::EnvironmentDeploymentFailed(format!(
            "{deploy_error}; automatic rollback also failed: {rollback_error}"
        ))),
    }
}

pub async fn rollback_transaction(
    journal: &SharedDeploymentJournal,
    backend: &dyn DeploymentBackend,
) -> Result<(), DeployError> {
    let entries = {
        let mut state = journal.lock().map_err(|_| {
            DeployError::EnvironmentDeploymentFailed(
                "Deployment journal lock is poisoned".to_string(),
            )
        })?;
        if state.rollback_attempted {
            if state.rollback_errors.is_empty() {
                return Ok(());
            }
            return Err(DeployError::EnvironmentDeploymentFailed(format!(
                "Rollback failed for: {}",
                state.rollback_errors.join("; ")
            )));
        }
        state.rollback_attempted = true;
        state.entries.clone()
    };
    if entries.is_empty() {
        return Ok(());
    }

    let mut errors = Vec::new();
    for snapshot in entries.iter().rev() {
        let result = if let Some(previous) = &snapshot.previous {
            backend.restore(&snapshot.identity, previous).await
        } else {
            backend.delete(&snapshot.identity).await
        };
        if let Err(error) = result {
            errors.push(error.to_string());
        }
    }
    let mut state = journal.lock().map_err(|_| {
        DeployError::EnvironmentDeploymentFailed("Deployment journal lock is poisoned".to_string())
    })?;
    state.rollback_errors = errors.clone();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(DeployError::EnvironmentDeploymentFailed(format!(
            "Rollback failed for: {}",
            errors.join("; ")
        )))
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeploymentHookStage {
    Pre,
    Post,
}

pub fn run_environment_hooks(
    env: &Environment,
    stage: DeploymentHookStage,
) -> Result<(), DeployError> {
    for service in &env.services {
        let hook = service.hooks.as_ref().and_then(|hooks| match stage {
            DeploymentHookStage::Pre => hooks.pre_deploy.as_ref(),
            DeploymentHookStage::Post => hooks.post_deploy.as_ref(),
        });
        if let Some(hook) = hook {
            let label = match stage {
                DeploymentHookStage::Pre => "pre_deploy",
                DeploymentHookStage::Post => "post_deploy",
            };
            run_service_hooks(label, hook, env, service)?;
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
    use sha2::Digest;
    use std::collections::{BTreeMap, BTreeSet};

    #[derive(Default)]
    struct FakeDeploymentBackend {
        operations: Mutex<Vec<String>>,
        objects: Mutex<BTreeMap<String, DynamicObject>>,
        fail_apply: Option<String>,
        fail_rollback: BTreeSet<String>,
    }

    #[async_trait::async_trait]
    impl DeploymentBackend for FakeDeploymentBackend {
        async fn get(
            &self,
            identity: &bundle::ResourceIdentity,
        ) -> Result<Option<DynamicObject>, DeployError> {
            Ok(self.objects.lock().unwrap().get(&identity.name).cloned())
        }

        async fn apply(
            &self,
            resource: &bundle::DeploymentResource,
        ) -> Result<DynamicObject, DeployError> {
            let name = resource.identity.name.clone();
            self.operations
                .lock()
                .unwrap()
                .push(format!("apply:{name}"));
            if self.fail_apply.as_deref() == Some(name.as_str()) {
                return Err(DeployError::ManifestApplicationFailed(format!(
                    "apply failed for {name}"
                )));
            }
            let applied = resource.object.clone();
            self.objects.lock().unwrap().insert(name, applied.clone());
            Ok(applied)
        }

        async fn restore(
            &self,
            identity: &bundle::ResourceIdentity,
            previous: &DynamicObject,
        ) -> Result<(), DeployError> {
            let name = identity.name.clone();
            self.operations
                .lock()
                .unwrap()
                .push(format!("restore:{name}"));
            if self.fail_rollback.contains(&name) {
                return Err(DeployError::ManifestApplicationFailed(format!(
                    "restore failed for {name}"
                )));
            }
            self.objects.lock().unwrap().insert(name, previous.clone());
            Ok(())
        }

        async fn delete(&self, identity: &bundle::ResourceIdentity) -> Result<(), DeployError> {
            let name = identity.name.clone();
            self.operations
                .lock()
                .unwrap()
                .push(format!("delete:{name}"));
            if self.fail_rollback.contains(&name) {
                return Err(DeployError::ManifestApplicationFailed(format!(
                    "delete failed for {name}"
                )));
            }
            self.objects.lock().unwrap().remove(&name);
            Ok(())
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

    fn resource(name: &str) -> bundle::DeploymentResource {
        let object = object(name);
        let raw_bytes = serde_json::to_vec(&object).unwrap();
        bundle::DeploymentResource {
            source_path: format!("{name}.yaml"),
            document_index: 0,
            identity: bundle::ResourceIdentity {
                api_version: "v1".to_string(),
                kind: "ConfigMap".to_string(),
                namespace: Some("default".to_string()),
                name: name.to_string(),
            },
            sha256: hex::encode(sha2::Sha256::digest(&raw_bytes)),
            raw_bytes,
            object,
        }
    }

    fn deployment_bundle(names: &[&str]) -> bundle::DeploymentBundle {
        bundle::DeploymentBundle {
            schema: bundle::DEPLOYMENT_BUNDLE_SCHEMA.to_string(),
            profile: "test".to_string(),
            environment: "test".to_string(),
            target: bundle::DeploymentTarget {
                context: "test".to_string(),
                namespace: "default".to_string(),
            },
            resources: names.iter().map(|name| resource(name)).collect(),
            plan_hash: "0".repeat(64),
        }
    }

    #[tokio::test]
    async fn partial_failure_journals_and_rolls_back_only_successful_mutations() {
        let backend = FakeDeploymentBackend {
            fail_apply: Some("third".to_string()),
            ..Default::default()
        };
        backend
            .objects
            .lock()
            .unwrap()
            .insert("first".to_string(), object("first"));
        let journal = new_deployment_journal();
        let result = deploy_bundle(
            &deployment_bundle(&["first", "second", "third", "fourth"]),
            &backend,
            journal.clone(),
        )
        .await;
        assert!(result.is_err());
        let state = journal.lock().unwrap().clone();
        assert_eq!(state.entries.len(), 2);
        assert!(state.rollback_attempted);
        assert_eq!(
            *backend.operations.lock().unwrap(),
            vec![
                "apply:first",
                "apply:second",
                "apply:third",
                "delete:second",
                "restore:first"
            ]
        );
        assert!(!backend.objects.lock().unwrap().contains_key("second"));
        assert!(!backend.objects.lock().unwrap().contains_key("fourth"));
    }

    #[tokio::test]
    async fn rollback_collects_backend_failures_without_stopping() {
        let backend = FakeDeploymentBackend {
            fail_apply: Some("third".to_string()),
            fail_rollback: BTreeSet::from(["second".to_string()]),
            ..Default::default()
        };
        backend
            .objects
            .lock()
            .unwrap()
            .insert("first".to_string(), object("first"));
        let journal = new_deployment_journal();
        let result = deploy_bundle(
            &deployment_bundle(&["first", "second", "third"]),
            &backend,
            journal.clone(),
        )
        .await;
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("delete failed for second"));
        assert_eq!(
            *backend.operations.lock().unwrap(),
            vec![
                "apply:first",
                "apply:second",
                "apply:third",
                "delete:second",
                "restore:first"
            ]
        );
        assert_eq!(journal.lock().unwrap().rollback_errors.len(), 1);
    }

    #[tokio::test]
    async fn deploy_uses_bundle_bytes_after_source_file_changes() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("config.yaml");
        std::fs::write(
            &path,
            "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: config\ndata:\n  value: approved\n",
        )
        .expect("approved manifest");
        let bundle = bundle::build_deployment_bundle(root.path(), "test", "test", "ctx", "default")
            .expect("bundle");
        let approved_digest = bundle.resources[0].sha256.clone();
        std::fs::write(
            &path,
            "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: config\ndata:\n  value: tampered\n",
        )
        .expect("tampered manifest");

        let backend = FakeDeploymentBackend::default();
        let journal = new_deployment_journal();
        deploy_bundle(&bundle, &backend, journal.clone())
            .await
            .expect("deploy bundle");
        let applied = backend
            .objects
            .lock()
            .unwrap()
            .get("config")
            .cloned()
            .expect("applied object");
        let value = serde_json::to_value(applied).expect("applied json");
        assert_eq!(value["data"]["value"], "approved");
        assert_eq!(journal.lock().unwrap().entries[0].sha256, approved_digest);
    }
}
