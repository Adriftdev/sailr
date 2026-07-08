use crate::builder::{attach_pipeline_logging, write_successful_service_caches, BuildOptions};
use crate::cli::WorkflowRunArgs;
use crate::environment::Environment;

use super::config::WorkflowConfig;

use super::planner::WorkflowPlanner;

pub struct WorkflowRunner;

impl WorkflowRunner {
    pub async fn run(args: WorkflowRunArgs) -> Result<(), String> {
        // 1. Load config and find profile
        let config = WorkflowConfig::load().map_err(|e| e.to_string())?;
        let profile = config
            .get_profile(&args.profile)
            .ok_or_else(|| format!("Workflow profile '{}' not found", args.profile))?;

        // 2. Load environment
        let env = Environment::load_from_file(&profile.environment)
            .map_err(|e| format!("Failed to load environment '{}': {}", profile.environment, e))?;

        // 3. Construct BuildOptions (incorporating CLI overrides)
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
            plan: profile.build.is_disabled() || profile.build == super::profile::WorkflowStepMode::Plan,
            dry_run: profile.build == super::profile::WorkflowStepMode::DryRun,
            explain: false,
            dump_scope: false,
            policy: env.build.clone(),
        };
        
        let _builder_guard = if let Some(ref remote) = profile.remote_builder {
            super::builder_context::RemoteBuilderContext::setup(&profile.name, remote)?
        } else {
            None
        };

        // 4. Plan Pipeline
        let planner = WorkflowPlanner::new(profile.clone(), std::sync::Arc::new(env), options);
        let (mut pipeline, build_plan) = planner.build_pipeline()?;

        // 5. Run Pipeline
        attach_pipeline_logging(&mut pipeline);
        
        crate::LOGGER.info(&format!("🚀 Running workflow profile '{}'", profile.name));
        let result = pipeline
            .run()
            .await
            .map_err(|e| format!("Pipeline execution failed: {:?}", e))?;

        // 6. Finalize
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
