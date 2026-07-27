use runkernel::{FailurePolicy, Pipeline, RollbackPolicy, Task};

use crate::builder::{
    add_runkernel_tasks_from_workflow_plan, create_sailr_build_plan, BuildOptions, SailrBuildPlan,
};
use crate::environment::Environment;
use crate::workflow::plan::{
    WorkflowEdge, WorkflowEffects, WorkflowFinalizerKind, WorkflowFinalizerPhase,
    WorkflowFinalizerPlan, WorkflowFinalizerTrigger, WorkflowPlan, WorkflowTaskCachePolicy,
    WorkflowTaskKind, WorkflowTaskPlan,
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
        let mut finalizers = Vec::new();
        let mut effects;
        let mut build_plan_opt = None;
        let mut image_push_plan_opt = None;
        let signer_key_fingerprint =
            if self.profile.approval == crate::workflow::profile::ApprovalMode::Signature {
                let signature = self.profile.signature.as_ref().ok_or_else(|| {
                    "approval=signature requires [workflow.<profile>.signature]".to_string()
                })?;
                if !self.profile.generate.is_active() {
                    return Err("approval=signature requires generate to be enabled".to_string());
                }
                if self.profile.deploy != crate::workflow::profile::WorkflowStepMode::Run {
                    return Err("approval=signature requires deploy=run".to_string());
                }
                Some(
                    crate::workflow::gate::trusted_key_fingerprint(&signature.trusted_public_key)
                        .map_err(|error| format!("Invalid trusted signer: {error}"))?,
                )
            } else {
                None
            };
        let deployment_state = crate::workflow::gate::DeploymentRunState::default();

        // 0. Validate Phase
        tasks.push(WorkflowTaskPlan {
            id: crate::workflow::task_id::VALIDATE_CONFIG.to_string(),
            label: "Validate Config".to_string(),
            kind: WorkflowTaskKind::ValidateConfig,
            cache_policy: WorkflowTaskCachePolicy::Disabled,
            service: None,
            phase: None,
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
                    cache_policy: WorkflowTaskCachePolicy::Disabled,
                    service: None,
                    phase: None,
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
                let translated = crate::workflow::translator::translate_build_plan(&plan, false);
                let translated_ids = translated
                    .iter()
                    .map(|task| task.id.clone())
                    .collect::<std::collections::BTreeSet<_>>();
                let depended_on = translated
                    .iter()
                    .flat_map(|task| task.dependencies.iter().cloned())
                    .collect::<std::collections::BTreeSet<_>>();

                for translated_task in &translated {
                    let mut dependencies = translated_task.dependencies.clone();
                    if dependencies.is_empty() {
                        dependencies.push(crate::workflow::task_id::VALIDATE_CONFIG.to_string());
                    }
                    tasks.push(WorkflowTaskPlan {
                        id: translated_task.id.clone(),
                        label: translated_task.label.clone(),
                        kind: WorkflowTaskKind::ServiceBuild,
                        cache_policy: if plan.force {
                            WorkflowTaskCachePolicy::ForcedBypass
                        } else {
                            match translated_task.cache_policy {
                                crate::workflow::translator::TranslatedCachePolicy::Disabled => {
                                    WorkflowTaskCachePolicy::Disabled
                                }
                                crate::workflow::translator::TranslatedCachePolicy::InputsAndKey => {
                                    WorkflowTaskCachePolicy::InputsAndKey
                                }
                            }
                        },
                        service: translated_task.service.clone(),
                        phase: Some(translated_task.phase.clone()),
                        dependencies,
                        effects: translated_task.effects.clone(),
                        description: format!(
                            "Runs deterministic build phase '{}'.",
                            translated_task.phase
                        ),
                    });
                }

                last_tasks = translated_ids
                    .difference(&depended_on)
                    .cloned()
                    .collect::<Vec<_>>();
                if last_tasks.is_empty() {
                    last_tasks = vec![crate::workflow::task_id::VALIDATE_CONFIG.to_string()];
                }
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
                let has_publications = build_plan_opt
                    .as_ref()
                    .map(|bp| bp.services.iter().any(|s| s.dirty))
                    .unwrap_or(false);

                let is_run = self.profile.push == crate::workflow::profile::WorkflowStepMode::Run;

                let source_revision = match (is_run, has_publications) {
                    (true, true) => Some(
                        self.source_revision_resolver
                            .resolve(&self.runner)
                            .map_err(|error| error.to_string())?
                            .ok_or_else(|| {
                                crate::workflow::error::ProvenanceError::MissingSourceRevision
                                    .to_string()
                            })?,
                    ),
                    (true, false) => None,
                    (false, _) => self
                        .source_revision_resolver
                        .resolve(&self.runner)
                        .ok()
                        .flatten(),
                };

                tasks.push(WorkflowTaskPlan {
                    id: crate::workflow::task_id::PUSH_PLAN.to_string(),
                    label: "Push Plan".to_string(),
                    kind: WorkflowTaskKind::PushPlan,
                    cache_policy: WorkflowTaskCachePolicy::Disabled,
                    service: None,
                    phase: None,
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
                        let build_task = build_plan_opt.as_ref().and_then(|build_plan| {
                            crate::workflow::translator::terminal_task_for_service(
                                build_plan,
                                &item.service,
                                false,
                            )
                        });
                        if let Some(build_task) =
                            build_task.filter(|id| tasks.iter().any(|task| task.id == *id))
                        {
                            dependencies.push(build_task);
                        }
                        dependencies.sort();
                        dependencies.dedup();

                        let push_task = crate::workflow::task_id::service_push(&item.service);
                        tasks.push(WorkflowTaskPlan {
                            id: push_task.clone(),
                            label: format!("Push {}", item.service),
                            kind: WorkflowTaskKind::ServicePush,
                            cache_policy: WorkflowTaskCachePolicy::Disabled,
                            service: Some(item.service.clone()),
                            phase: Some("push".to_string()),
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

                    if !push_tasks.is_empty() {
                        tasks.push(WorkflowTaskPlan {
                            id: crate::workflow::task_id::IMAGE_REPORT.to_string(),
                            label: "Finalize Image Artifacts".to_string(),
                            kind: WorkflowTaskKind::ImageReport,
                            cache_policy: WorkflowTaskCachePolicy::Disabled,
                            service: None,
                            phase: None,
                            dependencies: push_tasks,
                            effects: WorkflowEffects::default(),
                            description: "Ensures all service publication tasks have completed before workflow finalization.".to_string(),
                        });
                        last_tasks = vec![crate::workflow::task_id::IMAGE_REPORT.to_string()];
                    }
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
                cache_policy: WorkflowTaskCachePolicy::InputsAndKey,
                service: None,
                phase: None,
                dependencies: last_tasks.clone(),
                effects: generate_effects,
                description: "Generates Kubernetes manifests.".to_string(),
            });

            last_tasks = vec![crate::workflow::task_id::GENERATE.to_string()];
        }

        // 3. Deploy Phase
        if self.profile.deploy.is_active() {
            let has_pre_hooks = self.env.services.iter().any(|service| {
                service
                    .hooks
                    .as_ref()
                    .and_then(|hooks| hooks.pre_deploy.as_ref())
                    .is_some()
            });
            tasks.push(WorkflowTaskPlan {
                id: crate::workflow::task_id::PRE_DEPLOY_HOOKS.to_string(),
                label: "Pre-deployment Hooks".to_string(),
                kind: WorkflowTaskKind::PreDeployHooks,
                cache_policy: WorkflowTaskCachePolicy::Disabled,
                service: None,
                phase: Some("pre_deploy".to_string()),
                dependencies: last_tasks.clone(),
                effects: WorkflowEffects {
                    mutates_filesystem: has_pre_hooks,
                    ..Default::default()
                },
                description: "Runs pre-deployment hooks before constructing the signed bundle."
                    .to_string(),
            });
            last_tasks = vec![crate::workflow::task_id::PRE_DEPLOY_HOOKS.to_string()];

            tasks.push(WorkflowTaskPlan {
                id: crate::workflow::task_id::DEPLOYMENT_BUNDLE.to_string(),
                label: "Deployment Bundle".to_string(),
                kind: WorkflowTaskKind::DeploymentBundle,
                cache_policy: WorkflowTaskCachePolicy::Disabled,
                service: None,
                phase: None,
                dependencies: last_tasks.clone(),
                effects: WorkflowEffects {
                    mutates_filesystem: true,
                    ..Default::default()
                },
                description:
                    "Constructs the immutable in-memory deployment bundle and audit artifact."
                        .to_string(),
            });
            last_tasks = vec![crate::workflow::task_id::DEPLOYMENT_BUNDLE.to_string()];

            tasks.push(WorkflowTaskPlan {
                id: crate::workflow::task_id::DEPLOYMENT_PLAN.to_string(),
                label: "Deployment Plan".to_string(),
                kind: WorkflowTaskKind::DeploymentPlan,
                cache_policy: WorkflowTaskCachePolicy::Disabled,
                service: None,
                phase: None,
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
                        cache_policy: WorkflowTaskCachePolicy::Disabled,
                        service: None,
                        phase: None,
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

                if self.profile.approval == crate::workflow::profile::ApprovalMode::Signature {
                    tasks.push(WorkflowTaskPlan {
                        id: crate::workflow::task_id::VERIFICATION_GATE.to_string(),
                        label: "Cryptographic Verification Gate".to_string(),
                        kind: WorkflowTaskKind::VerificationGate,
                        cache_policy: WorkflowTaskCachePolicy::Disabled,
                        service: None,
                        phase: None,
                        dependencies: last_tasks.clone(),
                        effects: WorkflowEffects::default(),
                        description:
                            "Verifies an Ed25519 signature over the immutable deployment plan."
                                .to_string(),
                    });
                    last_tasks = vec![crate::workflow::task_id::VERIFICATION_GATE.to_string()];
                }

                if self.profile.apply {
                    tasks.push(WorkflowTaskPlan {
                        id: crate::workflow::task_id::DEPLOY.to_string(),
                        label: "Deploy".to_string(),
                        kind: WorkflowTaskKind::Deploy,
                        cache_policy: WorkflowTaskCachePolicy::Disabled,
                        service: None,
                        phase: None,
                        dependencies: last_tasks.clone(),
                        effects: WorkflowEffects {
                            mutates_cluster: true,
                            ..Default::default()
                        },
                        description:
                            "Apply generated manifests to the configured Kubernetes context."
                                .to_string(),
                    });
                    last_tasks = vec![crate::workflow::task_id::DEPLOY.to_string()];

                    let has_post_hooks = self.env.services.iter().any(|service| {
                        service
                            .hooks
                            .as_ref()
                            .and_then(|hooks| hooks.post_deploy.as_ref())
                            .is_some()
                    });
                    tasks.push(WorkflowTaskPlan {
                        id: crate::workflow::task_id::POST_DEPLOY_HOOKS.to_string(),
                        label: "Post-deployment Hooks".to_string(),
                        kind: WorkflowTaskKind::PostDeployHooks,
                        cache_policy: WorkflowTaskCachePolicy::Disabled,
                        service: None,
                        phase: Some("post_deploy".to_string()),
                        dependencies: last_tasks.clone(),
                        effects: WorkflowEffects {
                            mutates_filesystem: has_post_hooks,
                            ..Default::default()
                        },
                        description: "Runs post-deployment hooks after all bundle resources apply."
                            .to_string(),
                    });
                }
            }
        }

        if let Some(plan) = &build_plan_opt {
            if self.profile.build == crate::workflow::profile::WorkflowStepMode::Run {
                for service in plan
                    .services
                    .iter()
                    .rev()
                    .filter(|service| service.dirty && !service.phases.finally.is_empty())
                {
                    finalizers.push(WorkflowFinalizerPlan {
                        id: crate::workflow::task_id::service_finally(&service.service.name),
                        label: format!("Finalize {}", service.service.name),
                        kind: WorkflowFinalizerKind::RunServiceFinally {
                            service: service.service.name.clone(),
                            cwd: service.cwd.clone(),
                            commands: service.phases.finally.clone(),
                        },
                        phase: WorkflowFinalizerPhase::BeforeReport,
                        trigger: WorkflowFinalizerTrigger::Always,
                        effects: WorkflowEffects {
                            mutates_filesystem: true,
                            ..Default::default()
                        },
                        description: "Runs service cleanup after all active pipeline tasks settle."
                            .to_string(),
                    });
                }
                let dirty_services = plan.services.iter().filter(|s| s.dirty).count();
                if dirty_services > 0 {
                    finalizers.push(WorkflowFinalizerPlan {
                        id: crate::workflow::task_id::WRITE_BUILD_CACHE_FINALIZER.to_string(),
                        label: "Write Build Caches".to_string(),
                        kind: WorkflowFinalizerKind::WriteBuildCache,
                        phase: WorkflowFinalizerPhase::BeforeReport,
                        trigger: WorkflowFinalizerTrigger::OnSuccess,
                        effects: WorkflowEffects {
                            mutates_filesystem: true,
                            ..Default::default()
                        },
                        description: "Updates local build fingerprint caches for successfully built services.".to_string(),
                    });
                }
            }
        }

        if matches!(
            self.profile.report,
            crate::workflow::profile::ReportMode::Json | crate::workflow::profile::ReportMode::Both
        ) {
            finalizers.push(WorkflowFinalizerPlan {
                id: crate::workflow::task_id::WRITE_REPORT_FINALIZER.to_string(),
                label: "Write Workflow Report".to_string(),
                kind: WorkflowFinalizerKind::WriteWorkflowReport,
                phase: WorkflowFinalizerPhase::ReportSink,
                trigger: WorkflowFinalizerTrigger::Always,
                effects: WorkflowEffects {
                    mutates_filesystem: true,
                    ..Default::default()
                },
                description:
                    "Writes the versioned workflow execution report after pipeline completion."
                        .to_string(),
            });
        }

        effects = WorkflowEffects::default();
        for task in &tasks {
            effects.merge(&task.effects);
        }
        for finalizer in &finalizers {
            effects.merge(&finalizer.effects);
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
            finalizers,
            effects,
            cache_predictions: std::collections::BTreeMap::new(),
            signer_key_fingerprint,
            deployment_state,
        })
    }

    fn build_image_push_plan_report(
        &self,
        build_plan: &crate::builder::SailrBuildPlan,
        is_run: bool,
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

            let tag = crate::workflow::image::derive_image_tag(&service_plan.fingerprint.full_hash)
                .map_err(|error| error.to_string())?;

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

        let report = crate::workflow::image::ImagePushPlanReport {
            environment: self.profile.environment.clone(),
            mutates_registry: is_run && !items.is_empty(),
            items,
        };
        report.validate().map_err(|error| error.to_string())?;
        Ok(report)
    }

    pub fn build_pipeline_from_plan(
        &self,
        plan: &WorkflowPlan,
        accumulator: crate::workflow::image::WorkflowReportAccumulator,
    ) -> Result<(Pipeline, WorkflowBuildExecution), String> {
        let mut pipeline = Pipeline::new(format!("Workflow: {}", self.profile.name));
        if self.profile.deploy == crate::workflow::profile::WorkflowStepMode::Run
            && self.profile.apply
        {
            pipeline = pipeline
                .failure_policy(FailurePolicy::FinishRunning)
                .rollback_policy(RollbackPolicy::CompletedTasksReverseOrder);
        }
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

                if plan
                    .tasks
                    .iter()
                    .any(|task| task.id == crate::workflow::task_id::IMAGE_REPORT)
                {
                    let report_task = runtime_task(plan, crate::workflow::task_id::IMAGE_REPORT)?
                        .exec_fn(|_ctx| async move {
                            crate::LOGGER.info("Image publication evidence finalized.");
                            Ok(())
                        });
                    pipeline.add(report_task);
                }
            }
        }

        if self.profile.generate.is_active() {
            let mut task = runtime_task(plan, crate::workflow::task_id::GENERATE)?;

            let name = self.profile.environment.clone();
            let environment_input =
                format!("k8s/environments/{}/config.toml", self.profile.environment);
            task = task
                .inputs(&["k8s/templates/**/*.yaml", environment_input.as_str()])
                .cache_key("sailr-manifest-generator-v1");
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
            let env = self.env.clone();
            pipeline.add(
                runtime_task(plan, crate::workflow::task_id::PRE_DEPLOY_HOOKS)?
                    .cache_disabled()
                    .exec_fn(move |_ctx| {
                        let env = env.clone();
                        async move {
                            crate::deployment::run_environment_hooks(
                                &env,
                                crate::deployment::DeploymentHookStage::Pre,
                            )
                            .map_err(|error| anyhow::anyhow!(error.to_string()))
                        }
                    }),
            );

            let profile_name = self.profile.name.clone();
            let environment = self.profile.environment.clone();
            let context = self.profile.deploy_context.clone().unwrap_or_default();
            let namespace = self
                .profile
                .namespace
                .clone()
                .unwrap_or_else(|| "default".to_string());
            let signer_fingerprint = plan.signer_key_fingerprint.clone();
            let deployment_state = plan.deployment_state.clone();
            pipeline.add(
                runtime_task(plan, crate::workflow::task_id::DEPLOYMENT_BUNDLE)?
                    .cache_disabled()
                    .exec_fn(move |ctx| {
                        let profile_name = profile_name.clone();
                        let environment = environment.clone();
                        let context = context.clone();
                        let namespace = namespace.clone();
                        let signer_fingerprint = signer_fingerprint.clone();
                        let deployment_state = deployment_state.clone();
                        async move {
                            let bundle = crate::deployment::bundle::build_deployment_bundle(
                                &crate::workflow::gate::generated_manifest_root(&environment),
                                &profile_name,
                                &environment,
                                &context,
                                &namespace,
                            )
                            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                            crate::workflow::gate::write_artifact(
                                &crate::workflow::gate::audit_artifact_path(&profile_name),
                                &bundle.approval_artifact(),
                            )?;
                            ctx.set_output(
                                crate::workflow::gate::PLAN_HASH_OUTPUT,
                                bundle.plan_hash.clone(),
                            )?;
                            deployment_state.set_bundle(bundle, signer_fingerprint)?;
                            Ok(())
                        }
                    }),
            );

            let mut task = runtime_task(plan, crate::workflow::task_id::DEPLOYMENT_PLAN)?;

            let env_name = self.profile.environment.clone();
            let context = self.profile.deploy_context.clone().unwrap_or_default();
            let namespace = self
                .profile
                .namespace
                .clone()
                .unwrap_or_else(|| "default".to_string());

            let deployment_state = plan.deployment_state.clone();

            task = task.exec_fn(move |_ctx| {
                let env_name = env_name.clone();
                let context = context.clone();
                let namespace = namespace.clone();
                let deployment_state = deployment_state.clone();
                async move {
                    crate::LOGGER.info("Deployment plan:");
                    let bundle = deployment_state.bundle()?;
                    println!("Sailr deployment plan:");
                    println!("  environment: {}", env_name);
                    println!("  context: {}", context);
                    println!("  namespace: {}", namespace);
                    println!("  plan hash: {}", bundle.plan_hash);
                    println!("  resources: {}", bundle.resources.len());
                    for resource in bundle.resources {
                        println!(
                            "  - {} {} {}/{}",
                            resource.identity.api_version,
                            resource.identity.kind,
                            resource
                                .identity
                                .namespace
                                .as_deref()
                                .unwrap_or("<cluster>"),
                            resource.identity.name
                        );
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

            if self.profile.approval == crate::workflow::profile::ApprovalMode::Signature {
                let gate_plan = plan
                    .tasks
                    .iter()
                    .find(|task| task.id == crate::workflow::task_id::VERIFICATION_GATE)
                    .ok_or_else(|| {
                        "Signature approval selected but verification gate is missing".to_string()
                    })?;
                pipeline.add(crate::workflow::gate::build_verification_task(
                    &gate_plan.dependencies,
                    self.profile
                        .signature
                        .as_ref()
                        .ok_or_else(|| {
                            "Signature approval selected but trusted signer is missing".to_string()
                        })?
                        .trusted_public_key
                        .clone(),
                    plan.deployment_state.clone(),
                ));
            }

            if self.profile.deploy == crate::workflow::profile::WorkflowStepMode::Run
                && self.profile.apply
            {
                let mut task = runtime_task(plan, crate::workflow::task_id::DEPLOY)?;

                let context = self.profile.deploy_context.clone().unwrap_or_default();
                let deployment_state = plan.deployment_state.clone();
                let journal = crate::deployment::new_deployment_journal();
                plan.deployment_state
                    .set_journal(journal.clone())
                    .map_err(|error| error.to_string())?;
                let rollback_journal = journal.clone();
                let backend_state = Arc::new(tokio::sync::Mutex::new(
                    None::<Arc<crate::deployment::KubernetesDeploymentBackend>>,
                ));
                let rollback_backend_state = backend_state.clone();

                task = task
                    .exec_fn(move |_ctx| {
                        let context = context.clone();
                        let deployment_state = deployment_state.clone();
                        let journal = journal.clone();
                        let backend_state = backend_state.clone();

                        async move {
                            let bundle = deployment_state.bundle()?;
                            crate::LOGGER.info(&format!(
                                "Deploying immutable plan '{}' to context '{}'...",
                                bundle.plan_hash, context
                            ));
                            let backend = Arc::new(
                                crate::deployment::KubernetesDeploymentBackend::new(context)
                                    .await
                                    .map_err(|error| {
                                        anyhow::anyhow!("Deploy backend failed: {error}")
                                    })?,
                            );
                            *backend_state.lock().await = Some(backend.clone());
                            crate::deployment::deploy_bundle(&bundle, backend.as_ref(), journal)
                                .await
                                .map_err(|error| anyhow::anyhow!("Deploy failed: {error}"))?;

                            Ok(())
                        }
                    })
                    .rollback(move |_ctx| {
                        let rollback_journal = rollback_journal.clone();
                        let rollback_backend_state = rollback_backend_state.clone();
                        async move {
                            let backend =
                                rollback_backend_state.lock().await.clone().ok_or_else(|| {
                                    anyhow::anyhow!(
                                        "Deployment backend is unavailable for rollback"
                                    )
                                })?;
                            crate::deployment::rollback_transaction(
                                &rollback_journal,
                                backend.as_ref(),
                            )
                            .await
                            .map_err(|error| anyhow::anyhow!(error.to_string()))
                        }
                    });

                pipeline.add(task);

                let env = self.env.clone();
                pipeline.add(
                    runtime_task(plan, crate::workflow::task_id::POST_DEPLOY_HOOKS)?
                        .cache_disabled()
                        .exec_fn(move |_ctx| {
                            let env = env.clone();
                            async move {
                                crate::deployment::run_environment_hooks(
                                    &env,
                                    crate::deployment::DeploymentHookStage::Post,
                                )
                                .map_err(|error| anyhow::anyhow!(error.to_string()))
                            }
                        }),
                );
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
            signature: None,
            apply: false,
            report: ReportMode::Text,
        }
    }

    fn dummy_options(plan: bool) -> BuildOptions {
        BuildOptions {
            cache_dir: ".sailr/cache/build".to_string(),
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
            ignore_cache: None,
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
            ignore_cache: None,
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
            ignore_cache: None,
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
            crate::workflow::task_id::PRE_DEPLOY_HOOKS.to_string(),
            crate::workflow::task_id::DEPLOYMENT_BUNDLE.to_string(),
            crate::workflow::task_id::DEPLOYMENT_PLAN.to_string(),
            crate::workflow::task_id::APPROVAL.to_string(),
            crate::workflow::task_id::DEPLOY.to_string(),
            crate::workflow::task_id::POST_DEPLOY_HOOKS.to_string(),
        ];
        expected.sort();
        assert_eq!(task_names, expected);
    }

    #[test]
    fn signature_deploy_inserts_uncached_gate_before_deploy() {
        let env = Environment::new("local");
        let mut profile = dummy_profile(WorkflowStepMode::Run, WorkflowStepMode::Disabled);
        profile.approval = ApprovalMode::Signature;
        profile.signature = Some(crate::workflow::profile::SignatureApprovalConfig {
            trusted_public_key: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_string(),
        });
        profile.apply = true;
        profile.deploy_context = Some("minikube".to_string());
        let planner =
            WorkflowPlanner::new(profile, Arc::new(env), dummy_options(false), dummy_runner());
        let plan = planner.plan().unwrap();
        let deploy = plan
            .tasks
            .iter()
            .find(|task| task.id == crate::workflow::task_id::DEPLOY)
            .unwrap();
        assert_eq!(
            deploy.dependencies,
            vec![crate::workflow::task_id::VERIFICATION_GATE]
        );

        let (pipeline, _) = planner
            .build_pipeline_from_plan(&plan, Default::default())
            .unwrap();
        let gate = pipeline
            .task(crate::workflow::task_id::VERIFICATION_GATE)
            .unwrap();
        assert!(!gate.cacheable());
        assert_eq!(pipeline.failure_policy, FailurePolicy::FinishRunning);
        assert_eq!(
            pipeline.rollback_policy,
            RollbackPolicy::CompletedTasksReverseOrder
        );
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
            ignore_cache: None,
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
            .contains("Image publication requires a source revision"));
    }

    #[test]
    fn report_modes_plan_finalizers_and_effects() {
        for (report_mode, has_finalizer) in [
            (crate::workflow::profile::ReportMode::Text, false),
            (crate::workflow::profile::ReportMode::Json, true),
            (crate::workflow::profile::ReportMode::Both, true),
        ] {
            let report_value = match report_mode {
                crate::workflow::profile::ReportMode::Text => "text",
                crate::workflow::profile::ReportMode::Json => "json",
                crate::workflow::profile::ReportMode::Both => "both",
            };
            let mut profile: crate::workflow::profile::WorkflowProfile = toml::from_str(&format!(
                r#"
                environment = "test"
                mode = "check"
                build = "disabled"
                generate = "disabled"
                deploy = "disabled"
                report = "{report_value}"
                "#
            ))
            .unwrap();
            profile.name = format!("report-{report_mode:?}");
            let planner = WorkflowPlanner::new(
                profile.normalize(false),
                Arc::new(Environment::new("test")),
                BuildOptions {
                    cache_dir: ".sailr/test-finalizers".to_string(),
                    force: false,
                    only: vec![],
                    ignore: vec![],
                    plan: false,
                    dry_run: false,
                    explain: false,
                    dump_scope: false,
                    policy: None,
                },
                RunnerContext {
                    kind: crate::workflow::runner::RunnerKind::Local,
                    ci: false,
                    interactive: false,
                    ci_environment: None,
                },
            );
            let plan = planner.plan().unwrap();
            assert_eq!(plan.finalizers.len(), usize::from(has_finalizer));
            assert_eq!(plan.effects.mutates_filesystem, has_finalizer);
            if has_finalizer {
                assert_eq!(
                    plan.finalizers[0].id,
                    crate::workflow::task_id::WRITE_REPORT_FINALIZER
                );
            }
        }
    }

    #[test]
    fn no_op_push_run_has_no_registry_mutation_or_push_tasks() {
        let temp = tempfile::tempdir().unwrap();
        let service_path = temp.path().join("api");
        std::fs::create_dir_all(&service_path).unwrap();
        let mut environment = Environment::new("test");
        environment.registry = crate::environment::RegistryConfig::Simple("ghcr.io/acme".into());
        let mut service = crate::environment::Service::new("api", None, "1.0.0");
        service.build = Some(crate::environment::ServiceBuildConfig {
            path: service_path.to_string_lossy().to_string(),
            include: None,
            ignore_cache: None,
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
        let environment = Arc::new(environment);
        let cache_dir = temp.path().join("cache").to_string_lossy().to_string();

        let mut build_profile: crate::workflow::profile::WorkflowProfile = toml::from_str(
            r#"
            environment = "test"
            mode = "build"
            build = "run"
            report = "text"
            "#,
        )
        .unwrap();
        build_profile.name = "seed-cache".to_string();
        let options = BuildOptions {
            cache_dir: cache_dir.clone(),
            force: false,
            only: vec![],
            ignore: vec![],
            plan: false,
            dry_run: false,
            explain: false,
            dump_scope: false,
            policy: None,
        };
        let seed = WorkflowPlanner::new(
            build_profile.normalize(false),
            environment.clone(),
            options.clone(),
            RunnerContext {
                kind: crate::workflow::runner::RunnerKind::Local,
                ci: false,
                interactive: false,
                ci_environment: None,
            },
        )
        .plan()
        .unwrap()
        .build_plan
        .unwrap();
        crate::builder::write_successful_service_caches(
            &seed,
            &runkernel::PipelineResult {
                name: "seed".to_string(),
                duration: std::time::Duration::default(),
                tasks: vec![runkernel::TaskResult {
                    name: crate::workflow::task_id::service_build("api"),
                    status: runkernel::TaskStatus::Completed,
                    duration: None,
                    error: None,
                    cache_hit: false,
                    cache_reason: None,
                    rollback_status: None,
                    rollback_error: None,
                }],
                summary: runkernel::PipelineSummary {
                    name: "seed".to_string(),
                    success: true,
                    completed: 1,
                    failed: 0,
                    skipped: 0,
                    cached: 0,
                    cancelled: 0,
                    rolled_back: 0,
                    rollback_failed: 0,
                },
            },
        )
        .unwrap();

        struct RejectingResolver;

        impl SourceRevisionResolver for RejectingResolver {
            fn resolve(
                &self,
                _runner: &RunnerContext,
            ) -> Result<Option<String>, crate::workflow::error::ProvenanceError> {
                panic!("source revision resolver must not be called for no-op push-run");
            }
        }

        let mut push_profile: crate::workflow::profile::WorkflowProfile = toml::from_str(
            r#"
            environment = "test"
            mode = "build"
            build = "run"
            push = "run"
            report = "text"
            "#,
        )
        .unwrap();
        push_profile.name = "no-op-push".to_string();
        let planner = WorkflowPlanner::with_source_revision_resolver(
            push_profile.normalize(false),
            environment,
            options,
            RunnerContext {
                kind: crate::workflow::runner::RunnerKind::Local,
                ci: false,
                interactive: false,
                ci_environment: None,
            },
            Arc::new(RejectingResolver),
        );
        let plan = planner.plan().unwrap();
        let push_plan = plan.image_push_plan.as_ref().unwrap();
        assert!(push_plan.items.is_empty());
        assert!(!push_plan.mutates_registry);
        assert!(!plan.effects.mutates_registry);
        assert!(!plan.tasks.iter().any(|task| matches!(
            task.kind,
            WorkflowTaskKind::ServicePush | WorkflowTaskKind::ImageReport
        )));
        let (pipeline, _) = planner
            .build_pipeline_from_plan(&plan, Default::default())
            .unwrap();
        assert_plan_pipeline_parity(&plan, &pipeline);
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
            cache_dir: ".sailr/cache/build".to_string(),
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
                cache_dir: ".sailr/cache/build".to_string(),
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
        assert_eq!(
            plan.tasks
                .iter()
                .find(|task| task.id == crate::workflow::task_id::IMAGE_REPORT)
                .unwrap()
                .effects,
            WorkflowEffects::default()
        );
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
