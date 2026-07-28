use std::collections::HashMap;
use std::fs;
use std::path::Path;
use serde::Serialize;
use crate::environment::Environment;
use crate::workflow::config::WorkflowConfig;
use crate::workflow::profile::WorkflowStepMode;

#[derive(Serialize)]
pub struct InspectResult {
    pub workflow_config_exists: bool,
    pub workflow_profiles: Vec<String>,
    pub environments: HashMap<String, EnvironmentInfo>,
    pub ci_providers: Vec<String>,
    pub gitops_present: bool,
    pub gitops_tool: Option<String>,
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
                    environments.insert(name, EnvironmentInfo { config_exists, services });
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
    })
}

pub fn validate() -> Result<ValidationResult, Box<dyn std::error::Error>> {
    let mut errors = Vec::new();

    // 1. Workflow Config
    let workflow_config_exists = Path::new("sailr.workflow.toml").exists();
    if workflow_config_exists {
        match WorkflowConfig::load() {
            Ok(config) => {
                for (name, profile) in config.workflow {
                    let env_dir = Path::new("k8s/environments").join(&profile.environment);
                    if !env_dir.exists() {
                        errors.push(format!(
                            "Workflow profile '{}' references non-existent environment '{}'",
                            name, profile.environment
                        ));
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
                                if service.version == "latest" {
                                    errors.push(format!(
                                        "Environment '{}' service '{}' uses mutable tag 'latest'",
                                        name, service.name
                                    ));
                                }
                                if service.version.trim().is_empty() {
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
                    let ext = entry.path().extension().and_then(|s| s.to_str()).unwrap_or("");
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

pub fn generate_ci_merge() -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(".circleci/config.yml");
    let mut config_value: serde_yaml::Value = if path.exists() {
        let content = fs::read_to_string(path)?;
        serde_yaml::from_str(&content).unwrap_or_else(|_| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()))
    } else {
        serde_yaml::Value::Mapping(serde_yaml::Mapping::new())
    };

    let mapping = config_value.as_mapping_mut().ok_or("Invalid CircleCI config root")?;

    // 1. Version
    if !mapping.contains_key("version") {
        mapping.insert(serde_yaml::Value::String("version".to_string()), serde_yaml::Value::String("2.1".to_string()));
    }

    // 2. Jobs
    let jobs_key = serde_yaml::Value::String("jobs".to_string());
    if !mapping.contains_key(&jobs_key) {
        mapping.insert(jobs_key.clone(), serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    }
    let jobs_map = mapping.get_mut(&jobs_key).unwrap().as_mapping_mut().ok_or("CircleCI jobs must be a mapping")?;

    let job_name = "sailr-workflow";
    let has_job = jobs_map.contains_key(job_name);
    if !has_job {
        let job_yaml = r#"
docker:
  - image: cimg/base:current
steps:
  - checkout
  - run:
      name: Install Sailr
      command: curl -sSL https://sailr.dev/install.sh | bash
  - run:
      name: Run Workflow
      command: sailr workflow run ci
"#;
        let job_val: serde_yaml::Value = serde_yaml::from_str(job_yaml)?;
        jobs_map.insert(serde_yaml::Value::String(job_name.to_string()), job_val);
    }

    // 3. Workflows
    let wf_key = serde_yaml::Value::String("workflows".to_string());
    if !mapping.contains_key(&wf_key) {
        mapping.insert(wf_key.clone(), serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    }
    let wf_map = mapping.get_mut(&wf_key).unwrap().as_mapping_mut().ok_or("CircleCI workflows must be a mapping")?;

    let pipeline_name = "sailr-pipeline";
    if !wf_map.contains_key(pipeline_name) {
        let wf_yaml = r#"
jobs:
  - sailr-workflow
"#;
        let wf_val: serde_yaml::Value = serde_yaml::from_str(wf_yaml)?;
        wf_map.insert(serde_yaml::Value::String(pipeline_name.to_string()), wf_val);
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let output = serde_yaml::to_string(&config_value)?;
    fs::write(path, output)?;
    Ok(())
}

pub fn check_release() -> Result<CheckResult, Box<dyn std::error::Error>> {
    let mut findings = Vec::new();

    if let Ok(config) = WorkflowConfig::load() {
        for (name, profile) in config.workflow {
            let is_prod = profile.environment.to_lowercase().contains("prod") || 
                           profile.name.to_lowercase().contains("prod") ||
                           profile.name.to_lowercase().contains("release");

            if is_prod {
                if profile.build.as_ref().map(|s| s == &WorkflowStepMode::Run).unwrap_or(false) ||
                   profile.push.as_ref().map(|s| s == &WorkflowStepMode::Run).unwrap_or(false) {
                    findings.push(format!(
                        "Production profile '{}' has active build/push steps; production should deploy pre-built immutable artifacts.",
                        name
                    ));
                }

                if profile.deploy.as_ref().map(|s| s == &WorkflowStepMode::Run).unwrap_or(false) {
                    if profile.approval == crate::workflow::profile::ApprovalMode::None {
                        findings.push(format!(
                            "Production deploy profile '{}' has approval set to 'none'. Production requires explicit approval (external or signature).",
                            name
                        ));
                    }
                }
            }
        }
    }

    let environments_dir = Path::new("k8s/environments");
    if environments_dir.exists() && environments_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(environments_dir) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    let env_name = entry.file_name().to_string_lossy().into_owned();
                    let is_prod = env_name.to_lowercase().contains("prod");
                    if is_prod {
                        if let Ok(env) = Environment::load_from_file(&env_name) {
                            for service in env.services {
                                let is_digest = service.version.starts_with("sha256:") || service.version.contains("@sha256:");
                                if !is_digest {
                                    findings.push(format!(
                                        "Production environment '{}' service '{}' does not use an immutable image digest (tag: '{}').",
                                        env_name, service.name, service.version
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let passed = findings.is_empty();
    Ok(CheckResult { passed, findings })
}

pub fn check_gitops() -> Result<CheckResult, Box<dyn std::error::Error>> {
    let mut findings = Vec::new();

    if let Ok(config) = WorkflowConfig::load() {
        for (name, profile) in config.workflow {
            let is_gitops = name.to_lowercase().contains("gitops") || profile.environment.to_lowercase().contains("gitops");
            if is_gitops {
                if profile.deploy.as_ref().map(|s| s == &WorkflowStepMode::Run).unwrap_or(false) && profile.apply.unwrap_or(false) {
                    findings.push(format!(
                        "GitOps profile '{}' has active cluster deploy (apply=true). GitOps must deploy via desired-state write-back, not direct cluster mutation.",
                        name
                    ));
                }
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
    fn test_inspect_and_validate_with_temp_files() {
        let dir = tempdir().unwrap();
        let old_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        // 1. Initially both should fail or report false/missing
        let insp = inspect().unwrap();
        assert!(!insp.workflow_config_exists);
        assert!(insp.workflow_profiles.is_empty());

        let val = validate().unwrap();
        assert!(!val.is_valid);
        assert!(val.errors.iter().any(|e| e.contains("sailr.workflow.toml does not exist")));

        // Restore cwd
        std::env::set_current_dir(old_cwd).unwrap();
    }
}

