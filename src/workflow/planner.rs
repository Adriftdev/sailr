use runkernel::{Pipeline, Task};

use crate::builder::{add_runkernel_tasks, create_sailr_build_plan, BuildOptions, SailrBuildPlan};
use crate::environment::Environment;
use crate::workflow::plan::{
    WorkflowEdge, WorkflowEffects, WorkflowPlan, WorkflowTaskKind, WorkflowTaskPlan,
};
use crate::workflow::runner::RunnerContext;

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
    pub runner: RunnerContext,
}

impl WorkflowPlanner {
    pub fn new(
        profile: NormalizedWorkflowProfile,
        env: Arc<Environment>,
        options: BuildOptions,
        runner: RunnerContext,
    ) -> Self {
        Self {
            profile,
            env,
            options,
            runner,
        }
    }

    pub fn plan(&self) -> Result<WorkflowPlan, String> {
        let mut tasks = Vec::new();
        let mut edges = Vec::new();
        let mut effects = WorkflowEffects::default();
        let mut build_plan_opt = None;
        let mut image_push_plan_opt = None;

        // 0. Validate Phase
        tasks.push(WorkflowTaskPlan {
            id: "workflow:validate-config".to_string(),
            label: "Validate Config".to_string(),
            kind: WorkflowTaskKind::ValidateConfig,
            dependencies: vec![],
            effects: WorkflowEffects::default(),
            description: "Validates Sailr environment configuration.".to_string(),
        });
        let mut last_tasks = vec!["workflow:validate-config".to_string()];

        // 1. Build Phase
        match self.profile.build {
            crate::workflow::profile::WorkflowStepMode::Disabled => {}
            crate::workflow::profile::WorkflowStepMode::DryRun => {
                return Err("workflow build dry-run is not enabled in this PR".to_string());
            }
            crate::workflow::profile::WorkflowStepMode::Plan => {
                let plan = create_sailr_build_plan(&self.env, &self.options)?;
                build_plan_opt = Some(plan.clone());

                let task_effects = WorkflowEffects::default();
                tasks.push(WorkflowTaskPlan {
                    id: "workflow:build-plan".to_string(),
                    label: "Build Plan".to_string(),
                    kind: WorkflowTaskKind::BuildPlan,
                    dependencies: vec!["workflow:validate-config".to_string()],
                    effects: task_effects,
                    description: "Analyzes services to determine what needs to be built."
                        .to_string(),
                });
                edges.push(WorkflowEdge {
                    from: "workflow:validate-config".to_string(),
                    to: "workflow:build-plan".to_string(),
                });

                last_tasks = vec!["workflow:build-plan".to_string()];
            }
            crate::workflow::profile::WorkflowStepMode::Run => {
                let plan = create_sailr_build_plan(&self.env, &self.options)?;
                build_plan_opt = Some(plan.clone());

                let dirty_count = plan.services.iter().filter(|s| s.dirty).count();

                effects.mutates_docker = true;
                effects.mutates_registry = true;

                let mut build_tasks = Vec::new();
                for s in &plan.services {
                    if s.dirty {
                        let service_effects = WorkflowEffects {
                            mutates_docker: true,
                            mutates_registry: true,
                            ..Default::default()
                        };

                        tasks.push(WorkflowTaskPlan {
                            id: format!("build:{}", s.service.name),
                            label: format!("Build {}", s.service.name),
                            kind: WorkflowTaskKind::ServiceBuild,
                            dependencies: vec!["workflow:validate-config".to_string()],
                            effects: service_effects,
                            description: format!(
                                "Builds and pushes Docker image for {}.",
                                s.service.name
                            ),
                        });
                        edges.push(WorkflowEdge {
                            from: "workflow:validate-config".to_string(),
                            to: format!("build:{}", s.service.name),
                        });
                        build_tasks.push(format!("build:{}", s.service.name));
                    }
                }

                if build_tasks.is_empty() {
                    build_tasks = vec!["workflow:validate-config".to_string()];
                }

                if dirty_count > 0 && !plan.after_all.is_empty() {
                    tasks.push(WorkflowTaskPlan {
                        id: "build:after-all".to_string(),
                        label: "After All Build Hooks".to_string(),
                        kind: WorkflowTaskKind::ServiceBuild,
                        dependencies: build_tasks.clone(),
                        effects: WorkflowEffects::default(),
                        description: "Runs after-all build hooks.".to_string(),
                    });
                    for t in &build_tasks {
                        edges.push(WorkflowEdge {
                            from: t.clone(),
                            to: "build:after-all".to_string(),
                        });
                    }
                    build_tasks = vec!["build:after-all".to_string()];
                }

                last_tasks = build_tasks;
            }
        }

        // 1.5 Push Phase
        match self.profile.push {
            crate::workflow::profile::WorkflowStepMode::Disabled => {}
            crate::workflow::profile::WorkflowStepMode::DryRun => {
                return Err("workflow push dry-run is not supported".to_string());
            }
            crate::workflow::profile::WorkflowStepMode::Plan
            | crate::workflow::profile::WorkflowStepMode::Run => {
                let is_run = self.profile.push == crate::workflow::profile::WorkflowStepMode::Run;

                if is_run {
                    effects.mutates_docker = true;
                    effects.mutates_registry = true;
                    effects.mutates_filesystem = true;
                }

                tasks.push(WorkflowTaskPlan {
                    id: "workflow:push-plan".to_string(),
                    label: "Push Plan".to_string(),
                    kind: WorkflowTaskKind::PushPlan,
                    dependencies: last_tasks.clone(),
                    effects: WorkflowEffects::default(),
                    description: "Determine target images and tags without pushing.".to_string(),
                });
                for t in &last_tasks {
                    edges.push(WorkflowEdge {
                        from: t.clone(),
                        to: "workflow:push-plan".to_string(),
                    });
                }
                last_tasks = vec!["workflow:push-plan".to_string()];

                if let Some(ref bp) = build_plan_opt {
                    image_push_plan_opt = Some(self.build_image_push_plan_report(bp, is_run)?);
                } else {
                    return Err("push requires build=plan or build=run".to_string());
                }
            }
        }

        // 2. Generate Phase
        if self.profile.generate.is_active() {
            effects.mutates_filesystem = true;
            let generate_effects = WorkflowEffects {
                mutates_filesystem: true,
                ..Default::default()
            };

            tasks.push(WorkflowTaskPlan {
                id: "workflow:generate".to_string(),
                label: "Generate Manifests".to_string(),
                kind: WorkflowTaskKind::Generate,
                dependencies: last_tasks.clone(),
                effects: generate_effects,
                description: "Generates Kubernetes manifests.".to_string(),
            });

            for t in &last_tasks {
                edges.push(WorkflowEdge {
                    from: t.clone(),
                    to: "workflow:generate".to_string(),
                });
            }
            last_tasks = vec!["workflow:generate".to_string()];
        }

        // 3. Deploy Phase
        if self.profile.deploy.is_active() {
            tasks.push(WorkflowTaskPlan {
                id: "workflow:deployment-plan".to_string(),
                label: "Deployment Plan".to_string(),
                kind: WorkflowTaskKind::DeploymentPlan,
                dependencies: last_tasks.clone(),
                effects: WorkflowEffects::default(),
                description:
                    "Create and validate the Kubernetes deployment plan without applying changes."
                        .to_string(),
            });

            for t in &last_tasks {
                edges.push(WorkflowEdge {
                    from: t.clone(),
                    to: "workflow:deployment-plan".to_string(),
                });
            }

            last_tasks = vec!["workflow:deployment-plan".to_string()];

            if self.profile.deploy == crate::workflow::profile::WorkflowStepMode::Run {
                if self.profile.approval == crate::workflow::profile::ApprovalMode::Prompt {
                    effects.prompts_user = true;
                    tasks.push(WorkflowTaskPlan {
                        id: "workflow:approval".to_string(),
                        label: "Approval".to_string(),
                        kind: WorkflowTaskKind::Approval,
                        dependencies: last_tasks.clone(),
                        effects: WorkflowEffects {
                            prompts_user: true,
                            ..Default::default()
                        },
                        description:
                            "Ask for local confirmation before applying deployment changes."
                                .to_string(),
                    });

                    for t in &last_tasks {
                        edges.push(WorkflowEdge {
                            from: t.clone(),
                            to: "workflow:approval".to_string(),
                        });
                    }

                    last_tasks = vec!["workflow:approval".to_string()];
                }

                if self.profile.apply {
                    effects.mutates_cluster = true;
                    tasks.push(WorkflowTaskPlan {
                        id: "workflow:deploy".to_string(),
                        label: "Deploy".to_string(),
                        kind: WorkflowTaskKind::Deploy,
                        dependencies: last_tasks.clone(),
                        effects: WorkflowEffects {
                            mutates_cluster: true,
                            ..Default::default()
                        },
                        description:
                            "Apply generated manifests to the configured Kubernetes context."
                                .to_string(),
                    });

                    for t in &last_tasks {
                        edges.push(WorkflowEdge {
                            from: t.clone(),
                            to: "workflow:deploy".to_string(),
                        });
                    }
                }
            }
        }

        Ok(WorkflowPlan {
            profile: self.profile.clone(),
            runner: self.runner.clone(),
            tasks,
            edges,
            build_plan: build_plan_opt,
            image_push_plan: image_push_plan_opt,
            effects,
        })
    }

    fn build_image_push_plan_report(
        &self,
        build_plan: &crate::builder::SailrBuildPlan,
        mutates_registry: bool,
    ) -> Result<crate::workflow::image::ImagePushPlanReport, String> {
        let mut items = Vec::new();

        for service_plan in &build_plan.services {
            if !service_plan.dirty {
                continue;
            }

            let registry = if self.env.registry.host().is_empty() {
                "docker.io".to_string()
            } else {
                self.env.registry.host()
            };

            let namespace = self
                .env
                .registry
                .namespace()
                .unwrap_or_else(|| "adriftdev/sailr".to_string());

            let repository = format!("{}/{}", namespace, service_plan.service.name);

            let tag =
                crate::workflow::image::derive_image_tag(Some(&service_plan.fingerprint.full_hash));

            let image_ref = format!("{}/{}:{}", registry, repository, tag);

            items.push(crate::workflow::image::ImagePushPlanItem {
                service: service_plan.service.name.clone(),
                registry,
                repository,
                tag,
                image_ref,
                source_sha: Some(service_plan.fingerprint.full_hash.clone()),
                action: crate::workflow::image::ImagePushPlanAction::WouldPush,
            });
        }

        Ok(crate::workflow::image::ImagePushPlanReport {
            environment: self.profile.environment.clone(),
            mutates_registry,
            items,
        })
    }

    pub fn build_pipeline_from_plan(
        &self,
        plan: &WorkflowPlan,
        accumulator: crate::workflow::image::WorkflowReportAccumulator,
    ) -> Result<(Pipeline, WorkflowBuildExecution), String> {
        let mut pipeline = Pipeline::new(format!("Workflow: {}", self.profile.name));
        let mut build_execution = WorkflowBuildExecution::None;

        let validate_task = Task::new("workflow:validate-config").exec_fn(move |_ctx| async move {
            crate::LOGGER.info("Validating Sailr environment config...");
            Ok(())
        });
        pipeline.add(validate_task);

        let mut last_tasks: Vec<String> = vec!["workflow:validate-config".to_string()];

        match self.profile.build {
            crate::workflow::profile::WorkflowStepMode::Disabled => {}
            crate::workflow::profile::WorkflowStepMode::DryRun => {
                return Err("workflow build dry-run is not enabled in this PR".to_string());
            }
            crate::workflow::profile::WorkflowStepMode::Plan => {
                let p = plan.build_plan.clone().unwrap();
                let o = self.options.clone();

                let task = Task::new("workflow:build-plan")
                    .depends_on(&["workflow:validate-config"])
                    .exec_fn(move |_ctx| {
                        let p = p.clone();
                        let o = o.clone();
                        async move {
                            crate::builder::print_sailr_plan(&p, &o);
                            Ok(())
                        }
                    });

                pipeline.add(task);
                last_tasks = vec!["workflow:build-plan".to_string()];
                build_execution =
                    WorkflowBuildExecution::PlanOnly(plan.build_plan.clone().unwrap());
            }
            crate::workflow::profile::WorkflowStepMode::Run => {
                let bp = plan.build_plan.clone().unwrap();
                add_runkernel_tasks(&mut pipeline, &bp)?;

                let dirty_count = bp.services.iter().filter(|s| s.dirty).count();
                let mut build_deps = Vec::new();
                if dirty_count > 0 && !bp.after_all.is_empty() {
                    build_deps.push("build:after-all".to_string());
                } else {
                    for s in &bp.services {
                        build_deps.push(format!("service:{}:build", s.service.name));
                    }
                }

                last_tasks.extend(build_deps);
                build_execution = WorkflowBuildExecution::Executed(bp);
            }
        }

        match self.profile.push {
            crate::workflow::profile::WorkflowStepMode::Disabled => {}
            crate::workflow::profile::WorkflowStepMode::DryRun => {
                return Err("workflow push dry-run is not supported".to_string());
            }
            crate::workflow::profile::WorkflowStepMode::Plan => {
                let push_plan = plan.image_push_plan.clone().unwrap();
                let mut task = Task::new("workflow:push-plan");

                let deps_refs: Vec<&str> = last_tasks.iter().map(|s| s.as_str()).collect();
                if !deps_refs.is_empty() {
                    task = task.depends_on(&deps_refs);
                }

                task = task.exec_fn(move |_ctx| {
                    let push_plan = push_plan.clone();
                    async move {
                        crate::LOGGER.info(&crate::workflow::render::render_image_push_plan_text(
                            &push_plan,
                        ));
                        Ok(())
                    }
                });

                pipeline.add(task);
                last_tasks = vec!["workflow:push-plan".to_string()];
            }
            crate::workflow::profile::WorkflowStepMode::Run => {
                let push_plan = plan.image_push_plan.clone().unwrap();
                let mut push_tasks = Vec::new();

                for item in &push_plan.items {
                    if item.action == crate::workflow::image::ImagePushPlanAction::WouldPush {
                        let service_name = item.service.clone();
                        let image_ref = item.image_ref.clone();
                        let accumulator = accumulator.clone();
                        let item_clone = item.clone();
                        let env_clone = self.env.clone();

                        let build_task_name = format!("service:{}:build", service_name);
                        let push_task_name = format!("service:{}:push", service_name);

                        let task = Task::new(push_task_name.clone())
                            .depends_on(&[build_task_name.as_str()])
                            .exec_fn(move |_ctx| {
                                let image_ref = image_ref.clone();
                                let accumulator = accumulator.clone();
                                let item = item_clone.clone();
                                let env_clone = env_clone.clone();
                                async move {
                                    crate::LOGGER.info(&format!("Pushing {}", image_ref));

                                    let svc = env_clone
                                        .services
                                        .iter()
                                        .find(|s| s.name == item.service)
                                        .unwrap();
                                    let registry = if env_clone.registry.prefix().is_empty() {
                                        "docker.io".to_string()
                                    } else {
                                        env_clone.registry.prefix()
                                    };
                                    let local_image =
                                        format!("{}/{}:{}", registry, svc.name, svc.version);

                                    let mut tag_cmd = tokio::process::Command::new("docker");
                                    tag_cmd.arg("tag").arg(&local_image).arg(&image_ref);
                                    let tag_output = tag_cmd.output().await.map_err(|e| {
                                        anyhow::anyhow!("Failed to execute docker tag: {}", e)
                                    })?;
                                    if !tag_output.status.success() {
                                        let stderr = String::from_utf8_lossy(&tag_output.stderr);
                                        return Err(anyhow::anyhow!(
                                            "Docker tag failed for {} to {}: {}",
                                            local_image,
                                            image_ref,
                                            stderr
                                        ));
                                    }

                                    let mut cmd = tokio::process::Command::new("docker");
                                    cmd.arg("push").arg(&image_ref);

                                    let output = cmd.output().await.map_err(|e| {
                                        anyhow::anyhow!("Failed to execute docker push: {}", e)
                                    })?;

                                    if !output.status.success() {
                                        let stderr = String::from_utf8_lossy(&output.stderr);
                                        return Err(anyhow::anyhow!(
                                            "Docker push failed: {}",
                                            stderr
                                        ));
                                    }

                                    let stdout_str = String::from_utf8_lossy(&output.stdout);
                                    let stderr_str = String::from_utf8_lossy(&output.stderr);
                                    let combined_output = format!("{}\n{}", stdout_str, stderr_str);

                                    let artifact =
                                        crate::workflow::image::pushed_artifact_from_output(
                                            &env_clone.name,
                                            &item,
                                            &combined_output,
                                        )
                                        .map_err(|e| anyhow::anyhow!(e))?;

                                    accumulator.add_image(artifact).await;

                                    Ok(())
                                }
                            });
                        pipeline.add(task);
                        push_tasks.push(push_task_name);
                    }
                }

                let report_task = Task::new("workflow:image-report")
                    .depends_on(&push_tasks.iter().map(|s| s.as_str()).collect::<Vec<_>>())
                    .exec_fn(|_ctx| async move {
                        crate::LOGGER.info("Image push report generated.");
                        Ok(())
                    });

                pipeline.add(report_task);
                last_tasks = vec!["workflow:image-report".to_string()];
            }
        }

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

        if self.profile.deploy.is_active() {
            let mut task = Task::new("workflow:deployment-plan");

            let deps_refs: Vec<&str> = last_tasks.iter().map(|s| s.as_str()).collect();
            if !deps_refs.is_empty() {
                task = task.depends_on(&deps_refs);
            }

            let env_name = self.profile.environment.clone();
            let context = self.profile.deploy_context.clone().unwrap_or_default();
            let namespace = self
                .profile
                .namespace
                .clone()
                .unwrap_or_else(|| "default".to_string());

            let is_static_plan =
                self.profile.deploy == crate::workflow::profile::WorkflowStepMode::Plan;

            task = task.exec_fn(move |_ctx| {
                let env_name = env_name.clone();
                let context = context.clone();
                let namespace = namespace.clone();
                async move {
                    crate::LOGGER.info("Deployment plan:");

                    if is_static_plan {
                        let plan = crate::workflow::plan::generate_static_deployment_plan(
                            &env_name, &context, &namespace,
                        )
                        .map_err(|e| anyhow::anyhow!("Static deployment plan failed: {}", e))?;

                        println!("Sailr deployment plan:");
                        println!("  environment: {}", plan.environment);
                        println!("  context: {}", plan.context);
                        println!("  namespace: {}", plan.namespace);
                        println!("  mode: static");
                        println!("  requires cluster: no");
                        println!("  mutates cluster: no\n");
                        println!("Resources:");
                        for res in plan.resources {
                            println!("  - {} {} \twould apply", res.kind, res.name);
                        }
                    } else {
                        let plan =
                            crate::plan::generate_deployment_plan(&env_name, &context, &namespace)
                                .await
                                .map_err(|e| anyhow::anyhow!("Deployment plan failed: {}", e))?;

                        crate::plan::validate_plan_safety(&plan).map_err(|e| {
                            anyhow::anyhow!("Deployment plan validation failed: {}", e)
                        })?;
                    }

                    Ok(())
                }
            });

            pipeline.add(task);
            last_tasks = vec!["workflow:deployment-plan".to_string()];

            if self.profile.approval == crate::workflow::profile::ApprovalMode::Prompt {
                let mut task = Task::new("workflow:approval");
                let deps_refs: Vec<&str> = last_tasks.iter().map(|s| s.as_str()).collect();
                if !deps_refs.is_empty() {
                    task = task.depends_on(&deps_refs);
                }

                task = task.exec_fn(move |_ctx| async move {
                    let approved = tokio::task::spawn_blocking(|| {
                        inquire::Confirm::new("Proceed with deployment?")
                            .with_default(false)
                            .prompt()
                    })
                    .await
                    .map_err(|e| anyhow::anyhow!("Approval prompt failed: {}", e))?
                    .map_err(|e| anyhow::anyhow!("Approval prompt failed: {}", e))?;

                    if !approved {
                        return Err(anyhow::anyhow!("Deployment cancelled by user"));
                    }

                    Ok(())
                });

                pipeline.add(task);
                last_tasks = vec!["workflow:approval".to_string()];
            }

            if self.profile.deploy == crate::workflow::profile::WorkflowStepMode::Run
                && self.profile.apply
            {
                let mut task = Task::new("workflow:deploy");
                let deps_refs: Vec<&str> = last_tasks.iter().map(|s| s.as_str()).collect();
                if !deps_refs.is_empty() {
                    task = task.depends_on(&deps_refs);
                }

                let context = self.profile.deploy_context.clone().unwrap_or_default();
                let env_name = self.profile.environment.clone();

                task = task.exec_fn(move |_ctx| {
                    let context = context.clone();
                    let env_name = env_name.clone();

                    async move {
                        crate::LOGGER.info(&format!(
                            "Deploying environment '{}' to context '{}'...",
                            env_name, context
                        ));
                        crate::deployment::deploy(
                            context,
                            &env_name,
                            crate::cli::DeploymentStrategy::Rolling,
                        )
                        .await
                        .map_err(|e| anyhow::anyhow!("Deploy failed: {}", e))?;

                        Ok(())
                    }
                });

                pipeline.add(task);
            }
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
    use crate::workflow::runner::RunnerKind;

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
            push: WorkflowStepMode::Disabled,
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

    fn dummy_runner() -> RunnerContext {
        RunnerContext {
            kind: RunnerKind::Local,
            ci: false,
            interactive: false,
        }
    }

    #[test]
    fn ci_profile_validate_only() {
        let env = Environment::new("local");
        let mut profile = dummy_profile(WorkflowStepMode::Disabled, WorkflowStepMode::Disabled);
        profile.generate = WorkflowStepMode::Disabled;
        let planner =
            WorkflowPlanner::new(profile, Arc::new(env), dummy_options(false), dummy_runner());
        let plan = planner.plan().unwrap();
        let (pipeline, _) = planner
            .build_pipeline_from_plan(&plan, Default::default())
            .unwrap();
        let task_names: Vec<String> = pipeline.tasks().map(|t| t.name.clone()).collect();
        assert_eq!(task_names, vec!["workflow:validate-config"]);
    }

    #[test]
    fn ci_build_plan_creates_build_plan() {
        let mut env = Environment::new("local");
        let mut svc = crate::environment::Service::new("dummy", None, "latest");
        let temp_dir = tempfile::tempdir().unwrap();
        svc.build = Some(crate::environment::ServiceBuildConfig {
            path: temp_dir.path().to_string_lossy().to_string(),
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
        let planner =
            WorkflowPlanner::new(profile, Arc::new(env), dummy_options(true), dummy_runner());
        let plan = planner.plan().unwrap();
        let (pipeline, _) = planner
            .build_pipeline_from_plan(&plan, Default::default())
            .unwrap();
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
        let _temp_dir = tempfile::tempdir().unwrap();
        svc.build = Some(crate::environment::ServiceBuildConfig {
            path: _temp_dir.path().to_string_lossy().to_string(),
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
        let planner =
            WorkflowPlanner::new(profile, Arc::new(env), dummy_options(true), dummy_runner());
        let plan = planner.plan().unwrap();
        let (pipeline, _) = planner
            .build_pipeline_from_plan(&plan, Default::default())
            .unwrap();
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

    #[test]
    fn local_deploy_creates_deploy_tasks() {
        let mut env = Environment::new("local");
        let mut svc = crate::environment::Service::new("dummy", None, "latest");
        let _temp_dir = tempfile::tempdir().unwrap();
        svc.build = Some(crate::environment::ServiceBuildConfig {
            path: _temp_dir.path().to_string_lossy().to_string(),
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

        let mut profile = dummy_profile(WorkflowStepMode::Run, WorkflowStepMode::Plan);
        profile.approval = ApprovalMode::Prompt;
        profile.apply = true;
        profile.deploy_context = Some("minikube".to_string());

        let planner =
            WorkflowPlanner::new(profile, Arc::new(env), dummy_options(true), dummy_runner());
        let plan = planner.plan().unwrap();

        // Check tasks in plan
        let task_kinds: Vec<_> = plan.tasks.iter().map(|t| t.kind).collect();
        assert!(task_kinds.contains(&WorkflowTaskKind::DeploymentPlan));
        assert!(task_kinds.contains(&WorkflowTaskKind::Approval));
        assert!(task_kinds.contains(&WorkflowTaskKind::Deploy));

        let (pipeline, _) = planner
            .build_pipeline_from_plan(&plan, Default::default())
            .unwrap();
        let mut task_names: Vec<String> = pipeline.tasks().map(|t| t.name.clone()).collect();
        task_names.sort();

        let mut expected = vec![
            "workflow:validate-config".to_string(),
            "workflow:build-plan".to_string(),
            "workflow:generate".to_string(),
            "workflow:deployment-plan".to_string(),
            "workflow:approval".to_string(),
            "workflow:deploy".to_string(),
        ];
        expected.sort();
        assert_eq!(task_names, expected);
    }
}

#[cfg(test)]
mod tests_addendum {
    use super::*;

    #[test]
    fn ci_build_push_plan_workflow_plan_has_image_push_plan() {
        use crate::environment::Environment;
        use crate::workflow::profile::WorkflowProfile;

        let env_toml = r#"
        schema_version = "v0.5"
        name = "test"
        domain = "test.local"
        log_level = "info"
        default_replicas = 1
        registry = "ghcr.io"
        [[service]]
        name = "api"
        [service.build]
        path = "."
        "#;
        let env: Environment = toml::from_str(env_toml).unwrap();

        let profile_toml = r#"
        environment = "test"
        mode = "build"
        build = "plan"
        push = "plan"
        "#;
        let mut profile: WorkflowProfile = toml::from_str(profile_toml).unwrap();
        profile.name = "ci-build-push-plan".to_string();
        let normalized = profile.normalize(false);
        let runner_ctx = RunnerContext::detect(true);
        let options = crate::builder::BuildOptions {
            cache_dir: ".sailr/cache".to_string(),
            force: false,
            only: vec![],
            ignore: vec![],
            plan: false,
            dry_run: false,
            explain: false,
            dump_scope: false,
            policy: None,
        };

        let planner =
            WorkflowPlanner::new(normalized, std::sync::Arc::new(env), options, runner_ctx);

        let plan = planner.plan().unwrap();
        assert!(plan.image_push_plan.is_some());
    }

    #[test]
    fn existing_profiles_do_not_carry_push_plan() {
        use crate::environment::Environment;
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
        name = "api"
        [service.build]
        path = "{}"
        "#,
            temp_dir.path().to_string_lossy()
        );
        let env: Environment = toml::from_str(&env_toml).unwrap();

        let env_arc = std::sync::Arc::new(env);
        let profiles = vec![
            r#"
            environment = "test"
            mode = "check"
            "#,
            r#"
            environment = "test"
            mode = "build"
            build = "plan"
            "#,
            r#"
            environment = "test"
            mode = "build"
            build = "plan"
            generate = "run"
            "#,
        ];

        for p_toml in profiles {
            let profile: WorkflowProfile = toml::from_str(p_toml).unwrap();
            let normalized = profile.normalize(false);
            let runner_ctx = RunnerContext::detect(true);
            let options = crate::builder::BuildOptions {
                cache_dir: ".sailr/cache".to_string(),
                force: false,
                only: vec![],
                ignore: vec![],
                plan: false,
                dry_run: false,
                explain: false,
                dump_scope: false,
                policy: None,
            };

            let planner = WorkflowPlanner::new(normalized, env_arc.clone(), options, runner_ctx);

            let plan = planner.plan().unwrap();
            assert!(plan.image_push_plan.is_none());
        }
    }
    #[test]
    fn push_run_generates_correct_execution_graph() {
        use crate::environment::Environment;
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
        name = "api"
        [service.build]
        path = "{}"
        "#,
            temp_dir.path().to_string_lossy()
        );
        let env: Environment = toml::from_str(&env_toml).unwrap();

        let profile_toml = r#"
        environment = "test"
        mode = "build"
        build = "run"
        push = "run"
        "#;
        let mut profile: WorkflowProfile = toml::from_str(profile_toml).unwrap();
        profile.name = "ci-build-push".to_string();
        let normalized = profile.normalize(false);
        let runner_ctx = RunnerContext::detect(true);
        let options = crate::builder::BuildOptions {
            cache_dir: ".sailr/cache".to_string(),
            force: true, // force to ensure it's dirty
            only: vec![],
            ignore: vec![],
            plan: false,
            dry_run: false,
            explain: false,
            dump_scope: false,
            policy: None,
        };

        let planner =
            WorkflowPlanner::new(normalized, std::sync::Arc::new(env), options, runner_ctx);

        let plan = planner.plan().unwrap();
        assert!(plan.image_push_plan.is_some());
        assert!(plan.effects.mutates_registry);
        assert!(plan.effects.mutates_docker);
        assert!(plan.effects.mutates_filesystem);

        let accumulator = crate::workflow::image::WorkflowReportAccumulator::default();
        let (pipeline, _) = planner
            .build_pipeline_from_plan(&plan, accumulator)
            .unwrap();

        let tasks: Vec<_> = pipeline.tasks().collect();
        assert!(tasks.iter().any(|t| t.name == "service:api:build"));
        assert!(tasks.iter().any(|t| t.name == "service:api:push"));
        assert!(tasks.iter().any(|t| t.name == "workflow:image-report"));

        let api_push = tasks.iter().find(|t| t.name == "service:api:push").unwrap();
        assert_eq!(api_push.dependencies, vec!["service:api:build"]);

        let report = tasks
            .iter()
            .find(|t| t.name == "workflow:image-report")
            .unwrap();
        assert_eq!(report.dependencies, vec!["service:api:push"]);
    }
}
