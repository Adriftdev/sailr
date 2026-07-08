use runkernel::{Pipeline, Task};

use crate::builder::{add_runkernel_tasks, create_sailr_build_plan, BuildOptions, SailrBuildPlan};
use crate::environment::Environment;

use super::profile::NormalizedWorkflowProfile;

use std::sync::Arc;

pub enum WorkflowBuildExecution {
    None,
    PlanOnly(SailrBuildPlan),
    Executed(SailrBuildPlan),
}

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

    pub fn build_pipeline(&self) -> Result<(Pipeline, WorkflowBuildExecution), String> {
        let mut pipeline = Pipeline::new(format!("Workflow: {}", self.profile.name));
        let mut build_execution = WorkflowBuildExecution::None;

        // 0. Validate Phase

        let validate_task = Task::new("workflow:validate-config").exec_fn(move |_ctx| async move {
            crate::LOGGER.info("Validating Sailr environment config...");
            Ok(())
        });
        pipeline.add(validate_task);
        let mut last_tasks: Vec<String> = vec!["workflow:validate-config".to_string()];

        // 1. Build Phase
        match self.profile.build {
            crate::workflow::profile::WorkflowStepMode::Disabled => {}
            crate::workflow::profile::WorkflowStepMode::DryRun => {
                return Err("workflow build dry-run is not enabled in this PR".to_string());
            }
            crate::workflow::profile::WorkflowStepMode::Plan => {
                let plan = create_sailr_build_plan(&self.env, &self.options)?;
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
                build_execution = WorkflowBuildExecution::PlanOnly(plan);
            }
            crate::workflow::profile::WorkflowStepMode::Run => {
                let plan = create_sailr_build_plan(&self.env, &self.options)?;
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
                build_execution = WorkflowBuildExecution::Executed(plan);
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

        Ok((pipeline, build_execution))
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
    fn ci_profile_validate_only() {
        let env = Environment::new("local");
        let mut profile = dummy_profile(WorkflowStepMode::Disabled, WorkflowStepMode::Disabled);
        profile.generate = WorkflowStepMode::Disabled;
        let planner = WorkflowPlanner::new(profile, Arc::new(env), dummy_options(false));
        let (pipeline, _) = planner.build_pipeline().unwrap();
        let task_names: Vec<String> = pipeline.tasks().map(|t| t.name.clone()).collect();
        assert_eq!(task_names, vec!["workflow:validate-config"]);
    }

    #[test]
    fn ci_build_plan_creates_build_plan() {
        let mut env = Environment::new("local");
        let mut svc = crate::environment::Service::new("dummy", None, "latest");
        svc.build = Some(crate::environment::ServiceBuildConfig {
            path: ".".to_string(),
            include: None,
            relies_on: None,
            before_synchronous: None,
            before: None,
            run_parallel: None,
            run_synchronous: None,
            after: None,
            finally: None,
            dockerfile: None,
            build_command: None,
            push_command: None,
        });
        env.services.push(svc);

        let mut profile = dummy_profile(WorkflowStepMode::Disabled, WorkflowStepMode::Plan);
        profile.generate = WorkflowStepMode::Disabled;
        let planner = WorkflowPlanner::new(profile, Arc::new(env), dummy_options(true));
        let (pipeline, _) = planner.build_pipeline().unwrap();
        let mut task_names: Vec<String> = pipeline.tasks().map(|t| t.name.clone()).collect();
        task_names.sort();
        let mut expected = vec![
            "workflow:validate-config".to_string(),
            "workflow:build-plan".to_string(),
        ];
        expected.sort();
        assert_eq!(task_names, expected);
    }

    #[test]
    fn ci_generate_creates_generate() {
        let mut env = Environment::new("local");
        let mut svc = crate::environment::Service::new("dummy", None, "latest");
        svc.build = Some(crate::environment::ServiceBuildConfig {
            path: ".".to_string(),
            include: None,
            relies_on: None,
            before_synchronous: None,
            before: None,
            run_parallel: None,
            run_synchronous: None,
            after: None,
            finally: None,
            dockerfile: None,
            build_command: None,
            push_command: None,
        });
        env.services.push(svc);

        let profile = dummy_profile(WorkflowStepMode::Disabled, WorkflowStepMode::Plan);
        let planner = WorkflowPlanner::new(profile, Arc::new(env), dummy_options(true));
        let (pipeline, _) = planner.build_pipeline().unwrap();
        let mut task_names: Vec<String> = pipeline.tasks().map(|t| t.name.clone()).collect();
        task_names.sort();
        let mut expected = vec![
            "workflow:validate-config".to_string(),
            "workflow:build-plan".to_string(),
            "workflow:generate".to_string(),
        ];
        expected.sort();
        assert_eq!(task_names, expected);
    }
}
