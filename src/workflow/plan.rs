use crate::builder::SailrBuildPlan;
use crate::workflow::profile::NormalizedWorkflowProfile;
use crate::workflow::runner::RunnerContext;

#[derive(Debug, Clone)]
pub struct WorkflowPlan {
    pub profile: NormalizedWorkflowProfile,
    pub runner: RunnerContext,
    pub tasks: Vec<WorkflowTaskPlan>,
    pub edges: Vec<WorkflowEdge>,
    pub build_plan: Option<SailrBuildPlan>,
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
    Generate,
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
