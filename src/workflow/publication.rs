use crate::environment::Environment;
use crate::workflow::runner::{WorkflowReport, WorkflowReportType};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct PublicationValidationResult {
    pub schema_version: String,
    pub valid: bool,
    pub report_digest: String,
    pub environment: String,
    pub services: Vec<String>,
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
