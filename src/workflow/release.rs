use crate::cli::{WorkflowApplyArgs, WorkflowPrepareArgs};
use crate::deployment::bundle::{BoundPostDeployHook, PortableDeploymentBundle};
use crate::environment::{Environment, RequiredDeploymentApproval};
use crate::workflow::config::WorkflowConfig;
use crate::workflow::profile::{ApprovalMode, WorkflowStepMode};
use base64::Engine;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Serialize)]
struct PreparationEvidence<'a> {
    schema_version: &'static str,
    profile: &'a str,
    environment: &'a str,
    promotion_plan_digest: &'a str,
    publication_report_digest: &'a str,
    plan_hash: &'a str,
    resource_count: usize,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseOutcome {
    Success,
    ApplyFailed,
    VerificationFailed,
    PostHookFailed,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RollbackOutcome {
    NotAttempted,
    RollbackSucceeded,
    RollbackFailed,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ReleaseLockEvidence {
    pub name: String,
    pub namespace: String,
    pub holder_identity: String,
    pub ownership_lost: bool,
    pub released: bool,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ReleaseExecutionEvidence {
    pub schema_version: String,
    pub profile: String,
    pub environment: String,
    pub release_id: String,
    pub bundle_schema: String,
    pub plan_hash: String,
    pub promotion_plan_digest: String,
    pub publication_report_digest: String,
    pub approval: ApprovalMode,
    pub approval_verified: bool,
    pub release_lock: Option<ReleaseLockEvidence>,
    pub outcome: ReleaseOutcome,
    pub rollout: Vec<crate::deployment::rollout::RolloutResult>,
    pub applied_resources: usize,
    pub rollback_attempted: bool,
    pub rollback_outcome: RollbackOutcome,
    pub rollback_succeeded: bool,
    pub rollback_errors: Vec<String>,
    pub errors: Vec<String>,
}

pub async fn prepare(args: WorkflowPrepareArgs) -> Result<(), String> {
    if args.out.exists() {
        return Err(format!(
            "preparation output '{}' already exists; refusing to overwrite it",
            args.out.display()
        ));
    }

    let config = WorkflowConfig::load().map_err(|error| error.to_string())?;
    let profile = config
        .get_profile(&args.profile)
        .ok_or_else(|| format!("Workflow profile '{}' not found", args.profile))?;
    validate_release_profile(profile)?;
    let normalized = profile.normalize(true);
    let environment = Environment::load_from_file(&normalized.environment).map_err(|error| {
        format!(
            "failed to load environment '{}': {error}",
            normalized.environment
        )
    })?;
    validate_approval_policy(&normalized, &environment)?;
    if environment.services.iter().any(|service| {
        service
            .hooks
            .as_ref()
            .and_then(|hooks| hooks.pre_deploy.as_ref())
            .is_some()
    }) {
        return Err(
            "portable workflow preparation rejects pre-deployment hooks; move them before prepare"
                .to_string(),
        );
    }

    for warning in external_service_template_warnings(&environment) {
        eprintln!("Warning: {warning}");
    }

    let promotion = super::promotion::load(&args.promotion_plan)?;
    promotion.validate_for_environment(&environment)?;
    validate_service_image_templates(&environment)?;
    let overrides = promotion.image_overrides();
    let deployment_date = promotion
        .services
        .iter()
        .map(|service| service.published_at.as_str())
        .max()
        .map(str::to_string);
    crate::generate_with_context(
        &environment.name,
        &environment,
        environment.list_services(),
        &crate::GenerationContext {
            service_images: overrides.clone(),
            deployment_date,
            default_namespace: normalized.namespace.clone(),
        },
    )
    .map_err(|error| format!("release manifest generation failed: {error}"))?;
    validate_rendered_images(&environment, &overrides)?;

    let context = normalized
        .deploy_context
        .clone()
        .ok_or_else(|| "release profile requires deploy_context".to_string())?;
    let namespace = normalized
        .namespace
        .clone()
        .unwrap_or_else(|| "default".to_string());
    let runtime = crate::deployment::bundle::build_deployment_bundle(
        &crate::workflow::gate::generated_manifest_root(&environment.name),
        &normalized.name,
        &environment.name,
        &context,
        &namespace,
    )
    .map_err(|error| error.to_string())?;
    let signer_key_fingerprint = match normalized.approval {
        ApprovalMode::Signature => Some(
            crate::workflow::gate::trusted_key_fingerprint(
                &normalized
                    .signature
                    .as_ref()
                    .ok_or_else(|| "signature approval requires trusted_public_key".to_string())?
                    .trusted_public_key,
            )
            .map_err(|error| error.to_string())?,
        ),
        _ => None,
    };
    let portable = PortableDeploymentBundle::from_runtime(
        &runtime,
        normalized.approval,
        signer_key_fingerprint,
        &promotion,
        bound_post_hooks(&environment),
    )
    .map_err(|error| error.to_string())?;

    std::fs::create_dir_all(&args.out)
        .map_err(|error| format!("failed to create '{}': {error}", args.out.display()))?;
    let result = write_preparation_artifacts(&args.out, &portable);
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&args.out);
    }
    result?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": "sailr.deployment-preparation/v1",
            "bundle": args.out.join("deployment.bundle"),
            "plan_hash": portable.plan_hash,
        }))
        .map_err(|error| error.to_string())?
    );
    Ok(())
}

pub async fn apply(args: WorkflowApplyArgs) -> Result<(), String> {
    if !args.non_interactive || !args.apply {
        return Err("workflow apply requires --non-interactive and --apply".to_string());
    }
    let portable = crate::deployment::bundle::read_portable(&args.bundle)
        .map_err(|error| error.to_string())?;
    if portable.payload.profile != args.profile {
        return Err(format!(
            "bundle profile '{}' does not match requested profile '{}'",
            portable.payload.profile, args.profile
        ));
    }
    let config = WorkflowConfig::load().map_err(|error| error.to_string())?;
    let profile = config
        .get_profile(&args.profile)
        .ok_or_else(|| format!("Workflow profile '{}' not found", args.profile))?;
    validate_release_profile(profile)?;
    let normalized = profile.normalize(true);
    let environment = Environment::load_from_file(&normalized.environment).map_err(|error| {
        format!(
            "failed to load environment '{}': {error}",
            normalized.environment
        )
    })?;
    validate_approval_policy(&normalized, &environment)?;
    validate_bundle_binding(&portable, &normalized, &environment)?;
    let runtime = portable.to_runtime().map_err(|error| error.to_string())?;

    let approval_verified = match normalized.approval {
        ApprovalMode::External => true,
        ApprovalMode::Signature => {
            let signature =
                std::env::var(crate::workflow::gate::APPROVAL_SIGNATURE_ENV).map_err(|_| {
                    format!("missing {}", crate::workflow::gate::APPROVAL_SIGNATURE_ENV)
                })?;
            let key = &normalized
                .signature
                .as_ref()
                .ok_or_else(|| "signature approval requires trusted_public_key".to_string())?
                .trusted_public_key;
            crate::workflow::gate::verify_plan_hash_signature(&portable.plan_hash, key, &signature)
                .map_err(|error| error.to_string())?;
            true
        }
        _ => {
            return Err(
                "portable workflow apply requires external or signature approval".to_string(),
            )
        }
    };

    let release_id = args.release_id.unwrap_or_else(|| {
        format!(
            "{}-{}-{}",
            &portable.plan_hash[..12],
            chrono::Utc::now().format("%Y%m%d%H%M%S"),
            std::process::id()
        )
    });
    validate_release_id(&release_id)?;

    let context = runtime.target.context.clone();
    let client = crate::deployment::k8sm8::create_client(context.clone())
        .await
        .map_err(|error| error.to_string())?;
    let backend = crate::deployment::KubernetesDeploymentBackend::new(context)
        .await
        .map_err(|error| error.to_string())?;
    let lock_policy = environment
        .deployment_policy
        .release_lock
        .clone()
        .unwrap_or_default();
    let release_lock = crate::deployment::lease::ReleaseLease::acquire(
        client,
        &environment.name,
        &runtime.target,
        &release_id,
        &lock_policy,
    )
    .await
    .map_err(|error| error.to_string())?;
    let lock_name = release_lock.name().to_string();
    let lock_namespace = release_lock.namespace().to_string();
    let checked_backend = crate::deployment::lease::LeaseCheckedBackend {
        inner: &backend,
        lease: &release_lock,
    };
    let journal = crate::deployment::new_deployment_journal();
    let mut outcome = ReleaseOutcome::Success;
    let mut rollout = Vec::new();
    let mut errors = Vec::new();

    if let Err(error) =
        crate::deployment::apply_bundle(&runtime, &checked_backend, journal.clone()).await
    {
        outcome = ReleaseOutcome::ApplyFailed;
        errors.push(error.to_string());
        rollback_with_timeout(&journal, &checked_backend, profile.rollback.timeout_seconds).await;
    } else {
        match crate::deployment::rollout::verify_bundle_rollout(
            &runtime,
            &checked_backend,
            Duration::from_secs(profile.verification.rollout_timeout_seconds),
        )
        .await
        {
            Ok(results) => rollout = results,
            Err(error) => {
                outcome = ReleaseOutcome::VerificationFailed;
                errors.push(error.to_string());
                rollback_with_timeout(&journal, &checked_backend, profile.rollback.timeout_seconds)
                    .await;
            }
        }
        if errors.is_empty() {
            if let Err(error) =
                execute_bound_post_hooks(&portable.payload.post_deploy_hooks, &release_lock).await
            {
                outcome = ReleaseOutcome::PostHookFailed;
                errors.push(error);
                rollback_with_timeout(&journal, &checked_backend, profile.rollback.timeout_seconds)
                    .await;
            }
        }
    }

    let snapshot = match journal.lock() {
        Ok(state) => state.clone(),
        Err(_) => {
            errors.push("deployment journal lock is poisoned".to_string());
            crate::deployment::DeploymentJournal::default()
        }
    };
    let ownership_lost = release_lock.ensure_held().is_err();
    let release_result = release_lock.release().await;
    let released = release_result.is_ok();
    if let Err(error) = release_result {
        errors.push(error.to_string());
    }
    let rollback_outcome = if !snapshot.rollback_attempted {
        RollbackOutcome::NotAttempted
    } else if snapshot.rollback_errors.is_empty() {
        RollbackOutcome::RollbackSucceeded
    } else {
        RollbackOutcome::RollbackFailed
    };
    let evidence = ReleaseExecutionEvidence {
        schema_version: "sailr.release-report/v1".to_string(),
        profile: args.profile.clone(),
        environment: environment.name.clone(),
        release_id: release_id.clone(),
        bundle_schema: portable.payload.schema_version.clone(),
        plan_hash: portable.plan_hash.clone(),
        promotion_plan_digest: portable.payload.promotion_plan_digest.clone(),
        publication_report_digest: portable.payload.publication_report_digest.clone(),
        approval: normalized.approval,
        approval_verified,
        release_lock: Some(ReleaseLockEvidence {
            name: lock_name,
            namespace: lock_namespace,
            holder_identity: release_id,
            ownership_lost,
            released,
        }),
        outcome,
        rollout,
        applied_resources: snapshot.entries.len(),
        rollback_attempted: snapshot.rollback_attempted,
        rollback_outcome,
        rollback_succeeded: snapshot.rollback_attempted && snapshot.rollback_errors.is_empty(),
        rollback_errors: snapshot.rollback_errors,
        errors: errors.clone(),
    };
    write_release_evidence(&args.bundle, &evidence)?;
    if errors.is_empty() {
        println!(
            "{}",
            serde_json::to_string_pretty(&evidence).map_err(|error| error.to_string())?
        );
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

pub(crate) fn validate_release_profile(
    profile: &crate::workflow::profile::WorkflowProfile,
) -> Result<(), String> {
    let normalized = profile.normalize(true);
    if normalized.build != WorkflowStepMode::Disabled
        || normalized.push != WorkflowStepMode::Disabled
        || normalized.generate != WorkflowStepMode::Run
        || normalized.deploy != WorkflowStepMode::Run
        || !normalized.apply
        || normalized.interactive
        || normalized.engine != crate::workflow::profile::WorkflowEngine::Runkernel
        || !matches!(
            normalized.approval,
            ApprovalMode::External | ApprovalMode::Signature
        )
    {
        return Err(
            "portable release profiles require engine=runkernel, build/push=disabled, generate/deploy=run, apply=true, interactive=false, and approval=external|signature"
                .to_string(),
        );
    }
    if normalized
        .deploy_context
        .as_deref()
        .is_none_or(|context| context.trim().is_empty())
        || normalized
            .namespace
            .as_deref()
            .is_some_and(|namespace| namespace.trim().is_empty())
    {
        return Err(
            "portable release profiles require a nonblank deploy_context; a namespace override cannot be blank"
                .to_string(),
        );
    }
    if profile.verification.rollout_timeout_seconds == 0 || profile.rollback.timeout_seconds == 0 {
        return Err("rollout and rollback timeouts must be positive".to_string());
    }
    if normalized.approval == ApprovalMode::Signature {
        crate::workflow::gate::trusted_key_fingerprint(
            &normalized
                .signature
                .as_ref()
                .ok_or_else(|| "signature approval requires trusted_public_key".to_string())?
                .trusted_public_key,
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) fn validate_approval_policy(
    profile: &crate::workflow::profile::NormalizedWorkflowProfile,
    environment: &Environment,
) -> Result<(), String> {
    match environment.deployment_policy.required_approval {
        Some(RequiredDeploymentApproval::Signature)
            if profile.approval != ApprovalMode::Signature =>
        {
            Err("environment requires signature deployment approval".to_string())
        }
        Some(RequiredDeploymentApproval::External)
            if !matches!(
                profile.approval,
                ApprovalMode::External | ApprovalMode::Signature
            ) =>
        {
            Err("environment requires external or signature deployment approval".to_string())
        }
        _ => Ok(()),
    }
}

fn validate_bundle_binding(
    bundle: &PortableDeploymentBundle,
    profile: &crate::workflow::profile::NormalizedWorkflowProfile,
    environment: &Environment,
) -> Result<(), String> {
    let context = profile.deploy_context.as_deref().unwrap_or_default();
    let namespace = profile.namespace.as_deref().unwrap_or("default");
    if bundle.payload.profile != profile.name
        || bundle.payload.environment != profile.environment
        || environment.name != profile.environment
        || bundle.payload.target.context != context
        || bundle.payload.target.namespace != namespace
        || bundle.payload.approval != profile.approval
    {
        return Err(
            "prepared bundle no longer matches profile/environment/target binding".to_string(),
        );
    }
    let expected = environment
        .services
        .iter()
        .filter(|service| service.is_release_artifact())
        .map(|service| service.name.as_str())
        .collect::<BTreeSet<_>>();
    let actual = bundle
        .payload
        .services
        .iter()
        .map(|service| service.service.as_str())
        .collect::<BTreeSet<_>>();
    if expected != actual {
        return Err(
            "prepared bundle service coverage no longer matches the environment".to_string(),
        );
    }
    let registry = environment
        .registry
        .resolve()
        .map_err(|error| format!("invalid current target registry: {error}"))?;
    for service in &bundle.payload.services {
        let expected_image = registry
            .digest_ref(&service.service, &service.digest)
            .map_err(|error| error.to_string())?;
        if service.image_ref != expected_image {
            return Err(format!(
                "prepared image for '{}' no longer matches current target repository policy",
                service.service
            ));
        }
    }
    if profile.approval == ApprovalMode::Signature {
        let current = crate::workflow::gate::trusted_key_fingerprint(
            &profile
                .signature
                .as_ref()
                .ok_or_else(|| "signature configuration is missing".to_string())?
                .trusted_public_key,
        )
        .map_err(|error| error.to_string())?;
        if bundle.payload.signer_key_fingerprint.as_deref() != Some(current.as_str()) {
            return Err("prepared bundle signer does not match current trusted signer".to_string());
        }
    }
    Ok(())
}

pub(crate) fn validate_service_image_templates(environment: &Environment) -> Result<(), String> {
    for service in environment
        .services
        .iter()
        .filter(|service| service.is_release_artifact())
    {
        let root = Path::new("k8s/templates").join(service.get_path());
        let mut workload_templates = Vec::new();
        let mut bound = false;
        if root.exists() {
            for entry in walkdir::WalkDir::new(&root).into_iter().flatten() {
                if !entry.file_type().is_file() {
                    continue;
                }
                let contents = std::fs::read_to_string(entry.path()).map_err(|error| {
                    format!(
                        "failed to inspect release template '{}': {error}",
                        entry.path().display()
                    )
                })?;
                let (has_image_workload, uses_service_image) = workload_image_binding(&contents);
                if has_image_workload {
                    workload_templates.push(entry.path().display().to_string());
                    bound |= uses_service_image;
                }
            }
        }
        if !workload_templates.is_empty() && !bound {
            return Err(format!(
                "release service '{}' has image-bearing workload templates but none bind {{{{service_image}}}}: {}",
                service.name,
                workload_templates.join(", ")
            ));
        }
    }
    Ok(())
}

fn workload_image_binding(contents: &str) -> (bool, bool) {
    workload_image_variable_binding(contents, "service_image")
}

fn workload_image_variable_binding(contents: &str, variable: &str) -> (bool, bool) {
    let mut has_image_workload = false;
    let mut uses_service_image = false;
    let mut document_kind_is_workload = false;
    let mut document_has_image = false;
    let mut document_uses_service_image = false;

    let finish_document = |has_image_workload: &mut bool,
                           uses_service_image: &mut bool,
                           kind_is_workload: bool,
                           has_image: bool,
                           binds_image: bool| {
        if kind_is_workload && has_image {
            *has_image_workload = true;
            *uses_service_image |= binds_image;
        }
    };

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            finish_document(
                &mut has_image_workload,
                &mut uses_service_image,
                document_kind_is_workload,
                document_has_image,
                document_uses_service_image,
            );
            document_kind_is_workload = false;
            document_has_image = false;
            document_uses_service_image = false;
            continue;
        }
        if line.len() == line.trim_start().len() {
            if let Some(kind) = trimmed.strip_prefix("kind:").map(str::trim) {
                document_kind_is_workload = matches!(
                    kind,
                    "Pod"
                        | "Deployment"
                        | "StatefulSet"
                        | "DaemonSet"
                        | "ReplicaSet"
                        | "ReplicationController"
                        | "Job"
                        | "CronJob"
                );
            }
        }
        let field = trimmed.strip_prefix("- ").unwrap_or(trimmed);
        if field.starts_with("image:") {
            document_has_image = true;
            document_uses_service_image |=
                crate::utils::contains_template_variable(field, variable);
        }
    }
    finish_document(
        &mut has_image_workload,
        &mut uses_service_image,
        document_kind_is_workload,
        document_has_image,
        document_uses_service_image,
    );
    (has_image_workload, uses_service_image)
}

pub(crate) fn external_service_template_warnings(environment: &Environment) -> Vec<String> {
    let mut warnings = Vec::new();
    for service in environment
        .services
        .iter()
        .filter(|service| !service.is_release_artifact())
    {
        let root = Path::new("k8s/templates").join(service.get_path());
        if !root.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&root).into_iter().flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(entry.path()) else {
                continue;
            };
            let (has_image_workload, uses_service_version) =
                workload_image_variable_binding(&contents, "service_version");
            if has_image_workload && !uses_service_version {
                warnings.push(format!(
                    "external service '{}' has an image-bearing workload that does not use {{{{service_version}}}}: {}",
                    service.name,
                    entry.path().display()
                ));
            }
        }
    }
    warnings
}

fn validate_rendered_images(
    environment: &Environment,
    overrides: &std::collections::BTreeMap<String, String>,
) -> Result<(), String> {
    let root = crate::workflow::gate::generated_manifest_root(&environment.name);
    let mut rendered = String::new();
    for entry in walkdir::WalkDir::new(root).into_iter().flatten() {
        if entry.file_type().is_file() {
            rendered.push_str(
                &std::fs::read_to_string(entry.path())
                    .map_err(|error| format!("failed to inspect rendered manifest: {error}"))?,
            );
            rendered.push('\n');
        }
    }
    for (service, image_ref) in overrides {
        if !rendered.contains(image_ref) {
            return Err(format!(
                "rendered manifests do not contain promoted image '{}' for service '{}'",
                image_ref, service
            ));
        }
    }
    Ok(())
}

fn bound_post_hooks(environment: &Environment) -> Vec<BoundPostDeployHook> {
    let mut hooks = Vec::new();
    for service in &environment.services {
        if let Some(commands) = service
            .hooks
            .as_ref()
            .and_then(|service_hooks| service_hooks.post_deploy.as_ref())
        {
            for command in commands.as_vec() {
                let command = command
                    .replace("{{name}}", &service.name)
                    .replace("{{ name }}", &service.name)
                    .replace("{{version}}", &service.version)
                    .replace("{{ version }}", &service.version)
                    .replace("{{namespace}}", service.namespace_or(&environment.name))
                    .replace("{{ namespace }}", service.namespace_or(&environment.name));
                hooks.push(BoundPostDeployHook {
                    service: service.name.clone(),
                    command,
                });
            }
        }
    }
    hooks
}

fn write_preparation_artifacts(
    out: &Path,
    bundle: &PortableDeploymentBundle,
) -> Result<(), String> {
    let bundle_bytes = serde_json::to_vec_pretty(bundle).map_err(|error| error.to_string())?;
    std::fs::write(out.join("deployment.bundle"), bundle_bytes)
        .map_err(|error| error.to_string())?;
    let plan = serde_json::json!({
        "schema_version": "sailr.deployment-plan/v1",
        "profile": bundle.payload.profile,
        "environment": bundle.payload.environment,
        "target": bundle.payload.target,
        "promotion_plan_digest": bundle.payload.promotion_plan_digest,
        "publication_report_digest": bundle.payload.publication_report_digest,
        "resources": bundle.payload.resources.iter().map(|resource| serde_json::json!({
            "identity": resource.identity,
            "source_path": resource.source_path,
            "document_index": resource.document_index,
            "sha256": resource.sha256,
        })).collect::<Vec<_>>(),
        "plan_hash": bundle.plan_hash,
    });
    std::fs::write(
        out.join("deployment-plan.json"),
        serde_json::to_vec_pretty(&plan).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let mut diff = String::new();
    for resource in &bundle.payload.resources {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&resource.canonical_json_base64)
            .map_err(|error| error.to_string())?;
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        diff.push_str(&format!(
            "--- /dev/null\n+++ {}#{}\n",
            resource.source_path, resource.document_index
        ));
        for line in serde_json::to_string_pretty(&value)
            .map_err(|error| error.to_string())?
            .lines()
        {
            diff.push('+');
            diff.push_str(line);
            diff.push('\n');
        }
    }
    std::fs::write(out.join("deployment.diff"), diff).map_err(|error| error.to_string())?;
    let evidence = PreparationEvidence {
        schema_version: "sailr.deployment-preparation/v1",
        profile: &bundle.payload.profile,
        environment: &bundle.payload.environment,
        promotion_plan_digest: &bundle.payload.promotion_plan_digest,
        publication_report_digest: &bundle.payload.publication_report_digest,
        plan_hash: &bundle.plan_hash,
        resource_count: bundle.payload.resources.len(),
    };
    std::fs::write(
        out.join("preparation-evidence.json"),
        serde_json::to_vec_pretty(&evidence).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn rollback_with_timeout(
    journal: &crate::deployment::SharedDeploymentJournal,
    backend: &dyn crate::deployment::DeploymentBackend,
    timeout_seconds: u64,
) {
    let _ = crate::deployment::rollback_transaction_with_timeout(
        journal,
        backend,
        Duration::from_secs(timeout_seconds),
    )
    .await;
}

async fn execute_bound_post_hooks(
    hooks: &[BoundPostDeployHook],
    release_lock: &crate::deployment::lease::ReleaseLease,
) -> Result<(), String> {
    for hook in hooks {
        release_lock
            .ensure_held()
            .map_err(|error| error.to_string())?;
        let command = hook.command.clone();
        let service = hook.service.clone();
        let output = tokio::task::spawn_blocking(move || {
            std::process::Command::new("sh")
                .arg("-c")
                .arg(command)
                .output()
        })
        .await
        .map_err(|error| format!("post-deploy hook join failure for '{service}': {error}"))?
        .map_err(|error| format!("post-deploy hook launch failure for '{service}': {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "post-deploy hook failed for '{}': {}",
                hook.service,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        release_lock
            .ensure_held()
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn write_release_evidence(
    bundle_path: &Path,
    evidence: &ReleaseExecutionEvidence,
) -> Result<(), String> {
    let bundle_parent = bundle_path.parent().unwrap_or_else(|| Path::new("."));
    let failed = !evidence.errors.is_empty();
    let task = crate::workflow::runner::WorkflowReportTaskItem {
        name: crate::workflow::task_id::DEPLOY.to_string(),
        kind: crate::workflow::plan::WorkflowTaskKind::Deploy,
        service: None,
        phase: Some("release_apply".to_string()),
        status: if failed {
            crate::workflow::runner::WorkflowReportTaskStatus::Failed
        } else {
            crate::workflow::runner::WorkflowReportTaskStatus::Completed
        },
        error: failed.then(|| evidence.errors.join("; ")),
    };
    let report = crate::workflow::runner::WorkflowReport {
        schema_version: "sailr.workflow-report/v1".to_string(),
        report_type: crate::workflow::runner::WorkflowReportType::WorkflowExecution,
        profile: evidence.profile.clone(),
        mode: "deploy".to_string(),
        runner: crate::workflow::runner::RunnerContext::detect(true),
        environment: evidence.environment.clone(),
        approval: Some(evidence.approval),
        success: !failed,
        effects: crate::workflow::plan::WorkflowEffects {
            mutates_cluster: true,
            ..Default::default()
        },
        tasks: crate::workflow::runner::WorkflowReportTasks {
            completed: usize::from(!failed),
            failed: usize::from(failed),
            skipped: 0,
            cancelled: 0,
            cached: 0,
            rolled_back: 0,
            rollback_failed: usize::from(!evidence.rollback_errors.is_empty()),
            items: vec![task],
        },
        finalizers: crate::workflow::runner::WorkflowReportFinalizers::default(),
        plans: crate::workflow::runner::WorkflowReportPlans {
            image_push: None,
            deployment: None,
        },
        artifacts: crate::workflow::runner::WorkflowReportArtifacts {
            published_images: Vec::new(),
            deployment: None,
            release: Some(evidence.clone()),
        },
    };
    report.validate().map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?;
    std::fs::write(bundle_parent.join("release-report.json"), &bytes)
        .map_err(|error| error.to_string())?;
    let standard = PathBuf::from(".sailr")
        .join("reports")
        .join(&evidence.profile);
    std::fs::create_dir_all(&standard).map_err(|error| error.to_string())?;
    std::fs::write(standard.join("latest.json"), bytes).map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn validate_release_id(release_id: &str) -> Result<(), String> {
    if release_id.trim().is_empty()
        || release_id.len() > 253
        || release_id.chars().any(char::is_whitespace)
    {
        return Err(
            "release ID must be nonblank, at most 253 characters, and contain no whitespace"
                .to_string(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod template_binding_tests {
    use super::workload_image_binding;

    #[test]
    fn image_binding_only_applies_to_workload_documents() {
        let config_map = r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: settings
data:
  image: fixed-value
"#;
        assert_eq!(workload_image_binding(config_map), (false, false));

        let fixed_workload = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: api
spec:
  template:
    spec:
      containers:
        - name: api
          image: registry.example/api:mutable
"#;
        assert_eq!(workload_image_binding(fixed_workload), (true, false));
    }

    #[test]
    fn image_binding_accepts_whitespace_inside_template_variables() {
        let workload = r#"
apiVersion: batch/v1
kind: CronJob
metadata:
  name: worker
spec:
  jobTemplate:
    spec:
      template:
        spec:
          containers:
            - name: worker
              image: "{{ service_image }}"
"#;
        assert_eq!(workload_image_binding(workload), (true, true));
        assert_eq!(
            super::workload_image_variable_binding(workload, "service_version"),
            (true, false)
        );

        let external = workload.replace("{{ service_image }}", "emqx/nanomq:{{service_version}}");
        assert_eq!(
            super::workload_image_variable_binding(&external, "service_version"),
            (true, true)
        );
    }
}
