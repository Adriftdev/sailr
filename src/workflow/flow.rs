use crate::environment::Environment;
use crate::workflow::config::WorkflowConfig;
use crate::workflow::profile::{ApprovalMode, ReportMode, WorkflowEngine, WorkflowStepMode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FlowProvider {
    Circleci,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DeliveryFlowKind {
    Publication,
    Release,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FlowTriggerKind {
    Branch,
    Schedule,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FlowTrigger {
    pub kind: FlowTriggerKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cron: Option<String>,
    #[serde(default = "default_branch")]
    pub branch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

fn default_branch() -> String {
    "main".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CandidateAdapter {
    pub fetch_script: String,
    pub manifest_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SigningAdapter {
    pub script: String,
    pub signature_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "source", rename_all = "kebab-case", deny_unknown_fields)]
pub enum FlowToolchain {
    Release { version: String, sha256: String },
    Git { revision: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum PublicationStorageAdapter {
    CircleciArtifact,
    Script { script: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FlowStageAction {
    Publish,
    Prepare,
    Sign,
    Apply,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FlowStageApproval {
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeliveryFlowStage {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<FlowStageAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval: Option<FlowStageApproval>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeliveryFlowProfile {
    #[serde(skip)]
    pub name: String,
    pub kind: DeliveryFlowKind,
    pub provider: FlowProvider,
    pub environment: String,
    pub concurrency_key: String,
    pub trigger: FlowTrigger,
    pub toolchain: FlowToolchain,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate: Option<CandidateAdapter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<PublicationStorageAdapter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signing: Option<SigningAdapter>,
    #[serde(default, rename = "stage")]
    pub stages: Vec<DeliveryFlowStage>,
}

#[derive(Serialize)]
pub struct InspectResult {
    pub workflow_config_exists: bool,
    pub workflow_profiles: Vec<String>,
    pub environments: HashMap<String, EnvironmentInfo>,
    pub ci_providers: Vec<String>,
    pub gitops_present: bool,
    pub gitops_tool: Option<String>,
    pub delivery_flows: Vec<String>,
}

#[derive(Serialize)]
pub struct EnvironmentInfo {
    pub config_exists: bool,
    pub services: Vec<String>,
}

#[derive(Serialize)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
}

#[derive(Serialize)]
pub struct CheckResult {
    pub passed: bool,
    pub findings: Vec<String>,
}

pub fn inspect() -> Result<InspectResult, Box<dyn std::error::Error>> {
    let workflow_config_exists = Path::new("sailr.workflow.toml").exists();
    let workflow_profiles = if workflow_config_exists {
        match WorkflowConfig::load() {
            Ok(config) => config.workflow.keys().cloned().collect(),
            Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };

    let mut environments = HashMap::new();
    let environments_dir = Path::new("k8s/environments");
    if environments_dir.exists() && environments_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(environments_dir) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let config_path = entry.path().join("config.toml");
                    let config_exists = config_path.exists();
                    let services = if config_exists {
                        match Environment::load_from_file(&name) {
                            Ok(env) => env.services.iter().map(|s| s.name.clone()).collect(),
                            Err(_) => Vec::new(),
                        }
                    } else {
                        Vec::new()
                    };
                    environments.insert(
                        name,
                        EnvironmentInfo {
                            config_exists,
                            services,
                        },
                    );
                }
            }
        }
    }

    let mut ci_providers = Vec::new();
    if Path::new(".circleci/config.yml").exists() {
        ci_providers.push("circleci".to_string());
    }
    if Path::new(".github/workflows").exists() {
        ci_providers.push("github".to_string());
    }

    let mut gitops_present = false;
    let mut gitops_tool = None;
    if Path::new("argocd").exists() || Path::new("k8s/argocd").exists() {
        gitops_present = true;
        gitops_tool = Some("argocd".to_string());
    } else if Path::new("flux").exists() || Path::new("k8s/flux").exists() {
        gitops_present = true;
        gitops_tool = Some("flux".to_string());
    }

    Ok(InspectResult {
        workflow_config_exists,
        workflow_profiles,
        environments,
        ci_providers,
        gitops_present,
        gitops_tool,
        delivery_flows: if workflow_config_exists {
            WorkflowConfig::load()
                .map(|config| {
                    let mut names = config.flow.keys().cloned().collect::<Vec<_>>();
                    names.sort();
                    names
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        },
    })
}

pub fn validate() -> Result<ValidationResult, Box<dyn std::error::Error>> {
    let mut errors = Vec::new();

    // 1. Workflow Config
    let workflow_config_exists = Path::new("sailr.workflow.toml").exists();
    if workflow_config_exists {
        match WorkflowConfig::load() {
            Ok(config) => {
                for (name, profile) in &config.workflow {
                    let env_dir = Path::new("k8s/environments").join(&profile.environment);
                    if !env_dir.exists() {
                        errors.push(format!(
                            "Workflow profile '{}' references non-existent environment '{}'",
                            name, profile.environment
                        ));
                    }
                }
                for (name, flow) in &config.flow {
                    if let Err(error) = validate_delivery_flow(name, flow, &config) {
                        errors.push(error);
                    }
                }
            }
            Err(e) => {
                errors.push(format!("Failed to parse sailr.workflow.toml: {}", e));
            }
        }
    } else {
        errors.push("sailr.workflow.toml does not exist".to_string());
    }

    // 2. Environment config files
    let environments_dir = Path::new("k8s/environments");
    if environments_dir.exists() && environments_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(environments_dir) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    match Environment::load_from_file(&name) {
                        Ok(env) => {
                            for service in env.services {
                                if service.has_explicit_version() && service.version == "latest" {
                                    errors.push(format!(
                                        "Environment '{}' service '{}' uses mutable tag 'latest'",
                                        name, service.name
                                    ));
                                }
                                if service.has_explicit_version()
                                    && service.version.trim().is_empty()
                                {
                                    errors.push(format!(
                                        "Environment '{}' service '{}' has an empty version",
                                        name, service.name
                                    ));
                                }
                            }
                        }
                        Err(e) => {
                            errors.push(format!(
                                "Environment '{}' has invalid config.toml: {}",
                                name, e
                            ));
                        }
                    }
                }
            }
        }
    }

    // 3. YAML syntax validation
    let mut validate_yaml_dir = |dir_path: &Path| {
        if dir_path.exists() && dir_path.is_dir() {
            for entry in walkdir::WalkDir::new(dir_path).into_iter().flatten() {
                if entry.file_type().is_file() {
                    let ext = entry
                        .path()
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("");
                    if ext == "yaml" || ext == "yml" {
                        if let Ok(contents) = fs::read_to_string(entry.path()) {
                            if let Err(e) = serde_yaml::from_str::<serde_json::Value>(&contents) {
                                errors.push(format!(
                                    "YAML syntax error in '{}': {}",
                                    entry.path().display(),
                                    e
                                ));
                            }
                        }
                    }
                }
            }
        }
    };

    validate_yaml_dir(Path::new(".circleci"));
    validate_yaml_dir(Path::new(".github"));
    validate_yaml_dir(Path::new("k8s/templates"));

    let is_valid = errors.is_empty();
    Ok(ValidationResult { is_valid, errors })
}

pub fn validate_delivery_flow(
    name: &str,
    flow: &DeliveryFlowProfile,
    config: &WorkflowConfig,
) -> Result<(), String> {
    if name.trim().is_empty() || flow.environment.trim().is_empty() {
        return Err("flow name and environment cannot be blank".to_string());
    }
    validate_command_token(name, "environment", &flow.environment)?;
    if flow.concurrency_key.is_empty()
        || flow.concurrency_key.len() > 512
        || !flow.concurrency_key.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'.' | b'-' | b'_' | b'/')
        })
    {
        return Err(format!("flow '{name}' has an invalid concurrency_key"));
    }
    validate_command_token(name, "trigger branch", &flow.trigger.branch)?;
    validate_toolchain(name, &flow.toolchain)?;
    match flow.trigger.kind {
        FlowTriggerKind::Branch if flow.trigger.cron.is_some() || flow.trigger.name.is_some() => {
            return Err(format!(
                "flow '{name}' branch trigger cannot declare schedule fields"
            ));
        }
        FlowTriggerKind::Branch => {}
        FlowTriggerKind::Schedule => {
            if flow
                .trigger
                .cron
                .as_deref()
                .is_none_or(|cron| cron.split_whitespace().count() != 5)
                || flow
                    .trigger
                    .name
                    .as_deref()
                    .is_none_or(|schedule_name| schedule_name.trim().is_empty())
                || flow
                    .trigger
                    .name
                    .as_deref()
                    .is_some_and(|schedule_name| schedule_name.contains(['\r', '\n']))
                || flow.trigger.cron.as_deref().is_some_and(|cron| {
                    !cron.bytes().all(|byte| {
                        byte.is_ascii_digit() || matches!(byte, b' ' | b'*' | b',' | b'-' | b'/')
                    })
                })
            {
                return Err(format!(
                    "flow '{name}' schedule trigger requires a name and five-field cron"
                ));
            }
        }
        FlowTriggerKind::Manual if flow.trigger.cron.is_some() => {
            return Err(format!(
                "flow '{name}' manual trigger cannot declare a cron"
            ));
        }
        FlowTriggerKind::Manual => {}
    }
    if let Some(candidate) = &flow.candidate {
        validate_adapter_path(name, "candidate fetch script", &candidate.fetch_script)?;
        validate_relative_path(name, "candidate manifest path", &candidate.manifest_path)?;
        if Path::new(&candidate.manifest_path)
            .parent()
            .is_none_or(|parent| parent.as_os_str().is_empty())
        {
            return Err(format!(
                "flow '{name}' candidate manifest must have a repository-relative parent directory"
            ));
        }
    }
    if let Some(PublicationStorageAdapter::Script { script }) = &flow.storage {
        validate_adapter_path(name, "publication storage script", script)?;
    }
    if let Some(signing) = &flow.signing {
        validate_adapter_path(name, "signing script", &signing.script)?;
        validate_relative_path(name, "signature path", &signing.signature_path)?;
    }

    let mut shape = Vec::new();
    let mut stage_names = std::collections::BTreeSet::new();
    let mut action_profile: Option<&str> = None;
    for stage in &flow.stages {
        if stage.name.trim().is_empty() {
            return Err(format!("flow '{name}' contains a blank stage name"));
        }
        if !stage_names.insert(stage.name.as_str()) {
            return Err(format!(
                "flow '{name}' contains duplicate stage name '{}'",
                stage.name
            ));
        }
        match (&stage.action, &stage.approval) {
            (Some(action), None) => {
                shape.push(match action {
                    FlowStageAction::Publish => "publish",
                    FlowStageAction::Prepare => "prepare",
                    FlowStageAction::Sign => "sign",
                    FlowStageAction::Apply => "apply",
                });
                if matches!(
                    action,
                    FlowStageAction::Publish | FlowStageAction::Prepare | FlowStageAction::Apply
                ) {
                    let profile_name = stage.profile.as_deref().ok_or_else(|| {
                        format!("flow '{name}' stage '{}' requires profile", stage.name)
                    })?;
                    validate_command_token(name, "profile", profile_name)?;
                    let profile = config.workflow.get(profile_name).ok_or_else(|| {
                        format!("flow '{name}' references unknown profile '{profile_name}'")
                    })?;
                    if profile.environment != flow.environment {
                        return Err(format!(
                            "flow '{name}' profile '{profile_name}' targets '{}' instead of '{}'",
                            profile.environment, flow.environment
                        ));
                    }
                    if let Some(existing) = action_profile {
                        if existing != profile_name {
                            return Err(format!(
                                "flow '{name}' action stages must use the same profile"
                            ));
                        }
                    } else {
                        action_profile = Some(profile_name);
                    }
                }
            }
            (None, Some(FlowStageApproval::Manual)) => shape.push("approval"),
            _ => {
                return Err(format!(
                    "flow '{name}' stage '{}' must define exactly one action or approval",
                    stage.name
                ))
            }
        }
    }
    let profile_name =
        action_profile.ok_or_else(|| format!("flow '{name}' has no action profile"))?;
    let profile = config
        .workflow
        .get(profile_name)
        .ok_or_else(|| format!("flow '{name}' references unknown profile '{profile_name}'"))?;
    if flow.kind == DeliveryFlowKind::Publication {
        if flow.trigger.kind != FlowTriggerKind::Branch || shape != ["publish"] {
            return Err(format!(
                "flow '{name}' publication stages must be branch -> publish"
            ));
        }
        if flow.candidate.is_some() || flow.signing.is_some() || flow.storage.is_none() {
            return Err(format!("flow '{name}' publication requires storage and cannot declare candidate or signing adapters"));
        }
        let normalized = profile.normalize(true);
        if normalized.engine != WorkflowEngine::Runkernel
            || normalized.interactive
            || normalized.build != WorkflowStepMode::Run
            || normalized.push != WorkflowStepMode::Run
            || normalized.deploy != WorkflowStepMode::Disabled
            || normalized.approval != ApprovalMode::None
            || normalized.apply
            || !matches!(normalized.report, ReportMode::Json | ReportMode::Both)
        {
            return Err(format!(
                "flow '{name}' publication profile requires runkernel, non-interactive build/push=run, deploy=disabled, approval=none, apply=false, and JSON reporting"
            ));
        }
        return Ok(());
    }

    let valid_shape = shape == ["prepare", "approval", "apply"]
        || shape == ["prepare", "approval", "sign", "apply"];
    if flow.trigger.kind == FlowTriggerKind::Branch || !valid_shape {
        return Err(format!(
            "flow '{name}' release stages must be schedule/manual -> prepare -> manual approval -> [sign] -> apply"
        ));
    }
    if flow.candidate.is_none() || flow.storage.is_some() {
        return Err(format!("flow '{name}' release requires a candidate adapter and cannot declare publication storage"));
    }
    crate::workflow::release::validate_release_profile(profile)
        .map_err(|error| format!("flow '{name}' profile '{profile_name}': {error}"))?;
    let environment = Environment::load_from_file(&flow.environment)
        .map_err(|error| format!("flow '{name}' cannot load environment policy: {error}"))?;
    let normalized = profile.normalize(true);
    crate::workflow::release::validate_approval_policy(&normalized, &environment)
        .map_err(|error| format!("flow '{name}': {error}"))?;
    crate::workflow::release::validate_service_image_templates(&environment)
        .map_err(|error| format!("flow '{name}': {error}"))?;
    if environment.services.iter().any(|service| {
        service
            .hooks
            .as_ref()
            .and_then(|hooks| hooks.pre_deploy.as_ref())
            .is_some()
    }) {
        return Err(format!(
            "flow '{name}' portable preparation rejects pre-deployment hooks"
        ));
    }
    let lock = environment
        .deployment_policy
        .release_lock
        .clone()
        .unwrap_or_default();
    if lock.lease_duration_seconds == 0
        || lock.lease_duration_seconds > i32::MAX as u32
        || lock.renew_interval_seconds == 0
        || lock.renew_interval_seconds >= u64::from(lock.lease_duration_seconds)
    {
        return Err(format!("flow '{name}' has an invalid release lock policy"));
    }
    let needs_signature =
        profile.approval == Some(crate::workflow::profile::ApprovalMode::Signature);
    if needs_signature != shape.contains(&"sign") || needs_signature != flow.signing.is_some() {
        return Err(format!(
            "flow '{name}' signing stage/adapter must exactly match approval=signature"
        ));
    }
    Ok(())
}

fn validate_toolchain(flow: &str, toolchain: &FlowToolchain) -> Result<(), String> {
    match toolchain {
        FlowToolchain::Release { version, sha256 } => {
            let core = version.split('-').next().unwrap_or_default();
            let semantic_core = core.split('.').collect::<Vec<_>>();
            if semantic_core.len() != 3
                || semantic_core
                    .iter()
                    .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
                || version.starts_with('-')
                || version.contains(['*', '^', '~', '<', '>', '='])
                || matches!(version.to_ascii_lowercase().as_str(), "latest" | "current")
                || !version
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
            {
                return Err(format!(
                    "flow '{flow}' has an invalid immutable Sailr version"
                ));
            }
            validate_sha256_identity(sha256)
                .map_err(|error| format!("flow '{flow}' toolchain: {error}"))
        }
        FlowToolchain::Git { revision } => {
            if revision.len() != 40
                || !revision
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(format!(
                    "flow '{flow}' Git toolchain requires a full lowercase commit revision"
                ));
            }
            Ok(())
        }
    }
}

fn validate_sha256_identity(value: &str) -> Result<(), String> {
    let digest = value
        .strip_prefix("sha256:")
        .ok_or_else(|| "checksum must use sha256:<hex>".to_string())?;
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("checksum must use sha256:<64 lowercase hex>".to_string());
    }
    Ok(())
}

fn validate_adapter_path(flow: &str, label: &str, value: &str) -> Result<(), String> {
    validate_relative_path(flow, label, value)?;
    let path = Path::new(value);
    if !path.is_file() {
        return Err(format!("flow '{flow}' {label} '{}' is not a file", value));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::metadata(path)
            .map_err(|error| error.to_string())?
            .permissions()
            .mode()
            & 0o111
            == 0
        {
            return Err(format!(
                "flow '{flow}' {label} '{}' is not executable",
                value
            ));
        }
    }
    Ok(())
}

fn validate_relative_path(flow: &str, label: &str, value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'/'))
    {
        return Err(format!(
            "flow '{flow}' {label} must be a safe repository-relative path"
        ));
    }
    Ok(())
}

fn validate_command_token(flow: &str, label: &str, value: &str) -> Result<(), String> {
    if value.starts_with('-')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'/'))
    {
        return Err(format!(
            "flow '{flow}' {label} must be safe for generated command arguments"
        ));
    }
    Ok(())
}

pub fn generate_ci(
    requested_flow: Option<&str>,
    mode: crate::cli::FlowGenerationMode,
    output: Option<&Path>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let config = WorkflowConfig::load()?;
    let flow_name = match requested_flow {
        Some(name) => name.to_string(),
        None if config.flow.len() == 1 => config.flow.keys().next().cloned().unwrap_or_default(),
        None => return Err("flow name is required unless exactly one flow is configured".into()),
    };
    let flow = config
        .flow
        .get(&flow_name)
        .ok_or_else(|| format!("delivery flow '{flow_name}' not found"))?;
    validate_delivery_flow(&flow_name, flow, &config)?;
    let fragment = circleci_fragment(&flow_name, flow)?;
    let full = format!("version: 2.1\n{}", fragment);
    validate_circleci_root(&full)?;
    let target = output.unwrap_or_else(|| Path::new(".circleci/config.yml"));

    match mode {
        crate::cli::FlowGenerationMode::Print => print!("{full}"),
        crate::cli::FlowGenerationMode::Fragment => print!("{fragment}"),
        crate::cli::FlowGenerationMode::Create => {
            if target.exists() {
                return Err(
                    format!("refusing to overwrite existing '{}'", target.display()).into(),
                );
            }
            atomic_write(target, full.as_bytes())?;
        }
        crate::cli::FlowGenerationMode::Merge => merge_circleci(target, &fragment)?,
    }
    Ok(serde_json::json!({
        "schema_version": "sailr.flow-generation/v1",
        "flow": flow_name,
        "provider": "circleci",
        "mode": format!("{:?}", mode).to_lowercase(),
        "output": if matches!(mode, crate::cli::FlowGenerationMode::Create | crate::cli::FlowGenerationMode::Merge) {
            Some(target.to_string_lossy().to_string())
        } else { None },
        "trigger_setup": {
            "trigger_kind": flow.trigger.kind,
            "name": flow.trigger.name.as_deref().unwrap_or(&flow.name),
            "cron": flow.trigger.cron,
            "branch": flow.trigger.branch,
            "pipeline_parameter": managed_name(&flow_name, "enabled"),
            "pipeline_parameter_value": true,
        }
    }))
}

fn managed_name(flow: &str, suffix: &str) -> String {
    let base = flow
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("sailr_{base}_{suffix}")
}

fn circleci_fragment(
    name: &str,
    flow: &DeliveryFlowProfile,
) -> Result<String, Box<dyn std::error::Error>> {
    match flow.kind {
        DeliveryFlowKind::Publication => circleci_publication_fragment(name, flow),
        DeliveryFlowKind::Release => circleci_release_fragment(name, flow),
    }
}

fn toolchain_install(toolchain: &FlowToolchain, kind: DeliveryFlowKind) -> String {
    let capability_filter = match kind {
        DeliveryFlowKind::Publication => {
            r#".schema_version == \"sailr.capabilities/v1\"
              and .features.publication_consumption
              and .features.publication_flow_generation
              and .features.circleci_generation"#
        }
        DeliveryFlowKind::Release => {
            r#".schema_version == \"sailr.capabilities/v1\"
              and (.schemas.release_candidates | index(\"sailr.release-candidates/v1\") != null)
              and .features.multi_report_promotion
              and .features.portable_deployment_bundle
              and .features.signed_deployment
              and .features.transactional_rollback
              and .features.rollout_verification
              and .features.locking
              and .features.release_flow_generation
              and .features.circleci_generation"#
        }
    };
    let install = match toolchain {
        FlowToolchain::Release { version, sha256 } => {
            let checksum = sha256.strip_prefix("sha256:").unwrap_or(sha256);
            format!(
                "mkdir -p \"$HOME/bin\"\n            curl --fail --location --silent --show-error --output /tmp/sailr https://github.com/Adriftdev/sailr/releases/download/v{version}/sailr-v{version}-unknown-linux-gnu\n            printf '%s  %s\\n' {checksum} /tmp/sailr | sha256sum --check -\n            install -m 0755 /tmp/sailr \"$HOME/bin/sailr\"\n            echo 'export PATH=\"$HOME/bin:$PATH\"' >> \"$BASH_ENV\"\n            export PATH=\"$HOME/bin:$PATH\"\n            test \"$(sailr --version)\" = \"sailr {version}\""
            )
        }
        FlowToolchain::Git { revision } => format!(
            "mkdir -p .sailr/toolchain\n            SAILR_BUILD_REVISION={revision} cargo install --git https://github.com/Adriftdev/sailr --rev {revision} --locked --root .sailr/toolchain sailr\n            echo 'export PATH=\"$PWD/.sailr/toolchain/bin:$PATH\"' >> \"$BASH_ENV\"\n            export PATH=\"$PWD/.sailr/toolchain/bin:$PATH\"\n            sailr --version\n            test \"$(sailr capabilities --format json | jq -r .build_revision)\" = \"{revision}\""
        ),
    };
    format!(
        "{install}\n            sailr capabilities --format json > /tmp/sailr-capabilities.json\n            jq -e '{capability_filter}' /tmp/sailr-capabilities.json"
    )
}

fn circleci_publication_fragment(
    name: &str,
    flow: &DeliveryFlowProfile,
) -> Result<String, Box<dyn std::error::Error>> {
    let profile = flow
        .stages
        .iter()
        .find(|stage| stage.action == Some(FlowStageAction::Publish))
        .and_then(|stage| stage.profile.as_deref())
        .ok_or("publish stage is missing")?;
    let job = managed_name(name, "publish");
    let workflow = managed_name(name, "workflow");
    let report = format!(".sailr/reports/{profile}/latest.json");
    let install = toolchain_install(&flow.toolchain, flow.kind);
    let storage = match flow.storage.as_ref().ok_or("publication storage is missing")? {
        PublicationStorageAdapter::CircleciArtifact => format!(
            "      - store_artifacts:\n          path: {report}\n          when: always\n"
        ),
        PublicationStorageAdapter::Script { script } => format!(
            "      - run:\n          name: Persist publication candidate\n          command: {script} --report {report}\n"
        ),
    };
    let fragment = format!(
        r#"# Sailr publication trigger: branch={branch}
jobs:
  {job}:
    docker:
      - image: cimg/rust:1.89.0
    steps:
      - checkout
      - run:
          name: Install pinned Sailr
          command: |
            {install}
      - run:
          name: Publish immutable application artifacts
          command: |
            sailr workflow run {profile} --non-interactive --apply
            sailr publication validate {report}
{storage}workflows:
  {workflow}:
    jobs:
      - {job}:
          serial-group: << pipeline.project.slug >>/{concurrency}
          filters:
            branches:
              only: {branch}
"#,
        branch = flow.trigger.branch,
        concurrency = flow.concurrency_key,
    );
    let _: serde_yaml::Value = serde_yaml::from_str(&fragment)?;
    Ok(fragment)
}

fn circleci_release_fragment(
    name: &str,
    flow: &DeliveryFlowProfile,
) -> Result<String, Box<dyn std::error::Error>> {
    let prepare = flow
        .stages
        .iter()
        .find(|stage| stage.action == Some(FlowStageAction::Prepare))
        .and_then(|stage| stage.profile.as_deref())
        .ok_or("prepare stage is missing")?;
    let apply = flow
        .stages
        .iter()
        .find(|stage| stage.action == Some(FlowStageAction::Apply))
        .and_then(|stage| stage.profile.as_deref())
        .ok_or("apply stage is missing")?;
    let plan_job = managed_name(name, "prepare");
    let approve_job = managed_name(name, "approve");
    let sign_job = managed_name(name, "sign");
    let apply_job = managed_name(name, "apply");
    let workflow = managed_name(name, "workflow");
    let enabled_parameter = managed_name(name, "enabled");
    let release_dir = format!(".sailr/releases/{}", managed_name(name, "artifacts"));
    let promotion_path = format!("{release_dir}/promotion-plan.json");
    let bundle_path = format!("{release_dir}/prepared/deployment.bundle");
    let install = toolchain_install(&flow.toolchain, flow.kind);
    let candidate = flow
        .candidate
        .as_ref()
        .ok_or("candidate adapter is missing")?;
    let candidate_parent = Path::new(&candidate.manifest_path)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or("candidate manifest must have a repository-relative parent directory")?
        .to_string_lossy();
    let mut jobs = format!(
        r#"jobs:
  {plan_job}:
    docker:
      - image: cimg/rust:1.89.0
    steps:
      - checkout
      - run:
          name: Install pinned Sailr
          command: |
            {install}
      - run:
          name: Retrieve release candidate
          command: {fetch} --output {manifest}
      - run:
          name: Validate and prepare release
          command: |
            sailr promote plan --from-manifest {manifest} --to {environment} --out {promotion_path}
            sailr workflow prepare {prepare} --promotion-plan {promotion_path} --out {release_dir}/prepared
      - persist_to_workspace:
          root: .
          paths:
            - {release_dir}
            - {candidate_parent}
      - store_artifacts:
          path: {release_dir}/prepared
"#,
        fetch = candidate.fetch_script,
        manifest = candidate.manifest_path,
        environment = flow.environment,
    );
    if let Some(signing) = &flow.signing {
        jobs.push_str(&format!(
            r#"  {sign_job}:
    docker:
      - image: cimg/rust:1.89.0
    steps:
      - checkout
      - attach_workspace:
          at: .
      - run:
          name: Sign approved release
          command: |
            PLAN_HASH=$(sed -n 's/.*\"plan_hash\": \"\([^\"]*\)\".*/\1/p' {release_dir}/prepared/deployment-plan.json)
            {script} --message sailr-deployment-plan-v1:$PLAN_HASH --output {signature}
      - persist_to_workspace:
          root: .
          paths:
            - {signature}
"#,
            script = signing.script,
            signature = signing.signature_path,
        ));
    }
    let signature_prefix = flow.signing.as_ref().map_or(String::new(), |signing| {
        format!(
            "DEPLOY_APPROVAL_SIG=$(tr -d '\\n' < {}) ",
            signing.signature_path
        )
    });
    jobs.push_str(&format!(
        r#"  {apply_job}:
    docker:
      - image: cimg/rust:1.89.0
    steps:
      - checkout
      - attach_workspace:
          at: .
      - run:
          name: Install pinned Sailr
          command: |
            {install}
      - run:
          name: Apply prepared release
          command: {signature_prefix}sailr workflow apply {apply} --bundle {bundle_path} --non-interactive --apply --release-id "$CIRCLE_WORKFLOW_ID"
      - store_artifacts:
          path: .sailr/reports
          when: always
      - store_artifacts:
          path: {release_dir}
          when: always
"#,
    ));
    let sign_workflow = if flow.signing.is_some() {
        format!(
            "      - {sign_job}:\n          requires:\n            - {approve_job}\n      - {apply_job}:\n          serial-group: << pipeline.project.slug >>/{key}\n          requires:\n            - {sign_job}\n",
            key = flow.concurrency_key
        )
    } else {
        format!(
            "      - {apply_job}:\n          serial-group: << pipeline.project.slug >>/{key}\n          requires:\n            - {approve_job}\n",
            key = flow.concurrency_key
        )
    };
    let trigger_metadata = match flow.trigger.kind {
        FlowTriggerKind::Schedule => format!(
            "# Sailr schedule setup: name={} cron={} branch={} parameter={}\n",
            flow.trigger.name.as_deref().unwrap_or(name),
            flow.trigger.cron.as_deref().unwrap_or_default(),
            flow.trigger.branch,
            enabled_parameter
        ),
        FlowTriggerKind::Manual => format!(
            "# Sailr manual trigger: branch={} parameter={}\n",
            flow.trigger.branch, enabled_parameter
        ),
        FlowTriggerKind::Branch => return Err("release flow cannot use a branch trigger".into()),
    };
    let fragment = format!(
        r#"{trigger_metadata}parameters:
  {enabled_parameter}:
    type: boolean
    default: false
{jobs}workflows:
  {workflow}:
    when: pipeline.parameters.{enabled_parameter}
    jobs:
      - {plan_job}
      - {approve_job}:
          type: approval
          requires:
            - {plan_job}
{sign_workflow}"#
    );
    let _: serde_yaml::Value = serde_yaml::from_str(&fragment)?;
    Ok(fragment)
}

fn merge_circleci(path: &Path, fragment: &str) -> Result<(), Box<dyn std::error::Error>> {
    if !path.exists() {
        let full = format!("version: 2.1\n{fragment}");
        validate_circleci_root(&full)?;
        return atomic_write(path, full.as_bytes());
    }
    let existing = fs::read_to_string(path)?;
    if existing.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("setup:")
            || trimmed.starts_with("<<:")
            || trimmed.contains(" &")
            || trimmed.starts_with('*')
    }) {
        return Err(
            "CircleCI merge refuses dynamic configuration, aliases, anchors, or merge keys".into(),
        );
    }
    let mut root: serde_yaml::Value = serde_yaml::from_str(&existing)?;
    validate_circleci_root_value(&root)?;
    let generated: serde_yaml::Value = serde_yaml::from_str(fragment)?;
    let root_map = root
        .as_mapping_mut()
        .ok_or("CircleCI root must be a mapping")?;
    let generated_map = generated
        .as_mapping()
        .ok_or("generated fragment must be a mapping")?;
    for section in ["jobs", "workflows", "parameters"] {
        let key = serde_yaml::Value::String(section.to_string());
        let Some(additions) = generated_map.get(&key) else {
            continue;
        };
        let additions = additions
            .as_mapping()
            .ok_or("generated section must be a mapping")?;
        if !root_map.contains_key(&key) {
            root_map.insert(key.clone(), serde_yaml::Value::Mapping(Default::default()));
        }
        let destination = root_map
            .get_mut(&key)
            .and_then(serde_yaml::Value::as_mapping_mut)
            .ok_or_else(|| format!("CircleCI {section} must be a mapping"))?;
        for (managed_key, managed_value) in additions {
            if let Some(existing_value) = destination.get(managed_key) {
                if existing_value != managed_value {
                    return Err(format!(
                        "CircleCI {section} contains a conflicting Sailr-managed name"
                    )
                    .into());
                }
            } else {
                destination.insert(managed_key.clone(), managed_value.clone());
            }
        }
    }
    let output = serde_yaml::to_string(&root)?;
    let reparsed: serde_yaml::Value = serde_yaml::from_str(&output)?;
    if reparsed != root {
        return Err("CircleCI merge round-trip validation failed".into());
    }
    validate_circleci_root_value(&reparsed)?;
    atomic_write(path, output.as_bytes())
}

fn validate_circleci_root(contents: &str) -> Result<(), Box<dyn std::error::Error>> {
    let value: serde_yaml::Value = serde_yaml::from_str(contents)?;
    validate_circleci_root_value(&value)
}

fn validate_circleci_root_value(
    value: &serde_yaml::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = value
        .as_mapping()
        .ok_or("CircleCI root must be a mapping")?;
    let version = root
        .get(serde_yaml::Value::String("version".to_string()))
        .and_then(serde_yaml::Value::as_f64)
        .ok_or("CircleCI config must declare numeric version 2.1")?;
    if (version - 2.1).abs() > f64::EPSILON {
        return Err("CircleCI config version must be 2.1".into());
    }
    for required in ["jobs", "workflows"] {
        if !root
            .get(serde_yaml::Value::String(required.to_string()))
            .is_some_and(serde_yaml::Value::is_mapping)
        {
            return Err(format!("CircleCI {required} must be a mapping").into());
        }
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("yml.tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)?;
    Ok(())
}

pub fn check_release() -> Result<CheckResult, Box<dyn std::error::Error>> {
    let mut findings = Vec::new();

    let config = WorkflowConfig::load()?;
    if config.flow.is_empty() {
        findings.push("No declarative delivery flows are configured".to_string());
    }
    for (name, flow) in &config.flow {
        if let Err(error) = validate_delivery_flow(name, flow, &config) {
            findings.push(error);
        }
    }

    let passed = findings.is_empty();
    Ok(CheckResult { passed, findings })
}

pub fn check_gitops() -> Result<CheckResult, Box<dyn std::error::Error>> {
    let mut findings = Vec::new();

    if let Ok(config) = WorkflowConfig::load() {
        for (name, profile) in config.workflow {
            let is_gitops = name.to_lowercase().contains("gitops")
                || profile.environment.to_lowercase().contains("gitops");
            if is_gitops
                && profile
                    .deploy
                    .as_ref()
                    .map(|s| s == &WorkflowStepMode::Run)
                    .unwrap_or(false)
                && profile.apply.unwrap_or(false)
            {
                findings.push(format!(
                        "GitOps profile '{}' has active cluster deploy (apply=true). GitOps must deploy via desired-state write-back, not direct cluster mutation.",
                        name
                    ));
            }
        }
    }

    let passed = findings.is_empty();
    Ok(CheckResult { passed, findings })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn circleci_release_fragment_has_approval_workspace_and_serialization() {
        let flow = DeliveryFlowProfile {
            name: "release".to_string(),
            kind: DeliveryFlowKind::Release,
            provider: FlowProvider::Circleci,
            environment: "prod".to_string(),
            concurrency_key: "prod-release".to_string(),
            trigger: FlowTrigger {
                kind: FlowTriggerKind::Schedule,
                cron: Some("0 6 * * 2".to_string()),
                branch: "main".to_string(),
                name: Some("production-release".to_string()),
            },
            toolchain: FlowToolchain::Release {
                version: "1.26.0".to_string(),
                sha256: format!("sha256:{}", "a".repeat(64)),
            },
            candidate: Some(CandidateAdapter {
                fetch_script: "scripts/fetch-candidate".to_string(),
                manifest_path: "artifacts/release-candidates.json".to_string(),
            }),
            storage: None,
            signing: None,
            stages: vec![
                DeliveryFlowStage {
                    name: "plan".to_string(),
                    profile: Some("release".to_string()),
                    action: Some(FlowStageAction::Prepare),
                    approval: None,
                },
                DeliveryFlowStage {
                    name: "approve".to_string(),
                    profile: None,
                    action: None,
                    approval: Some(FlowStageApproval::Manual),
                },
                DeliveryFlowStage {
                    name: "deploy".to_string(),
                    profile: Some("release".to_string()),
                    action: Some(FlowStageAction::Apply),
                    approval: None,
                },
            ],
        };
        let yaml = circleci_fragment("release", &flow).expect("fragment");
        assert!(yaml.contains("type: approval"));
        assert!(yaml.contains("persist_to_workspace"));
        assert!(yaml.contains("attach_workspace"));
        assert!(yaml.contains("serial-group: << pipeline.project.slug >>/prod-release"));
        assert!(yaml.contains("sailr workflow prepare release"));
        assert!(yaml.contains("sailr workflow apply release"));
        assert!(yaml.contains("sailr promote plan --from-manifest"));
        assert!(yaml.contains("sha256sum --check"));
        assert!(yaml.contains(".schemas.release_candidates"));
        assert!(yaml.contains(".features.locking"));
        assert!(!yaml.contains("sailr.dev/install.sh"));
        assert!(yaml.contains("pipeline.parameters.sailr_release_enabled"));

        let dir = tempdir().expect("tempdir");
        let config_path = dir.path().join("config.yml");
        std::fs::write(
            &config_path,
            "version: 2.1\njobs:\n  unrelated:\n    docker:\n      - image: cimg/base:current\n    steps:\n      - run: echo ok\nworkflows:\n  unrelated:\n    jobs:\n      - unrelated\n",
        )
        .expect("existing config");
        merge_circleci(&config_path, &yaml).expect("first merge");
        let first = std::fs::read(&config_path).expect("merged config");
        merge_circleci(&config_path, &yaml).expect("idempotent merge");
        assert_eq!(first, std::fs::read(&config_path).expect("merged config"));

        std::fs::write(
            &config_path,
            "version: 2.1\njobs:\n  sailr_release_prepare:\n    docker: []\n    steps: []\nworkflows: {}\n",
        )
        .expect("conflicting config");
        assert!(merge_circleci(&config_path, &yaml).is_err());
    }

    #[test]
    fn circleci_publication_uses_invocation_consent_and_typed_storage() {
        let flow = DeliveryFlowProfile {
            name: "publication".to_string(),
            kind: DeliveryFlowKind::Publication,
            provider: FlowProvider::Circleci,
            environment: "source".to_string(),
            concurrency_key: "publication".to_string(),
            trigger: FlowTrigger {
                kind: FlowTriggerKind::Branch,
                cron: None,
                branch: "main".to_string(),
                name: None,
            },
            toolchain: FlowToolchain::Git {
                revision: "a".repeat(40),
            },
            candidate: None,
            storage: Some(PublicationStorageAdapter::CircleciArtifact),
            signing: None,
            stages: vec![DeliveryFlowStage {
                name: "publish".to_string(),
                profile: Some("production-publication".to_string()),
                action: Some(FlowStageAction::Publish),
                approval: None,
            }],
        };
        let yaml = circleci_fragment("publication", &flow).expect("fragment");
        assert!(
            yaml.contains("sailr workflow run production-publication --non-interactive --apply")
        );
        assert!(yaml.contains("--rev aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        assert!(yaml.contains("store_artifacts"));
        assert!(yaml.contains("only: main"));
        assert!(yaml.contains(".features.publication_flow_generation"));
    }

    #[test]
    fn flow_types_reject_unknown_fields_and_floating_toolchains() {
        let unknown = r#"
kind = "manual"
branch = "main"
cronn = "0 0 * * *"
"#;
        assert!(toml::from_str::<FlowTrigger>(unknown).is_err());
        assert!(validate_toolchain(
            "bad",
            &FlowToolchain::Release {
                version: "latest".to_string(),
                sha256: format!("sha256:{}", "a".repeat(64)),
            },
        )
        .is_err());
        assert!(validate_toolchain(
            "bad",
            &FlowToolchain::Git {
                revision: "abc123".to_string(),
            },
        )
        .is_err());
    }

    #[test]
    fn circleci_validation_rejects_duplicate_keys() {
        let duplicate = "version: 2.1\njobs: {}\njobs: {}\nworkflows: {}\n";
        assert!(validate_circleci_root(duplicate).is_err());
    }
}
