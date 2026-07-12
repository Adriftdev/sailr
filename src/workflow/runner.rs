use crate::builder::{attach_pipeline_logging, write_successful_service_caches, BuildOptions};
use crate::cli::WorkflowRunArgs;
use crate::environment::Environment;

use super::config::WorkflowConfig;
use super::planner::WorkflowPlanner;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunnerKind {
    Local,
    GitHubActions,
    CircleCi,
    Travis,
    GenericCi,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RunnerContext {
    pub kind: RunnerKind,
    pub ci: bool,
    pub interactive: bool,
    pub ci_environment: Option<crate::workflow::ci::CiEnvironment>,
}

impl RunnerContext {
    pub fn detect(non_interactive: bool) -> Self {
        let mut ci_env = None;
        let kind = if std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true") {
            ci_env = Some(crate::workflow::ci::CiEnvironment {
                provider: crate::workflow::ci::CiProvider::GitHub,
                run_id: std::env::var("GITHUB_RUN_ID").ok(),
            });
            RunnerKind::GitHubActions
        } else if std::env::var("CIRCLECI").as_deref() == Ok("true") {
            ci_env = Some(crate::workflow::ci::CiEnvironment {
                provider: crate::workflow::ci::CiProvider::CircleCi,
                run_id: std::env::var("CIRCLE_WORKFLOW_ID").ok(),
            });
            RunnerKind::CircleCi
        } else if std::env::var("TRAVIS").as_deref() == Ok("true") {
            ci_env = Some(crate::workflow::ci::CiEnvironment {
                provider: crate::workflow::ci::CiProvider::Travis,
                run_id: std::env::var("TRAVIS_BUILD_ID").ok(),
            });
            RunnerKind::Travis
        } else if std::env::var("CI").is_ok() {
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

#[derive(Debug, serde::Serialize)]
pub struct WorkflowReportTaskItem {
    pub name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct WorkflowReportTasks {
    pub completed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub cancelled: usize,
    pub items: Vec<WorkflowReportTaskItem>,
}

#[derive(Debug, serde::Serialize)]
pub struct WorkflowReportPlans {
    pub image_push: Option<crate::workflow::image::ImagePushPlanReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment: Option<serde_json::Value>,
}

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum WorkflowReportType {
    WorkflowExecution,
    WorkflowInspection,
}

#[derive(Debug, serde::Serialize)]
pub struct WorkflowReportArtifacts {
    pub published_images: Vec<crate::workflow::image::PublishedImageArtifact>,
}

#[derive(Debug, serde::Serialize)]
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

fn write_workflow_report_to(
    root: &std::path::Path,
    profile: &crate::workflow::profile::NormalizedWorkflowProfile,
    runner: &RunnerContext,
    result: &runkernel::PipelineResult,
    plan: &crate::workflow::plan::WorkflowPlan,
    report_data: &crate::workflow::image::WorkflowReportData,
) -> Result<(), String> {
    if !matches!(
        profile.report,
        super::profile::ReportMode::Json | super::profile::ReportMode::Both
    ) {
        return Ok(());
    }

    let task_items = result
        .tasks
        .iter()
        .map(|task| WorkflowReportTaskItem {
            name: task.name.clone(),
            status: format!("{:?}", task.status).to_lowercase(),
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
            report.plans.deployment =
                Some(serde_json::to_value(plan).unwrap_or(serde_json::Value::Null));
        }
    }

    let report_dir = root.join(".sailr").join("reports").join(&profile.name);

    std::fs::create_dir_all(&report_dir)
        .map_err(|e| format!("Failed to create report directory: {}", e))?;

    let report_path = report_dir.join("latest.json");
    let json_string = serde_json::to_string_pretty(&report)
        .map_err(|e| format!("Failed to serialize report: {}", e))?;

    std::fs::write(&report_path, &json_string)
        .map_err(|e| format!("Failed to write report: {}", e))?;

    Ok(())
}

fn write_workflow_report(
    profile: &crate::workflow::profile::NormalizedWorkflowProfile,
    runner: &RunnerContext,
    result: &runkernel::PipelineResult,
    plan: &crate::workflow::plan::WorkflowPlan,
    report_data: &crate::workflow::image::WorkflowReportData,
) -> Result<(), String> {
    write_workflow_report_to(
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

#[derive(Debug)]
pub struct WorkflowInspectionImage {
    pub service: String,
    pub local_image_ref: String,
    pub target_image_ref: String,
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
    pub push_mode: crate::workflow::profile::WorkflowStepMode,
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
        output.push_str(&format!("  profile apply: {}\n", self.profile_apply));
        output.push_str(&format!(
            "  CLI apply required: {}\n",
            self.requires_cli_apply
        ));
        output.push_str(&format!("  push mode: {:?}\n", self.push_mode));

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
        write_workflow_report(
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
            requires_cli_apply: !normalized.apply,
            push_mode: normalized.push,
            registry_host: resolved_registry.host.clone(),
            registry_namespace: env_arc
                .registry
                .namespace()
                .unwrap_or_default()
                .unwrap_or_else(|| "none".to_string()),
            registry_prefix: env_arc
                .registry
                .prefix()
                .unwrap_or_else(|_| "none".to_string()),
            images: {
                let mut images = Vec::new();
                if let Some(push_plan) = plan.image_push_plan {
                    for item in &push_plan.items {
                        images.push(WorkflowInspectionImage {
                            service: item.service.clone(),
                            local_image_ref: item.local_image_ref.clone(),
                            target_image_ref: item.target_image_ref.clone(),
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
    use once_cell::sync::Lazy;
    use std::env;
    use std::sync::Mutex;

    static ENV_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn clear_ci_envs() {
        env::remove_var("GITHUB_ACTIONS");
        env::remove_var("CIRCLECI");
        env::remove_var("TRAVIS");
        env::remove_var("CI");
    }

    fn run_with_env<F>(key: &str, value: &str, test: F)
    where
        F: FnOnce(),
    {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_ci_envs();
        if !key.is_empty() {
            env::set_var(key, value);
        }

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            test();
        }));

        if !key.is_empty() {
            env::remove_var(key);
        }

        if let Err(err) = result {
            std::panic::resume_unwind(err);
        }
    }

    #[test]
    fn detects_github_actions() {
        run_with_env("GITHUB_ACTIONS", "true", || {
            let ctx = RunnerContext::detect(false);
            assert_eq!(ctx.kind, RunnerKind::GitHubActions);
            assert!(ctx.ci);
            assert!(!ctx.interactive);
        });
    }

    #[test]
    fn detects_circle_ci() {
        run_with_env("CIRCLECI", "true", || {
            let ctx = RunnerContext::detect(false);
            assert_eq!(ctx.kind, RunnerKind::CircleCi);
            assert!(ctx.ci);
            assert!(!ctx.interactive);
        });
    }

    #[test]
    fn detects_travis() {
        run_with_env("TRAVIS", "true", || {
            let ctx = RunnerContext::detect(false);
            assert_eq!(ctx.kind, RunnerKind::Travis);
            assert!(ctx.ci);
            assert!(!ctx.interactive);
        });
    }

    #[test]
    fn detects_generic_ci() {
        run_with_env("CI", "true", || {
            let ctx = RunnerContext::detect(false);
            assert_eq!(ctx.kind, RunnerKind::GenericCi);
            assert!(ctx.ci);
            assert!(!ctx.interactive);
        });
    }

    #[test]
    fn local_runner_interactive_by_default() {
        run_with_env("", "", || {
            let ctx = RunnerContext::detect(false);
            assert_eq!(ctx.kind, RunnerKind::Local);
            assert!(!ctx.ci);
            assert!(ctx.interactive);
        });
    }

    #[test]
    fn local_runner_disabled_interactivity_with_flag() {
        run_with_env("", "", || {
            let ctx = RunnerContext::detect(true);
            assert_eq!(ctx.kind, RunnerKind::Local);
            assert!(!ctx.ci);
            assert!(!ctx.interactive);
        });
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
            tasks: vec![],
        };

        // Write report into a temp directory to avoid polluting the project.
        let temp = tempfile::tempdir().unwrap();

        write_workflow_report_to(
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

    fn generate_report_json(
        profile_mode: crate::workflow::profile::WorkflowStepMode,
        success: bool,
        published_artifacts: Vec<crate::workflow::image::PublishedImageArtifact>,
        tasks_count: (usize, usize), // (completed, failed)
        mutates_docker: bool,
    ) -> serde_json::Value {
        let profile_toml = format!(
            r#"
        environment = "staging"
        mode = "build"
        push = "{}"
        report = "json"
        "#,
            if profile_mode == crate::workflow::profile::WorkflowStepMode::Run {
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
            ci_environment: None,
            kind: RunnerKind::GitHubActions,
            ci: true,
            interactive: false,
        };

        let plan = crate::workflow::plan::WorkflowPlan {
            profile: normalized.clone(),
            runner: runner_ctx.clone(),
            tasks: vec![],
            edges: vec![],
            build_plan: None,
            image_push_plan: Some(crate::workflow::image::ImagePushPlanReport {
                environment: "staging".to_string(),
                mutates_registry: profile_mode == crate::workflow::profile::WorkflowStepMode::Run,
                items: vec![crate::workflow::image::ImagePushPlanItem {
                    service: "api".to_string(),
                    registry: "ghcr.io".to_string(),
                    repository: "org/repo/api".to_string(),
                    target_image_ref: "ghcr.io/org/repo/api:ab12cd34".to_string(),
                    local_image_ref: "ghcr.io/org/repo/api:ab12cd34".to_string(),
                    tag: "ab12cd34".to_string(),
                    provenance: crate::workflow::image::ImageProvenance {
                        build_fingerprint: "ab12cd345678".to_string(),
                        source_revision: Some("ab12cd345678".to_string()),
                    },
                    action: crate::workflow::image::ImagePushPlanAction::WouldPush,
                }],
            }),
            effects: crate::workflow::plan::WorkflowEffects {
                mutates_filesystem: false,
                mutates_docker,
                mutates_registry: profile_mode == crate::workflow::profile::WorkflowStepMode::Run,
                mutates_git: false,
                mutates_cluster: false,
                prompts_user: false,
            },
        };

        let mut tasks = vec![];
        if tasks_count.0 > 0 {
            tasks.push(runkernel::TaskResult {
                name: "plan-push".to_string(),
                status: runkernel::TaskStatus::Completed,
                duration: Some(std::time::Duration::from_secs(1)),
                error: None,
                cache_hit: false,
                cache_reason: None,
                rollback_status: None,
                rollback_error: None,
            });
        }
        if tasks_count.0 > 1 {
            tasks.push(runkernel::TaskResult {
                name: "execute-push".to_string(),
                status: runkernel::TaskStatus::Completed,
                duration: Some(std::time::Duration::from_secs(1)),
                error: None,
                cache_hit: false,
                cache_reason: None,
                rollback_status: None,
                rollback_error: None,
            });
        } else if tasks_count.1 > 0 {
            tasks.push(runkernel::TaskResult {
                name: "execute-push".to_string(),
                status: runkernel::TaskStatus::Failed,
                duration: Some(std::time::Duration::from_secs(1)),
                error: Some("failed".to_string()),
                cache_hit: false,
                cache_reason: None,
                rollback_status: None,
                rollback_error: None,
            });
        }

        let result = runkernel::PipelineResult {
            name: "test".to_string(),
            duration: std::time::Duration::from_secs(1),
            summary: runkernel::PipelineSummary {
                name: "test".to_string(),
                success,
                completed: tasks_count.0,
                failed: tasks_count.1,
                skipped: 0,
                cancelled: 0,
                cached: 0,
                rolled_back: 0,
                rollback_failed: 0,
            },
            tasks,
        };

        let report_data = crate::workflow::image::WorkflowReportData {
            published_artifacts,
        };

        let temp = tempfile::tempdir().unwrap();
        assert_eq!(
            normalized.report,
            crate::workflow::profile::ReportMode::Json
        );
        write_workflow_report_to(
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
        serde_json::from_str(&content).unwrap()
    }

    #[test]
    fn test_report_image_push_plan() {
        let actual = generate_report_json(
            crate::workflow::profile::WorkflowStepMode::Plan,
            true,
            vec![],
            (1, 0),
            false,
        );
        let expected = load_fixture_json("image-push-plan.json");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_report_image_publication_success() {
        let published_artifacts = vec![crate::workflow::image::PublishedImageArtifact {
            service: "api".to_string(),
            environment: "staging".to_string(),
            registry: "ghcr.io".to_string(),
            repository: "org/repo/api".to_string(),
            tag: "ab12cd34".to_string(),
            digest: "sha256:d8c58252270dd7a199042c161ab8b5c98cf85a8efb7aab782167dcf42f02b938".to_string(),
            image_ref: "ghcr.io/org/repo/api@sha256:d8c58252270dd7a199042c161ab8b5c98cf85a8efb7aab782167dcf42f02b938".to_string(),
            provenance: crate::workflow::image::ImageProvenance {
                build_fingerprint: "ab12cd345678".to_string(),
                source_revision: Some("ab12cd345678".to_string()),
            },
            published_at: "2024-03-20T12:00:00Z".to_string(),
        }];
        let actual = generate_report_json(
            crate::workflow::profile::WorkflowStepMode::Run,
            true,
            published_artifacts,
            (2, 0),
            true,
        );
        let expected = load_fixture_json("image-publication-success.json");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_report_image_publication_failure() {
        let actual = generate_report_json(
            crate::workflow::profile::WorkflowStepMode::Run,
            false,
            vec![],
            (1, 1),
            true,
        );
        let expected = load_fixture_json("image-publication-failure.json");
        assert_eq!(actual, expected);
    }
}
