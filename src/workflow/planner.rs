use runkernel::{Pipeline, Task};

use crate::builder::{
    add_runkernel_tasks_from_workflow_plan, create_sailr_build_plan, BuildOptions, SailrBuildPlan,
};
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

fn runtime_task(plan: &WorkflowPlan, id: &str) -> Result<Task, String> {
    let task = plan
        .tasks
        .iter()
        .find(|task| task.id == id)
        .ok_or_else(|| format!("Task '{id}' is missing from the workflow plan"))?;
    let dependencies = task
        .dependencies
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    Ok(Task::new(id)
        .description(task.description.clone())
        .depends_on(&dependencies))
}

pub struct WorkflowPlanner {
    pub profile: NormalizedWorkflowProfile,
    pub env: Arc<Environment>,
    pub options: BuildOptions,
    pub runner: RunnerContext,
    source_revision_resolver: Arc<dyn SourceRevisionResolver>,
}

pub trait SourceRevisionResolver: Send + Sync {
    fn resolve(
        &self,
        runner: &RunnerContext,
    ) -> Result<Option<String>, crate::workflow::error::ProvenanceError>;
}

#[derive(Debug, Default)]
pub struct SystemSourceRevisionResolver;

fn validate_source_revision(
    value: String,
) -> Result<String, crate::workflow::error::ProvenanceError> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        return Err(
            crate::workflow::error::ProvenanceError::InvalidSourceRevision(
                "Source revision is empty".to_string(),
            ),
        );
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err(
            crate::workflow::error::ProvenanceError::InvalidSourceRevision(
                "Source revision cannot contain whitespace".to_string(),
            ),
        );
    }
    Ok(trimmed)
}

impl SourceRevisionResolver for SystemSourceRevisionResolver {
    fn resolve(
        &self,
        runner: &RunnerContext,
    ) -> Result<Option<String>, crate::workflow::error::ProvenanceError> {
        resolve_source_revision_with(
            runner,
            |variable| match std::env::var(variable) {
                Ok(value) => Ok(Some(value)),
                Err(std::env::VarError::NotPresent) => Ok(None),
                Err(std::env::VarError::NotUnicode(_)) => Err(
                    crate::workflow::error::ProvenanceError::InvalidSourceRevision(format!(
                        "{variable} is not valid Unicode"
                    )),
                ),
            },
            || {
                let output = std::process::Command::new("git")
                    .args(["rev-parse", "HEAD"])
                    .output()
                    .map_err(|error| {
                        crate::workflow::error::ProvenanceError::Git(error.to_string())
                    })?;
                if !output.status.success() {
                    return Err(crate::workflow::error::ProvenanceError::Git(
                        String::from_utf8_lossy(&output.stderr).trim().to_string(),
                    ));
                }
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            },
        )
    }
}

fn resolve_source_revision_with<E, G>(
    runner: &RunnerContext,
    read_environment: E,
    read_git: G,
) -> Result<Option<String>, crate::workflow::error::ProvenanceError>
where
    E: Fn(&str) -> Result<Option<String>, crate::workflow::error::ProvenanceError>,
    G: Fn() -> Result<String, crate::workflow::error::ProvenanceError>,
{
    let provider_variable =
        runner
            .ci_environment
            .as_ref()
            .and_then(|environment| match environment.provider {
                crate::workflow::ci::CiProvider::GitHub => Some("GITHUB_SHA"),
                crate::workflow::ci::CiProvider::CircleCi => Some("CIRCLE_SHA1"),
                crate::workflow::ci::CiProvider::Travis => Some("TRAVIS_COMMIT"),
                crate::workflow::ci::CiProvider::Generic => None,
            });

    if let Some(variable) = provider_variable {
        if let Some(value) = read_environment(variable)? {
            return validate_source_revision(value).map(Some);
        }
    }

    validate_source_revision(read_git()?).map(Some)
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
            source_revision_resolver: Arc::new(SystemSourceRevisionResolver),
        }
    }

    pub fn with_source_revision_resolver(
        profile: NormalizedWorkflowProfile,
        env: Arc<Environment>,
        options: BuildOptions,
        runner: RunnerContext,
        source_revision_resolver: Arc<dyn SourceRevisionResolver>,
    ) -> Self {
        Self {
            profile,
            env,
            options,
            runner,
            source_revision_resolver,
        }
    }

    pub fn plan(&self) -> Result<WorkflowPlan, String> {
        let mut tasks = Vec::new();
        let mut effects;
        let mut build_plan_opt = None;
        let mut image_push_plan_opt = None;

        // 0. Validate Phase
        tasks.push(WorkflowTaskPlan {
            id: crate::workflow::task_id::VALIDATE_CONFIG.to_string(),
            label: "Validate Config".to_string(),
            kind: WorkflowTaskKind::ValidateConfig,
            dependencies: vec![],
            effects: WorkflowEffects::default(),
            description: "Validates Sailr environment configuration.".to_string(),
        });
        let mut last_tasks = vec![crate::workflow::task_id::VALIDATE_CONFIG.to_string()];

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
                    id: crate::workflow::task_id::BUILD_PLAN.to_string(),
                    label: "Build Plan".to_string(),
                    kind: WorkflowTaskKind::BuildPlan,
                    dependencies: vec![crate::workflow::task_id::VALIDATE_CONFIG.to_string()],
                    effects: task_effects,
                    description: "Analyzes services to determine what needs to be built."
                        .to_string(),
                });
                last_tasks = vec![crate::workflow::task_id::BUILD_PLAN.to_string()];
            }
            crate::workflow::profile::WorkflowStepMode::Run => {
                let plan = create_sailr_build_plan(&self.env, &self.options)?;
                build_plan_opt = Some(plan.clone());

                let dirty_services = plan
                    .services
                    .iter()
                    .filter(|service| service.dirty)
                    .map(|service| service.service.name.as_str())
                    .collect::<std::collections::BTreeSet<_>>();
                let has_before_all = !dirty_services.is_empty() && !plan.before_all.is_empty();

                if has_before_all {
                    tasks.push(WorkflowTaskPlan {
                        id: crate::workflow::task_id::BUILD_BEFORE_ALL.to_string(),
                        label: "Before All Build Hooks".to_string(),
                        kind: WorkflowTaskKind::ServiceBuild,
                        dependencies: vec![crate::workflow::task_id::VALIDATE_CONFIG.to_string()],
                        effects: WorkflowEffects {
                            mutates_filesystem: true,
                            ..Default::default()
                        },
                        description: "Runs before-all build hooks.".to_string(),
                    });
                }

                let mut build_tasks = Vec::new();
                for s in &plan.services {
                    if s.dirty {
                        let service_effects = WorkflowEffects {
                            mutates_docker: true,
                            ..Default::default()
                        };
                        let task_id = crate::workflow::task_id::service_build(&s.service.name);
                        let mut dependencies = s
                            .dependencies
                            .iter()
                            .filter(|dependency| dirty_services.contains(dependency.as_str()))
                            .map(|dependency| crate::workflow::task_id::service_build(dependency))
                            .collect::<Vec<_>>();
                        if has_before_all {
                            dependencies
                                .push(crate::workflow::task_id::BUILD_BEFORE_ALL.to_string());
                        }
                        if dependencies.is_empty() {
                            dependencies
                                .push(crate::workflow::task_id::VALIDATE_CONFIG.to_string());
                        }
                        dependencies.sort();
                        dependencies.dedup();

                        tasks.push(WorkflowTaskPlan {
                            id: task_id.clone(),
                            label: format!("Build {}", s.service.name),
                            kind: WorkflowTaskKind::ServiceBuild,
                            dependencies,
                            effects: service_effects,
                            description: format!(
                                "Builds the local Docker image for {}.",
                                s.service.name
                            ),
                        });
                        build_tasks.push(task_id);
                    }
                }

                if build_tasks.is_empty() {
                    build_tasks = vec![crate::workflow::task_id::VALIDATE_CONFIG.to_string()];
                }

                if !dirty_services.is_empty() && !plan.after_all.is_empty() {
                    tasks.push(WorkflowTaskPlan {
                        id: crate::workflow::task_id::BUILD_AFTER_ALL.to_string(),
                        label: "After All Build Hooks".to_string(),
                        kind: WorkflowTaskKind::ServiceBuild,
                        dependencies: build_tasks.clone(),
                        effects: WorkflowEffects {
                            mutates_filesystem: true,
                            ..Default::default()
                        },
                        description: "Runs after-all build hooks.".to_string(),
                    });
                    build_tasks = vec![crate::workflow::task_id::BUILD_AFTER_ALL.to_string()];
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
                let source_revision = match self.source_revision_resolver.resolve(&self.runner) {
                    Ok(revision) => revision,
                    Err(crate::workflow::error::ProvenanceError::Git(_)) if !is_run => None,
                    Err(error) => return Err(error.to_string()),
                };

                if is_run && self.runner.ci && source_revision.is_none() {
                    return Err(
                        crate::workflow::error::ProvenanceError::MissingSourceRevision.to_string(),
                    );
                }

                tasks.push(WorkflowTaskPlan {
                    id: crate::workflow::task_id::PUSH_PLAN.to_string(),
                    label: "Push Plan".to_string(),
                    kind: WorkflowTaskKind::PushPlan,
                    dependencies: last_tasks.clone(),
                    effects: WorkflowEffects::default(),
                    description: "Determine target images and tags without pushing.".to_string(),
                });
                last_tasks = vec![crate::workflow::task_id::PUSH_PLAN.to_string()];

                if let Some(ref bp) = build_plan_opt {
                    image_push_plan_opt =
                        Some(self.build_image_push_plan_report(bp, is_run, source_revision)?);
                } else {
                    return Err("push requires build=plan or build=run".to_string());
                }

                if is_run {
                    let mut push_tasks = Vec::new();
                    for item in &image_push_plan_opt
                        .as_ref()
                        .expect("push plan exists")
                        .items
                    {
                        let mut dependencies =
                            vec![crate::workflow::task_id::PUSH_PLAN.to_string()];
                        let build_task = crate::workflow::task_id::service_build(&item.service);
                        if tasks.iter().any(|task| task.id == build_task) {
                            dependencies.push(build_task);
                        }
                        dependencies.sort();
                        dependencies.dedup();

                        let push_task = crate::workflow::task_id::service_push(&item.service);
                        tasks.push(WorkflowTaskPlan {
                            id: push_task.clone(),
                            label: format!("Push {}", item.service),
                            kind: WorkflowTaskKind::ServicePush,
                            dependencies,
                            effects: WorkflowEffects {
                                mutates_docker: true,
                                mutates_registry: true,
                                ..Default::default()
                            },
                            description: format!(
                                "Publishes {} as {}.",
                                item.local_image_ref, item.target_image_ref
                            ),
                        });
                        push_tasks.push(push_task);
                    }

                    if push_tasks.is_empty() {
                        push_tasks.push(crate::workflow::task_id::PUSH_PLAN.to_string());
                    }
                    tasks.push(WorkflowTaskPlan {
                        id: crate::workflow::task_id::IMAGE_REPORT.to_string(),
                        label: "Image Publication Report".to_string(),
                        kind: WorkflowTaskKind::ImageReport,
                        dependencies: push_tasks,
                        effects: WorkflowEffects {
                            mutates_filesystem: true,
                            ..Default::default()
                        },
                        description: "Writes the image publication report.".to_string(),
                    });
                    last_tasks = vec![crate::workflow::task_id::IMAGE_REPORT.to_string()];
                }
            }
        }

        // 2. Generate Phase
        if self.profile.generate.is_active() {
            let generate_effects = WorkflowEffects {
                mutates_filesystem: true,
                ..Default::default()
            };

            tasks.push(WorkflowTaskPlan {
                id: crate::workflow::task_id::GENERATE.to_string(),
                label: "Generate Manifests".to_string(),
                kind: WorkflowTaskKind::Generate,
                dependencies: last_tasks.clone(),
                effects: generate_effects,
                description: "Generates Kubernetes manifests.".to_string(),
            });

            last_tasks = vec![crate::workflow::task_id::GENERATE.to_string()];
        }

        // 3. Deploy Phase
        if self.profile.deploy.is_active() {
            tasks.push(WorkflowTaskPlan {
                id: crate::workflow::task_id::DEPLOYMENT_PLAN.to_string(),
                label: "Deployment Plan".to_string(),
                kind: WorkflowTaskKind::DeploymentPlan,
                dependencies: last_tasks.clone(),
                effects: WorkflowEffects::default(),
                description:
                    "Create and validate the Kubernetes deployment plan without applying changes."
                        .to_string(),
            });

            last_tasks = vec![crate::workflow::task_id::DEPLOYMENT_PLAN.to_string()];

            if self.profile.deploy == crate::workflow::profile::WorkflowStepMode::Run {
                if self.profile.approval == crate::workflow::profile::ApprovalMode::Prompt {
                    tasks.push(WorkflowTaskPlan {
                        id: crate::workflow::task_id::APPROVAL.to_string(),
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

                    last_tasks = vec![crate::workflow::task_id::APPROVAL.to_string()];
                }

                if self.profile.apply {
                    tasks.push(WorkflowTaskPlan {
                        id: crate::workflow::task_id::DEPLOY.to_string(),
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
                }
            }
        }

        effects = WorkflowEffects::default();
        for task in &tasks {
            effects.merge(&task.effects);
        }
        let edges = tasks
            .iter()
            .flat_map(|task| {
                task.dependencies.iter().map(|dependency| WorkflowEdge {
                    from: dependency.clone(),
                    to: task.id.clone(),
                })
            })
            .collect();

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
        source_revision: Option<String>,
    ) -> Result<crate::workflow::image::ImagePushPlanReport, String> {
        let mut items = Vec::new();

        for service_plan in &build_plan.services {
            if !service_plan.dirty {
                continue;
            }

            let resolved_registry = self
                .env
                .registry
                .resolve()
                .map_err(|e| format!("Invalid registry: {}", e))?;

            let repository = resolved_registry
                .repository_for(&service_plan.service.name)
                .map_err(|e| format!("Invalid repository: {}", e))?;

            let tag =
                crate::workflow::image::derive_image_tag(Some(&service_plan.fingerprint.full_hash));

            let target_image_ref = resolved_registry
                .tagged_ref(&service_plan.service.name, &tag)
                .map_err(|e| format!("Invalid target ref: {}", e))?;

            let local_image_ref = resolved_registry
                .tagged_ref(&service_plan.service.name, &service_plan.service.version)
                .map_err(|e| format!("Invalid local ref: {}", e))?;

            items.push(crate::workflow::image::ImagePushPlanItem {
                service: service_plan.service.name.clone(),
                registry: resolved_registry.host,
                repository,
                target_image_ref,
                local_image_ref,
                tag,
                provenance: crate::workflow::image::ImageProvenance {
                    build_fingerprint: service_plan.fingerprint.full_hash.clone(),
                    source_revision: source_revision.clone(),
                },
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

        let validate_task = runtime_task(plan, crate::workflow::task_id::VALIDATE_CONFIG)?.exec_fn(
            move |_ctx| async move {
                crate::LOGGER.info("Validating Sailr environment config...");
                Ok(())
            },
        );
        pipeline.add(validate_task);

        match self.profile.build {
            crate::workflow::profile::WorkflowStepMode::Disabled => {}
            crate::workflow::profile::WorkflowStepMode::DryRun => {
                return Err("workflow build dry-run is not enabled in this PR".to_string());
            }
            crate::workflow::profile::WorkflowStepMode::Plan => {
                let p = plan.build_plan.clone().unwrap();
                let o = self.options.clone();

                let task = runtime_task(plan, crate::workflow::task_id::BUILD_PLAN)?.exec_fn(
                    move |_ctx| {
                        let p = p.clone();
                        let o = o.clone();
                        async move {
                            crate::builder::print_sailr_plan(&p, &o);
                            Ok(())
                        }
                    },
                );

                pipeline.add(task);
                build_execution =
                    WorkflowBuildExecution::PlanOnly(plan.build_plan.clone().unwrap());
            }
            crate::workflow::profile::WorkflowStepMode::Run => {
                let bp = plan.build_plan.clone().unwrap();
                add_runkernel_tasks_from_workflow_plan(&mut pipeline, &bp, &plan.tasks)?;
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
                let mut task = runtime_task(plan, crate::workflow::task_id::PUSH_PLAN)?;

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
            }
            crate::workflow::profile::WorkflowStepMode::Run => {
                let push_plan = plan.image_push_plan.clone().unwrap();
                let rendered_push_plan = push_plan.clone();
                pipeline.add(
                    runtime_task(plan, crate::workflow::task_id::PUSH_PLAN)?.exec_fn(move |_ctx| {
                        let rendered_push_plan = rendered_push_plan.clone();
                        async move {
                            crate::LOGGER.info(
                                &crate::workflow::render::render_image_push_plan_text(
                                    &rendered_push_plan,
                                ),
                            );
                            Ok(())
                        }
                    }),
                );

                for item in &push_plan.items {
                    if item.action == crate::workflow::image::ImagePushPlanAction::WouldPush {
                        let service_name = item.service.clone();
                        let target_image_ref = item.target_image_ref.clone();
                        let local_image_ref = item.local_image_ref.clone();
                        let accumulator = accumulator.clone();
                        let item_clone = item.clone();
                        let env_clone = self.env.clone();

                        let push_task_name = crate::workflow::task_id::service_push(&service_name);

                        let task = runtime_task(plan, &push_task_name)?
                            .exec_fn(move |_ctx| {
                                let target_image_ref = target_image_ref.clone();
                                let local_image_ref = local_image_ref.clone();
                                let accumulator = accumulator.clone();
                                let item = item_clone.clone();
                                let env_clone = env_clone.clone();
                                async move {
                                    crate::LOGGER.info(&format!("Pushing {}", target_image_ref));

                                    let mut tag_cmd = tokio::process::Command::new("docker");
                                    tag_cmd
                                        .arg("tag")
                                        .arg(&local_image_ref)
                                        .arg(&target_image_ref);
                                    let tag_output = tag_cmd.output().await.map_err(|e| {
                                        anyhow::anyhow!("Failed to execute docker tag: {}", e)
                                    })?;
                                    if !tag_output.status.success() {
                                        let stderr = String::from_utf8_lossy(&tag_output.stderr);
                                        return Err(anyhow::anyhow!(
                                            "Docker tag failed. source: {}, target: {}, status: {}, stderr: {}",
                                            local_image_ref,
                                            target_image_ref,
                                            tag_output.status,
                                            stderr.trim()
                                        ));
                                    }

                                    let mut cmd = tokio::process::Command::new("docker");
                                    cmd.arg("push").arg(&target_image_ref);

                                    let output = cmd.output().await.map_err(|e| {
                                        anyhow::anyhow!("Failed to execute docker push: {}", e)
                                    })?;

                                    if !output.status.success() {
                                        let stderr = String::from_utf8_lossy(&output.stderr);
                                        return Err(anyhow::anyhow!(
                                            "Docker push failed. target: {}, status: {}, stderr: {}",
                                            target_image_ref,
                                            output.status,
                                            stderr.trim()
                                        ));
                                    }

                                    let stdout_str = String::from_utf8_lossy(&output.stdout);
                                    let stderr_str = String::from_utf8_lossy(&output.stderr);
                                    let combined_output = format!("{}\n{}", stdout_str, stderr_str);

                                    let mut inspect_cmd = tokio::process::Command::new("docker");
                                    inspect_cmd
                                        .arg("inspect")
                                        .arg("--format={{index .RepoDigests 0}}")
                                        .arg(&target_image_ref);
                                    let structured_digest = match inspect_cmd.output().await {
                                        Ok(output) if output.status.success() => {
                                            let stdout = String::from_utf8_lossy(&output.stdout)
                                                .trim()
                                                .to_string();
                                            Some(
                                                stdout
                                                    .split_once('@')
                                                    .map(|(_, digest)| digest.to_string())
                                                    .unwrap_or(stdout),
                                            )
                                        }
                                        Ok(output) => {
                                            crate::LOGGER.debug(&format!(
                                                "Docker inspection failed. target: {}, status: {}, stderr: {}",
                                                target_image_ref,
                                                output.status,
                                                String::from_utf8_lossy(&output.stderr).trim()
                                            ));
                                            None
                                        }
                                        Err(error) => {
                                            crate::LOGGER.debug(&format!(
                                                "Docker inspection could not execute. target: {}, error: {}",
                                                target_image_ref, error
                                            ));
                                            None
                                        }
                                    };

                                    let artifact =
                                        crate::workflow::image::pushed_artifact_from_output(
                                            &env_clone.name,
                                            &item,
                                            &combined_output,
                                            structured_digest.as_deref(),
                                        )
                                        .map_err(|e| anyhow::anyhow!(e))?;

                                    accumulator.add_image(artifact).await;

                                    Ok(())
                                }
                            });
                        pipeline.add(task);
                    }
                }

                let report_task = runtime_task(plan, crate::workflow::task_id::IMAGE_REPORT)?
                    .exec_fn(|_ctx| async move {
                        crate::LOGGER.info("Image push report generated.");
                        Ok(())
                    });

                pipeline.add(report_task);
            }
        }

        if self.profile.generate.is_active() {
            let mut task = runtime_task(plan, crate::workflow::task_id::GENERATE)?;

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
        }

        if self.profile.deploy.is_active() {
            let mut task = runtime_task(plan, crate::workflow::task_id::DEPLOYMENT_PLAN)?;

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
            if self.profile.approval == crate::workflow::profile::ApprovalMode::Prompt {
                let mut task = runtime_task(plan, crate::workflow::task_id::APPROVAL)?;

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
            }

            if self.profile.deploy == crate::workflow::profile::WorkflowStepMode::Run
                && self.profile.apply
            {
                let mut task = runtime_task(plan, crate::workflow::task_id::DEPLOY)?;

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
            ci_environment: None,
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
        assert_eq!(
            task_names,
            vec![crate::workflow::task_id::VALIDATE_CONFIG.to_string()]
        );
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
            crate::workflow::task_id::VALIDATE_CONFIG.to_string(),
            crate::workflow::task_id::BUILD_PLAN.to_string(),
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
            crate::workflow::task_id::VALIDATE_CONFIG.to_string(),
            crate::workflow::task_id::BUILD_PLAN.to_string(),
            crate::workflow::task_id::GENERATE.to_string(),
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
            crate::workflow::task_id::VALIDATE_CONFIG.to_string(),
            crate::workflow::task_id::BUILD_PLAN.to_string(),
            crate::workflow::task_id::GENERATE.to_string(),
            crate::workflow::task_id::DEPLOYMENT_PLAN.to_string(),
            crate::workflow::task_id::APPROVAL.to_string(),
            crate::workflow::task_id::DEPLOY.to_string(),
        ];
        expected.sort();
        assert_eq!(task_names, expected);
    }
}

#[cfg(test)]
mod tests_addendum {
    use super::*;

    fn assert_plan_pipeline_parity(plan: &WorkflowPlan, pipeline: &runkernel::Pipeline) {
        let planned = plan
            .tasks
            .iter()
            .map(|task| {
                let mut dependencies = task.dependencies.clone();
                dependencies.sort();
                (task.id.clone(), dependencies)
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let runtime = pipeline
            .tasks()
            .map(|task| {
                let mut dependencies = task.dependencies.clone();
                dependencies.sort();
                (task.name.clone(), dependencies)
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(planned, runtime);
    }

    #[test]
    fn source_revision_resolution_is_provider_aware_without_process_globals() {
        for (provider, expected_variable) in [
            (crate::workflow::ci::CiProvider::GitHub, "GITHUB_SHA"),
            (crate::workflow::ci::CiProvider::CircleCi, "CIRCLE_SHA1"),
            (crate::workflow::ci::CiProvider::Travis, "TRAVIS_COMMIT"),
        ] {
            let runner = RunnerContext {
                kind: crate::workflow::runner::RunnerKind::GenericCi,
                ci: true,
                interactive: false,
                ci_environment: Some(crate::workflow::ci::CiEnvironment {
                    provider,
                    run_id: None,
                }),
            };
            let revision = resolve_source_revision_with(
                &runner,
                |variable| {
                    assert_eq!(variable, expected_variable);
                    Ok(Some(" provider-revision ".to_string()))
                },
                || panic!("provider revision must take precedence over Git"),
            )
            .unwrap();
            assert_eq!(revision.as_deref(), Some("provider-revision"));

            assert!(resolve_source_revision_with(
                &runner,
                |_| Ok(Some("   ".to_string())),
                || Ok("git-revision".to_string()),
            )
            .is_err());

            assert_eq!(
                resolve_source_revision_with(
                    &runner,
                    |_| Ok(None),
                    || Ok("git-revision".to_string()),
                )
                .unwrap()
                .as_deref(),
                Some("git-revision")
            );
        }

        let local = RunnerContext {
            kind: crate::workflow::runner::RunnerKind::Local,
            ci: false,
            interactive: true,
            ci_environment: None,
        };
        assert!(resolve_source_revision_with(
            &local,
            |_| Ok(None),
            || Err(crate::workflow::error::ProvenanceError::Git(
                "unavailable".to_string(),
            )),
        )
        .is_err());
    }

    #[test]
    fn ci_publication_rejects_missing_revision_during_planning() {
        struct MissingRevision;
        impl SourceRevisionResolver for MissingRevision {
            fn resolve(
                &self,
                _runner: &RunnerContext,
            ) -> Result<Option<String>, crate::workflow::error::ProvenanceError> {
                Ok(None)
            }
        }

        let temp = tempfile::tempdir().unwrap();
        let mut environment = Environment::new("staging");
        environment.registry = crate::environment::RegistryConfig::Simple("ghcr.io/acme".into());
        let mut service = crate::environment::Service::new("api", None, "1.0.0");
        service.build = Some(crate::environment::ServiceBuildConfig {
            path: temp.path().to_string_lossy().to_string(),
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
        environment.services.push(service);
        let mut profile: crate::workflow::profile::WorkflowProfile = toml::from_str(
            r#"
            environment = "staging"
            mode = "build"
            build = "run"
            push = "run"
            "#,
        )
        .unwrap();
        profile.name = "publish".to_string();
        let planner = WorkflowPlanner::with_source_revision_resolver(
            profile.normalize(true),
            Arc::new(environment),
            BuildOptions {
                cache_dir: temp.path().join("cache").to_string_lossy().to_string(),
                force: true,
                only: vec![],
                ignore: vec![],
                plan: false,
                dry_run: false,
                explain: false,
                dump_scope: false,
                policy: None,
            },
            RunnerContext {
                kind: crate::workflow::runner::RunnerKind::GenericCi,
                ci: true,
                interactive: false,
                ci_environment: Some(crate::workflow::ci::CiEnvironment {
                    provider: crate::workflow::ci::CiProvider::Generic,
                    run_id: None,
                }),
            },
            Arc::new(MissingRevision),
        );
        assert!(planner
            .plan()
            .unwrap_err()
            .contains("Source revision is unavailable"));
    }

    #[test]
    fn ci_build_push_plan_workflow_plan_has_image_push_plan() {
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
            cache_dir: temp_dir
                .path()
                .join(".sailr/cache")
                .to_string_lossy()
                .to_string(),
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
        assert_plan_pipeline_parity(&plan, &pipeline);

        let tasks: Vec<_> = pipeline.tasks().collect();
        assert!(tasks.iter().any(|t| t.name == "service:api:build"));
        assert!(tasks.iter().any(|t| t.name == "service:api:push"));
        assert!(tasks
            .iter()
            .any(|t| t.name == crate::workflow::task_id::IMAGE_REPORT));

        let api_push = tasks.iter().find(|t| t.name == "service:api:push").unwrap();
        assert_eq!(
            api_push.dependencies,
            vec![
                crate::workflow::task_id::service_build("api"),
                crate::workflow::task_id::PUSH_PLAN.to_string()
            ]
        );

        let report = tasks
            .iter()
            .find(|t| t.name == crate::workflow::task_id::IMAGE_REPORT)
            .unwrap();
        assert_eq!(report.dependencies, vec!["service:api:push"]);
    }

    #[test]
    fn dependent_builds_and_hooks_have_plan_runtime_parity_and_derived_effects() {
        let temp = tempfile::tempdir().unwrap();
        let shared = temp.path().join("shared");
        let api = temp.path().join("api");
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::create_dir_all(&api).unwrap();
        let environment: Environment = toml::from_str(&format!(
            r#"
            schema_version = "v0.5"
            name = "test"
            domain = "test.local"
            log_level = "info"
            default_replicas = 1
            registry = "ghcr.io/acme"

            [build]
            before_all = "echo before"
            after_all = "echo after"

            [[service]]
            name = "shared"
            version = "1.0.0"
            [service.build]
            path = "{}"

            [[service]]
            name = "api"
            version = "1.0.0"
            [service.build]
            path = "{}"
            depends_on = ["shared"]
            "#,
            shared.display(),
            api.display()
        ))
        .unwrap();
        let mut profile: crate::workflow::profile::WorkflowProfile = toml::from_str(
            r#"
            environment = "test"
            mode = "build"
            build = "run"
            push = "plan"
            "#,
        )
        .unwrap();
        let build_policy = environment.build.clone();
        profile.name = "dependency-hooks".to_string();
        let planner = WorkflowPlanner::new(
            profile.normalize(false),
            Arc::new(environment),
            BuildOptions {
                cache_dir: temp.path().join("cache").to_string_lossy().to_string(),
                force: true,
                only: vec![],
                ignore: vec![],
                plan: false,
                dry_run: false,
                explain: false,
                dump_scope: false,
                policy: build_policy,
            },
            RunnerContext {
                kind: crate::workflow::runner::RunnerKind::Local,
                ci: false,
                interactive: false,
                ci_environment: None,
            },
        );
        let plan = planner.plan().unwrap();
        let (pipeline, _) = planner
            .build_pipeline_from_plan(&plan, Default::default())
            .unwrap();
        assert_plan_pipeline_parity(&plan, &pipeline);

        let api_task = plan
            .tasks
            .iter()
            .find(|task| task.id == crate::workflow::task_id::service_build("api"))
            .unwrap();
        assert!(api_task
            .dependencies
            .contains(&crate::workflow::task_id::service_build("shared")));
        assert!(api_task
            .dependencies
            .contains(&crate::workflow::task_id::BUILD_BEFORE_ALL.to_string()));

        let mut merged = WorkflowEffects::default();
        for task in &plan.tasks {
            merged.merge(&task.effects);
        }
        assert_eq!(plan.effects, merged);
        assert!(plan.effects.mutates_filesystem);
        assert!(plan.effects.mutates_docker);
        assert!(!plan.effects.mutates_registry);
    }
}
