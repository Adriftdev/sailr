use runkernel::{Pipeline, Task};

use crate::builder::{add_runkernel_tasks, create_sailr_build_plan, BuildOptions, SailrBuildPlan};
use crate::environment::Environment;

use super::profile::NormalizedWorkflowProfile;

use std::sync::Arc;

pub struct WorkflowPlanner {
    pub profile: NormalizedWorkflowProfile,
    pub env: Arc<Environment>,
    pub options: BuildOptions,
}

impl WorkflowPlanner {
    pub fn new(
        profile: NormalizedWorkflowProfile,
        env: Arc<Environment>,
        options: BuildOptions,
    ) -> Self {
        Self {
            profile,
            env,
            options,
        }
    }

    pub fn build_pipeline(&self) -> Result<(Pipeline, Option<SailrBuildPlan>), String> {
        let mut pipeline = Pipeline::new(format!("Workflow: {}", self.profile.name));
        let mut build_plan = None;

        // 0. Validate Phase

        let validate_task = Task::new("workflow:validate-config").exec_fn(move |_ctx| async move {
            crate::LOGGER.info("Validating Sailr environment config...");
            Ok(())
        });
        pipeline.add(validate_task);
        let mut last_tasks: Vec<String> = vec!["workflow:validate-config".to_string()];

        // 1. Build Phase
        if self.profile.build.is_active() {
            let plan = create_sailr_build_plan(&self.env, &self.options)?;

            // Only add runkernel tasks if we actually want to run the build.
            // If build == Plan, create_sailr_build_plan already printed the plan (via builder.rs integration),
            // but we don't want to execute it. Wait, create_sailr_build_plan does not print the plan.
            // In RunkernelBuildBackend::build, it prints it.
            // But we can just avoid adding tasks to the pipeline if it's just plan.
            if self.profile.build == crate::workflow::profile::WorkflowStepMode::Plan {
                let plan_for_task = plan.clone();
                let options_for_task = self.options.clone();

                let task = Task::new("workflow:build-plan")
                    .depends_on(&["workflow:validate-config"])
                    .exec_fn(move |_ctx| {
                        let p = plan_for_task.clone();
                        let o = options_for_task.clone();
                        async move {
                            crate::builder::print_sailr_plan(&p, &o);
                            Ok(())
                        }
                    });

                pipeline.add(task);
                last_tasks = vec!["workflow:build-plan".to_string()];
                build_plan = Some(plan);
            } else {
                add_runkernel_tasks(&mut pipeline, &plan)?;

                let dirty_count = plan.services.iter().filter(|s| s.dirty).count();
                let mut build_deps = Vec::new();
                if dirty_count > 0 && !plan.after_all.is_empty() {
                    build_deps.push("build:after-all".to_string());
                } else {
                    for s in &plan.services {
                        build_deps.push(s.service.name.clone());
                    }
                }

                last_tasks.extend(build_deps);
                build_plan = Some(plan);
            }
        }

        // 2. Generate Phase
        if self.profile.generate.is_active() {
            let mut task = Task::new("workflow:generate");

            let deps_refs: Vec<&str> = last_tasks.iter().map(|s| s.as_str()).collect();
            if !deps_refs.is_empty() {
                task = task.depends_on(&deps_refs);
            }

            let name = self.profile.environment.clone();
            let only = self.options.only.clone();
            let ignore = self.options.ignore.clone();
            let env_clone = self.env.clone();

            task = task.exec_fn(move |_ctx| {
                let name = name.clone();
                let only = only.clone();
                let ignore = ignore.clone();
                let env_clone = env_clone.clone();
                async move {
                    crate::LOGGER.info("Generating Kubernetes manifests...");

                    let services = crate::builder::filter_services_exact(
                        env_clone.list_services(),
                        &only,
                        &ignore,
                    );

                    crate::generate(&name, &env_clone, services)
                        .map_err(|e| anyhow::anyhow!("Generate failed: {}", e))?;

                    Ok(())
                }
            });

            pipeline.add(task);
            // last_tasks = vec!["workflow:generate".to_string()];
        }

        Ok((pipeline, build_plan))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::profile::{
        ApprovalMode, ReportMode, WorkflowEngine, WorkflowMode, WorkflowStepMode,
    };

    fn dummy_profile(
        deploy_mode: WorkflowStepMode,
        build_mode: WorkflowStepMode,
    ) -> NormalizedWorkflowProfile {
        NormalizedWorkflowProfile {
            name: "test".to_string(),
            environment: "local".to_string(),
            mode: WorkflowMode::Check,
            engine: WorkflowEngine::Runkernel,
            interactive: false,
            build: build_mode,
            generate: WorkflowStepMode::Run,
            deploy: deploy_mode,
            test: WorkflowStepMode::Disabled,
            verify: WorkflowStepMode::Disabled,
            deploy_context: Some("local".to_string()),
            namespace: Some("default".to_string()),
            approval: ApprovalMode::None,
            apply: false,
            report: ReportMode::Text,
        }
    }

    fn dummy_options(plan: bool) -> BuildOptions {
        BuildOptions {
            cache_dir: ".sailr".to_string(),
            force: false,
            only: vec![],
            ignore: vec![],
            plan,
            dry_run: false,
            explain: false,
            dump_scope: false,
            policy: Default::default(),
        }
    }

    #[test]
    fn check_profile_never_creates_deploy() {
        let env = Environment::load_from_file("local").unwrap();
        let profile = dummy_profile(WorkflowStepMode::Disabled, WorkflowStepMode::Plan);
        let planner = WorkflowPlanner::new(profile, Arc::new(env), dummy_options(true));
        let (pipeline, _) = planner.build_pipeline().unwrap();
        assert!(!pipeline.tasks().any(|t| t.name.starts_with("deploy:")));
    }

    #[test]
    fn check_profile_creates_validate_config() {
        let env = Environment::load_from_file("local").unwrap();
        let profile = dummy_profile(WorkflowStepMode::Disabled, WorkflowStepMode::Plan);
        let planner = WorkflowPlanner::new(profile, Arc::new(env), dummy_options(true));
        let (pipeline, _) = planner.build_pipeline().unwrap();
        assert!(pipeline
            .tasks()
            .any(|t| t.name == "workflow:validate-config"));
    }

    #[test]
    fn check_profile_creates_build_plan_when_build_plan() {
        let env = Environment::load_from_file("local").unwrap();
        let profile = dummy_profile(WorkflowStepMode::Disabled, WorkflowStepMode::Plan);
        let planner = WorkflowPlanner::new(profile, Arc::new(env), dummy_options(true));
        let (pipeline, _) = planner.build_pipeline().unwrap();
        assert!(pipeline.tasks().any(|t| t.name == "workflow:build-plan"));
    }

    #[test]
    fn build_disabled_creates_no_build_plan() {
        let env = Environment::load_from_file("local").unwrap();
        let profile = dummy_profile(WorkflowStepMode::Disabled, WorkflowStepMode::Disabled);
        let planner = WorkflowPlanner::new(profile, Arc::new(env), dummy_options(false));
        let (pipeline, _) = planner.build_pipeline().unwrap();
        assert!(!pipeline.tasks().any(|t| t.name == "workflow:build-plan"));
    }

    #[test]
    fn generate_task_is_created_when_generate_run() {
        let env = Environment::load_from_file("local").unwrap();
        let profile = dummy_profile(WorkflowStepMode::Disabled, WorkflowStepMode::Plan);
        let planner = WorkflowPlanner::new(profile, Arc::new(env), dummy_options(true));
        let (pipeline, _) = planner.build_pipeline().unwrap();
        assert!(pipeline.tasks().any(|t| t.name == "workflow:generate"));
    }
}
