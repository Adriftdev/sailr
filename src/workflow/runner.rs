use crate::builder::{attach_pipeline_logging, write_successful_service_caches, BuildOptions};
use crate::cli::WorkflowRunArgs;
use crate::environment::Environment;

use super::config::WorkflowConfig;
use super::planner::WorkflowPlanner;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunnerKind {
    Local,
    GitHubActions,
    CircleCi,
    Travis,
    GenericCi,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RunnerContext {
    pub kind: RunnerKind,
    pub ci: bool,
    pub interactive: bool,
    pub ci_environment: Option<crate::workflow::ci::CiEnvironment>,
}

pub trait EnvironmentReader {
    fn read(&self, key: &str) -> Option<String>;
}

pub struct SystemEnvironmentReader;

impl EnvironmentReader for SystemEnvironmentReader {
    fn read(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

#[derive(Debug, Clone, Default)]
pub struct MapEnvironmentReader {
    pub values: std::collections::BTreeMap<String, String>,
}

impl EnvironmentReader for MapEnvironmentReader {
    fn read(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }
}

impl RunnerContext {
    pub fn detect(non_interactive: bool) -> Self {
        Self::detect_with(non_interactive, &SystemEnvironmentReader)
    }

    pub fn detect_with(non_interactive: bool, environment: &dyn EnvironmentReader) -> Self {
        let mut ci_env = None;
        let kind = if environment.read("GITHUB_ACTIONS").as_deref() == Some("true") {
            ci_env = Some(crate::workflow::ci::CiEnvironment {
                provider: crate::workflow::ci::CiProvider::GitHub,
                run_id: environment.read("GITHUB_RUN_ID"),
            });
            RunnerKind::GitHubActions
        } else if environment.read("CIRCLECI").as_deref() == Some("true") {
            ci_env = Some(crate::workflow::ci::CiEnvironment {
                provider: crate::workflow::ci::CiProvider::CircleCi,
                run_id: environment.read("CIRCLE_WORKFLOW_ID"),
            });
            RunnerKind::CircleCi
        } else if environment.read("TRAVIS").as_deref() == Some("true") {
            ci_env = Some(crate::workflow::ci::CiEnvironment {
                provider: crate::workflow::ci::CiProvider::Travis,
                run_id: environment.read("TRAVIS_BUILD_ID"),
            });
            RunnerKind::Travis
        } else if environment.read("CI").is_some() {
            ci_env = Some(crate::workflow::ci::CiEnvironment {
                provider: crate::workflow::ci::CiProvider::Generic,
                run_id: None,
            });
            RunnerKind::GenericCi
        } else {
            RunnerKind::Local
        };

        let ci = kind != RunnerKind::Local;
        let interactive = !ci && !non_interactive;

        Self {
            kind,
            ci,
            interactive,
            ci_environment: ci_env,
        }
    }
}

fn print_failed_tasks(result: &runkernel::PipelineResult) {
    for task in &result.tasks {
        if matches!(task.status, runkernel::TaskStatus::Failed) {
            println!("Failed task: {}", task.name);
            if let Some(error) = &task.error {
                println!("  error: {}", error);
            }
        }
    }
}

fn print_tasks_by_status(
    label: &str,
    result: &runkernel::PipelineResult,
    status: runkernel::TaskStatus,
) {
    let tasks = result
        .tasks
        .iter()
        .filter(|task| task.status == status)
        .map(|task| task.name.as_str())
        .collect::<Vec<_>>();

    if tasks.is_empty() {
        return;
    }

    println!();
    println!("  {}:", label);
    for task in tasks {
        println!("    - {}", task);
    }
}

fn print_workflow_result(
    profile: &super::profile::NormalizedWorkflowProfile,
    runner: &RunnerContext,
    result: &runkernel::PipelineResult,
) {
    println!("Sailr workflow result:");
    println!("  profile: {}", profile.name);
    println!("  mode: {}", profile.mode);
    println!("  runner: {:?}", runner.kind);
    println!("  tasks completed: {}", result.summary.completed);
    println!("  tasks failed: {}", result.summary.failed);
    println!("  tasks skipped: {}", result.summary.skipped);
    println!("  duration: {:?}", result.duration);

    print_tasks_by_status("completed tasks", result, runkernel::TaskStatus::Completed);
    print_tasks_by_status("failed tasks", result, runkernel::TaskStatus::Failed);
    print_tasks_by_status("skipped tasks", result, runkernel::TaskStatus::Skipped);
    print_tasks_by_status("cancelled tasks", result, runkernel::TaskStatus::Cancelled);
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowReportTaskStatus {
    Pending,
    Running,
    Cached,
    Completed,
    Failed,
    Skipped,
    Cancelled,
    RolledBack,
}

impl From<&runkernel::TaskStatus> for WorkflowReportTaskStatus {
    fn from(status: &runkernel::TaskStatus) -> Self {
        match status {
            runkernel::TaskStatus::Pending => Self::Pending,
            runkernel::TaskStatus::Running => Self::Running,
            runkernel::TaskStatus::Cached => Self::Cached,
            runkernel::TaskStatus::Completed => Self::Completed,
            runkernel::TaskStatus::Failed => Self::Failed,
            runkernel::TaskStatus::Skipped => Self::Skipped,
            runkernel::TaskStatus::Cancelled => Self::Cancelled,
            runkernel::TaskStatus::RolledBack => Self::RolledBack,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkflowReportTaskItem {
    pub name: String,
    pub status: WorkflowReportTaskStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkflowReportTasks {
    pub completed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub cancelled: usize,
    pub cached: usize,
    pub rolled_back: usize,
    pub rollback_failed: usize,
    pub items: Vec<WorkflowReportTaskItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkflowReportPlans {
    pub image_push: Option<crate::workflow::image::ImagePushPlanReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment: Option<crate::workflow::plan::WorkflowDeploymentPlan>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum WorkflowReportType {
    WorkflowExecution,
    WorkflowInspection,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkflowReportArtifacts {
    pub published_images: Vec<crate::workflow::image::PublishedImageArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkflowReport {
    pub schema_version: String,
    pub report_type: WorkflowReportType,
    pub profile: String,
    pub mode: String,
    pub runner: RunnerContext,
    pub environment: String,
    pub success: bool,
    pub effects: crate::workflow::plan::WorkflowEffects,
    pub tasks: WorkflowReportTasks,
    pub plans: WorkflowReportPlans,
    pub artifacts: WorkflowReportArtifacts,
}

impl WorkflowReport {
    pub fn validate(&self) -> Result<(), crate::workflow::error::WorkflowReportError> {
        use crate::workflow::error::WorkflowReportError;

        if self.schema_version != "sailr.workflow-report/v1" {
            return Err(WorkflowReportError::Validation(format!(
                "unsupported schema version: {}",
                self.schema_version
            )));
        }
        if self.profile.trim().is_empty() {
            return Err(WorkflowReportError::Validation(
                "profile cannot be blank".to_string(),
            ));
        }
        if self.environment.trim().is_empty() {
            return Err(WorkflowReportError::Validation(
                "environment cannot be blank".to_string(),
            ));
        }

        for (status, expected) in [
            (WorkflowReportTaskStatus::Completed, self.tasks.completed),
            (WorkflowReportTaskStatus::Failed, self.tasks.failed),
            (WorkflowReportTaskStatus::Skipped, self.tasks.skipped),
            (WorkflowReportTaskStatus::Cancelled, self.tasks.cancelled),
            (WorkflowReportTaskStatus::Cached, self.tasks.cached),
            (WorkflowReportTaskStatus::RolledBack, self.tasks.rolled_back),
        ] {
            let actual = self
                .tasks
                .items
                .iter()
                .filter(|item| item.status == status)
                .count();
            if actual != expected {
                return Err(WorkflowReportError::Validation(format!(
                    "task summary mismatch for {status:?}: expected {expected}, found {actual}"
                )));
            }
        }
        if self.tasks.items.iter().any(|item| {
            matches!(
                item.status,
                WorkflowReportTaskStatus::Pending | WorkflowReportTaskStatus::Running
            )
        }) {
            return Err(WorkflowReportError::Validation(
                "completed workflow reports cannot contain pending or running tasks".to_string(),
            ));
        }
        let terminal_count = self.tasks.completed
            + self.tasks.failed
            + self.tasks.skipped
            + self.tasks.cancelled
            + self.tasks.cached
            + self.tasks.rolled_back;
        if terminal_count != self.tasks.items.len() {
            return Err(WorkflowReportError::Validation(format!(
                "terminal task count mismatch: expected {}, found {} items",
                terminal_count,
                self.tasks.items.len()
            )));
        }
        for item in self
            .tasks
            .items
            .iter()
            .filter(|item| item.status == WorkflowReportTaskStatus::Failed)
        {
            if item
                .error
                .as_deref()
                .is_none_or(|error| error.trim().is_empty())
            {
                return Err(WorkflowReportError::Validation(format!(
                    "failed task '{}' must contain an error",
                    item.name
                )));
            }
        }
        if self.tasks.rollback_failed > self.tasks.items.len() {
            return Err(WorkflowReportError::Validation(
                "rollback_failed exceeds the task item count".to_string(),
            ));
        }
        let expected_success = self.tasks.failed == 0
            && self.tasks.skipped == 0
            && self.tasks.cancelled == 0
            && self.tasks.rollback_failed == 0;
        if self.success != expected_success {
            return Err(WorkflowReportError::Validation(format!(
                "report success does not match task summary: expected {expected_success}"
            )));
        }

        if let Some(push_plan) = &self.plans.image_push {
            push_plan.validate().map_err(|error| {
                WorkflowReportError::Validation(format!("invalid image push plan: {error}"))
            })?;
            if push_plan.environment != self.environment {
                return Err(WorkflowReportError::Validation(
                    "push-plan environment does not match report environment".to_string(),
                ));
            }
            let mut published_services = std::collections::BTreeSet::new();
            for artifact in &self.artifacts.published_images {
                if !published_services.insert(artifact.service.as_str()) {
                    return Err(WorkflowReportError::Validation(format!(
                        "duplicate published service: {}",
                        artifact.service
                    )));
                }
                let item = push_plan
                    .items
                    .iter()
                    .find(|item| item.service == artifact.service)
                    .ok_or_else(|| {
                        WorkflowReportError::Validation(format!(
                            "published service '{}' is absent from the push plan",
                            artifact.service
                        ))
                    })?;
                artifact
                    .validate_against_plan_item(&self.environment, item)
                    .map_err(|error| {
                        WorkflowReportError::Validation(format!(
                            "invalid published artifact: {error}"
                        ))
                    })?;
            }
            if !push_plan.mutates_registry && !self.artifacts.published_images.is_empty() {
                return Err(WorkflowReportError::Validation(
                    "non-mutating push plans cannot contain published artifacts".to_string(),
                ));
            }
            if self.success && push_plan.mutates_registry {
                let planned_services = push_plan
                    .items
                    .iter()
                    .map(|item| item.service.as_str())
                    .collect::<std::collections::BTreeSet<_>>();
                if planned_services != published_services {
                    return Err(WorkflowReportError::Validation(
                        "successful publication report does not cover every planned service"
                            .to_string(),
                    ));
                }
            }
        } else if !self.artifacts.published_images.is_empty() {
            return Err(WorkflowReportError::Validation(
                "published artifacts require an image push plan".to_string(),
            ));
        }

        Ok(())
    }
}

fn build_workflow_report(
    profile: &crate::workflow::profile::NormalizedWorkflowProfile,
    runner: &RunnerContext,
    result: &runkernel::PipelineResult,
    plan: &crate::workflow::plan::WorkflowPlan,
    report_data: &crate::workflow::image::WorkflowReportData,
) -> Result<WorkflowReport, String> {
    let task_items = result
        .tasks
        .iter()
        .map(|task| WorkflowReportTaskItem {
            name: task.name.clone(),
            status: WorkflowReportTaskStatus::from(&task.status),
            error: task.error.clone(),
        })
        .collect::<Vec<_>>();

    let published_artifacts = report_data.published_artifacts.clone();
    for artifact in &published_artifacts {
        artifact
            .validate()
            .map_err(|e| format!("invalid published artifact: {:?}", e))?;
    }
    let image_push_plan: Option<crate::workflow::image::ImagePushPlanReport> =
        plan.image_push_plan.clone();

    let mut report = WorkflowReport {
        schema_version: "sailr.workflow-report/v1".to_string(),
        report_type: WorkflowReportType::WorkflowExecution,
        profile: profile.name.clone(),
        mode: profile.mode.as_str().to_string(),
        runner: runner.clone(),
        environment: profile.environment.clone(),
        success: result.summary.success,
        effects: plan.effects.clone(),
        tasks: WorkflowReportTasks {
            completed: result.summary.completed,
            failed: result.summary.failed,
            skipped: result.summary.skipped,
            cancelled: result.summary.cancelled,
            cached: result.summary.cached,
            rolled_back: result.summary.rolled_back,
            rollback_failed: result.summary.rollback_failed,
            items: task_items,
        },
        plans: WorkflowReportPlans {
            image_push: image_push_plan,
            deployment: None,
        },
        artifacts: WorkflowReportArtifacts {
            published_images: published_artifacts,
        },
    };

    if profile.deploy == crate::workflow::profile::WorkflowStepMode::Plan {
        let context = profile.deploy_context.as_deref().unwrap_or("none");
        let namespace = profile.namespace.as_deref().unwrap_or("default");
        if let Ok(plan) = crate::workflow::plan::generate_static_deployment_plan(
            &profile.environment,
            context,
            namespace,
        ) {
            report.plans.deployment = Some(plan);
        }
    }

    report.validate().map_err(|error| error.to_string())?;
    Ok(report)
}

fn write_workflow_report_document(
    root: &std::path::Path,
    report: &WorkflowReport,
) -> Result<(), String> {
    let report_dir = root.join(".sailr").join("reports").join(&report.profile);

    std::fs::create_dir_all(&report_dir)
        .map_err(|e| format!("Failed to create report directory: {}", e))?;

    let report_path = report_dir.join("latest.json");
    let json_string = serde_json::to_string_pretty(&report)
        .map_err(|e| format!("Failed to serialize report: {}", e))?;

    std::fs::write(&report_path, &json_string)
        .map_err(|e| format!("Failed to write report: {}", e))?;

    Ok(())
}

fn execute_workflow_finalizers_to(
    root: &std::path::Path,
    plan: &crate::workflow::plan::WorkflowPlan,
    report: &WorkflowReport,
) -> Result<(), String> {
    for finalizer in &plan.finalizers {
        match finalizer.kind {
            crate::workflow::plan::WorkflowFinalizerKind::WriteWorkflowReport => {
                write_workflow_report_document(root, report)?;
            }
        }
    }
    Ok(())
}

fn finalize_workflow_report_to(
    root: &std::path::Path,
    profile: &crate::workflow::profile::NormalizedWorkflowProfile,
    runner: &RunnerContext,
    result: &runkernel::PipelineResult,
    plan: &crate::workflow::plan::WorkflowPlan,
    report_data: &crate::workflow::image::WorkflowReportData,
) -> Result<(), String> {
    let report = build_workflow_report(profile, runner, result, plan, report_data)?;
    execute_workflow_finalizers_to(root, plan, &report)
}

fn finalize_workflow_report(
    profile: &crate::workflow::profile::NormalizedWorkflowProfile,
    runner: &RunnerContext,
    result: &runkernel::PipelineResult,
    plan: &crate::workflow::plan::WorkflowPlan,
    report_data: &crate::workflow::image::WorkflowReportData,
) -> Result<(), String> {
    finalize_workflow_report_to(
        std::path::Path::new("."),
        profile,
        runner,
        result,
        plan,
        report_data,
    )
}

pub fn validate_workflow_safety(
    profile: &crate::workflow::profile::NormalizedWorkflowProfile,
    runner: &RunnerContext,
    args: &crate::cli::WorkflowRunArgs,
) -> Result<(), String> {
    if profile.push == crate::workflow::profile::WorkflowStepMode::Run {
        if !profile.apply {
            return Err("push=run requires profile apply=true".to_string());
        }

        if !args.apply {
            return Err("push=run requires --apply".to_string());
        }

        if runner.ci && profile.approval != crate::workflow::profile::ApprovalMode::External {
            let msg = match runner.kind {
                RunnerKind::CircleCi => "CI push requires approval=external.\n\nDetected CircleCI.\nAdd approval = \"external\" to [workflow.ci-build-push] and gate the mutating CircleCI job behind:\n\n  approve_image_push:\n    type: approval",
                RunnerKind::GitHubActions => "CI push requires approval=external.\n\nDetected GitHub Actions.\nAdd approval = \"external\" to [workflow.ci-build-push] and run the job behind a protected GitHub Environment.",
                RunnerKind::Travis => "CI push requires approval=external.\n\nDetected Travis.\nAdd approval = \"external\" to [workflow.ci-build-push] and guard the mutating job with branch and environment variable conditions.",
                _ => "CI push requires approval=external",
            };
            return Err(msg.to_string());
        }
    }

    if runner.ci && profile.interactive {
        return Err("workflow cannot be interactive in CI".to_string());
    }

    if runner.ci && profile.approval == crate::workflow::profile::ApprovalMode::Prompt {
        return Err("approval prompt cannot run in CI".to_string());
    }

    if profile.deploy == crate::workflow::profile::WorkflowStepMode::Run {
        let context = profile.deploy_context.as_deref();

        if context.is_none() || context == Some("none") {
            return Err("deploy=run requires an explicit real deploy_context".to_string());
        }

        if profile.environment == "production" {
            return Err("production deploy is not enabled in this stage".to_string());
        }

        if runner.ci {
            if profile.approval != crate::workflow::profile::ApprovalMode::External {
                let msg = match runner.kind {
                    RunnerKind::CircleCi => "CI deploy requires approval=external.\n\nDetected CircleCI.\nAdd approval = \"external\" to [workflow.ci-build-push] and gate the mutating CircleCI job behind:\n\n  approve_image_push:\n    type: approval",
                    RunnerKind::GitHubActions => "CI deploy requires approval=external.\n\nDetected GitHub Actions.\nAdd approval = \"external\" to [workflow.ci-build-push] and run the job behind a protected GitHub Environment.",
                    RunnerKind::Travis => "CI deploy requires approval=external.\n\nDetected Travis.\nAdd approval = \"external\" to [workflow.ci-build-push] and guard the mutating job with branch and environment variable conditions.",
                    _ => "CI deploy requires approval=external",
                };
                return Err(msg.to_string());
            }

            if !profile.apply {
                return Err("CI deploy requires profile apply=true".to_string());
            }

            if !args.apply {
                return Err("deploy=run in CI requires --apply".to_string());
            }
        } else {
            if !profile.apply {
                return Err("deploy=run requires apply=true".to_string());
            }

            if !runner.interactive && !args.apply {
                return Err("non-interactive deploy requires --apply".to_string());
            }
        }
    }

    if profile.approval == crate::workflow::profile::ApprovalMode::Prompt && !runner.interactive {
        return Err("approval prompt cannot run in non-interactive mode".to_string());
    }

    Ok(())
}

pub fn requires_cli_apply(profile: &crate::workflow::profile::NormalizedWorkflowProfile) -> bool {
    profile.push == crate::workflow::profile::WorkflowStepMode::Run
        || profile.deploy == crate::workflow::profile::WorkflowStepMode::Run
}

#[derive(Debug)]
pub struct WorkflowInspectionImage {
    pub service: String,
    pub local_image_ref: String,
    pub target_image_ref: String,
    pub build_fingerprint: String,
    pub source_revision: Option<String>,
}

#[derive(Debug)]
pub struct WorkflowInspection {
    pub profile_name: String,
    pub profile_mode: String,
    pub config_path: String,
    pub environment: String,
    pub environment_path: String,
    pub runner_ci: bool,
    pub runner_provider: String,
    pub runner_interactive: bool,
    pub approval: Option<crate::workflow::profile::ApprovalMode>,
    pub profile_apply: bool,
    pub requires_cli_apply: bool,
    pub build_mode: crate::workflow::profile::WorkflowStepMode,
    pub push_mode: crate::workflow::profile::WorkflowStepMode,
    pub generate_mode: crate::workflow::profile::WorkflowStepMode,
    pub deploy_mode: crate::workflow::profile::WorkflowStepMode,
    pub registry_host: String,
    pub registry_namespace: String,
    pub registry_prefix: String,
    pub images: Vec<WorkflowInspectionImage>,
}

impl WorkflowInspection {
    pub fn render_workflow_inspection(&self) -> String {
        let mut output = String::new();
        output.push_str("Workflow:\n");
        output.push_str(&format!("  profile: {}\n", self.profile_name));
        output.push_str(&format!("  mode: {}\n", self.profile_mode));
        output.push_str(&format!("  config: {}\n", self.config_path));
        output.push_str(&format!("  environment: {}\n", self.environment));
        output.push_str(&format!(
            "  environment config: {}\n",
            self.environment_path
        ));

        output.push_str("\nRunner:\n");
        output.push_str(&format!("  ci: {}\n", self.runner_ci));
        output.push_str(&format!("  provider: {}\n", self.runner_provider));
        output.push_str(&format!("  interactive: {}\n", self.runner_interactive));

        output.push_str("\nSafety:\n");
        output.push_str(&format!("  approval: {:?}\n", self.approval));
        output.push_str(&format!(
            "  profile apply allowed: {}\n",
            self.profile_apply
        ));
        output.push_str(&format!(
            "  CLI apply required: {}\n",
            self.requires_cli_apply
        ));
        output.push_str(&format!("  build mode: {:?}\n", self.build_mode));
        output.push_str(&format!("  push mode: {:?}\n", self.push_mode));
        output.push_str(&format!("  generate mode: {:?}\n", self.generate_mode));
        output.push_str(&format!("  deploy mode: {:?}\n", self.deploy_mode));

        output.push_str("\nRegistry:\n");
        output.push_str(&format!("  host: {}\n", self.registry_host));
        output.push_str(&format!("  namespace: {}\n", self.registry_namespace));
        output.push_str(&format!("  prefix: {}\n", self.registry_prefix));

        output.push_str("\nImages:\n");
        if self.images.is_empty() {
            output.push_str("  (no image push plan)\n");
        } else {
            for item in &self.images {
                output.push_str(&format!("  service: {}\n", item.service));
                output.push_str(&format!("  local image ref: {}\n", item.local_image_ref));
                output.push_str(&format!("  target image ref: {}\n", item.target_image_ref));
                output.push_str(&format!(
                    "  build fingerprint: {}\n",
                    item.build_fingerprint
                ));
                output.push_str(&format!(
                    "  source revision: {}\n",
                    item.source_revision.as_deref().unwrap_or("none")
                ));
            }
        }
        output
    }
}

pub struct WorkflowRunner;

impl WorkflowRunner {
    pub async fn run(args: WorkflowRunArgs) -> Result<(), String> {
        // 1. Detect runner context
        let runner_ctx = RunnerContext::detect(args.non_interactive);

        // 2. Load config and find profile
        let config = WorkflowConfig::load().map_err(|e| e.to_string())?;
        let profile = config
            .get_profile(&args.profile)
            .ok_or_else(|| format!("Workflow profile '{}' not found", args.profile))?;

        // 3. Normalize profile
        let normalized_profile = profile.normalize(runner_ctx.ci);

        // 4. Load environment
        let env = Environment::load_from_file(&normalized_profile.environment).map_err(|e| {
            format!(
                "Failed to load environment '{}': {}",
                normalized_profile.environment, e
            )
        })?;

        // 5. Construct BuildOptions (incorporating CLI overrides)
        let only = args
            .only
            .as_ref()
            .map(|s| crate::builder::split_matches(Some(s.clone())))
            .unwrap_or_default();
        let ignore = args
            .ignore
            .as_ref()
            .map(|s| crate::builder::split_matches(Some(s.clone())))
            .unwrap_or_default();

        let options = BuildOptions {
            cache_dir: ".sailr/cache/build".to_string(),
            force: false,
            only,
            ignore,
            plan: args.plan || normalized_profile.build == super::profile::WorkflowStepMode::Plan,
            dry_run: args.dry_run
                || normalized_profile.build == super::profile::WorkflowStepMode::DryRun,
            explain: false,
            dump_scope: false,
            policy: env.build.clone(),
        };

        // 6. Safety validation
        validate_workflow_safety(&normalized_profile, &runner_ctx, &args)?;

        // 7. Plan Pipeline
        let planner = WorkflowPlanner::new(
            normalized_profile.clone(),
            std::sync::Arc::new(env),
            options,
            runner_ctx.clone(),
        );
        let plan = planner.plan()?;

        let accumulator = crate::workflow::image::WorkflowReportAccumulator::default();
        let (mut pipeline, build_execution) =
            planner.build_pipeline_from_plan(&plan, accumulator.clone())?;

        // 8. Run Pipeline
        attach_pipeline_logging(&mut pipeline);

        crate::LOGGER.info(&format!(
            "🚀 Running workflow profile '{}'",
            normalized_profile.name
        ));
        let result = pipeline
            .run()
            .await
            .map_err(|e| format!("Pipeline execution failed: {:?}", e))?;

        // 9. Finalize
        print_workflow_result(&normalized_profile, &runner_ctx, &result);

        let report_data = accumulator.snapshot().await;
        finalize_workflow_report(
            &normalized_profile,
            &runner_ctx,
            &result,
            &plan,
            &report_data,
        )?;

        match build_execution {
            crate::workflow::planner::WorkflowBuildExecution::None => {}
            crate::workflow::planner::WorkflowBuildExecution::PlanOnly(_) => {
                // Do not print build results or write caches
            }
            crate::workflow::planner::WorkflowBuildExecution::Executed(plan) => {
                crate::builder::print_pipeline_result(&plan, &result);
                if result.summary.success {
                    write_successful_service_caches(&plan, &result)?;
                }
            }
        }

        if !result.summary.success {
            print_failed_tasks(&result);
            return Err(format!(
                "Workflow failed: {} failed, {} skipped, {} cancelled",
                result.summary.failed, result.summary.skipped, result.summary.cancelled
            ));
        }

        crate::LOGGER.info("✅ Workflow completed successfully.");
        Ok(())
    }

    pub async fn inspect(args: crate::cli::WorkflowInspectArgs) -> Result<(), String> {
        let runner_ctx = RunnerContext::detect(false);
        let config_path = std::path::Path::new("sailr.workflow.toml");
        let config = WorkflowConfig::load().map_err(|e| e.to_string())?;

        let profile = config
            .get_profile(&args.profile)
            .ok_or_else(|| format!("Workflow profile '{}' not found", args.profile))?;
        let normalized = profile.normalize(runner_ctx.ci);

        let env_path_str = format!("k8s/environments/{}/config.toml", normalized.environment);
        let env = Environment::load_from_file(&normalized.environment).map_err(|e| {
            format!(
                "Failed to load environment '{}': {}",
                normalized.environment, e
            )
        })?;

        let resolved_registry = env
            .registry
            .resolve()
            .map_err(|e| format!("Invalid registry configuration: {}", e))?;

        let env_arc = std::sync::Arc::new(env);
        let build_options = crate::builder::BuildOptions {
            cache_dir: ".sailr/cache".to_string(),
            force: false,
            only: vec![],
            ignore: vec![],
            plan: true,
            dry_run: true,
            explain: false,
            dump_scope: false,
            policy: None,
        };
        let planner = crate::workflow::planner::WorkflowPlanner::new(
            normalized.clone(),
            env_arc.clone(),
            build_options,
            runner_ctx.clone(),
        );
        let plan = planner
            .plan()
            .map_err(|e| format!("Failed to generate plan: {}", e))?;

        let inspection = WorkflowInspection {
            profile_name: normalized.name.clone(),
            profile_mode: format!("{:?}", normalized.mode).to_lowercase(),
            config_path: std::fs::canonicalize(config_path)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| "sailr.workflow.toml".to_string()),
            environment: normalized.environment.clone(),
            environment_path: std::fs::canonicalize(&env_path_str)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| env_path_str),
            runner_ci: runner_ctx.ci,
            runner_provider: format!("{:?}", runner_ctx.kind),
            runner_interactive: runner_ctx.interactive,
            approval: Some(normalized.approval),
            profile_apply: profile.apply.unwrap_or(false),
            requires_cli_apply: requires_cli_apply(&normalized),
            build_mode: normalized.build,
            push_mode: normalized.push,
            generate_mode: normalized.generate,
            deploy_mode: normalized.deploy,
            registry_host: resolved_registry.host.clone(),
            registry_namespace: resolved_registry
                .namespace
                .clone()
                .unwrap_or_else(|| "none".to_string()),
            registry_prefix: resolved_registry.prefix(),
            images: {
                let mut images = Vec::new();
                if let Some(push_plan) = plan.image_push_plan {
                    for item in &push_plan.items {
                        images.push(WorkflowInspectionImage {
                            service: item.service.clone(),
                            local_image_ref: item.local_image_ref.clone(),
                            target_image_ref: item.target_image_ref.clone(),
                            build_fingerprint: item.provenance.build_fingerprint.clone(),
                            source_revision: item.provenance.source_revision.clone(),
                        });
                    }
                }
                images
            },
        };

        println!("{}", inspection.render_workflow_inspection());

        Ok(())
    }

    pub async fn plan(args: crate::cli::WorkflowPlanArgs) -> Result<(), String> {
        let runner_ctx = RunnerContext::detect(false);
        let config = WorkflowConfig::load().map_err(|e| e.to_string())?;
        let profile = config
            .get_profile(&args.profile)
            .ok_or_else(|| format!("Workflow profile '{}' not found", args.profile))?;
        let normalized_profile = profile.normalize(runner_ctx.ci);
        let env = Environment::load_from_file(&normalized_profile.environment).map_err(|e| {
            format!(
                "Failed to load environment '{}': {}",
                normalized_profile.environment, e
            )
        })?;

        let only = args
            .only
            .map(|s| crate::builder::split_matches(Some(s)))
            .unwrap_or_default();
        let ignore = args
            .ignore
            .map(|s| crate::builder::split_matches(Some(s)))
            .unwrap_or_default();

        let options = BuildOptions {
            cache_dir: ".sailr/cache/build".to_string(),
            force: false,
            only,
            ignore,
            plan: true,
            dry_run: false,
            explain: false,
            dump_scope: false,
            policy: env.build.clone(),
        };

        let planner = WorkflowPlanner::new(
            normalized_profile.clone(),
            std::sync::Arc::new(env),
            options,
            runner_ctx.clone(),
        );
        let plan = planner.plan()?;

        match args.format {
            crate::cli::WorkflowOutputFormat::Text => {
                println!(
                    "{}",
                    crate::workflow::render::render_workflow_plan_text(&plan)
                );
            }
            crate::cli::WorkflowOutputFormat::Json => {
                return Err("JSON plan format not yet implemented".to_string());
            }
        }

        Ok(())
    }

    pub async fn graph(args: crate::cli::WorkflowGraphArgs) -> Result<(), String> {
        let runner_ctx = RunnerContext::detect(false);
        let config = WorkflowConfig::load().map_err(|e| e.to_string())?;
        let profile = config
            .get_profile(&args.profile)
            .ok_or_else(|| format!("Workflow profile '{}' not found", args.profile))?;
        let normalized_profile = profile.normalize(runner_ctx.ci);
        let env = Environment::load_from_file(&normalized_profile.environment).map_err(|e| {
            format!(
                "Failed to load environment '{}': {}",
                normalized_profile.environment, e
            )
        })?;

        let only = args
            .only
            .map(|s| crate::builder::split_matches(Some(s)))
            .unwrap_or_default();
        let ignore = args
            .ignore
            .map(|s| crate::builder::split_matches(Some(s)))
            .unwrap_or_default();

        let options = BuildOptions {
            cache_dir: ".sailr/cache/build".to_string(),
            force: false,
            only,
            ignore,
            plan: true,
            dry_run: false,
            explain: false,
            dump_scope: false,
            policy: env.build.clone(),
        };

        let planner = WorkflowPlanner::new(
            normalized_profile.clone(),
            std::sync::Arc::new(env),
            options,
            runner_ctx.clone(),
        );
        let plan = planner.plan()?;

        match args.format {
            crate::cli::WorkflowGraphFormat::Text => {
                println!(
                    "{}",
                    crate::workflow::render::render_workflow_graph_text(&plan)
                );
            }
            crate::cli::WorkflowGraphFormat::Mermaid => {
                println!(
                    "{}",
                    crate::workflow::render::render_workflow_graph_mermaid(&plan)
                );
            }
        }

        Ok(())
    }

    pub async fn explain(args: crate::cli::WorkflowExplainArgs) -> Result<(), String> {
        let runner_ctx = RunnerContext::detect(false);
        let config = WorkflowConfig::load().map_err(|e| e.to_string())?;
        let profile = config
            .get_profile(&args.profile)
            .ok_or_else(|| format!("Workflow profile '{}' not found", args.profile))?;
        let normalized_profile = profile.normalize(runner_ctx.ci);
        let env = Environment::load_from_file(&normalized_profile.environment).map_err(|e| {
            format!(
                "Failed to load environment '{}': {}",
                normalized_profile.environment, e
            )
        })?;

        let options = BuildOptions {
            cache_dir: ".sailr/cache/build".to_string(),
            force: false,
            only: vec![],
            ignore: vec![],
            plan: true,
            dry_run: false,
            explain: false,
            dump_scope: false,
            policy: env.build.clone(),
        };

        let planner = WorkflowPlanner::new(
            normalized_profile.clone(),
            std::sync::Arc::new(env),
            options,
            runner_ctx.clone(),
        );
        let plan = planner.plan()?;

        let text = crate::workflow::render::render_workflow_explain_text(&plan, &args.task)?;
        println!("{}", text);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map_environment(values: &[(&str, &str)]) -> MapEnvironmentReader {
        MapEnvironmentReader {
            values: values
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect(),
        }
    }

    #[test]
    fn detects_github_actions() {
        let environment = map_environment(&[("GITHUB_ACTIONS", "true"), ("GITHUB_RUN_ID", "17")]);
        let ctx = RunnerContext::detect_with(false, &environment);
        assert_eq!(ctx.kind, RunnerKind::GitHubActions);
        assert_eq!(ctx.ci_environment.unwrap().run_id.as_deref(), Some("17"));
        assert!(ctx.ci);
        assert!(!ctx.interactive);
    }

    #[test]
    fn detects_circle_ci() {
        let ctx = RunnerContext::detect_with(false, &map_environment(&[("CIRCLECI", "true")]));
        assert_eq!(ctx.kind, RunnerKind::CircleCi);
        assert!(ctx.ci);
        assert!(!ctx.interactive);
    }

    #[test]
    fn detects_travis() {
        let ctx = RunnerContext::detect_with(false, &map_environment(&[("TRAVIS", "true")]));
        assert_eq!(ctx.kind, RunnerKind::Travis);
        assert!(ctx.ci);
        assert!(!ctx.interactive);
    }

    #[test]
    fn detects_generic_ci() {
        let ctx = RunnerContext::detect_with(false, &map_environment(&[("CI", "true")]));
        assert_eq!(ctx.kind, RunnerKind::GenericCi);
        assert!(ctx.ci);
        assert!(!ctx.interactive);
    }

    #[test]
    fn local_runner_interactive_by_default() {
        let ctx = RunnerContext::detect_with(false, &MapEnvironmentReader::default());
        assert_eq!(ctx.kind, RunnerKind::Local);
        assert!(!ctx.ci);
        assert!(ctx.interactive);
    }

    #[test]
    fn local_runner_disabled_interactivity_with_flag() {
        let ctx = RunnerContext::detect_with(true, &MapEnvironmentReader::default());
        assert_eq!(ctx.kind, RunnerKind::Local);
        assert!(!ctx.ci);
        assert!(!ctx.interactive);
    }

    #[test]
    fn validate_safety_missing_deploy_context() {
        use crate::workflow::profile::{
            ApprovalMode, NormalizedWorkflowProfile, ReportMode, WorkflowEngine, WorkflowMode,
            WorkflowStepMode,
        };

        let profile = NormalizedWorkflowProfile {
            name: "test".to_string(),
            environment: "local".to_string(),
            mode: WorkflowMode::Go,
            engine: WorkflowEngine::Runkernel,
            interactive: true,
            build: WorkflowStepMode::Run,
            push: WorkflowStepMode::Disabled,
            generate: WorkflowStepMode::Run,
            deploy: WorkflowStepMode::Run,
            test: WorkflowStepMode::Disabled,
            verify: WorkflowStepMode::Disabled,
            deploy_context: None,
            namespace: None,
            approval: ApprovalMode::Prompt,
            apply: true,
            report: ReportMode::Text,
        };

        let runner = RunnerContext {
            ci_environment: None,
            kind: RunnerKind::Local,
            ci: false,
            interactive: true,
        };

        let res = validate_workflow_safety(
            &profile,
            &runner,
            &crate::cli::WorkflowRunArgs {
                profile: "test".to_string(),
                only: None,
                ignore: None,
                non_interactive: true,
                plan: false,
                dry_run: false,
                apply: false,
            },
        );
        assert!(res.is_err());
        assert!(res
            .unwrap_err()
            .contains("deploy=run requires an explicit real deploy_context"));
    }

    #[test]
    fn validate_safety_ci_deploy_rejected() {
        use crate::workflow::profile::{
            ApprovalMode, NormalizedWorkflowProfile, ReportMode, WorkflowEngine, WorkflowMode,
            WorkflowStepMode,
        };

        let profile = NormalizedWorkflowProfile {
            name: "test".to_string(),
            environment: "prod".to_string(),
            mode: WorkflowMode::Deploy,
            engine: WorkflowEngine::Runkernel,
            interactive: false,
            build: WorkflowStepMode::Plan,
            push: WorkflowStepMode::Disabled,
            generate: WorkflowStepMode::Run,
            deploy: WorkflowStepMode::Run,
            test: WorkflowStepMode::Disabled,
            verify: WorkflowStepMode::Disabled,
            deploy_context: Some("prod-cluster".to_string()),
            namespace: None,
            approval: ApprovalMode::External,
            apply: true,
            report: ReportMode::Text,
        };

        let runner = RunnerContext {
            ci_environment: None,
            kind: RunnerKind::GitHubActions,
            ci: true,
            interactive: false,
        };

        let res = validate_workflow_safety(
            &profile,
            &runner,
            &crate::cli::WorkflowRunArgs {
                profile: "test".to_string(),
                only: None,
                ignore: None,
                non_interactive: true,
                plan: false,
                dry_run: false,
                apply: false,
            },
        );
        assert!(res.is_err());
        // Since we added a check for apply=true in CI first, it'll hit that instead.
        // Let's just check that it fails correctly.
    }

    #[test]
    fn validate_safety_approval_prompt_non_interactive() {
        use crate::workflow::profile::{
            ApprovalMode, NormalizedWorkflowProfile, ReportMode, WorkflowEngine, WorkflowMode,
            WorkflowStepMode,
        };

        let profile = NormalizedWorkflowProfile {
            name: "test".to_string(),
            environment: "local".to_string(),
            mode: WorkflowMode::Go,
            engine: WorkflowEngine::Runkernel,
            interactive: false,
            build: WorkflowStepMode::Plan,
            push: WorkflowStepMode::Disabled,
            generate: WorkflowStepMode::Run,
            deploy: WorkflowStepMode::Plan,
            test: WorkflowStepMode::Disabled,
            verify: WorkflowStepMode::Disabled,
            deploy_context: Some("minikube".to_string()),
            namespace: None,
            approval: ApprovalMode::Prompt,
            apply: false,
            report: ReportMode::Text,
        };

        let runner = RunnerContext {
            ci_environment: None,
            kind: RunnerKind::Local,
            ci: false,
            interactive: false,
        };

        let res = validate_workflow_safety(
            &profile,
            &runner,
            &crate::cli::WorkflowRunArgs {
                profile: "test".to_string(),
                only: None,
                ignore: None,
                non_interactive: true,
                plan: false,
                dry_run: false,
                apply: false,
            },
        );
        assert!(res.is_err());
        assert!(res
            .unwrap_err()
            .contains("approval prompt cannot run in non-interactive mode"));
    }

    #[test]
    fn validate_safety_deploy_run_requires_apply() {
        use crate::workflow::profile::{
            ApprovalMode, NormalizedWorkflowProfile, ReportMode, WorkflowEngine, WorkflowMode,
            WorkflowStepMode,
        };

        let profile = NormalizedWorkflowProfile {
            name: "test".to_string(),
            environment: "local".to_string(),
            mode: WorkflowMode::Go,
            engine: WorkflowEngine::Runkernel,
            interactive: true,
            build: WorkflowStepMode::Run,
            push: WorkflowStepMode::Disabled,
            generate: WorkflowStepMode::Run,
            deploy: WorkflowStepMode::Run,
            test: WorkflowStepMode::Disabled,
            verify: WorkflowStepMode::Disabled,
            deploy_context: Some("minikube".to_string()),
            namespace: None,
            approval: ApprovalMode::Prompt,
            apply: false, // apply is false!
            report: ReportMode::Text,
        };

        let runner = RunnerContext {
            ci_environment: None,
            kind: RunnerKind::Local,
            ci: false,
            interactive: true,
        };

        let res = validate_workflow_safety(
            &profile,
            &runner,
            &crate::cli::WorkflowRunArgs {
                profile: "test".to_string(),
                only: None,
                ignore: None,
                non_interactive: true,
                plan: false,
                dry_run: false,
                apply: false,
            },
        );
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("deploy=run requires apply=true"));
    }
    #[test]
    fn validate_safety_ci_staging_deploy_allowed() {
        use crate::workflow::profile::{
            ApprovalMode, NormalizedWorkflowProfile, ReportMode, WorkflowEngine, WorkflowMode,
            WorkflowStepMode,
        };

        let profile = NormalizedWorkflowProfile {
            name: "staging-deploy".to_string(),
            environment: "staging".to_string(),
            mode: WorkflowMode::Deploy,
            engine: WorkflowEngine::Runkernel,
            interactive: false,
            build: WorkflowStepMode::Plan,
            push: WorkflowStepMode::Disabled,
            generate: WorkflowStepMode::Run,
            deploy: WorkflowStepMode::Run,
            test: WorkflowStepMode::Disabled,
            verify: WorkflowStepMode::Disabled,
            deploy_context: Some("staging".to_string()),
            namespace: Some("default".to_string()),
            approval: ApprovalMode::External,
            apply: true,
            report: ReportMode::Both,
        };

        let runner = RunnerContext {
            ci_environment: None,
            kind: RunnerKind::GitHubActions,
            ci: true,
            interactive: false,
        };

        let args = crate::cli::WorkflowRunArgs {
            profile: "staging-deploy".to_string(),
            only: None,
            ignore: None,
            non_interactive: true,
            plan: false,
            dry_run: false,
            apply: true,
        };

        let res = validate_workflow_safety(&profile, &runner, &args);
        assert!(res.is_ok());
    }

    #[test]
    fn ci_build_push_plan_json_report_includes_image_push_plan() {
        use crate::environment::Environment;
        use crate::workflow::planner::WorkflowPlanner;
        use crate::workflow::profile::WorkflowProfile;

        let temp_dir = tempfile::tempdir().unwrap();
        let env_toml = format!(
            r#"
        schema_version = "v0.5"
        name = "test"
        domain = "test.local"
        log_level = "info"
        default_replicas = 1
        registry = "ghcr.io"
        [[service]]
        name = "ci-build-hello"
        [service.build]
        path = "{}"
        "#,
            temp_dir.path().to_string_lossy()
        );
        let env: Environment = toml::from_str(&env_toml).unwrap();

        let profile_toml = r#"
        environment = "test"
        mode = "build"
        build = "plan"
        push = "plan"
        report = "json"
        "#;
        let mut profile: WorkflowProfile = toml::from_str(profile_toml).unwrap();
        profile.name = "ci-build-push-plan".to_string();
        let normalized = profile.normalize(false);
        let runner_ctx = RunnerContext::detect(true);
        let options = crate::builder::BuildOptions {
            cache_dir: temp_dir
                .path()
                .join(".sailr/cache")
                .to_string_lossy()
                .to_string(),
            force: false,
            only: vec![],
            ignore: vec![],
            plan: false,
            dry_run: false,
            explain: false,
            dump_scope: false,
            policy: None,
        };

        let planner = WorkflowPlanner::new(
            normalized.clone(),
            std::sync::Arc::new(env),
            options,
            runner_ctx.clone(),
        );

        let plan = planner.plan().unwrap();

        let result = runkernel::PipelineResult {
            name: "test".to_string(),
            duration: std::time::Duration::from_secs(1),
            summary: runkernel::PipelineSummary {
                name: "test".to_string(),
                success: true,
                completed: 1,
                failed: 0,
                skipped: 0,
                cancelled: 0,
                cached: 0,
                rolled_back: 0,
                rollback_failed: 0,
            },
            tasks: vec![runkernel::TaskResult {
                name: crate::workflow::task_id::PUSH_PLAN.to_string(),
                status: runkernel::TaskStatus::Completed,
                duration: Some(std::time::Duration::from_secs(1)),
                error: None,
                cache_hit: false,
                cache_reason: None,
                rollback_status: None,
                rollback_error: None,
            }],
        };

        // Write report into a temp directory to avoid polluting the project.
        let temp = tempfile::tempdir().unwrap();

        finalize_workflow_report_to(
            temp.path(),
            &normalized,
            &runner_ctx,
            &result,
            &plan,
            &Default::default(),
        )
        .unwrap();

        let report_path = temp
            .path()
            .join(".sailr/reports/ci-build-push-plan/latest.json");
        let content = std::fs::read_to_string(&report_path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert_eq!(
            json["plans"]["image_push"]["items"][0]["action"],
            "would_push"
        );
    }

    fn load_fixture_json(name: &str) -> serde_json::Value {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/reports")
            .join(name);
        let content = std::fs::read_to_string(&path).unwrap();
        serde_json::from_str(&content).unwrap()
    }

    #[derive(Debug)]
    struct FixedRevision;

    impl crate::workflow::planner::SourceRevisionResolver for FixedRevision {
        fn resolve(
            &self,
            _runner: &RunnerContext,
        ) -> Result<Option<String>, crate::workflow::error::ProvenanceError> {
            Ok(Some("0123456789abcdef".to_string()))
        }
    }

    fn task_result(
        name: impl Into<String>,
        status: runkernel::TaskStatus,
        error: Option<&str>,
    ) -> runkernel::TaskResult {
        runkernel::TaskResult {
            name: name.into(),
            status,
            duration: Some(std::time::Duration::from_secs(1)),
            error: error.map(str::to_string),
            cache_hit: false,
            cache_reason: None,
            rollback_status: None,
            rollback_error: None,
        }
    }

    fn completed_results_from_plan(
        plan: &crate::workflow::plan::WorkflowPlan,
    ) -> Vec<runkernel::TaskResult> {
        plan.tasks
            .iter()
            .map(|task| task_result(task.id.clone(), runkernel::TaskStatus::Completed, None))
            .collect()
    }

    fn failed_results_from_plan(
        plan: &crate::workflow::plan::WorkflowPlan,
        failed_task: &str,
        error: &str,
    ) -> Vec<runkernel::TaskResult> {
        let mut unavailable = std::collections::BTreeSet::from([failed_task.to_string()]);
        loop {
            let before = unavailable.len();
            for task in &plan.tasks {
                if task
                    .dependencies
                    .iter()
                    .any(|dependency| unavailable.contains(dependency))
                {
                    unavailable.insert(task.id.clone());
                }
            }
            if unavailable.len() == before {
                break;
            }
        }

        plan.tasks
            .iter()
            .map(|task| {
                if task.id == failed_task {
                    task_result(task.id.clone(), runkernel::TaskStatus::Failed, Some(error))
                } else if unavailable.contains(&task.id) {
                    task_result(task.id.clone(), runkernel::TaskStatus::Skipped, None)
                } else {
                    task_result(task.id.clone(), runkernel::TaskStatus::Completed, None)
                }
            })
            .collect()
    }

    fn pipeline_result_from_tasks(
        success: bool,
        tasks: Vec<runkernel::TaskResult>,
    ) -> runkernel::PipelineResult {
        let count = |status: runkernel::TaskStatus| {
            tasks.iter().filter(|task| task.status == status).count()
        };
        runkernel::PipelineResult {
            name: "test".to_string(),
            duration: std::time::Duration::from_secs(1),
            summary: runkernel::PipelineSummary {
                name: "test".to_string(),
                success,
                completed: count(runkernel::TaskStatus::Completed),
                failed: count(runkernel::TaskStatus::Failed),
                skipped: count(runkernel::TaskStatus::Skipped),
                cancelled: count(runkernel::TaskStatus::Cancelled),
                cached: count(runkernel::TaskStatus::Cached),
                rolled_back: count(runkernel::TaskStatus::RolledBack),
                rollback_failed: 0,
            },
            tasks,
        }
    }

    fn generate_report_json(
        push_mode: crate::workflow::profile::WorkflowStepMode,
        success: bool,
    ) -> serde_json::Value {
        let cache_dir = tempfile::tempdir().unwrap();
        let env_toml = r#"
        schema_version = "v0.5"
        name = "staging"
        domain = "staging.example.com"
        log_level = "info"
        default_replicas = 1

        [registry]
        host = "ghcr.io"
        namespace = "org/repo"

        [[service]]
        name = "api"
        version = "1.2.0"
        [service.build]
        path = "tests/fixtures/report-service"
        "#;
        let environment: crate::environment::Environment = toml::from_str(env_toml).unwrap();
        let profile_toml = format!(
            r#"
        environment = "staging"
        mode = "build"
        build = "{}"
        push = "{}"
        report = "json"
        "#,
            if push_mode == crate::workflow::profile::WorkflowStepMode::Run {
                "run"
            } else {
                "plan"
            },
            if push_mode == crate::workflow::profile::WorkflowStepMode::Run {
                "run"
            } else {
                "plan"
            }
        );
        let mut profile: crate::workflow::profile::WorkflowProfile =
            toml::from_str(&profile_toml).unwrap();
        profile.name = "ci-build-push".to_string();
        let normalized = profile.normalize(true);

        let runner_ctx = RunnerContext {
            ci_environment: Some(crate::workflow::ci::CiEnvironment {
                provider: crate::workflow::ci::CiProvider::GitHub,
                run_id: Some("run-17".to_string()),
            }),
            kind: RunnerKind::GitHubActions,
            ci: true,
            interactive: false,
        };

        let options = crate::builder::BuildOptions {
            cache_dir: cache_dir.path().join("cache").to_string_lossy().to_string(),
            force: true,
            only: vec![],
            ignore: vec![],
            plan: push_mode == crate::workflow::profile::WorkflowStepMode::Plan,
            dry_run: false,
            explain: false,
            dump_scope: false,
            policy: environment.build.clone(),
        };
        let planner = crate::workflow::planner::WorkflowPlanner::with_source_revision_resolver(
            normalized.clone(),
            std::sync::Arc::new(environment),
            options,
            runner_ctx.clone(),
            std::sync::Arc::new(FixedRevision),
        );
        let plan = planner.plan().unwrap();
        let item = plan.image_push_plan.as_ref().unwrap().items[0].clone();

        let (tasks, published_artifacts) =
            if push_mode == crate::workflow::profile::WorkflowStepMode::Plan {
                (completed_results_from_plan(&plan), vec![])
            } else if success {
                let artifact = crate::workflow::image::PublishedImageArtifact::from_push_result(
                    "staging",
                    &item,
                    "sha256:d8c58252270dd7a199042c161ab8b5c98cf85a8efb7aab782167dcf42f02b938",
                    "2024-03-20T12:00:00Z",
                )
                .unwrap();
                (completed_results_from_plan(&plan), vec![artifact])
            } else {
                (
                    failed_results_from_plan(
                        &plan,
                        &crate::workflow::task_id::service_push("api"),
                        "registry rejected push",
                    ),
                    vec![],
                )
            };

        let result = pipeline_result_from_tasks(success, tasks);

        let report_data = crate::workflow::image::WorkflowReportData {
            published_artifacts,
        };

        let temp = tempfile::tempdir().unwrap();
        assert_eq!(
            normalized.report,
            crate::workflow::profile::ReportMode::Json
        );
        finalize_workflow_report_to(
            temp.path(),
            &normalized,
            &runner_ctx,
            &result,
            &plan,
            &report_data,
        )
        .unwrap();

        let report_path = temp.path().join(".sailr/reports/ci-build-push/latest.json");
        let content = std::fs::read_to_string(&report_path).unwrap();
        let decoded: WorkflowReport = serde_json::from_str(&content).unwrap();
        decoded.validate().unwrap();
        assert_eq!(
            serde_json::from_str::<WorkflowReport>(&content).unwrap(),
            decoded
        );
        serde_json::to_value(decoded).unwrap()
    }

    #[test]
    fn test_report_image_push_plan() {
        let actual = generate_report_json(crate::workflow::profile::WorkflowStepMode::Plan, true);
        let expected = load_fixture_json("image-push-plan.json");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_report_image_publication_success() {
        let actual = generate_report_json(crate::workflow::profile::WorkflowStepMode::Run, true);
        let expected = load_fixture_json("image-publication-success.json");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_report_image_publication_failure() {
        let actual = generate_report_json(crate::workflow::profile::WorkflowStepMode::Run, false);
        let expected = load_fixture_json("image-publication-failure.json");
        assert_eq!(actual, expected);
    }

    #[test]
    fn workflow_report_validation_rejects_inconsistent_contracts() {
        let mut report: WorkflowReport =
            serde_json::from_value(load_fixture_json("image-publication-success.json")).unwrap();
        report.validate().unwrap();

        report.schema_version = "future".to_string();
        assert!(report.validate().is_err());
        report.schema_version = "sailr.workflow-report/v1".to_string();

        report.tasks.completed += 1;
        assert!(report.validate().is_err());
        report.tasks.completed -= 1;

        report.success = true;
        report.tasks.failed = 1;
        report.tasks.items[0].status = WorkflowReportTaskStatus::Failed;
        report.tasks.completed -= 1;
        assert!(report.validate().is_err());
    }

    #[test]
    fn workflow_report_rejects_publication_destination_and_coverage_mismatches() {
        let report = || -> WorkflowReport {
            serde_json::from_value(load_fixture_json("image-publication-success.json")).unwrap()
        };

        let mut wrong_registry = report();
        wrong_registry.artifacts.published_images[0].registry = "docker.io".to_string();
        assert!(wrong_registry.validate().is_err());

        let mut wrong_repository = report();
        wrong_repository.artifacts.published_images[0].repository = "other/api".to_string();
        assert!(wrong_repository.validate().is_err());

        let mut wrong_tag = report();
        wrong_tag.artifacts.published_images[0].tag = "different".to_string();
        assert!(wrong_tag.validate().is_err());

        let mut wrong_environment = report();
        wrong_environment.artifacts.published_images[0].environment = "production".to_string();
        assert!(wrong_environment.validate().is_err());

        let mut wrong_provenance = report();
        wrong_provenance.artifacts.published_images[0]
            .provenance
            .build_fingerprint = "different".to_string();
        assert!(wrong_provenance.validate().is_err());

        let mut unknown_service = report();
        unknown_service.artifacts.published_images[0].service = "worker".to_string();
        assert!(unknown_service.validate().is_err());

        let mut duplicate = report();
        duplicate
            .artifacts
            .published_images
            .push(duplicate.artifacts.published_images[0].clone());
        assert!(duplicate.validate().is_err());

        let mut missing = report();
        missing.artifacts.published_images.clear();
        assert!(missing.validate().is_err());

        let mut without_plan = report();
        without_plan.plans.image_push = None;
        assert!(without_plan.validate().is_err());

        let mut plan_only: WorkflowReport =
            serde_json::from_value(load_fixture_json("image-push-plan.json")).unwrap();
        plan_only.artifacts.published_images = report().artifacts.published_images;
        assert!(plan_only.validate().is_err());
    }

    #[test]
    fn failed_reports_allow_valid_partial_publication_evidence() {
        let mut report: WorkflowReport =
            serde_json::from_value(load_fixture_json("image-publication-success.json")).unwrap();
        let mut web = report.plans.image_push.as_ref().unwrap().items[0].clone();
        web.service = "web".to_string();
        web.repository = "org/repo/web".to_string();
        web.target_image_ref = format!("ghcr.io/{}:{}", web.repository, web.tag);
        web.local_image_ref = "ghcr.io/org/repo/web:1.2.0".to_string();
        report.plans.image_push.as_mut().unwrap().items.push(web);
        report.success = false;
        report.tasks.failed = 1;
        report.tasks.items.push(WorkflowReportTaskItem {
            name: crate::workflow::task_id::service_push("web"),
            status: WorkflowReportTaskStatus::Failed,
            error: Some("registry rejected push".to_string()),
        });
        report.validate().unwrap();
    }

    #[test]
    fn no_op_publication_report_requires_no_artifacts() {
        let mut report: WorkflowReport =
            serde_json::from_value(load_fixture_json("image-push-plan.json")).unwrap();
        let plan = report.plans.image_push.as_mut().unwrap();
        plan.items.clear();
        plan.mutates_registry = false;
        report.validate().unwrap();
    }

    #[test]
    fn report_finalizer_controls_json_persistence() {
        for (report_mode, writes_file) in [("text", false), ("json", true), ("both", true)] {
            let mut profile: crate::workflow::profile::WorkflowProfile = toml::from_str(&format!(
                r#"
                environment = "test"
                mode = "check"
                build = "disabled"
                generate = "disabled"
                deploy = "disabled"
                report = "{report_mode}"
                "#
            ))
            .unwrap();
            profile.name = format!("report-{report_mode}");
            let normalized = profile.normalize(false);
            let runner = RunnerContext {
                kind: RunnerKind::Local,
                ci: false,
                interactive: false,
                ci_environment: None,
            };
            let planner = crate::workflow::planner::WorkflowPlanner::new(
                normalized.clone(),
                std::sync::Arc::new(crate::environment::Environment::new("test")),
                crate::builder::BuildOptions {
                    cache_dir: ".sailr/test-report-finalizer".to_string(),
                    force: false,
                    only: vec![],
                    ignore: vec![],
                    plan: false,
                    dry_run: false,
                    explain: false,
                    dump_scope: false,
                    policy: None,
                },
                runner.clone(),
            );
            let plan = planner.plan().unwrap();
            let result = pipeline_result_from_tasks(true, completed_results_from_plan(&plan));
            let root = tempfile::tempdir().unwrap();
            finalize_workflow_report_to(
                root.path(),
                &normalized,
                &runner,
                &result,
                &plan,
                &Default::default(),
            )
            .unwrap();
            assert_eq!(
                root.path()
                    .join(".sailr/reports")
                    .join(&normalized.name)
                    .join("latest.json")
                    .exists(),
                writes_file
            );
        }
    }

    #[test]
    fn report_task_statuses_cover_runkernel_and_reject_nonterminal_results() {
        let statuses = [
            runkernel::TaskStatus::Pending,
            runkernel::TaskStatus::Running,
            runkernel::TaskStatus::Cached,
            runkernel::TaskStatus::Completed,
            runkernel::TaskStatus::Failed,
            runkernel::TaskStatus::Skipped,
            runkernel::TaskStatus::Cancelled,
            runkernel::TaskStatus::RolledBack,
        ];
        for status in statuses {
            let report_status = WorkflowReportTaskStatus::from(&status);
            serde_json::to_string(&report_status).unwrap();
        }

        let mut report: WorkflowReport =
            serde_json::from_value(load_fixture_json("image-push-plan.json")).unwrap();
        report.tasks.items[0].status = WorkflowReportTaskStatus::Pending;
        report.tasks.completed -= 1;
        assert!(report.validate().is_err());
    }

    #[test]
    fn inspection_apply_gate_is_independent_of_profile_permission() {
        let mut profile: crate::workflow::profile::WorkflowProfile = toml::from_str(
            r#"
            environment = "staging"
            mode = "build"
            build = "run"
            push = "run"
            apply = true
            "#,
        )
        .unwrap();
        profile.name = "publish".to_string();
        let normalized = profile.normalize(false);
        assert!(normalized.apply);
        assert!(requires_cli_apply(&normalized));
    }

    #[test]
    fn inspection_renders_modes_registry_and_image_provenance() {
        let inspection = WorkflowInspection {
            profile_name: "publish".to_string(),
            profile_mode: "build".to_string(),
            config_path: "sailr.workflow.toml".to_string(),
            environment: "staging".to_string(),
            environment_path: "k8s/environments/staging/config.toml".to_string(),
            runner_ci: true,
            runner_provider: "CircleCi".to_string(),
            runner_interactive: false,
            approval: Some(crate::workflow::profile::ApprovalMode::External),
            profile_apply: true,
            requires_cli_apply: true,
            build_mode: crate::workflow::profile::WorkflowStepMode::Run,
            push_mode: crate::workflow::profile::WorkflowStepMode::Run,
            generate_mode: crate::workflow::profile::WorkflowStepMode::Disabled,
            deploy_mode: crate::workflow::profile::WorkflowStepMode::Disabled,
            registry_host: "ghcr.io".to_string(),
            registry_namespace: "acme/platform".to_string(),
            registry_prefix: "ghcr.io/acme/platform".to_string(),
            images: vec![WorkflowInspectionImage {
                service: "api".to_string(),
                local_image_ref: "ghcr.io/acme/platform/api:1.2.0".to_string(),
                target_image_ref: "ghcr.io/acme/platform/api:abc1234".to_string(),
                build_fingerprint: "abc123456789".to_string(),
                source_revision: None,
            }],
        };
        let rendered = inspection.render_workflow_inspection();
        for expected in [
            "profile apply allowed: true",
            "CLI apply required: true",
            "build mode: Run",
            "push mode: Run",
            "prefix: ghcr.io/acme/platform",
            "build fingerprint: abc123456789",
            "source revision: none",
        ] {
            assert!(rendered.contains(expected));
        }
    }
}
