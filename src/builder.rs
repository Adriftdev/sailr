use crate::{
    environment::{BuildPolicy, CommandSpec, Environment, Service, ServiceBuildConfig},
    roomservice::{
        room::{Hooks, RoomBuilder},
        BuildPlan, GlobalPolicy, RoomserviceBuilder,
    },
};

const DEFAULT_PUSH_TEMPLATE: &str = "docker push {{ registry }}/{{ name }}:{{ version }}";

pub struct Builder {
    roomservice: RoomserviceBuilder,
    only: Vec<String>,
    ignore: Vec<String>,
    plan: bool,
    dry_run: bool,
    explain: bool,
    dump_scope: bool,
}

#[derive(Debug, Clone)]
pub struct BuildRunResult {
    pub executed: bool,
    pub success: bool,
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
    ) -> Builder {
        Builder {
            roomservice: RoomserviceBuilder::new(
                "./".to_string(),
                cache_dir,
                force,
                map_policy(policy),
            ),
            only,
            ignore,
            plan,
            dry_run,
            explain,
            dump_scope,
        }
    }

    pub fn build(&mut self, env: &Environment) -> Result<BuildRunResult, String> {
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

        for service in buildable_services {
            if !self.only.is_empty() && !self.only.contains(&service.name) {
                continue;
            }

            if !self.ignore.is_empty() && self.ignore.contains(&service.name) {
                continue;
            }

            let room = build_room(env, service, &buildable_names);
            self.roomservice.add_room(room)?;
        }

        let plan = self.roomservice.plan(self.dump_scope)?;
        self.print_plan(&plan);

        if self.plan {
            return Ok(BuildRunResult {
                executed: false,
                success: true,
            });
        }

        let result = self.roomservice.execute(&plan, self.dry_run)?;
        if result.executed && !result.success {
            return Err("Roomservice 2.0 build execution failed".to_string());
        }

        Ok(BuildRunResult {
            executed: result.executed,
            success: result.success,
        })
    }

    fn print_plan(&self, plan: &BuildPlan) {
        if self.plan || self.dry_run || self.explain {
            plan.print(self.explain, self.plan || self.dry_run);
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
}

pub fn split_matches(val: Option<String>) -> Vec<String> {
    match val {
        Some(ignore_values) => ignore_values.split(',').map(|t| t.to_string()).collect(),
        None => vec![],
    }
}

fn build_room(env: &Environment, service: &Service, buildable_names: &[String]) -> RoomBuilder {
    let build_cfg = service
        .build
        .as_ref()
        .expect("buildable service should include build config");

    let (dependency_rooms, dependency_paths) = split_dependencies(
        build_cfg.relies_on.clone().unwrap_or_default(),
        buildable_names,
    );

    let normalized = normalize_build_config(env, build_cfg);

    RoomBuilder::new(
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
            before_synchronously: normalized.before_synchronously,
            before: normalized.before,
            run_parallel: normalized.run_parallel,
            run_synchronously: normalized.run_synchronously,
            after: normalized.after,
            finally: normalized.finally,
        },
        normalized.build_command,
        normalized.push_command,
        Some(format!(
            "{}/{}:{}",
            env.registry, service.name, service.version
        )),
    )
}

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
