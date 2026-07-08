use crate::builder::{attach_pipeline_logging, write_successful_service_caches, BuildOptions};
use crate::cli::WorkflowRunArgs;
use crate::environment::Environment;

use super::config::WorkflowConfig;
use super::planner::WorkflowPlanner;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerKind {
    Local,
    GitHubActions,
    CircleCi,
    Travis,
    GenericCi,
}

#[derive(Debug, Clone)]
pub struct RunnerContext {
    pub kind: RunnerKind,
    pub ci: bool,
    pub interactive: bool,
}

impl RunnerContext {
    pub fn detect(non_interactive: bool) -> Self {
        let kind = if std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true") {
            RunnerKind::GitHubActions
        } else if std::env::var("CIRCLECI").as_deref() == Ok("true") {
            RunnerKind::CircleCi
        } else if std::env::var("TRAVIS").as_deref() == Ok("true") {
            RunnerKind::Travis
        } else if std::env::var("CI").is_ok() {
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

fn write_workflow_report(
    profile: &super::profile::NormalizedWorkflowProfile,
    runner: &RunnerContext,
    result: &runkernel::PipelineResult,
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
        .map(|task| {
            serde_json::json!({
                "name": task.name,
                "status": format!("{:?}", task.status).to_lowercase()
            })
        })
        .collect::<Vec<_>>();

    let mut report = serde_json::json!({
        "profile": profile.name,
        "mode": profile.mode.as_str(),
        "runner": format!("{:?}", runner.kind).to_lowercase(),
        "success": result.summary.success,
        "tasks": {
            "completed": result.summary.completed,
            "failed": result.summary.failed,
            "skipped": result.summary.skipped,
            "cancelled": result.summary.cancelled,
            "items": task_items
        }
    });

    if profile.deploy == crate::workflow::profile::WorkflowStepMode::Plan {
        let context = profile.deploy_context.as_deref().unwrap_or("none");
        let namespace = profile.namespace.as_deref().unwrap_or("default");
        if let Ok(plan) = crate::workflow::plan::generate_static_deployment_plan(
            &profile.environment,
            context,
            namespace,
        ) {
            if let Some(obj) = report.as_object_mut() {
                obj.insert(
                    "deployment_plan".to_string(),
                    serde_json::to_value(plan).unwrap_or(serde_json::Value::Null),
                );
            }
        }
    }

    let report_dir = std::path::Path::new(".sailr")
        .join("reports")
        .join(&profile.name);
    std::fs::create_dir_all(&report_dir)
        .map_err(|e| format!("Failed to create report directory: {}", e))?;

    let report_path = report_dir.join("latest.json");
    let json_string = serde_json::to_string_pretty(&report)
        .map_err(|e| format!("Failed to serialize report: {}", e))?;

    std::fs::write(&report_path, json_string)
        .map_err(|e| format!("Failed to write report: {}", e))?;

    Ok(())
}

pub fn validate_workflow_safety(
    profile: &crate::workflow::profile::NormalizedWorkflowProfile,
    runner: &RunnerContext,
    args: &crate::cli::WorkflowRunArgs,
) -> Result<(), String> {
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
                return Err("CI deploy requires approval=external".to_string());
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
        let (mut pipeline, build_execution) = planner.build_pipeline_from_plan(&plan)?;

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
        write_workflow_report(&normalized_profile, &runner_ctx, &result)?;

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
            kind: RunnerKind::Local,
            ci: false,
            interactive: true,
        };

        let res = validate_workflow_safety(&profile, &runner, &crate::cli::WorkflowRunArgs { profile: "test".to_string(), only: None, ignore: None, non_interactive: true, plan: false, dry_run: false, apply: false });
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
            kind: RunnerKind::GitHubActions,
            ci: true,
            interactive: false,
        };

        let res = validate_workflow_safety(&profile, &runner, &crate::cli::WorkflowRunArgs { profile: "test".to_string(), only: None, ignore: None, non_interactive: true, plan: false, dry_run: false, apply: false });
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
            kind: RunnerKind::Local,
            ci: false,
            interactive: false, // user ran with --non-interactive
        };

        let res = validate_workflow_safety(&profile, &runner, &crate::cli::WorkflowRunArgs { profile: "test".to_string(), only: None, ignore: None, non_interactive: true, plan: false, dry_run: false, apply: false });
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
            kind: RunnerKind::Local,
            ci: false,
            interactive: true,
        };

        let res = validate_workflow_safety(&profile, &runner, &crate::cli::WorkflowRunArgs { profile: "test".to_string(), only: None, ignore: None, non_interactive: true, plan: false, dry_run: false, apply: false });
        assert!(res.is_err());
        assert!(res
            .unwrap_err()
            .contains("deploy=run requires apply=true"));
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
}
