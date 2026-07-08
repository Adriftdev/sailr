

use runkernel::{Pipeline, Task};

use crate::builder::{add_runkernel_tasks, create_sailr_build_plan, BuildOptions, SailrBuildPlan};
use crate::environment::Environment;

use super::profile::{ApprovalMode, WorkflowProfile};

use std::sync::Arc;

pub struct WorkflowPlanner {
    pub profile: WorkflowProfile,
    pub env: Arc<Environment>,
    pub options: BuildOptions,
}

impl WorkflowPlanner {
    pub fn new(profile: WorkflowProfile, env: Arc<Environment>, options: BuildOptions) -> Self {
        Self {
            profile,
            env,
            options,
        }
    }

    pub fn build_pipeline(&self) -> Result<(Pipeline, Option<SailrBuildPlan>), String> {
        let mut pipeline = Pipeline::new(format!("Workflow: {}", self.profile.name));
        let mut build_plan = None;

        // 1. Build Phase
        let mut last_tasks: Vec<String> = Vec::new();
        if self.profile.build.is_active() {
            let plan = create_sailr_build_plan(&self.env, &self.options)?;
            add_runkernel_tasks(&mut pipeline, &plan)?;
            
            let dirty_count = plan.services.iter().filter(|s| s.dirty).count();
            if dirty_count > 0 && !plan.after_all.is_empty() {
                last_tasks.push("build:after-all".to_string());
            } else {
                for s in &plan.services {
                    last_tasks.push(s.service.name.clone());
                }
            }
            build_plan = Some(plan);
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
            last_tasks = vec!["workflow:generate".to_string()];
        }

        // 3. Approval Phase (if prompt)
        if self.profile.approval == ApprovalMode::Prompt {
            let mut task = Task::new("workflow:approve");
            let deps_refs: Vec<&str> = last_tasks.iter().map(|s| s.as_str()).collect();
            if !deps_refs.is_empty() {
                task = task.depends_on(&deps_refs);
            }
            
            task = task.exec_fn(move |_ctx| {
                async move {
                    crate::LOGGER.info("Waiting for user approval...");
                    
                    // Use tokio's spawn_blocking for the synchronous inquire prompt
                    let confirm = tokio::task::spawn_blocking(|| {
                        inquire::Confirm::new("Proceed with deployment?")
                            .with_default(true)
                            .prompt()
                    })
                    .await
                    .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
                    .map_err(|e| anyhow::anyhow!("Prompt error: {}", e))?;

                    if !confirm {
                        return Err(anyhow::anyhow!("Deployment cancelled by user"));
                    }
                    
                    Ok(())
                }
            });

            pipeline.add(task);
            last_tasks = vec!["workflow:approve".to_string()];
        }

        // 4. Deploy Phase
        if self.profile.deploy.is_active() {
            let mut task = Task::new("workflow:deploy");
            
            let deps_refs: Vec<&str> = last_tasks.iter().map(|s| s.as_str()).collect();
            if !deps_refs.is_empty() {
                task = task.depends_on(&deps_refs);
            }

            let name = self.profile.environment.clone();
            let context = self
                .profile
                .deploy_context
                .clone()
                .unwrap_or_else(|| "default".to_string()); // Fallback if none

            // We don't have strategy specified on profile yet, defaulting to Apply
            task = task.exec_fn(move |_ctx| {
                let name = name.clone();
                let context = context.clone();
                async move {
                    crate::LOGGER.info(&format!("Deploying to context '{}'...", context));
                    
                    crate::deployment::deploy(context, &name, crate::cli::DeploymentStrategy::Rolling)
                        .await
                        .map_err(|e| anyhow::anyhow!("Deploy failed: {}", e))?;
                    
                    Ok(())
                }
            });

            pipeline.add(task);
        }

        Ok((pipeline, build_plan))
    }
}
