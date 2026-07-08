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
            plan: args.plan || normalized_profile.build.is_disabled() || normalized_profile.build == super::profile::WorkflowStepMode::Plan,
            dry_run: args.dry_run || normalized_profile.build == super::profile::WorkflowStepMode::DryRun,
            explain: false,
            dump_scope: false,
            policy: env.build.clone(),
        };

        // 6. Plan Pipeline
        let planner = WorkflowPlanner::new(normalized_profile.clone(), std::sync::Arc::new(env), options);
        let (mut pipeline, build_plan) = planner.build_pipeline()?;

        // 7. Run Pipeline
        attach_pipeline_logging(&mut pipeline);

        crate::LOGGER.info(&format!("🚀 Running workflow profile '{}'", normalized_profile.name));
        let result = pipeline
            .run()
            .await
            .map_err(|e| format!("Pipeline execution failed: {:?}", e))?;

        // 8. Finalize
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
