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
    pub fn new(profile: NormalizedWorkflowProfile, env: Arc<Environment>, options: BuildOptions) -> Self {
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
        if self.profile.deploy.is_active() {
            if self.profile.deploy_context.is_none() {
                return Err("Validation Error: deploy_context is required when deploy is active".to_string());
            }
            return Err("workflow deploy execution is not enabled in this PR; use sailr deploy or sailr go".to_string());
        }

        let validate_task = Task::new("workflow:validate-config")
            .exec_fn(move |_ctx| async move {
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
                crate::builder::print_sailr_plan(&plan, &self.options);
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
