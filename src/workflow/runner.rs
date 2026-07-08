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

fn validate_workflow_safety(
    profile: &crate::workflow::profile::NormalizedWorkflowProfile,
    runner: &RunnerContext,
) -> Result<(), String> {
    if runner.ci && profile.interactive {
        return Err("workflow cannot be interactive in CI".to_string());
    }

    if profile.deploy.is_active() {
        if profile.deploy_context.is_none() {
            return Err(
                "Validation Error: deploy_context is required when deploy is active".to_string(),
            );
        }

        return Err(
            "workflow deploy execution is not enabled in this PR; use sailr deploy or sailr go"
                .to_string(),
        );
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
            plan: args.plan || normalized_profile.build == super::profile::WorkflowStepMode::Plan,
            dry_run: args.dry_run
                || normalized_profile.build == super::profile::WorkflowStepMode::DryRun,
            explain: false,
            dump_scope: false,
            policy: env.build.clone(),
        };

        // 6. Safety validation
        validate_workflow_safety(&normalized_profile, &runner_ctx)?;

        // 7. Plan Pipeline
        let planner = WorkflowPlanner::new(
            normalized_profile.clone(),
            std::sync::Arc::new(env),
            options,
        );
        let (mut pipeline, build_plan) = planner.build_pipeline()?;

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
        if let Some(plan) = build_plan {
            crate::builder::print_pipeline_result(&plan, &result);
            if result.summary.success {
                write_successful_service_caches(&plan, &result)?;
            }
        }

        if !result.summary.success {
            return Err(format!(
                "Workflow failed: {} failed, {} skipped, {} cancelled",
                result.summary.failed, result.summary.skipped, result.summary.cancelled
            ));
        }

        crate::LOGGER.info("✅ Workflow completed successfully.");
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
}
