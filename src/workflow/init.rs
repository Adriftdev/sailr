use crate::cli::{WorkflowInitApproval, WorkflowInitArgs, WorkflowInitPreset};
use crate::environment::{Environment, RequiredDeploymentApproval};
use std::io::Write;
use std::path::Path;
use toml_edit::{value, DocumentMut, Item, Table};

pub fn run(args: WorkflowInitArgs) -> Result<(), String> {
    validate_profile_name(&args.profile)?;
    validate_environment_name(&args.environment)?;
    let environment = Environment::load_from_file(&args.environment)
        .map_err(|error| format!("failed to load environment '{}': {error}", args.environment))?;
    if environment.name != args.environment {
        return Err(format!(
            "environment config resolved name '{}' instead of '{}'",
            environment.name, args.environment
        ));
    }
    if matches!(
        args.preset,
        WorkflowInitPreset::Deploy | WorkflowInitPreset::PortableRelease
    ) {
        emit_template_warnings(&environment);
    }

    let trusted_public_key = validate_options(&args, &environment)?;
    let profile = build_profile_table(&args, trusted_public_key.as_deref())?;
    let existing = if args.config.exists() {
        std::fs::read_to_string(&args.config).map_err(|error| {
            format!(
                "failed to read workflow config '{}': {error}",
                args.config.display()
            )
        })?
    } else {
        String::new()
    };
    let rendered = insert_profile(&existing, &args.profile, profile)?;
    let generated_config = crate::workflow::config::WorkflowConfig::parse(&rendered)
        .map_err(|error| format!("generated workflow configuration is invalid: {error}"))?;
    if matches!(args.preset, WorkflowInitPreset::Publication) {
        let profile = generated_config
            .get_profile(&args.profile)
            .ok_or_else(|| format!("generated workflow profile '{}' is missing", args.profile))?;
        crate::workflow::publication::validate_publication_profile(profile)?;
    } else if matches!(args.preset, WorkflowInitPreset::PortableRelease) {
        let profile = generated_config
            .get_profile(&args.profile)
            .ok_or_else(|| format!("generated workflow profile '{}' is missing", args.profile))?;
        crate::workflow::release::validate_release_profile(profile)?;
        crate::workflow::release::validate_approval_policy(&profile.normalize(true), &environment)?;
    }

    if args.print {
        print!("{rendered}");
        return Ok(());
    }

    atomic_write(&args.config, rendered.as_bytes())?;
    println!(
        "Created workflow profile '{}' for environment '{}' in {}",
        args.profile,
        args.environment,
        args.config.display()
    );
    if matches!(args.preset, WorkflowInitPreset::Publication) {
        println!(
            "Next: sailr publication run {} --apply --out artifacts/publication-report.json",
            args.profile
        );
    }
    Ok(())
}

fn emit_template_warnings(environment: &Environment) {
    for warning in crate::workflow::release::external_service_template_warnings(environment) {
        eprintln!("Warning: {warning}");
    }
}

fn validate_options(
    args: &WorkflowInitArgs,
    environment: &Environment,
) -> Result<Option<String>, String> {
    if matches!(args.preset, WorkflowInitPreset::Publication)
        && !environment
            .services
            .iter()
            .any(|service| service.build.is_some())
    {
        return Err(format!(
            "environment '{}' has no build-backed services to publish; add [service.build] to each service Sailr should build and push",
            environment.name
        ));
    }
    if matches!(
        args.preset,
        WorkflowInitPreset::Build | WorkflowInitPreset::Publication
    ) && (args.context.is_some() || args.namespace.is_some())
    {
        return Err(format!(
            "the {} preset does not accept --context or --namespace",
            match args.preset {
                WorkflowInitPreset::Build => "build",
                WorkflowInitPreset::Publication => "publication",
                _ => unreachable!(),
            }
        ));
    }
    if !matches!(args.preset, WorkflowInitPreset::PortableRelease)
        && (args.approval.is_some() || args.trusted_public_key_file.is_some())
    {
        return Err(
            "--approval and --trusted-public-key-file require --preset portable-release"
                .to_string(),
        );
    }
    if matches!(args.preset, WorkflowInitPreset::PortableRelease) {
        if args
            .context
            .as_deref()
            .is_none_or(|context| context.trim().is_empty())
        {
            return Err("the portable-release preset requires --context".to_string());
        }
        if environment.services.iter().any(|service| {
            service
                .hooks
                .as_ref()
                .and_then(|hooks| hooks.pre_deploy.as_ref())
                .is_some()
        }) {
            return Err(
                "portable releases reject pre-deployment hooks; run migrations in a separate CI stage"
                    .to_string(),
            );
        }
    }

    let approval = args.approval.unwrap_or(WorkflowInitApproval::External);
    if matches!(args.preset, WorkflowInitPreset::PortableRelease)
        && environment.deployment_policy.required_approval
            == Some(RequiredDeploymentApproval::Signature)
        && approval != WorkflowInitApproval::Signature
    {
        return Err(format!(
            "environment '{}' requires signature approval; pass --approval signature and --trusted-public-key-file",
            environment.name
        ));
    }
    match (approval, &args.trusted_public_key_file) {
        (WorkflowInitApproval::Signature, Some(path)) => {
            let key = std::fs::read_to_string(path).map_err(|error| {
                format!("failed to read trusted public key '{}': {error}", path.display())
            })?;
            let key = key.trim().to_string();
            crate::workflow::gate::trusted_key_fingerprint(&key)
                .map_err(|error| format!("invalid trusted public key: {error}"))?;
            Ok(Some(key))
        }
        (WorkflowInitApproval::Signature, None) => Err(
            "signature approval requires --trusted-public-key-file; Sailr never generates or stores private keys"
                .to_string(),
        ),
        (WorkflowInitApproval::External, Some(_)) => Err(
            "--trusted-public-key-file is only valid with --approval signature".to_string(),
        ),
        (WorkflowInitApproval::External, None) => Ok(None),
    }
}

fn build_profile_table(
    args: &WorkflowInitArgs,
    trusted_public_key: Option<&str>,
) -> Result<Table, String> {
    let mut profile = Table::new();
    profile.insert("environment", value(&args.environment));
    profile.insert("engine", value("runkernel"));
    profile.insert("interactive", value(false));

    match args.preset {
        WorkflowInitPreset::Build => {
            profile.insert("mode", value("build"));
            profile.insert("build", value("run"));
            profile.insert("push", value("disabled"));
            profile.insert("generate", value("disabled"));
            profile.insert("deploy", value("disabled"));
            profile.insert("approval", value("none"));
            profile.insert("apply", value(false));
        }
        WorkflowInitPreset::Publication => {
            profile.insert("mode", value("build"));
            profile.insert("build", value("run"));
            profile.insert("push", value("run"));
            profile.insert("generate", value("disabled"));
            profile.insert("deploy", value("disabled"));
            profile.insert("approval", value("none"));
            profile.insert("apply", value(false));
        }
        WorkflowInitPreset::Deploy => {
            profile.insert("mode", value("deploy"));
            profile.insert("build", value("disabled"));
            profile.insert("push", value("disabled"));
            profile.insert("generate", value("run"));
            profile.insert("deploy", value("plan"));
            insert_target(&mut profile, args);
            profile.insert("approval", value("none"));
            profile.insert("apply", value(false));
        }
        WorkflowInitPreset::PortableRelease => {
            profile.insert("mode", value("deploy"));
            profile.insert("build", value("disabled"));
            profile.insert("push", value("disabled"));
            profile.insert("generate", value("run"));
            profile.insert("deploy", value("run"));
            insert_target(&mut profile, args);
            let approval = args.approval.unwrap_or(WorkflowInitApproval::External);
            profile.insert(
                "approval",
                value(match approval {
                    WorkflowInitApproval::External => "external",
                    WorkflowInitApproval::Signature => "signature",
                }),
            );
            profile.insert("apply", value(true));
            if let Some(key) = trusted_public_key {
                let mut signature = Table::new();
                signature.insert("trusted_public_key", value(key));
                profile.insert("signature", Item::Table(signature));
            }
            let mut verification = Table::new();
            verification.insert("rollout_timeout_seconds", value(300));
            profile.insert("verification", Item::Table(verification));
            let mut rollback = Table::new();
            rollback.insert("timeout_seconds", value(300));
            profile.insert("rollback", Item::Table(rollback));
        }
    }
    profile.insert(
        "report",
        value(if matches!(args.preset, WorkflowInitPreset::Publication) {
            "json"
        } else {
            "both"
        }),
    );
    Ok(profile)
}

fn insert_target(profile: &mut Table, args: &WorkflowInitArgs) {
    if let Some(context) = &args.context {
        profile.insert("deploy_context", value(context));
    }
    if let Some(namespace) = &args.namespace {
        profile.insert("namespace", value(namespace));
    } else if matches!(args.preset, WorkflowInitPreset::PortableRelease) {
        profile.insert("namespace", value("default"));
    }
}

fn insert_profile(contents: &str, name: &str, profile: Table) -> Result<String, String> {
    let mut document = if contents.trim().is_empty() {
        DocumentMut::new()
    } else {
        contents
            .parse::<DocumentMut>()
            .map_err(|error| format!("failed to parse workflow config: {error}"))?
    };
    if !document.as_table().contains_key("workflow") {
        document["workflow"] = Item::Table(Table::new());
    }
    let workflows = document["workflow"]
        .as_table_mut()
        .ok_or_else(|| "top-level 'workflow' must be a table".to_string())?;
    if workflows.contains_key(name) {
        return Err(format!(
            "workflow profile '{name}' already exists; refusing to overwrite it"
        ));
    }
    workflows.insert(name, Item::Table(profile));
    Ok(document.to_string())
}

fn validate_profile_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 128
        || name.starts_with('-')
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(
            "profile name must be 1-128 safe characters using letters, digits, '.', '-', or '_'"
                .to_string(),
        );
    }
    Ok(())
}

fn validate_environment_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.starts_with('-')
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err("environment must be a safe environment name".to_string());
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create '{}': {error}", parent.display()))?;
    }
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid workflow config path '{}'", path.display()))?;
    let temporary = path.with_file_name(format!(".{filename}.{}.tmp", std::process::id()));
    let existing_permissions = std::fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions());
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        let mut file = options.open(&temporary).map_err(|error| {
            format!(
                "failed to create temporary workflow config '{}': {error}",
                temporary.display()
            )
        })?;
        if let Some(permissions) = existing_permissions {
            file.set_permissions(permissions).map_err(|error| {
                format!(
                    "failed to preserve workflow config permissions '{}': {error}",
                    temporary.display()
                )
            })?;
        }
        file.write_all(bytes).map_err(|error| {
            format!(
                "failed to write temporary workflow config '{}': {error}",
                temporary.display()
            )
        })?;
        file.sync_all().map_err(|error| {
            format!(
                "failed to sync temporary workflow config '{}': {error}",
                temporary.display()
            )
        })?;
        std::fs::rename(&temporary, path).map_err(|error| {
            format!(
                "failed to install workflow config '{}': {error}",
                path.display()
            )
        })
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::Service;
    use std::path::PathBuf;

    fn args(preset: WorkflowInitPreset) -> WorkflowInitArgs {
        WorkflowInitArgs {
            profile: "release-prod".to_string(),
            environment: "prod".to_string(),
            preset,
            context: Some("prod-cluster".to_string()),
            namespace: Some("production".to_string()),
            approval: None,
            trusted_public_key_file: None,
            print: false,
            config: PathBuf::from("sailr.workflow.toml"),
        }
    }

    #[test]
    fn portable_profile_has_release_contract_defaults() {
        let args = args(WorkflowInitPreset::PortableRelease);
        let table = build_profile_table(&args, None).expect("portable profile");
        let rendered = insert_profile("", &args.profile, table).expect("insert profile");
        let parsed = crate::workflow::config::WorkflowConfig::parse(&rendered)
            .expect("parse generated config");
        let profile = parsed
            .get_profile("release-prod")
            .expect("generated profile");
        crate::workflow::release::validate_release_profile(profile)
            .expect("valid portable profile");
        assert_eq!(profile.environment, "prod");
        assert_eq!(profile.namespace.as_deref(), Some("production"));
        assert_eq!(profile.verification.rollout_timeout_seconds, 300);
        assert_eq!(profile.rollback.timeout_seconds, 300);
    }

    #[test]
    fn publication_profile_has_safe_registry_only_defaults() {
        let mut args = args(WorkflowInitPreset::Publication);
        args.context = None;
        args.namespace = None;
        let table = build_profile_table(&args, None).expect("publication profile");
        let rendered = insert_profile("", &args.profile, table).expect("insert profile");
        let parsed = crate::workflow::config::WorkflowConfig::parse(&rendered)
            .expect("parse generated config");
        let profile = parsed
            .get_profile("release-prod")
            .expect("generated profile");
        crate::workflow::publication::validate_publication_profile(profile)
            .expect("valid publication profile");
        let normalized = profile.normalize(true);
        assert_eq!(
            normalized.push,
            crate::workflow::profile::WorkflowStepMode::Run
        );
        assert_eq!(
            normalized.deploy,
            crate::workflow::profile::WorkflowStepMode::Disabled
        );
        assert!(!normalized.apply);
        assert_eq!(
            normalized.report,
            crate::workflow::profile::ReportMode::Json
        );
    }

    #[test]
    fn publication_initialization_rejects_an_environment_with_nothing_to_publish() {
        let mut args = args(WorkflowInitPreset::Publication);
        args.context = None;
        args.namespace = None;
        let environment = Environment::new("prod");
        let error = validate_options(&args, &environment).expect_err("no build services");
        assert!(error.contains("no build-backed services"));
    }

    #[test]
    fn insertion_preserves_unrelated_content_and_refuses_collisions() {
        let args = args(WorkflowInitPreset::Deploy);
        let existing = "# retained\n[workflow.existing]\nenvironment = \"dev\"\nmode = \"check\"\n";
        let rendered = insert_profile(
            existing,
            &args.profile,
            build_profile_table(&args, None).expect("profile"),
        )
        .expect("insert profile");
        assert!(rendered.contains("# retained"));
        assert!(rendered.contains("[workflow.existing]"));
        crate::workflow::config::WorkflowConfig::parse(&rendered)
            .expect("complete generated config");
        assert!(insert_profile(
            &rendered,
            &args.profile,
            build_profile_table(&args, None).expect("profile")
        )
        .is_err());
    }

    #[test]
    fn non_release_presets_disable_kubernetes_mutation() {
        for preset in [
            WorkflowInitPreset::Build,
            WorkflowInitPreset::Publication,
            WorkflowInitPreset::Deploy,
        ] {
            let mut args = args(preset);
            if matches!(
                preset,
                WorkflowInitPreset::Build | WorkflowInitPreset::Publication
            ) {
                args.context = None;
                args.namespace = None;
            }
            let table = build_profile_table(&args, None).expect("profile");
            let rendered = insert_profile("", &args.profile, table).expect("insert profile");
            let parsed = crate::workflow::config::WorkflowConfig::parse(&rendered)
                .expect("parse generated config");
            let normalized = parsed
                .get_profile("release-prod")
                .expect("profile")
                .normalize(false);
            assert!(!normalized.apply);
            assert_ne!(
                normalized.deploy,
                crate::workflow::profile::WorkflowStepMode::Run
            );
        }
    }

    #[test]
    fn portable_initialization_does_not_require_every_service_to_have_a_workload() {
        let args = args(WorkflowInitPreset::PortableRelease);
        let mut environment = Environment::new("prod");
        environment.services = vec![Service::new("configuration-only", None, "v1")];
        assert!(validate_options(&args, &environment).is_ok());
    }
}
