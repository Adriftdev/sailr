use crate::builder::SailrBuildPlan;
use crate::workflow::profile::NormalizedWorkflowProfile;
use crate::workflow::runner::RunnerContext;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct WorkflowPlan {
    pub profile: NormalizedWorkflowProfile,
    pub runner: RunnerContext,
    pub tasks: Vec<WorkflowTaskPlan>,
    pub edges: Vec<WorkflowEdge>,
    pub build_plan: Option<SailrBuildPlan>,
    pub push_plan: Option<crate::workflow::image::ImagePushPlan>,
    pub effects: WorkflowEffects,
}

#[derive(Debug, Clone)]
pub struct WorkflowTaskPlan {
    pub id: String,
    pub label: String,
    pub kind: WorkflowTaskKind,
    pub dependencies: Vec<String>,
    pub effects: WorkflowEffects,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowTaskKind {
    ValidateConfig,
    BuildPlan,
    ServiceBuild,
    PushPlan,
    Generate,
    DeploymentPlan,
    Deploy,
    Verify,
    Approval,
}

#[derive(Debug, Clone)]
pub struct WorkflowEdge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Default)]
pub struct WorkflowEffects {
    pub mutates_filesystem: bool,
    pub mutates_docker: bool,
    pub mutates_registry: bool,
    pub mutates_cluster: bool,
    pub prompts_user: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DeploymentPlanMode {
    Static,
    LiveDiff,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowDeploymentPlan {
    pub environment: String,
    pub context: String,
    pub namespace: String,
    pub mode: DeploymentPlanMode,
    pub resources: Vec<DeploymentResourcePlan>,
    pub summary: DeploymentPlanSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeploymentResourcePlan {
    pub kind: String,
    pub name: String,
    pub namespace: Option<String>,
    pub source_path: String,
    pub action: DeploymentPlanAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentPlanAction {
    WouldApply,
    WouldCreate,
    WouldUpdate,
    Unknown,
}

impl std::fmt::Display for DeploymentPlanAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WouldApply => write!(f, "would apply"),
            Self::WouldCreate => write!(f, "would create"),
            Self::WouldUpdate => write!(f, "would update"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DeploymentPlanSummary {
    pub total_resources: usize,
    pub would_apply: usize,
    pub requires_cluster: bool,
    pub mutates_cluster: bool,
}

pub fn generate_static_deployment_plan(
    environment: &str,
    context: &str,
    namespace: &str,
) -> Result<WorkflowDeploymentPlan, String> {
    use serde_yaml::Value;
    use std::path::Path;
    use walkdir::WalkDir;

    let mut resources = Vec::new();
    let root_path = format!("k8s/generated/{}", environment);

    if Path::new(&root_path).exists() {
        for entry in WalkDir::new(&root_path).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "yaml" || ext == "yml" {
                        if let Ok(contents) = std::fs::read_to_string(entry.path()) {
                            for document in contents.split("---") {
                                if document.trim().is_empty() {
                                    continue;
                                }
                                if let Ok(parsed) = serde_yaml::from_str::<Value>(document) {
                                    if let Some(kind) = parsed.get("kind").and_then(|v| v.as_str())
                                    {
                                        let meta = parsed.get("metadata");
                                        let name = meta
                                            .and_then(|m| m.get("name"))
                                            .and_then(|n| n.as_str())
                                            .unwrap_or("unknown")
                                            .to_string();
                                        let ns = meta
                                            .and_then(|m| m.get("namespace"))
                                            .and_then(|n| n.as_str())
                                            .map(|s| s.to_string());

                                        resources.push(DeploymentResourcePlan {
                                            kind: kind.to_string(),
                                            name,
                                            namespace: ns,
                                            source_path: entry.path().to_string_lossy().to_string(),
                                            action: DeploymentPlanAction::WouldApply,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let summary = DeploymentPlanSummary {
        total_resources: resources.len(),
        would_apply: resources.len(),
        requires_cluster: false,
        mutates_cluster: false,
    };

    Ok(WorkflowDeploymentPlan {
        environment: environment.to_string(),
        context: context.to_string(),
        namespace: namespace.to_string(),
        mode: DeploymentPlanMode::Static,
        resources,
        summary,
    })
}
