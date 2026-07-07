use crate::environment::{BuildPolicy, CommandSpec, Environment, Service, ServiceBuildConfig};
use runkernel::cache::{CacheEligibility, CacheManager};
use runkernel::{Pipeline, PipelineEvent, Task};

const DEFAULT_PUSH_TEMPLATE: &str = "docker push {{ registry }}/{{ name }}:{{ version }}";

pub struct Builder {
    only: Vec<String>,
    ignore: Vec<String>,
    plan: bool,
    dry_run: bool,
    explain: bool,
}

#[derive(Debug, Clone)]
pub struct BuildRunResult {
    pub executed: bool,
    pub success: bool,
}

impl Builder {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        _cache_dir: String,
        _force: bool,
        only: Vec<String>,
        ignore: Vec<String>,
        plan: bool,
        dry_run: bool,
        explain: bool,
        _dump_scope: bool,
        _policy: Option<BuildPolicy>,
    ) -> Builder {
        Builder {
            only,
            ignore,
            plan,
            dry_run,
            explain,
        }
    }

    pub async fn build(&mut self, env: &Environment) -> Result<BuildRunResult, String> {
        let buildable_services = env
            .services
            .iter()
            .filter(|service| service.build.is_some())
            .collect::<Vec<&Service>>();

        if buildable_services.is_empty() {
            return Err("No buildable services found in environment config.".to_string());
        }

        let buildable_names = buildable_services
            .iter()
            .map(|service| service.name.clone())
            .collect::<Vec<_>>();

        let mut pipeline = Pipeline::new("Sailr Service Build Pipeline");
        let mut tasks = std::collections::HashMap::new();

        for &service in &buildable_services {
            if !self.only.is_empty() && !self.only.contains(&service.name) {
                continue;
            }

            if !self.ignore.is_empty() && self.ignore.contains(&service.name) {
                continue;
            }

            let build_cfg = service
                .build
                .as_ref()
                .expect("buildable service should include build config");

            let (dependency_rooms, dependency_paths) = split_dependencies(
                build_cfg.relies_on.clone().unwrap_or_default(),
                &buildable_names,
            );

            // Construct task dependencies
            let depends_on_list: Vec<String> = dependency_rooms.clone();

            // Build task inputs (include patterns)
            let base_inputs = build_cfg
                .include
                .clone()
                .unwrap_or_else(|| vec!["./**/*.*".to_string()]);
            let mut inputs = prepend_path_to_globs(&build_cfg.path, &base_inputs);

            // Add path dependencies if any
            for path_dep in &dependency_paths {
                let clean_path = path_dep.trim_start_matches("./").trim_start_matches('/');
                inputs.push(format!("{}/**/*.*", clean_path));
            }

            // For transitive dirty caching: B must depend on cache record files of its dependency tasks!
            for dep in &dependency_rooms {
                inputs.push(format!(".runkernel/cache/{}.json", dep));
            }

            // Environment variables task depends on
            let env_vars = vec!["TARGET_ENV".to_string()];

            let normalized = normalize_build_config(env, build_cfg);
            let service_name = service.name.clone();
            let build_path = build_cfg.path.clone();
            let dry_run = self.dry_run;

            let task = Task::new(service.name.clone())
                .depends_on(
                    &depends_on_list
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>(),
                )
                .inputs(&inputs.iter().map(|s| s.as_str()).collect::<Vec<_>>())
                .env_vars(&env_vars.iter().map(|s| s.as_str()).collect::<Vec<_>>())
                .exec_fn(move |_ctx| {
                    let service_name = service_name.clone();
                    let build_path = build_path.clone();
                    let normalized = normalized.clone();
                    async move {
                        execute_service_build(service_name, build_path, normalized, dry_run).await
                    }
                });

            tasks.insert(service.name.clone(), task);
        }

        // Support --plan, --dry-run, --explain
        if self.plan || self.dry_run || self.explain {
            self.print_pipeline_plan(&tasks)?;
            if self.plan {
                return Ok(BuildRunResult {
                    executed: false,
                    success: true,
                });
            }
        }

        // Add all tasks to pipeline
        for (_, task) in tasks {
            pipeline.add(task);
        }

        pipeline.on_event(|event| {
            use crate::LOGGER;
            match event {
                PipelineEvent::PipelineStarted { name, task_count } => {
                    LOGGER.info(&format!(
                        "Pipeline '{}' started with {} tasks.",
                        name, task_count
                    ));
                }
                PipelineEvent::TaskStarted { name } => {
                    LOGGER.info(&format!("Task '{}' started.", name));
                }
                PipelineEvent::TaskCompleted { name, duration } => {
                    LOGGER.info(&format!("Task '{}' completed in {:.2?}.", name, duration));
                }
                PipelineEvent::TaskQueued { name } => {
                    LOGGER.info(&format!("Task '{}' queued.", name));
                }
                PipelineEvent::TaskFailed { name, error } => {
                    LOGGER.error(&format!("Task '{}' failed: {}", name, error));
                }
                PipelineEvent::TaskCached { name, reason } => {
                    LOGGER.info(&format!("Task '{}' cached: {}", name, reason));
                }
                PipelineEvent::TaskCancelled { name } => {
                    LOGGER.warn(&format!("Task '{}' cancelled.", name));
                }
                PipelineEvent::TaskSkipped { name, reason } => {
                    LOGGER.info(&format!("Task '{}' skipped: {}", name, reason));
                }
                PipelineEvent::TaskRollbackFailed { name, error } => {
                    LOGGER.error(&format!("Task '{}' rollback failed: {}", name, error));
                }
                PipelineEvent::PipelineFinished { result } => {
                    LOGGER.info(&format!(
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
            }
        });

        // Run the pipeline
        if let Err(e) = pipeline.run().await {
            return Err(format!(
                "runkernel build pipeline execution failed: {:?}",
                e
            ));
        }

        Ok(BuildRunResult {
            executed: true,
            success: true,
        })
    }

    fn print_pipeline_plan(
        &self,
        tasks: &std::collections::HashMap<String, Task>,
    ) -> Result<(), String> {
        let cache_manager = CacheManager::new();
        println!("runkernel build plan:");

        let mut dirty_rooms = Vec::new();
        for (name, task) in tasks {
            let mut dirty = true;
            let mut explain_reason = "cache miss";

            if task.is_cacheable() {
                if let Ok(hash) = cache_manager.compute_hash(name, task) {
                    if let CacheEligibility::Enabled { hash: h, reason: _ } = hash {
                        if cache_manager.is_cache_valid(name, &h) {
                            dirty = false;
                        } else {
                            explain_reason = "source or environment changed";
                        }
                    }
                }
            } else {
                explain_reason = "not cacheable";
            }

            let status = if dirty { "dirty" } else { "clean" };
            println!(" - {} [{}]", name, status);

            if dirty {
                dirty_rooms.push(name.clone());
            }

            if self.explain && dirty {
                println!("   reason: {}", explain_reason);
            }
        }

        if !self.plan && !self.dry_run {
            if dirty_rooms.is_empty() {
                println!("All rooms appear to be up to date!");
            } else {
                println!("The following rooms have changed:");
                for room in dirty_rooms {
                    println!("==> {}", room);
                }
            }
        }

        Ok(())
    }
}

async fn execute_service_build(
    service_name: String,
    build_path: String,
    normalized: NormalizedBuildConfig,
    dry_run: bool,
) -> anyhow::Result<()> {
    use crate::LOGGER;
    if dry_run {
        LOGGER.info(&format!(
            "Dry run: Would build and push service '{}'",
            service_name
        ));
        return Ok(());
    }

    let mut started = false;

    // Run hook phase execution
    let phases = vec![
        ("before_synchronous", normalized.before_synchronously),
        ("before", normalized.before),
        ("run_parallel", normalized.run_parallel),
        ("run_synchronously", normalized.run_synchronously),
        ("build", normalized.build_command.into_iter().collect()),
        ("push", normalized.push_command.into_iter().collect()),
        ("after", normalized.after),
    ];

    let mut run_success = true;
    let mut err_msg = None;

    for (phase_name, commands) in phases {
        if commands.is_empty() {
            continue;
        }
        started = true;
        if LOGGER.is_verbose() {
            LOGGER.info(&format!(
                "Executing phase: {} -> {}",
                service_name, phase_name
            ));
        }
        for command in commands {
            if let Err(e) = exec_cmd(&build_path, &command, &service_name).await {
                run_success = false;
                err_msg = Some(e);
                break;
            }
        }
        if !run_success {
            break;
        }
    }

    // Always run finally if execution started
    if started {
        if !normalized.finally.is_empty() {
            if LOGGER.is_verbose() {
                LOGGER.info(&format!("Executing finalizer: {} -> finally", service_name));
            }
            for command in normalized.finally {
                if let Err(e) = exec_cmd(&build_path, &command, &service_name).await {
                    LOGGER.warn(&format!(
                        "finalizer command failed for service {}: {}",
                        service_name, e
                    ));
                }
            }
        }
    }

    if !run_success {
        if let Some(err) = err_msg {
            anyhow::bail!("Service build failed: {}", err);
        } else {
            anyhow::bail!("Service build failed");
        }
    }

    Ok(())
}

async fn exec_cmd(cwd: &str, cmd: &str, name: &str) -> Result<(), String> {
    use crate::LOGGER;
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
    } else if LOGGER.is_verbose() {
        if !stdout.trim().is_empty() {
            LOGGER.info(&format!("{} (stdout):\n{}", name, stdout));
        }
        if !stderr.trim().is_empty() {
            LOGGER.info(&format!("{} (stderr):\n{}", name, stderr));
        }
    }

    Ok(())
}

pub fn split_matches(val: Option<String>) -> Vec<String> {
    match val {
        Some(ignore_values) => ignore_values.split(',').map(|t| t.to_string()).collect(),
        None => vec![],
    }
}

fn split_dependencies(
    dependencies: Vec<String>,
    buildable_names: &[String],
) -> (Vec<String>, Vec<String>) {
    let buildable_names = buildable_names
        .iter()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    let mut room_dependencies = Vec::new();
    let mut path_dependencies = Vec::new();

    for dependency in dependencies {
        if buildable_names.contains(&dependency) {
            room_dependencies.push(dependency);
        } else {
            path_dependencies.push(dependency);
        }
    }

    (room_dependencies, path_dependencies)
}

fn prepend_path_to_globs(path: &str, patterns: &[String]) -> Vec<String> {
    patterns
        .iter()
        .map(|pattern| {
            let clean_pat = pattern.trim_start_matches("./").trim_start_matches('/');
            let clean_path = path.trim_start_matches("./").trim_start_matches('/');
            if clean_path.is_empty() {
                clean_pat.to_string()
            } else if clean_path.ends_with('/') {
                format!("{}{}", clean_path, clean_pat)
            } else {
                format!("{}/{}", clean_path, clean_pat)
            }
        })
        .collect()
}

#[derive(Clone)]
struct NormalizedBuildConfig {
    before_synchronously: Vec<String>,
    before: Vec<String>,
    run_parallel: Vec<String>,
    run_synchronously: Vec<String>,
    after: Vec<String>,
    finally: Vec<String>,
    build_command: Option<String>,
    push_command: Option<String>,
}

fn normalize_build_config(
    env: &Environment,
    build_cfg: &ServiceBuildConfig,
) -> NormalizedBuildConfig {
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
        render_commands(build_cfg.before.clone(), env)
    };
    let after = if legacy_semantics && !explicit_new_phase_fields {
        Vec::new()
    } else {
        render_commands(build_cfg.after.clone(), env)
    };

    let build_command = build_cfg
        .build_command
        .clone()
        .map(|command| inject_global_vars(&command, env))
        .or_else(|| {
            if legacy_semantics && !explicit_new_phase_fields {
                build_cfg
                    .before
                    .clone()
                    .map(command_spec_to_shell)
                    .map(|command| inject_global_vars(&command, env))
            } else {
                None
            }
        })
        .or_else(|| Some(default_build_command(env, build_cfg)));

    let push_command = build_cfg
        .push_command
        .clone()
        .map(|command| inject_global_vars(&command, env))
        .or_else(|| {
            if legacy_semantics && !explicit_new_phase_fields {
                build_cfg
                    .after
                    .clone()
                    .map(command_spec_to_shell)
                    .map(|command| inject_global_vars(&command, env))
            } else {
                None
            }
        })
        .or_else(|| Some(default_push_command(env)));

    NormalizedBuildConfig {
        before_synchronously: render_commands(build_cfg.before_synchronous.clone(), env),
        before,
        run_parallel: render_commands(build_cfg.run_parallel.clone(), env),
        run_synchronously: render_commands(build_cfg.run_synchronous.clone(), env),
        after,
        finally: render_commands(build_cfg.finally.clone(), env),
        build_command,
        push_command,
    }
}

fn render_commands(commands: Option<CommandSpec>, env: &Environment) -> Vec<String> {
    commands
        .map(CommandSpec::into_vec)
        .unwrap_or_default()
        .into_iter()
        .map(|command| inject_global_vars(&command, env))
        .collect()
}

fn inject_global_vars(command: &str, env: &Environment) -> String {
    replace_template_var(
        &replace_template_var(
            &replace_template_var(command, "registry", &env.registry),
            "platform",
            env.platform.as_deref().unwrap_or(""),
        ),
        "environment",
        &env.name,
    )
}

fn default_build_command(env: &Environment, build_cfg: &ServiceBuildConfig) -> String {
    let dockerfile_segment = build_cfg
        .dockerfile
        .as_ref()
        .map(|dockerfile| format!(" -f {}", dockerfile))
        .unwrap_or_default();

    match env.platform.as_deref() {
        Some(platform) if !platform.trim().is_empty() => format!(
            "docker buildx build --ssh default --platform {}{} -t {{{{ registry }}}}/{{{{ name }}}}:{{{{ version }}}} .",
            platform, dockerfile_segment
        ),
        _ => format!(
            "docker buildx build --ssh default{} -t {{ registry }}/{{ name }}:{{ version }} .",
            dockerfile_segment
        ),
    }
}

fn default_push_command(_env: &Environment) -> String {
    DEFAULT_PUSH_TEMPLATE.to_string()
}

fn replace_template_var(input: &str, key: &str, value: &str) -> String {
    input
        .replace(&format!("{{{{ {} }}}}", key), value)
        .replace(&format!("{{{{{}}}}}", key), value)
}

fn command_spec_to_shell(command: CommandSpec) -> String {
    command.into_vec().join(" && ")
}
