use crate::environment::Environment;
use crate::workflow::config::WorkflowConfig;
use crate::workflow::profile::{
    ApprovalMode, ReportMode, WorkflowEngine, WorkflowMode, WorkflowProfile, WorkflowStepMode,
};
use crate::workflow::runner::{WorkflowReport, WorkflowReportType};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct PublicationValidationResult {
    pub schema_version: String,
    pub valid: bool,
    pub report_digest: String,
    pub environment: String,
    pub services: Vec<String>,
}

/// Validate the deliberately narrow profile contract used to create publication reports.
/// Registry mutation is authorized per invocation with `--apply`; this profile can never
/// mutate Kubernetes resources.
pub fn validate_publication_profile(profile: &WorkflowProfile) -> Result<(), String> {
    let normalized = profile.normalize(true);
    if normalized.mode != WorkflowMode::Build
        || normalized.engine != WorkflowEngine::Runkernel
        || normalized.interactive
        || normalized.build != WorkflowStepMode::Run
        || normalized.push != WorkflowStepMode::Run
        || normalized.deploy != WorkflowStepMode::Disabled
        || normalized.approval != ApprovalMode::None
        || normalized.apply
        || !matches!(normalized.report, ReportMode::Json | ReportMode::Both)
    {
        return Err(format!(
            "publication profile '{}' requires mode=build, runkernel, non-interactive build/push=run, deploy=disabled, approval=none, apply=false, and JSON reporting",
            profile.name
        ));
    }
    Ok(())
}

pub async fn run(args: crate::cli::PublicationRunArgs) -> Result<(), String> {
    if !args.apply {
        return Err(
            "publication pushes images; rerun with --apply to consent to registry mutation"
                .to_string(),
        );
    }

    // Fail before building or pushing when the requested profile is not a publication profile.
    let config = WorkflowConfig::load().map_err(|error| error.to_string())?;
    let profile = config
        .get_profile(&args.profile)
        .ok_or_else(|| format!("Workflow profile '{}' not found", args.profile))?;
    validate_publication_profile(profile)?;

    crate::workflow::runner::WorkflowRunner::run(crate::cli::WorkflowRunArgs {
        profile: args.profile.clone(),
        only: args.only,
        ignore: args.ignore,
        non_interactive: true,
        plan: false,
        dry_run: false,
        apply: true,
        release_id: None,
    })
    .await?;

    let generated = PathBuf::from(".sailr")
        .join("reports")
        .join(&args.profile)
        .join("latest.json");
    let (_, result) = load_and_validate(&generated)?;
    let output = match args.out {
        Some(output) if output != generated => {
            atomic_copy(&generated, &output)?;
            output
        }
        Some(output) => output,
        None => generated,
    };

    eprintln!("Validated publication report: {}", output.display());
    println!(
        "{}",
        serde_json::to_string_pretty(&result)
            .map_err(|error| format!("failed to serialize validation result: {error}"))?
    );
    Ok(())
}

fn atomic_copy(source: &Path, destination: &Path) -> Result<(), String> {
    let bytes = std::fs::read(source).map_err(|error| {
        format!(
            "failed to read validated publication report '{}': {error}",
            source.display()
        )
    })?;
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    if let Some(parent) = parent {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create publication report directory '{}': {error}",
                parent.display()
            )
        })?;
    }
    let file_name = destination.file_name().ok_or_else(|| {
        format!(
            "publication report output '{}' is invalid",
            destination.display()
        )
    })?;
    let temporary = destination.with_file_name(format!(
        ".{}.tmp-{}",
        file_name.to_string_lossy(),
        std::process::id()
    ));
    let result = (|| -> Result<(), String> {
        let mut file = std::fs::File::create(&temporary).map_err(|error| {
            format!(
                "failed to create temporary publication report '{}': {error}",
                temporary.display()
            )
        })?;
        file.write_all(&bytes).map_err(|error| {
            format!(
                "failed to write temporary publication report '{}': {error}",
                temporary.display()
            )
        })?;
        file.sync_all().map_err(|error| {
            format!(
                "failed to sync temporary publication report '{}': {error}",
                temporary.display()
            )
        })?;
        std::fs::rename(&temporary, destination).map_err(|error| {
            format!(
                "failed to install publication report '{}': {error}",
                destination.display()
            )
        })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

pub fn canonical_report_digest(report: &WorkflowReport) -> Result<String, String> {
    let bytes = serde_json::to_vec(report)
        .map_err(|error| format!("failed to canonicalize publication report: {error}"))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}

pub fn load_and_validate(
    path: &Path,
) -> Result<(WorkflowReport, PublicationValidationResult), String> {
    let bytes = std::fs::read(path).map_err(|error| {
        format!(
            "failed to read publication report '{}': {error}",
            path.display()
        )
    })?;
    let report: WorkflowReport = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "failed to parse publication report '{}': {error}",
            path.display()
        )
    })?;

    report.validate().map_err(|error| error.to_string())?;
    if report.report_type != WorkflowReportType::WorkflowExecution {
        return Err("publication report must describe a workflow execution".to_string());
    }
    if !report.success {
        return Err("publication report must be successful".to_string());
    }
    if !report.effects.mutates_registry {
        return Err("publication report does not record a registry mutation".to_string());
    }
    let push_plan = report
        .plans
        .image_push
        .as_ref()
        .ok_or_else(|| "publication report is missing an image push plan".to_string())?;
    if !push_plan.mutates_registry {
        return Err("publication report does not describe a registry mutation".to_string());
    }
    if report.artifacts.published_images.is_empty() {
        return Err("publication report contains no published images".to_string());
    }

    let environment = Environment::load_from_file(&report.environment).map_err(|error| {
        format!(
            "failed to load publication environment '{}': {error}",
            report.environment
        )
    })?;
    let known = environment
        .services
        .iter()
        .map(|service| service.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut services = BTreeSet::new();
    for artifact in &report.artifacts.published_images {
        artifact.validate().map_err(|error| error.to_string())?;
        if artifact
            .provenance
            .source_revision
            .as_deref()
            .is_none_or(|revision| revision.trim().is_empty())
        {
            return Err(format!(
                "published service '{}' is missing source_revision",
                artifact.service
            ));
        }
        if !known.contains(artifact.service.as_str()) {
            return Err(format!(
                "publication report contains unknown service '{}' for environment '{}'",
                artifact.service, report.environment
            ));
        }
        if !services.insert(artifact.service.clone()) {
            return Err(format!(
                "publication report contains duplicate service '{}'",
                artifact.service
            ));
        }
    }

    let report_digest = canonical_report_digest(&report)?;
    Ok((
        report,
        PublicationValidationResult {
            schema_version: "sailr.publication-validation/v1".to_string(),
            valid: true,
            report_digest,
            environment: environment.name,
            services: services.into_iter().collect(),
        },
    ))
}

pub fn validate_to_stdout(path: &Path) -> Result<(), String> {
    match load_and_validate(path) {
        Ok((_, result)) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&result)
                    .map_err(|error| format!("failed to serialize validation result: {error}"))?
            );
            Ok(())
        }
        Err(error) => {
            let value = serde_json::json!({
                "schema_version": "sailr.publication-validation/v1",
                "valid": false,
                "errors": [error],
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&value)
                    .map_err(|serialization| serialization.to_string())?
            );
            Err(error)
        }
    }
}
