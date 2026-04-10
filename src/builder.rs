use crate::{
    environment::{Environment, Service},
    roomservice::{
        room::{Hooks, RoomBuilder},
        util::Failable,
    },
};

use crate::roomservice::RoomserviceBuilder;

const DEFAULT_AFTER_TEMPLATE: &str = "docker push {{ registry }}/{{ name }}:{{ version }}";

pub struct Builder {
    roomservice: RoomserviceBuilder,
    only: Vec<String>,
    ignore: Vec<String>,
}

impl Builder {
    pub fn new(cache_dir: String, force: bool, only: Vec<String>, ignore: Vec<String>) -> Builder {
        Builder {
            roomservice: RoomserviceBuilder::new("./".to_string(), cache_dir, force),
            only,
            ignore,
        }
    }

    pub fn build(&mut self, env: &Environment) -> Result<(), String> {
        let canonical_project_path = std::path::Path::new(&"./").canonicalize().unwrap();
        let path_buf = canonical_project_path.join(".roomservice");
        let cache_dir = path_buf.to_str().unwrap().to_owned().to_string();

        let buildable_services = env
            .services
            .iter()
            .filter(|service| service.build.is_some())
            .collect::<Vec<&Service>>();

        if buildable_services.is_empty() {
            return Err("No buildable services found in environment config.".to_string());
        }

        for service in buildable_services {
            let mut should_add = true;

            if !self.only.is_empty() {
                should_add = self.only.contains(&service.name);
            }

            if !self.ignore.is_empty() {
                should_add = !self.ignore.contains(&service.name);
            }

            if !should_add {
                continue;
            }

            let build_cfg = service
                .build
                .as_ref()
                .unwrap_fail("Service marked as buildable but missing build config.");

            let before_template = build_cfg
                .before
                .clone()
                .map(|commands| commands.into_vec())
                .unwrap_or_else(|| vec![default_before_template(env)]);
            let after_template = build_cfg
                .after
                .clone()
                .map(|commands| commands.into_vec())
                .unwrap_or_else(|| vec![DEFAULT_AFTER_TEMPLATE.to_string()]);

            self.roomservice.add_room(RoomBuilder::new(
                service.name.clone(),
                build_cfg.path.clone(),
                cache_dir.clone(),
                "./**/*.*".to_string(),
                build_cfg.relies_on.clone().unwrap_or_default(),
                Hooks {
                    before: inject_commands(before_template, service, env),
                    before_synchronously: None,
                    run_synchronously: None,
                    run_parallel: None,
                    after: inject_commands(after_template, service, env),
                    finally: None,
                },
            ));
        }

        self.roomservice.exec(false, false, false);
        Ok(())
    }
}

pub fn split_matches(val: Option<String>) -> Vec<String> {
    match val {
        Some(ignore_values) => ignore_values.split(',').map(|t| t.to_string()).collect(),
        None => vec![],
    }
}

fn replace_template_var(input: &str, key: &str, value: &str) -> String {
    input
        .replace(&format!("{{{{ {} }}}}", key), value)
        .replace(&format!("{{{{{}}}}}", key), value)
}

fn default_before_template(env: &Environment) -> String {
    match env.platform.as_deref() {
        Some(platform) if !platform.trim().is_empty() => format!(
            "docker buildx build --ssh default --platform {} -t {{{{ registry }}}}/{{{{ name }}}}:{{{{ version }}}} .",
            platform
        ),
        _ => {
            "docker buildx build --ssh default -t {{ registry }}/{{ name }}:{{ version }} ."
                .to_string()
        }
    }
}

fn inject_command(
    script: &str,
    service: &Service,
    env: &crate::environment::Environment,
) -> String {
    let namespace = service.namespace_or(&env.name);
    let s = replace_template_var(script, "registry", &env.registry);
    let s = replace_template_var(&s, "platform", env.platform.as_deref().unwrap_or(""));
    let s = replace_template_var(&s, "name", &service.name);
    let s = replace_template_var(&s, "version", &service.version);
    replace_template_var(&s, "namespace", namespace)
}

fn inject_commands(
    commands: Vec<String>,
    service: &Service,
    env: &crate::environment::Environment,
) -> Option<String> {
    if commands.is_empty() {
        return None;
    }

    let rendered = commands
        .iter()
        .map(|command| inject_command(command, service, env))
        .collect::<Vec<String>>();

    Some(rendered.join(" && "))
}
