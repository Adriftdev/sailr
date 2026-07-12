use crate::environment::{
    BuildEngine, BuildPolicy, CommandSpec, Environment, Service, ServiceBuildConfig,
};
use crate::roomservice::{
    room::{Hooks, RoomBuilder},
    BuildPlan as RoomservicePlan, GlobalPolicy, RoomserviceBuilder,
};
use async_trait::async_trait;
use checksums::{hash_file, Algorithm::BLAKE2S};
use ignore::{overrides::OverrideBuilder, WalkBuilder};
use runkernel::{FailurePolicy, Pipeline, PipelineEvent, PipelineResult, Task, TaskStatus};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

const RUNKERNEL_PIPELINE_NAME: &str = "Sailr Service Build Pipeline";

pub struct Builder {
    backend: Box<dyn BuildBackend + Send>,
    engine: BuildEngine,
}

#[derive(Debug, Clone)]
pub struct BuildRunResult {
    pub executed: bool,
    pub success: bool,
}

#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub(crate) cache_dir: String,
    pub(crate) force: bool,
    pub(crate) only: Vec<String>,
    pub(crate) ignore: Vec<String>,
    pub(crate) plan: bool,
    pub(crate) dry_run: bool,
    pub(crate) explain: bool,
    pub(crate) dump_scope: bool,
    pub(crate) policy: Option<BuildPolicy>,
}

#[async_trait]
pub trait BuildBackend {
    async fn build(&mut self, env: &Environment) -> Result<BuildRunResult, String>;
}

pub struct RoomserviceBuildBackend {
    options: BuildOptions,
}

pub struct RunkernelBuildBackend {
    options: BuildOptions,
}

impl Builder {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cache_dir: String,
        force: bool,
        only: Vec<String>,
        ignore: Vec<String>,
        plan: bool,
        dry_run: bool,
        explain: bool,
        dump_scope: bool,
        policy: Option<BuildPolicy>,
        engine_override: Option<BuildEngine>,
    ) -> Builder {
        let engine = engine_override
            .or_else(|| policy.as_ref().and_then(|policy| policy.engine))
            .unwrap_or(BuildEngine::Roomservice);
        let options = BuildOptions {
            cache_dir,
            force,
            only,
            ignore,
            plan,
            dry_run,
            explain,
            dump_scope,
            policy,
        };

        let backend: Box<dyn BuildBackend + Send> = match engine {
            BuildEngine::Roomservice => Box::new(RoomserviceBuildBackend { options }),
            BuildEngine::Runkernel => Box::new(RunkernelBuildBackend { options }),
        };

        Builder { backend, engine }
    }

    pub async fn build(&mut self, env: &Environment) -> Result<BuildRunResult, String> {
        self.backend.build(env).await
    }

    pub fn engine(&self) -> BuildEngine {
        self.engine
    }
}

#[async_trait]
impl BuildBackend for RoomserviceBuildBackend {
    async fn build(&mut self, env: &Environment) -> Result<BuildRunResult, String> {
        let selected_services = select_services(env, &self.options.only, &self.options.ignore)?;
        let buildable_names = buildable_service_names(env);
        let mut roomservice = RoomserviceBuilder::new(
            "./".to_string(),
            self.options.cache_dir.clone(),
            self.options.force,
            map_policy(self.options.policy.clone()),
        );

        for service in selected_services {
            let room = build_room(env, service, &buildable_names)?;
            roomservice.add_room(room).map_err(|e| e.to_string())?;
        }

        let plan = roomservice.plan(self.options.dump_scope)?;
        print_roomservice_plan(&plan, &self.options);

        if self.options.plan {
            return Ok(BuildRunResult {
                executed: false,
                success: true,
            });
        }

        let result = roomservice.execute(&plan, self.options.dry_run)?;
        if result.executed && !result.success {
            return Err("Roomservice 2.0 build execution failed".to_string());
        }

        Ok(BuildRunResult {
            executed: result.executed,
            success: result.success,
        })
    }
}

#[async_trait]
impl BuildBackend for RunkernelBuildBackend {
    async fn build(&mut self, env: &Environment) -> Result<BuildRunResult, String> {
        let plan = create_sailr_build_plan(env, &self.options)?;
        print_sailr_plan(&plan, &self.options);

        if self.options.plan || self.options.dry_run {
            return Ok(BuildRunResult {
                executed: false,
                success: true,
            });
        }

        let policy = self.options.policy.clone().unwrap_or_default();
        if policy.max_parallelism.is_some() {
            crate::LOGGER.warn(
                "warning: [build].max_parallelism is not yet enforced by the runkernel backend",
            );
        }

        let failure_policy = if policy.fail_fast.unwrap_or(false) {
            FailurePolicy::FailFast
        } else {
            FailurePolicy::FinishRunning
        };
        let mut pipeline = Pipeline::new(RUNKERNEL_PIPELINE_NAME).failure_policy(failure_policy);
        add_runkernel_tasks(&mut pipeline, &plan)?;
        attach_pipeline_logging(&mut pipeline);

        let result = pipeline
            .run()
            .await
            .map_err(|e| format!("runkernel build pipeline execution failed: {:?}", e))?;

        print_pipeline_result(&plan, &result);
        if !result.summary.success {
            return Err(format!(
                "runkernel build failed: {} failed, {} skipped, {} cancelled",
                result.summary.failed, result.summary.skipped, result.summary.cancelled
            ));
        }

        write_successful_service_caches(&plan, &result)?;

        Ok(BuildRunResult {
            executed: true,
            success: true,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SailrBuildPlan {
    pub services: Vec<ServiceBuildPlan>,
    pub before_all: Vec<String>,
    pub after_all: Vec<String>,
    pub force: bool,
    cache_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ServiceBuildPlan {
    pub service: Service,
    pub build: ServiceBuildConfig,
    pub cwd: PathBuf,
    pub dependencies: Vec<String>,
    pub dependency_paths: Vec<String>,
    pub input_patterns: Vec<String>,
    pub matched_input_files: Vec<PathBuf>,
    pub dirty: bool,
    pub dirty_reasons: Vec<DirtyReason>,
    pub fingerprint: ServiceFingerprint,
    pub phases: ServicePhases,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirtyReason {
    Force,
    SourceChanged,
    DependencyChanged(String),
    CommandChanged,
    ConfigChanged,
    CacheMiss,
}

impl DirtyReason {
    fn describe(&self) -> String {
        match self {
            DirtyReason::Force => "forced rebuild".to_string(),
            DirtyReason::SourceChanged => "source changed".to_string(),
            DirtyReason::DependencyChanged(name) => format!("dependency changed ({})", name),
            DirtyReason::CommandChanged => "command changed".to_string(),
            DirtyReason::ConfigChanged => "config changed".to_string(),
            DirtyReason::CacheMiss => "cache miss".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceFingerprint {
    pub source_hash: String,
    pub dependency_hash: String,
    pub command_hash: String,
    pub config_hash: String,
    pub full_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServiceCacheRecord {
    fingerprint: ServiceFingerprint,
    last_outcome: String,
}

#[derive(Debug, Clone)]
pub struct ServicePhases {
    before_synchronously: Vec<String>,
    before: Vec<String>,
    run_parallel: Vec<String>,
    run_synchronously: Vec<String>,
    build: Vec<String>,
    push: Vec<String>,
    after: Vec<String>,
    finally: Vec<String>,
}

impl ServicePhases {
    fn printable(&self) -> Vec<(&'static str, &[String])> {
        vec![
            ("before_synchronous", &self.before_synchronously),
            ("before", &self.before),
            ("run_parallel", &self.run_parallel),
            ("run_synchronous", &self.run_synchronously),
            ("build", &self.build),
            ("push", &self.push),
            ("after", &self.after),
            ("finally", &self.finally),
        ]
    }

    fn commands_for_hash(&self) -> Vec<String> {
        let mut commands = Vec::new();
        commands.extend(self.before_synchronously.clone());
        commands.extend(self.before.clone());
        commands.extend(self.run_parallel.clone());
        commands.extend(self.run_synchronously.clone());
        commands.extend(self.build.clone());
        commands.extend(self.push.clone());
        commands.extend(self.after.clone());
        commands.extend(self.finally.clone());
        commands
    }
}

#[derive(Clone)]
struct NormalizedBuildConfig {
    phases: ServicePhases,
}

pub(crate) fn create_sailr_build_plan(
    env: &Environment,
    options: &BuildOptions,
) -> Result<SailrBuildPlan, String> {
    let selected_services = select_services(env, &options.only, &options.ignore)?;
    let buildable_names = buildable_service_names(env);
    let policy = options.policy.clone().unwrap_or_default();
    let before_all = policy
        .before_all
        .map(CommandSpec::into_vec)
        .unwrap_or_default();
    let after_all = policy
        .after_all
        .map(CommandSpec::into_vec)
        .unwrap_or_default();
    let cache_dir = sailr_build_cache_dir(&options.cache_dir);
    let mut fingerprints = HashMap::new();
    let mut dirty_state = HashMap::new();
    let mut plans = Vec::new();

    fs::create_dir_all(cache_dir.join("services"))
        .map_err(|error| format!("Failed to create Sailr build cache directory: {}", error))?;
    fs::create_dir_all(cache_dir.join("scopes"))
        .map_err(|error| format!("Failed to create Sailr build scope directory: {}", error))?;

    for service in selected_services {
        let build = service
            .build
            .clone()
            .ok_or_else(|| format!("Service '{}' is not buildable", service.name))?;
        let (dependencies, dependency_paths) = split_dependencies(
            build.relies_on.clone().unwrap_or_default(),
            &buildable_names,
        );
        let input_patterns = build
            .include
            .clone()
            .unwrap_or_else(|| vec!["./**/*.*".to_string()]);
        let matched_input_files =
            resolve_input_files(&build.path, &input_patterns, &dependency_paths)?;
        let source_hash = hash_files(&matched_input_files);
        let normalized = normalize_build_config(env, service, &build)?;
        let dependency_hash = hash_text(
            &dependencies
                .iter()
                .filter_map(|dependency| fingerprints.get(dependency))
                .map(|fingerprint: &ServiceFingerprint| fingerprint.full_hash.clone())
                .chain(dependency_paths.iter().cloned())
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let command_hash = hash_text(&normalized.phases.commands_for_hash().join("\n"));
        let config_hash = hash_text(
            &serde_json::to_string(&(
                &service.name,
                &service.version,
                &env.registry,
                &env.platform,
                &build,
            ))
            .map_err(|error| format!("Failed to serialize service build config: {}", error))?,
        );
        let fingerprint = ServiceFingerprint {
            source_hash,
            dependency_hash,
            command_hash,
            config_hash,
            full_hash: String::new(),
        };
        let fingerprint = ServiceFingerprint {
            full_hash: hash_text(
                &[
                    fingerprint.source_hash.as_str(),
                    fingerprint.dependency_hash.as_str(),
                    fingerprint.command_hash.as_str(),
                    fingerprint.config_hash.as_str(),
                ]
                .join("\n"),
            ),
            ..fingerprint
        };
        let cache = load_service_cache(&cache_dir, &service.name)?;
        let mut dirty_reasons = Vec::new();

        if options.force {
            dirty_reasons.push(DirtyReason::Force);
        } else if let Some(cache) = cache {
            if cache.fingerprint.source_hash != fingerprint.source_hash {
                dirty_reasons.push(DirtyReason::SourceChanged);
            }
            if cache.fingerprint.command_hash != fingerprint.command_hash {
                dirty_reasons.push(DirtyReason::CommandChanged);
            }
            if cache.fingerprint.config_hash != fingerprint.config_hash {
                dirty_reasons.push(DirtyReason::ConfigChanged);
            }
            if cache.fingerprint.dependency_hash != fingerprint.dependency_hash {
                dirty_reasons.push(DirtyReason::DependencyChanged("fingerprint".to_string()));
            }
        } else {
            dirty_reasons.push(DirtyReason::CacheMiss);
        }

        for dependency in &dependencies {
            if dirty_state.get(dependency).copied().unwrap_or(false) {
                dirty_reasons.push(DirtyReason::DependencyChanged(dependency.clone()));
            }
        }
        dedupe_dirty_reasons(&mut dirty_reasons);

        if options.dump_scope {
            write_scope_dump(&cache_dir, &service.name, &matched_input_files)?;
        }

        let dirty = !dirty_reasons.is_empty();
        dirty_state.insert(service.name.clone(), dirty);
        fingerprints.insert(service.name.clone(), fingerprint.clone());

        plans.push(ServiceBuildPlan {
            service: service.clone(),
            build: build.clone(),
            cwd: PathBuf::from(&build.path),
            dependencies,
            dependency_paths,
            input_patterns,
            matched_input_files,
            dirty,
            dirty_reasons,
            fingerprint,
            phases: normalized.phases,
        });
    }

    Ok(SailrBuildPlan {
        services: plans,
        before_all,
        after_all,
        force: options.force,
        cache_dir,
    })
}

pub(crate) fn add_runkernel_tasks(
    pipeline: &mut Pipeline,
    plan: &SailrBuildPlan,
) -> Result<(), String> {
    let dirty_services = plan
        .services
        .iter()
        .filter(|service| service.dirty)
        .map(|service| service.service.name.clone())
        .collect::<BTreeSet<_>>();
    let has_dirty_services = !dirty_services.is_empty();
    let has_before_all = has_dirty_services && !plan.before_all.is_empty();

    if has_before_all {
        let commands = plan.before_all.clone();
        pipeline.add(
            Task::new(crate::workflow::task_id::BUILD_BEFORE_ALL)
                .cache_disabled()
                .exec_fn(move |_ctx| {
                    let commands = commands.clone();
                    async move {
                        for command in commands {
                            exec_cmd(".", &command, crate::workflow::task_id::BUILD_BEFORE_ALL)
                                .await
                                .map_err(anyhow::Error::msg)?;
                        }
                        Ok(())
                    }
                }),
        );
    }

    for service_plan in &plan.services {
        let mut dependencies: Vec<String> = service_plan
            .dependencies
            .iter()
            .map(|d| crate::workflow::task_id::service_build(d))
            .collect();
        if service_plan.dirty && has_before_all {
            dependencies.push(crate::workflow::task_id::BUILD_BEFORE_ALL.to_string());
        }
        dependencies.sort();
        dependencies.dedup();

        let mut task = Task::new(crate::workflow::task_id::service_build(
            &service_plan.service.name,
        ))
        .depends_on(&dependencies.iter().map(String::as_str).collect::<Vec<_>>())
        .cache_disabled();

        if service_plan.dirty {
            let service_name = service_plan.service.name.clone();
            let cwd = service_plan.cwd.clone();
            let phases = service_plan.phases.clone();
            task = task.exec_fn(move |_ctx| {
                let service_name = service_name.clone();
                let cwd = cwd.clone();
                let phases = phases.clone();
                async move { execute_service_build(service_name, cwd, phases).await }
            });
        }

        pipeline.add(task);
    }

    if has_dirty_services && !plan.after_all.is_empty() {
        let commands = plan.after_all.clone();
        pipeline.add(
            Task::new(crate::workflow::task_id::BUILD_AFTER_ALL)
                .depends_on(
                    &dirty_services
                        .iter()
                        .map(|s| crate::workflow::task_id::service_build(s))
                        .collect::<Vec<_>>()
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>(),
                )
                .cache_disabled()
                .exec_fn(move |_ctx| {
                    let commands = commands.clone();
                    async move {
                        for command in commands {
                            exec_cmd(".", &command, crate::workflow::task_id::BUILD_AFTER_ALL)
                                .await
                                .map_err(anyhow::Error::msg)?;
                        }
                        Ok(())
                    }
                }),
        );
    }

    Ok(())
}

async fn execute_service_build(
    service_name: String,
    cwd: PathBuf,
    phases: ServicePhases,
) -> anyhow::Result<()> {
    let cwd = cwd.to_string_lossy().to_string();
    let mut started = false;
    let mut first_error = None;

    for (phase_name, commands) in phases.printable() {
        if commands.is_empty() || phase_name == "finally" || phase_name == "push" {
            continue;
        }
        started = true;
        if crate::LOGGER.is_verbose() {
            crate::LOGGER.info(&format!(
                "Executing phase: {} -> {}",
                service_name, phase_name
            ));
        }

        if phase_name == "run_parallel" {
            let results = futures::future::join_all(
                commands
                    .iter()
                    .map(|command| exec_cmd(&cwd, command, &service_name)),
            )
            .await;
            if let Some(error) = results.into_iter().find_map(Result::err) {
                first_error = Some(error);
                break;
            }
        } else {
            for command in commands {
                if let Err(error) = exec_cmd(&cwd, command, &service_name).await {
                    first_error = Some(error);
                    break;
                }
            }
            if first_error.is_some() {
                break;
            }
        }
    }

    if started && !phases.finally.is_empty() {
        if crate::LOGGER.is_verbose() {
            crate::LOGGER.info(&format!("Executing finalizer: {} -> finally", service_name));
        }
        for command in phases.finally {
            if let Err(error) = exec_cmd(&cwd, &command, &service_name).await {
                crate::LOGGER.warn(&format!(
                    "finalizer command failed for service {}: {}",
                    service_name, error
                ));
            }
        }
    }

    if let Some(error) = first_error {
        anyhow::bail!("Service build failed: {}", error);
    }

    Ok(())
}

async fn exec_cmd(cwd: &str, cmd: &str, name: &str) -> Result<(), String> {
    let child = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            format!(
                "Failed to spawn command '{}' for service '{}': {}",
                cmd, name, e
            )
        })?;

    let output = child.wait_with_output().await.map_err(|e| {
        format!(
            "Failed to wait for command '{}' for service '{}': {}",
            cmd, name, e
        )
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        let mut err_msg = format!("Command exited with status code: {}\n", output.status);
        if !stdout.trim().is_empty() {
            err_msg.push_str(&format!("--- stdout ---\n{}\n", stdout));
        }
        if !stderr.trim().is_empty() {
            err_msg.push_str(&format!("--- stderr ---\n{}\n", stderr));
        }
        return Err(err_msg);
    }

    if crate::LOGGER.is_verbose() {
        if !stdout.trim().is_empty() {
            crate::LOGGER.info(&format!("{} (stdout):\n{}", name, stdout));
        }
        if !stderr.trim().is_empty() {
            crate::LOGGER.info(&format!("{} (stderr):\n{}", name, stderr));
        }
    }

    Ok(())
}

pub fn split_matches(val: Option<String>) -> Vec<String> {
    val.map(|values| {
        values
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    })
    .unwrap_or_default()
}

pub fn filter_services_exact<'a>(
    mut services: Vec<&'a Service>,
    only: &[String],
    ignore: &[String],
) -> Vec<&'a Service> {
    if !ignore.is_empty() {
        services.retain(|service| !ignore.contains(&service.name));
    }

    if !only.is_empty() {
        services.retain(|service| only.contains(&service.name));
    }

    services
}

fn select_services<'a>(
    env: &'a Environment,
    only: &[String],
    ignore: &[String],
) -> Result<Vec<&'a Service>, String> {
    let buildable = env
        .services
        .iter()
        .filter(|service| service.build.is_some())
        .collect::<Vec<_>>();

    if buildable.is_empty() {
        return Err("No buildable services found in environment config.".to_string());
    }

    let buildable_by_name = buildable
        .iter()
        .map(|service| (service.name.clone(), *service))
        .collect::<BTreeMap<_, _>>();
    let buildable_names = buildable_by_name.keys().cloned().collect::<BTreeSet<_>>();

    for name in only {
        if !buildable_names.contains(name) {
            return Err(format!("Unknown --only service '{}'", name));
        }
    }
    for name in ignore {
        if !buildable_names.contains(name) {
            return Err(format!("Unknown --ignore service '{}'", name));
        }
    }

    let mut selected = if only.is_empty() {
        buildable_names.clone()
    } else {
        let mut closure = BTreeSet::new();
        for name in only {
            collect_dependency_closure(name, &buildable_by_name, &buildable_names, &mut closure)?;
        }
        closure
    };

    for ignored in ignore {
        selected.remove(ignored);
    }

    for name in &selected {
        let service = buildable_by_name
            .get(name)
            .ok_or_else(|| format!("Missing buildable service '{}'", name))?;
        let build = service
            .build
            .as_ref()
            .ok_or_else(|| format!("Service '{}' is not buildable", service.name))?;
        let (service_dependencies, _) = split_dependencies(
            build.relies_on.clone().unwrap_or_default(),
            &buildable_names,
        );
        for dependency in service_dependencies {
            if ignore.contains(&dependency) {
                return Err(format!(
                    "Service '{}' depends on ignored service '{}'.\n\nEither remove '{}' from --ignore or ignore '{}' too.",
                    service.name, dependency, dependency, service.name
                ));
            }
            if !selected.contains(&dependency) {
                return Err(format!(
                    "Service '{}' depends on missing service '{}'",
                    service.name, dependency
                ));
            }
        }
    }

    topologically_sort_services(&selected, &buildable_by_name, &buildable_names)
}

fn collect_dependency_closure(
    name: &str,
    services: &BTreeMap<String, &Service>,
    buildable_names: &BTreeSet<String>,
    selected: &mut BTreeSet<String>,
) -> Result<(), String> {
    if !selected.insert(name.to_string()) {
        return Ok(());
    }

    let service = services
        .get(name)
        .ok_or_else(|| format!("Unknown buildable service '{}'", name))?;
    let build = service
        .build
        .as_ref()
        .ok_or_else(|| format!("Service '{}' is not buildable", service.name))?;
    let (dependencies, _) =
        split_dependencies(build.relies_on.clone().unwrap_or_default(), buildable_names);
    for dependency in dependencies {
        collect_dependency_closure(&dependency, services, buildable_names, selected)?;
    }

    Ok(())
}

fn topologically_sort_services<'a>(
    selected: &BTreeSet<String>,
    services: &BTreeMap<String, &'a Service>,
    buildable_names: &BTreeSet<String>,
) -> Result<Vec<&'a Service>, String> {
    let mut indegree = selected
        .iter()
        .map(|name| (name.clone(), 0usize))
        .collect::<BTreeMap<_, _>>();
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();

    for name in selected {
        let service = services
            .get(name)
            .ok_or_else(|| format!("Missing buildable service '{}'", name))?;
        let build = service
            .build
            .as_ref()
            .ok_or_else(|| format!("Service '{}' is not buildable", service.name))?;
        let (dependencies, _) =
            split_dependencies(build.relies_on.clone().unwrap_or_default(), buildable_names);
        for dependency in dependencies
            .into_iter()
            .filter(|dependency| selected.contains(dependency))
        {
            *indegree
                .get_mut(name)
                .ok_or_else(|| format!("Missing indegree for '{}'", name))? += 1;
            adjacency.entry(dependency).or_default().push(name.clone());
        }
    }

    let mut queue = indegree
        .iter()
        .filter_map(|(name, degree)| (*degree == 0).then_some(name.clone()))
        .collect::<VecDeque<_>>();
    let mut ordered = Vec::new();

    while let Some(name) = queue.pop_front() {
        ordered.push(
            *services
                .get(&name)
                .ok_or_else(|| format!("Missing service '{}'", name))?,
        );

        if let Some(children) = adjacency.get(&name) {
            for child in children {
                let degree = indegree
                    .get_mut(child)
                    .ok_or_else(|| format!("Missing indegree for '{}'", child))?;
                *degree -= 1;
                if *degree == 0 {
                    queue.push_back(child.clone());
                }
            }
        }
    }

    if ordered.len() != selected.len() {
        return Err("Detected circular service build dependencies".to_string());
    }

    Ok(ordered)
}

fn buildable_service_names(env: &Environment) -> BTreeSet<String> {
    env.services
        .iter()
        .filter(|service| service.build.is_some())
        .map(|service| service.name.clone())
        .collect()
}

fn split_dependencies(
    dependencies: Vec<String>,
    buildable_names: &BTreeSet<String>,
) -> (Vec<String>, Vec<String>) {
    let mut service_dependencies = Vec::new();
    let mut path_dependencies = Vec::new();

    for dependency in dependencies {
        if buildable_names.contains(&dependency) {
            service_dependencies.push(dependency);
        } else {
            path_dependencies.push(dependency);
        }
    }

    (service_dependencies, path_dependencies)
}

fn build_room(
    env: &Environment,
    service: &Service,
    buildable_names: &BTreeSet<String>,
) -> Result<RoomBuilder, String> {
    let build_cfg = service
        .build
        .as_ref()
        .expect("buildable service should include build config");
    let (dependency_rooms, dependency_paths) = split_dependencies(
        build_cfg.relies_on.clone().unwrap_or_default(),
        buildable_names,
    );
    let normalized = normalize_build_config(env, service, build_cfg)?;
    let phases = normalized.phases;
    let build_command = phases.build.into_iter().next();
    let push_command = phases.push.into_iter().next();

    Ok(RoomBuilder::new(
        service.name.clone(),
        build_cfg.path.clone(),
        ".roomservice".to_string(),
        build_cfg
            .include
            .clone()
            .unwrap_or_else(|| vec!["./**/*.*".to_string()]),
        dependency_rooms,
        dependency_paths,
        build_cfg.dockerfile.clone(),
        Hooks {
            before_synchronously: phases.before_synchronously,
            before: phases.before,
            run_parallel: phases.run_parallel,
            run_synchronously: phases.run_synchronously,
            after: phases.after,
            finally: phases.finally,
        },
        build_command,
        push_command,
        Some(
            env.registry
                .resolve()
                .map_err(|e| format!("Invalid registry configuration: {e}"))?
                .tagged_ref(&service.name, &service.version)
                .map_err(|e| format!("Failed to resolve image reference: {e}"))?,
        ),
    ))
}

fn normalize_build_config(
    env: &Environment,
    service: &Service,
    build_cfg: &ServiceBuildConfig,
) -> Result<NormalizedBuildConfig, String> {
    let legacy_semantics = env.schema_version != "0.5.0";
    let explicit_new_phase_fields = build_cfg.before_synchronous.is_some()
        || build_cfg.run_parallel.is_some()
        || build_cfg.run_synchronous.is_some()
        || build_cfg.finally.is_some()
        || build_cfg.build_command.is_some()
        || build_cfg.push_command.is_some();

    let before = if legacy_semantics && !explicit_new_phase_fields {
        Vec::new()
    } else {
        render_commands(build_cfg.before.clone(), env, service)?
    };
    let after = if legacy_semantics && !explicit_new_phase_fields {
        Vec::new()
    } else {
        render_commands(build_cfg.after.clone(), env, service)?
    };

    let build_command = build_cfg
        .build_command
        .clone()
        .or_else(|| {
            if legacy_semantics && !explicit_new_phase_fields {
                build_cfg.before.clone().map(command_spec_to_shell)
            } else {
                None
            }
        })
        .unwrap_or_else(|| default_build_command(env, build_cfg));
    let push_command = build_cfg
        .push_command
        .clone()
        .or_else(|| {
            if legacy_semantics && !explicit_new_phase_fields {
                build_cfg.after.clone().map(command_spec_to_shell)
            } else {
                None
            }
        })
        .unwrap_or_else(|| default_push_command(env));

    Ok(NormalizedBuildConfig {
        phases: ServicePhases {
            before_synchronously: render_commands(
                build_cfg.before_synchronous.clone(),
                env,
                service,
            )?,
            before,
            run_parallel: render_commands(build_cfg.run_parallel.clone(), env, service)?,
            run_synchronously: render_commands(build_cfg.run_synchronous.clone(), env, service)?,
            after,
            finally: render_commands(build_cfg.finally.clone(), env, service)?,
            build: vec![render_build_command(&build_command, env, service)?],
            push: vec![render_build_command(&push_command, env, service)?],
        },
    })
}

fn render_commands(
    commands: Option<CommandSpec>,
    env: &Environment,
    service: &Service,
) -> Result<Vec<String>, String> {
    commands
        .map(CommandSpec::into_vec)
        .unwrap_or_default()
        .into_iter()
        .map(|command| render_build_command(&command, env, service))
        .collect()
}

fn render_build_command(
    command: &str,
    env: &Environment,
    service: &Service,
) -> Result<String, String> {
    let mut rendered = command.to_string();

    let resolved_registry = env
        .registry
        .resolve()
        .map_err(|e| format!("Failed to parse registry config: {e}"))?;
    let image_ref = resolved_registry
        .tagged_ref(&service.name, &service.version)
        .map_err(|e| format!("Failed to build image ref: {e}"))?;

    for (key, value) in [
        ("image_ref", image_ref.as_str()),
        ("registry", resolved_registry.host.as_str()),
        ("platform", env.platform.as_deref().unwrap_or("")),
        ("environment", env.name.as_str()),
        ("name", service.name.as_str()),
        ("service_name", service.name.as_str()),
        ("version", service.version.as_str()),
    ] {
        rendered = replace_template_var(&rendered, key, value);
    }

    Ok(rendered)
}

fn default_build_command(env: &Environment, build_cfg: &ServiceBuildConfig) -> String {
    let dockerfile_segment = build_cfg
        .dockerfile
        .as_ref()
        .map(|dockerfile| format!(" -f {}", dockerfile))
        .unwrap_or_default();

    match env.platform.as_deref() {
        Some(platform) if !platform.trim().is_empty() => format!(
            "docker buildx build --ssh default --platform {}{} -t {{{{ image_ref }}}} .",
            platform, dockerfile_segment
        ),
        _ => format!(
            "docker buildx build --ssh default{} -t {{{{ image_ref }}}} .",
            dockerfile_segment
        ),
    }
}

fn default_push_command(_env: &Environment) -> String {
    "docker push {{ image_ref }}".to_string()
}

fn replace_template_var(input: &str, key: &str, value: &str) -> String {
    input
        .replace(&format!("{{{{ {} }}}}", key), value)
        .replace(&format!("{{{{{}}}}}", key), value)
}

fn command_spec_to_shell(command: CommandSpec) -> String {
    command.into_vec().join(" && ")
}

fn resolve_input_files(
    path: &str,
    include: &[String],
    dependency_paths: &[String],
) -> Result<Vec<PathBuf>, String> {
    let mut files = walk_file_paths(Path::new(path), Some(include))?;

    for dependency_path in dependency_paths {
        files.extend(walk_file_paths(Path::new(dependency_path), None)?);
    }

    files.sort();
    files.dedup();
    Ok(files)
}

fn walk_file_paths(root: &Path, include: Option<&[String]>) -> Result<Vec<PathBuf>, String> {
    if !root.exists() {
        return Err(format!("Build path does not exist: {}", root.display()));
    }

    let mut builder = WalkBuilder::new(root);
    if let Some(include) = include {
        if !include.is_empty() {
            let mut overrides = OverrideBuilder::new(root);
            for pattern in include {
                let clean_pattern = pattern.trim_start_matches("./");
                overrides.add(clean_pattern).map_err(|error| {
                    format!("Failed to parse include pattern '{}': {}", pattern, error)
                })?;
            }
            builder.overrides(
                overrides
                    .build()
                    .map_err(|error| format!("Failed to build include overrides: {}", error))?,
            );
        }
    }

    let mut files = Vec::new();
    for maybe_file in builder.build() {
        let Ok(file) = maybe_file else {
            continue;
        };
        if file.file_type().is_some_and(|entry| entry.is_file()) {
            files.push(file.path().to_path_buf());
        }
    }
    files.sort();
    Ok(files)
}

fn hash_files(files: &[PathBuf]) -> String {
    let mut file_hashes = files
        .iter()
        .map(|file| format!("{}:{}", file.display(), hash_file(file, BLAKE2S)))
        .collect::<Vec<_>>();
    file_hashes.sort();
    hash_text(&file_hashes.join("\n"))
}

fn hash_text(value: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn load_service_cache(
    cache_dir: &Path,
    service_name: &str,
) -> Result<Option<ServiceCacheRecord>, String> {
    let path = service_cache_path(cache_dir, service_name);
    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(path)
        .map_err(|error| format!("Failed to read Sailr service cache: {}", error))?;
    serde_json::from_str(&contents)
        .map(Some)
        .map_err(|error| format!("Failed to parse Sailr service cache: {}", error))
}

pub(crate) fn write_successful_service_caches(
    plan: &SailrBuildPlan,
    result: &PipelineResult,
) -> Result<(), String> {
    let completed_tasks = result
        .tasks
        .iter()
        .filter(|task| matches!(task.status, TaskStatus::Completed))
        .map(|task| task.name.as_str())
        .collect::<HashSet<_>>();

    for service in plan.services.iter().filter(|service| service.dirty) {
        let task_name = crate::workflow::task_id::service_build(&service.service.name);
        if !completed_tasks.contains(task_name.as_str()) {
            continue;
        }

        let record = ServiceCacheRecord {
            fingerprint: service.fingerprint.clone(),
            last_outcome: "success".to_string(),
        };
        let serialized = serde_json::to_string_pretty(&record)
            .map_err(|error| format!("Failed to serialize Sailr service cache: {}", error))?;
        fs::write(
            service_cache_path(&plan.cache_dir, &service.service.name),
            serialized,
        )
        .map_err(|error| format!("Failed to write Sailr service cache: {}", error))?;
    }
    Ok(())
}

fn service_cache_path(cache_dir: &Path, service_name: &str) -> PathBuf {
    cache_dir
        .join("services")
        .join(format!("{}.json", service_name))
}

fn sailr_build_cache_dir(configured_cache_dir: &str) -> PathBuf {
    if configured_cache_dir == ".roomservice" {
        PathBuf::from(".sailr").join("cache").join("build")
    } else {
        PathBuf::from(configured_cache_dir)
    }
}

fn write_scope_dump(cache_dir: &Path, service_name: &str, files: &[PathBuf]) -> Result<(), String> {
    let path = cache_dir
        .join("scopes")
        .join(format!("{}.txt", service_name));
    let contents = files
        .iter()
        .map(|file| file.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(path, contents).map_err(|error| format!("Failed to write scope dump: {}", error))
}

fn dedupe_dirty_reasons(reasons: &mut Vec<DirtyReason>) {
    let mut seen = HashSet::new();
    reasons.retain(|reason| seen.insert(reason.describe()));
}

fn map_policy(policy: Option<BuildPolicy>) -> GlobalPolicy {
    let Some(policy) = policy else {
        return GlobalPolicy::default();
    };

    GlobalPolicy {
        before_all: policy
            .before_all
            .map(CommandSpec::into_vec)
            .unwrap_or_default(),
        after_all: policy
            .after_all
            .map(CommandSpec::into_vec)
            .unwrap_or_default(),
        max_parallelism: policy.max_parallelism,
        fail_fast: policy.fail_fast.unwrap_or(false),
    }
}

fn print_roomservice_plan(plan: &RoomservicePlan, options: &BuildOptions) {
    if options.plan || options.dry_run || options.explain {
        plan.print(options.explain, options.plan || options.dry_run);
        return;
    }

    let changed_rooms = plan
        .rooms
        .iter()
        .filter(|room| room.dirty)
        .map(|room| format!("==> {}", room.room.name))
        .collect::<Vec<_>>();

    if changed_rooms.is_empty() {
        println!("All rooms appear to be up to date!");
    } else {
        println!("The following rooms have changed:");
        println!("{}", changed_rooms.join("\n"));
    }
}

pub(crate) fn print_sailr_plan(plan: &SailrBuildPlan, options: &BuildOptions) {
    if !(options.plan || options.dry_run || options.explain) {
        let dirty_services = plan
            .services
            .iter()
            .filter(|service| service.dirty)
            .map(|service| format!("==> {}", service.service.name))
            .collect::<Vec<_>>();

        if dirty_services.is_empty() {
            println!("All rooms appear to be up to date!");
        } else {
            println!("The following rooms have changed:");
            println!("{}", dirty_services.join("\n"));
        }
        return;
    }

    println!("Sailr build plan:");
    println!("Engine: runkernel");
    println!();
    for service in &plan.services {
        let status = if service.dirty { "dirty" } else { "clean" };
        println!(" - {} [{}]", service.service.name, status);

        if options.explain && !service.dirty_reasons.is_empty() {
            let reasons = service
                .dirty_reasons
                .iter()
                .map(DirtyReason::describe)
                .collect::<Vec<_>>()
                .join(", ");
            println!("   reasons: {}", reasons);
        }

        if (options.plan || options.dry_run) && service.dirty {
            for (phase, commands) in service.phases.printable() {
                if commands.is_empty() {
                    continue;
                }
                println!("   phase {}:", phase);
                for command in commands {
                    println!("     {}", command);
                }
            }
        }
    }
}

pub(crate) fn print_pipeline_result(plan: &SailrBuildPlan, result: &PipelineResult) {
    let task_statuses = result
        .tasks
        .iter()
        .map(|task| (task.name.as_str(), &task.status))
        .collect::<HashMap<_, _>>();
    let built = plan
        .services
        .iter()
        .filter(|service| {
            service.dirty
                && matches!(
                    task_statuses.get(service.service.name.as_str()),
                    Some(TaskStatus::Completed)
                )
        })
        .count();
    let clean = plan
        .services
        .iter()
        .filter(|service| !service.dirty)
        .count();
    let failed = plan
        .services
        .iter()
        .filter(|service| {
            service.dirty
                && matches!(
                    task_statuses.get(service.service.name.as_str()),
                    Some(TaskStatus::Failed)
                )
        })
        .count();
    let skipped = plan
        .services
        .iter()
        .filter(|service| {
            service.dirty
                && matches!(
                    task_statuses.get(service.service.name.as_str()),
                    Some(TaskStatus::Skipped | TaskStatus::Cancelled)
                )
        })
        .count();

    println!(
        "Sailr build result:\n  engine: runkernel\n  built: {}\n  clean: {}\n  failed: {}\n  skipped: {}\n  duration: {:.1}s",
        built,
        clean,
        failed,
        skipped,
        result.duration.as_secs_f64()
    );
}

pub(crate) fn attach_pipeline_logging(pipeline: &mut Pipeline) {
    pipeline.on_event(|event| match event {
        PipelineEvent::PipelineStarted { name, task_count } => {
            crate::LOGGER.info(&format!(
                "Pipeline '{}' started with {} tasks.",
                name, task_count
            ));
        }
        PipelineEvent::TaskStarted { name } => {
            crate::LOGGER.info(&format!("Task '{}' started.", name));
        }
        PipelineEvent::TaskCompleted { name, duration } => {
            crate::LOGGER.task_completed(&name, duration);
        }
        PipelineEvent::TaskQueued { name } => {
            crate::LOGGER.info(&format!("Task '{}' queued.", name));
        }
        PipelineEvent::TaskFailed { name, error } => {
            crate::LOGGER.task_failed(&name, &error);
        }
        PipelineEvent::TaskCached { name, .. } => {
            crate::LOGGER.task_cached(&name);
        }
        PipelineEvent::TaskCancelled { name } => {
            crate::LOGGER.warn(&format!("Task '{}' cancelled.", name));
        }
        PipelineEvent::TaskSkipped { name, reason } => {
            crate::LOGGER.debug(&format!("Task '{}' skipped: {}", name, reason));
        }
        PipelineEvent::TaskRollbackCompleted { name } => {
            crate::LOGGER.task_rollback(&name);
        }
        PipelineEvent::TaskRollbackFailed { name, error } => {
            crate::LOGGER.task_failed(&name, &error);
        }
        PipelineEvent::PipelineFinished { result } => {
            crate::LOGGER.info(&format!(
                "Pipeline summary: {} completed, {} failed, {} skipped, {} cancelled, {} rollback failed, {} cached.",
                result.completed,
                result.failed,
                result.skipped,
                result.cancelled,
                result.rollback_failed,
                result.cached
            ));
        }
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::TempDir;

    fn options(cache_dir: PathBuf) -> BuildOptions {
        BuildOptions {
            cache_dir: cache_dir.to_string_lossy().to_string(),
            force: false,
            only: Vec::new(),
            ignore: Vec::new(),
            plan: false,
            dry_run: false,
            explain: false,
            dump_scope: false,
            policy: None,
        }
    }

    fn build_config(path: &Path, command: String) -> ServiceBuildConfig {
        ServiceBuildConfig {
            path: path.to_string_lossy().to_string(),
            include: Some(vec!["./**/*.*".to_string()]),
            relies_on: None,
            before_synchronous: None,
            before: None,
            run_parallel: None,
            run_synchronous: None,
            after: None,
            finally: None,
            dockerfile: None,
            build_command: Some(command),
            push_command: Some("true".to_string()),
        }
    }

    fn service(name: &str, path: &Path, command: String) -> Service {
        let mut service = Service::new(name, None, "1.2.3");
        service.build = Some(build_config(path, command));
        service
    }

    fn write_project(path: &Path) {
        fs::create_dir_all(path.join("src")).expect("service src should be created");
        fs::write(path.join("package.json"), "{\"version\":\"1.0.0\"}")
            .expect("package should be written");
        fs::write(path.join("src/index.js"), "console.log('hello');")
            .expect("source should be written");
    }

    fn pipeline_result(statuses: Vec<(&str, TaskStatus)>) -> PipelineResult {
        PipelineResult {
            name: RUNKERNEL_PIPELINE_NAME.to_string(),
            duration: Duration::from_millis(1),
            summary: runkernel::PipelineSummary {
                name: RUNKERNEL_PIPELINE_NAME.to_string(),
                success: statuses
                    .iter()
                    .all(|(_, status)| matches!(status, TaskStatus::Completed)),
                completed: statuses
                    .iter()
                    .filter(|(_, status)| matches!(status, TaskStatus::Completed))
                    .count(),
                failed: statuses
                    .iter()
                    .filter(|(_, status)| matches!(status, TaskStatus::Failed))
                    .count(),
                skipped: statuses
                    .iter()
                    .filter(|(_, status)| matches!(status, TaskStatus::Skipped))
                    .count(),
                cached: 0,
                cancelled: statuses
                    .iter()
                    .filter(|(_, status)| matches!(status, TaskStatus::Cancelled))
                    .count(),
                rolled_back: 0,
                rollback_failed: 0,
            },
            tasks: statuses
                .into_iter()
                .map(|(name, status)| runkernel::TaskResult {
                    name: name.to_string(),
                    status,
                    duration: Some(Duration::from_millis(1)),
                    error: None,
                    cache_hit: false,
                    cache_reason: None,
                    rollback_status: None,
                    rollback_error: None,
                })
                .collect(),
        }
    }

    #[test]
    fn backend_selection_defaults_to_roomservice() {
        let builder = Builder::new(
            ".roomservice".to_string(),
            false,
            Vec::new(),
            Vec::new(),
            false,
            false,
            false,
            false,
            None,
            None,
        );
        assert_eq!(builder.engine(), BuildEngine::Roomservice);
    }

    #[test]
    fn backend_selection_uses_config_engine() {
        let builder = Builder::new(
            ".roomservice".to_string(),
            false,
            Vec::new(),
            Vec::new(),
            false,
            false,
            false,
            false,
            Some(BuildPolicy {
                engine: Some(BuildEngine::Runkernel),
                ..BuildPolicy::default()
            }),
            None,
        );
        assert_eq!(builder.engine(), BuildEngine::Runkernel);
    }

    #[test]
    fn backend_selection_cli_override_beats_config() {
        let builder = Builder::new(
            ".roomservice".to_string(),
            false,
            Vec::new(),
            Vec::new(),
            false,
            false,
            false,
            false,
            Some(BuildPolicy {
                engine: Some(BuildEngine::Runkernel),
                ..BuildPolicy::default()
            }),
            Some(BuildEngine::Roomservice),
        );
        assert_eq!(builder.engine(), BuildEngine::Roomservice);
    }

    #[test]
    fn exact_service_filtering_does_not_substring_match() {
        let mut env = Environment::new("dev");
        env.services = vec![
            Service::new("api", None, "latest"),
            Service::new("api-gateway", None, "latest"),
            Service::new("web", None, "latest"),
        ];

        let only_api =
            filter_services_exact(env.list_services(), &["api".to_string()], &Vec::new());
        assert_eq!(
            only_api
                .iter()
                .map(|service| service.name.as_str())
                .collect::<Vec<_>>(),
            vec!["api"]
        );

        let ignore_api =
            filter_services_exact(env.list_services(), &Vec::new(), &["api".to_string()]);
        assert_eq!(
            ignore_api
                .iter()
                .map(|service| service.name.as_str())
                .collect::<Vec<_>>(),
            vec!["api-gateway", "web"]
        );
    }

    #[test]
    fn only_includes_build_dependencies() {
        let temp = TempDir::new().expect("tempdir should be created");
        let api_path = temp.path().join("api");
        let web_path = temp.path().join("web");
        write_project(&api_path);
        write_project(&web_path);

        let api = service("api", &api_path, "true".to_string());
        let mut web = service("web", &web_path, "true".to_string());
        web.build.as_mut().unwrap().relies_on = Some(vec!["api".to_string()]);

        let mut env = Environment::new("dev");
        env.services = vec![api, web];

        let selected = select_services(&env, &["web".to_string()], &[])
            .expect("selection should include dependency closure");
        assert_eq!(
            selected
                .iter()
                .map(|service| service.name.as_str())
                .collect::<Vec<_>>(),
            vec!["api", "web"]
        );
    }

    #[test]
    fn ignore_required_dependency_errors() {
        let temp = TempDir::new().expect("tempdir should be created");
        let api_path = temp.path().join("api");
        let web_path = temp.path().join("web");
        write_project(&api_path);
        write_project(&web_path);

        let api = service("api", &api_path, "true".to_string());
        let mut web = service("web", &web_path, "true".to_string());
        web.build.as_mut().unwrap().relies_on = Some(vec!["api".to_string()]);

        let mut env = Environment::new("dev");
        env.services = vec![api, web];

        let err = select_services(&env, &[], &["api".to_string()])
            .expect_err("ignoring a dependency should fail");
        assert!(err.contains("depends on ignored service 'api'"));
    }

    #[test]
    fn default_commands_render_service_variables() {
        let temp = TempDir::new().expect("tempdir should be created");
        let service_path = temp.path().join("api");
        write_project(&service_path);

        let mut env = Environment::new("dev");
        env.registry = crate::environment::RegistryConfig::Simple("registry.local".to_string());
        env.platform = Some("linux/amd64".to_string());
        let service = service("api", &service_path, "true".to_string());
        let mut build = service.build.clone().unwrap();
        build.build_command = None;
        build.push_command = None;

        let normalized = normalize_build_config(&env, &service, &build).unwrap();
        let commands = normalized.phases.commands_for_hash().join("\n");
        assert!(commands.contains("registry.local/api:1.2.3"));
        assert!(!commands.contains("{{"));
        assert!(!commands.contains("}}"));
    }

    #[test]
    fn dump_scope_writes_root_and_nested_files() {
        let temp = TempDir::new().expect("tempdir should be created");
        let service_path = temp.path().join("api");
        write_project(&service_path);

        let mut env = Environment::new("dev");
        env.services = vec![service("api", &service_path, "true".to_string())];
        let cache_dir = temp.path().join(".sailr/cache/build");
        let mut opts = options(cache_dir.clone());
        opts.dump_scope = true;

        create_sailr_build_plan(&env, &opts).expect("plan should be created");

        let scope = fs::read_to_string(cache_dir.join("scopes/api.txt"))
            .expect("scope dump should be written");
        assert!(scope.contains("package.json"));
        assert!(scope.contains("src/index.js"));
    }

    #[test]
    fn force_marks_service_dirty() {
        let temp = TempDir::new().expect("tempdir should be created");
        let service_path = temp.path().join("api");
        write_project(&service_path);

        let mut env = Environment::new("dev");
        env.services = vec![service("api", &service_path, "true".to_string())];
        let mut opts = options(temp.path().join(".sailr/cache/build"));
        opts.force = true;

        let plan = create_sailr_build_plan(&env, &opts).expect("plan should be created");
        assert!(plan.services[0].dirty);
        assert!(plan.services[0].dirty_reasons.contains(&DirtyReason::Force));
    }

    #[tokio::test]
    async fn runkernel_dry_run_does_not_execute_commands() {
        let temp = TempDir::new().expect("tempdir should be created");
        let service_path = temp.path().join("api");
        write_project(&service_path);
        let marker = temp.path().join("marker.txt");

        let mut env = Environment::new("dev");
        env.services = vec![service(
            "api",
            &service_path,
            format!("printf hi > {}", marker.display()),
        )];

        let mut builder = Builder::new(
            temp.path()
                .join(".sailr/cache/build")
                .to_string_lossy()
                .to_string(),
            false,
            Vec::new(),
            Vec::new(),
            false,
            true,
            false,
            false,
            None,
            Some(BuildEngine::Runkernel),
        );
        let result = builder.build(&env).await.expect("dry run should succeed");
        assert!(!result.executed);
        assert!(!marker.exists());
    }

    #[tokio::test]
    async fn runkernel_failed_task_returns_error() {
        let temp = TempDir::new().expect("tempdir should be created");
        let service_path = temp.path().join("api");
        write_project(&service_path);

        let mut env = Environment::new("dev");
        env.services = vec![service("api", &service_path, "exit 7".to_string())];

        let mut builder = Builder::new(
            temp.path()
                .join(".sailr/cache/build")
                .to_string_lossy()
                .to_string(),
            false,
            Vec::new(),
            Vec::new(),
            false,
            false,
            false,
            false,
            None,
            Some(BuildEngine::Runkernel),
        );
        let err = builder
            .build(&env)
            .await
            .expect_err("failed task should fail build");
        assert!(err.contains("runkernel build failed"));
    }

    #[tokio::test]
    async fn successful_dirty_service_writes_cache() {
        let temp = TempDir::new().expect("tempdir should be created");
        let service_path = temp.path().join("api");
        write_project(&service_path);
        let cache_dir = temp.path().join(".sailr/cache/build");

        let mut env = Environment::new("dev");
        env.services = vec![service("api", &service_path, "true".to_string())];

        let mut builder = Builder::new(
            cache_dir.to_string_lossy().to_string(),
            false,
            Vec::new(),
            Vec::new(),
            false,
            false,
            false,
            false,
            None,
            Some(BuildEngine::Runkernel),
        );
        builder.build(&env).await.expect("build should succeed");

        let cache = load_service_cache(&cache_dir, "api")
            .expect("cache should load")
            .expect("cache should be written");
        let plan =
            create_sailr_build_plan(&env, &options(cache_dir)).expect("plan should be created");
        assert_eq!(
            cache.fingerprint.full_hash,
            plan.services[0].fingerprint.full_hash
        );
    }

    #[tokio::test]
    async fn failing_dirty_service_does_not_write_cache() {
        let temp = TempDir::new().expect("tempdir should be created");
        let service_path = temp.path().join("api");
        write_project(&service_path);
        let cache_dir = temp.path().join(".sailr/cache/build");

        let mut env = Environment::new("dev");
        env.services = vec![service("api", &service_path, "exit 9".to_string())];

        let mut builder = Builder::new(
            cache_dir.to_string_lossy().to_string(),
            false,
            Vec::new(),
            Vec::new(),
            false,
            false,
            false,
            false,
            None,
            Some(BuildEngine::Runkernel),
        );
        assert!(builder.build(&env).await.is_err());
        assert!(load_service_cache(&cache_dir, "api")
            .expect("cache lookup should succeed")
            .is_none());
    }

    #[test]
    fn skipped_dirty_service_does_not_write_cache() {
        let temp = TempDir::new().expect("tempdir should be created");
        let api_path = temp.path().join("api");
        let web_path = temp.path().join("web");
        write_project(&api_path);
        write_project(&web_path);
        let cache_dir = temp.path().join(".sailr/cache/build");

        let mut api = service("api", &api_path, "true".to_string());
        let mut web = service("web", &web_path, "true".to_string());
        web.build.as_mut().unwrap().relies_on = Some(vec!["api".to_string()]);
        api.version = "1.0.0".to_string();

        let mut env = Environment::new("dev");
        env.services = vec![api, web];
        let plan =
            create_sailr_build_plan(&env, &options(cache_dir.clone())).expect("plan should build");

        write_successful_service_caches(
            &plan,
            &pipeline_result(vec![
                (
                    crate::workflow::task_id::service_build("api").as_str(),
                    TaskStatus::Completed,
                ),
                (
                    crate::workflow::task_id::service_build("web").as_str(),
                    TaskStatus::Skipped,
                ),
            ]),
        )
        .expect("cache write should succeed");

        assert!(load_service_cache(&cache_dir, "api")
            .expect("api cache lookup should succeed")
            .is_some());
        assert!(load_service_cache(&cache_dir, "web")
            .expect("web cache lookup should succeed")
            .is_none());
    }

    #[tokio::test]
    async fn clean_service_does_not_rewrite_cache() {
        let temp = TempDir::new().expect("tempdir should be created");
        let service_path = temp.path().join("api");
        write_project(&service_path);
        let cache_dir = temp.path().join(".sailr/cache/build");

        let mut env = Environment::new("dev");
        env.services = vec![service("api", &service_path, "true".to_string())];

        let mut first = Builder::new(
            cache_dir.to_string_lossy().to_string(),
            false,
            Vec::new(),
            Vec::new(),
            false,
            false,
            false,
            false,
            None,
            Some(BuildEngine::Runkernel),
        );
        first.build(&env).await.expect("first build should succeed");
        let cache_path = service_cache_path(&cache_dir, "api");
        let before = fs::read_to_string(&cache_path).expect("cache should exist");

        let mut second = Builder::new(
            cache_dir.to_string_lossy().to_string(),
            false,
            Vec::new(),
            Vec::new(),
            false,
            false,
            false,
            false,
            None,
            Some(BuildEngine::Runkernel),
        );
        second
            .build(&env)
            .await
            .expect("second build should succeed");
        let after = fs::read_to_string(&cache_path).expect("cache should still exist");

        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn before_all_and_after_all_wrap_dirty_services() {
        let temp = TempDir::new().expect("tempdir should be created");
        let service_path = temp.path().join("api");
        write_project(&service_path);
        let log = temp.path().join("order.log");

        let mut env = Environment::new("dev");
        env.services = vec![service(
            "api",
            &service_path,
            format!("printf service >> {}", log.display()),
        )];
        env.build = Some(BuildPolicy {
            engine: Some(BuildEngine::Runkernel),
            before_all: Some(CommandSpec::Single(format!(
                "printf -- before- > {}",
                log.display()
            ))),
            after_all: Some(CommandSpec::Single(format!(
                "printf -- -after >> {}",
                log.display()
            ))),
            ..BuildPolicy::default()
        });

        let mut builder = Builder::new(
            temp.path()
                .join(".sailr/cache/build")
                .to_string_lossy()
                .to_string(),
            false,
            Vec::new(),
            Vec::new(),
            false,
            false,
            false,
            false,
            env.build.clone(),
            None,
        );
        builder.build(&env).await.expect("build should succeed");

        let contents = fs::read_to_string(log).expect("log should exist");
        assert_eq!(contents, "before-service-after");
    }
}
