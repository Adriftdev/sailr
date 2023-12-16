use std::println;
use std::{fs::File, io, io::Write, path::Path, sync::Mutex};

use anyhow::Result;

use chrono::Utc;
use scribe_rust::{log, Color};
use serde_yaml;
use toml::{from_str, to_string};

use crate::environment::{Environment, EnvironmentVariable, Service};
use crate::errors::SailrError;

const ENV_DIR: &str = "./k8s/environments/";

const TEMPLATE_DIR: &str = "./k8s/templates/";

const GENERATED_DIR: &str = "./k8s/generated/";

pub fn ensure_dir(dir_name: &str) -> Result<(), SailrError> {
    if !std::path::Path::new(dir_name).exists() {
        std::fs::create_dir_all(dir_name)?;
    }
    Ok(())
}

pub fn rm_dir(dir_name: &str) -> Result<(), SailrError> {
    if std::path::Path::new(dir_name).exists() {
        std::fs::remove_dir_all(dir_name)?;
    }
    Ok(())
}

pub fn list_envs() -> Result<(), SailrError> {
    let mut envs = Vec::new();
    for entry in std::fs::read_dir(ENV_DIR)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let env = std::fs::read_to_string(path)?;
            let toml: Environment = from_str(&env)?;
            envs.push(toml.name);
        }
    }
    println!("{:?}", envs);
    Ok(())
}

pub fn copy_templates() -> Result<(), SailrError> {
    let mut templates = Vec::new();

    templates.push((
        "redis/deployment.yaml",
        include_str!("k8s/templates/redis/deployment.yaml"),
    ));
    templates.push((
        "redis/service.yaml",
        include_str!("k8s/templates/redis/service.yaml"),
    ));
    templates.push((
        "postgres/deployment.yaml",
        include_str!("k8s/templates/postgres/deployment.yaml"),
    ));
    templates.push((
        "postgres/service.yaml",
        include_str!("k8s/templates/postgres/service.yaml"),
    ));
    templates.push((
        "postgres/pvc.yaml",
        include_str!("k8s/templates/postgres/pvc.yaml"),
    ));
    templates.push((
        "registry/deployment.yaml",
        include_str!("k8s/templates/registry/deployment.yaml"),
    ));
    templates.push((
        "registry/service.yaml",
        include_str!("k8s/templates/registry/service.yaml"),
    ));
    templates.push((
        "registry/pvc.yaml",
        include_str!("k8s/templates/registry/pvc.yaml"),
    ));

    for (name, template) in templates {
        let path = Path::new(TEMPLATE_DIR).join(name);
        ensure_dir(path.parent().unwrap().to_str().unwrap())?;
        let mut file = File::create(path)?;
        file.write_all(template.as_bytes())?;
    }

    log(
        Color::Green,
        "Success",
        "Copied Sailr base templates to ./k8s/templates",
    );

    Ok(())
}

pub fn list_services(env_name: &str) -> Result<(), SailrError> {
    let env = get_env_toml(env_name)?;
    println!("{:?}", env.service_whitelist);
    Ok(())
}

pub fn delete_env(env_name: &str) -> Result<(), SailrError> {
    std::fs::remove_file(Path::new(ENV_DIR).join(format!(
        "{}.toml",
        env_name.to_lowercase().replace(' ', "-")
    )))?;
    Ok(())
}

pub fn create_env_toml(
    env_name: &str,
    redis: bool,
    postgres: bool,
    registry: bool,
) -> Result<(), SailrError> {
    ensure_dir(ENV_DIR)?;

    let mut env = Environment::new(env_name);

    if redis {
        let redis = Service::new(
            "redis",
            "default",
            Some("redis"),
            None,
            None,
            None,
            Some("latest".to_string()),
        );
        env.add_service(redis);

        let redis_host =
            EnvironmentVariable::new("REDIS_HOST", Some(toml::Value::String("redis".to_string())));

        let redis_port =
            EnvironmentVariable::new("REDIS_PORT", Some(toml::Value::String("6379".to_string())));

        env.add_environment_variable(redis_host);
        env.add_environment_variable(redis_port);
    }

    if postgres {
        let postgres = Service::new(
            "postgres",
            "default",
            Some("postgres"),
            None,
            None,
            None,
            Some("latest".to_string()),
        );
        env.add_service(postgres);

        let db_host =
            EnvironmentVariable::new("DB_HOST", Some(toml::Value::String("postgres".to_string())));

        let db_port =
            EnvironmentVariable::new("DB_PORT", Some(toml::Value::String("5432".to_string())));

        let db_user =
            EnvironmentVariable::new("DB_USER", Some(toml::Value::String("postgres".to_string())));

        env.add_environment_variable(db_host);
        env.add_environment_variable(db_port);
        env.add_environment_variable(db_user);
    }

    if registry {
        let registry = Service::new(
            "registry",
            "kube-system",
            Some("registry"),
            None,
            None,
            None,
            Some("latest".to_string()),
        );
        env.add_service(registry);
    }

    let toml_value = to_string(&env)?;

    let file = File::create(Path::new(ENV_DIR).join(format!(
        "{}.toml",
        env_name.to_lowercase().replace(' ', "-")
    )))?;

    let toml = Mutex::new(io::BufWriter::new(file));
    {
        let mut guard = toml.lock().unwrap();
        guard.write_all(toml_value.as_bytes())?;
    }
    Ok(())
}

pub fn get_env_toml_file(env_name: &str) -> Result<(), SailrError> {
    let env_name = env_name.to_lowercase().trim().replace(" ", "-");
    let env = std::fs::read_to_string(&format!("{}{}.toml", ENV_DIR, env_name))?;
    println!("{}", env);
    let toml = from_str(&env)?;
    println!("{:?}", toml);
    Ok(())
}

pub fn get_env_toml(env_name: &str) -> Result<Environment, SailrError> {
    let env_name = env_name.to_lowercase().trim().replace(" ", "-");
    let env = std::fs::read_to_string(&format!("{}{}.toml", ENV_DIR, env_name))?;
    let toml: Environment = from_str(&env)?;
    Ok(toml)
}

pub fn append_service_toml(env_name: &str, service: Service) -> Result<(), SailrError> {
    let mut env = get_env_toml(env_name)?;
    env.add_service(service);
    let toml_value = to_string(&env)?;
    let file = File::create(Path::new(ENV_DIR).join(format!(
        "{}.toml",
        env_name.to_lowercase().replace(' ', "-")
    )))?;
    let toml = Mutex::new(io::BufWriter::new(file));
    {
        let mut guard = toml.lock().unwrap();
        guard.write_all(toml_value.as_bytes())?;
    }
    Ok(())
}

pub fn remove_service_toml(env_name: &str, service_name: &str) -> Result<(), SailrError> {
    let mut env = get_env_toml(env_name)?;
    env.remove_service(service_name);
    let toml_value = to_string(&env)?;
    let file = File::create(Path::new(ENV_DIR).join(format!(
        "{}.toml",
        env_name.to_lowercase().replace(' ', "-")
    )))?;
    let toml = Mutex::new(io::BufWriter::new(file));
    {
        let mut guard = toml.lock().unwrap();
        guard.write_all(toml_value.as_bytes())?;
    }
    Ok(())
}

pub fn get_current_timestamp() -> String {
    let now = Utc::now();
    now.format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn replace_vars(content: &str, env: &Environment, service: &Service) -> String {
    let mut generated = content.to_string();

    generated = generated.replace(
        &format!("{{{{{}}}}}", "deployment_date"),
        &get_current_timestamp(),
    );

    generated = generated.replace(
        &format!("{{{{{}}}}}", "name"),
        &env.name.to_lowercase().replace(' ', "-"),
    );

    generated = generated.replace(
        &format!("{{{{{}}}}}", "log_level"),
        &env.log_level.to_uppercase(),
    );

    generated = generated.replace(
        &format!("{{{{{}}}}}", "domain"),
        &env.domain.clone().to_lowercase(),
    );

    generated = generated.replace(
        &format!("{{{{{}}}}}", "default_replicas"),
        &env.default_replicas.to_string(),
    );

    generated = generated.replace(
        &format!("{{{{{}}}}}", "registry"),
        &env.registry.clone().to_lowercase(),
    );

    generated = generated.replace(
        &format!("{{{{{}}}}}", "service_name"),
        &service.name.to_lowercase().replace(' ', "-"),
    );

    generated = generated.replace(
        &format!("{{{{{}}}}}", "service_version"),
        &service.get_version(),
    );

    generated = generated.replace(
        &format!("{{{{{}}}}}", "service_namespace"),
        &service.namespace.to_lowercase().replace(' ', "-"),
    );

    env.environment_variables.iter().for_each(|env_var| {
        for ele in env_var {
            if generated.contains(&ele.name) {
                generated = generated.replace(
                    &format!("{{{{{}}}}}", ele.name),
                    match &ele.value.clone() {
                        Some(value) => toml::Value::try_from(value)
                            .unwrap()
                            .as_str()
                            .unwrap()
                            .to_string(),
                        None => "!!ERROR!!".to_string(),
                    }
                    .as_str(),
                );
            }
        }
    });

    generated
}

pub fn validate_yaml(input: &str) -> Result<(), SailrError> {
    serde_yaml::from_str::<serde_yaml::Value>(input)?;
    Ok(())
}

pub fn inject_env_values_templates(env_name: &str) -> Result<(), SailrError> {
    rm_dir(GENERATED_DIR)?;
    ensure_dir(TEMPLATE_DIR)?;
    ensure_dir(GENERATED_DIR)?;
    let env = get_env_toml(env_name)?;

    for service in &env.list_services() {
        if service.path.is_some() && service.path.as_ref().unwrap() != "" {
            let path = Path::new(TEMPLATE_DIR).join(service.path.clone().unwrap());
            for entry in std::fs::read_dir(path)? {
                let file_name = &entry.as_ref().unwrap().file_name();
                let path = entry.as_ref().unwrap().path();
                let content = std::fs::read_to_string(&path);
                let generated = replace_vars(&content?, &env, &service);
                validate_yaml(&generated)?;
                let generate_path = Path::new(GENERATED_DIR)
                    .join(env_name)
                    .join(match service.path.clone() {
                        Some(path) => path,
                        None => "".to_string(),
                    })
                    .join(file_name);
                ensure_dir(
                    match generate_path.parent() {
                        Some(parent) => parent.display().to_string(),
                        None => "".to_string(),
                    }
                    .as_str(),
                )?;
                std::fs::write(&generate_path, generated)?;
                log(
                    Color::Green,
                    "Generated",
                    &format!("{}", &generate_path.display()),
                );
            }
        } else {
            let path = Path::new(TEMPLATE_DIR)
                .join(&service.namespace)
                .join(&service.name);
            for entry in std::fs::read_dir(path)? {
                let file_name = &entry.as_ref().unwrap().file_name();
                let path = entry.as_ref().unwrap().path();

                let content = std::fs::read_to_string(&path);
                let generated = replace_vars(&content?, &env, &service);
                validate_yaml(&generated)?;
                let generate_path = Path::new(GENERATED_DIR)
                    .join(env_name)
                    .join(&service.namespace)
                    .join(&service.name)
                    .join(file_name);
                ensure_dir(
                    match generate_path.parent() {
                        Some(parent) => parent.display().to_string(),
                        None => "".to_string(),
                    }
                    .as_str(),
                )?;
                std::fs::write(&generate_path, generated)?;
                log(
                    Color::Green,
                    "Generated",
                    &format!("{}", &generate_path.display()),
                );
            }
        }
    }

    Ok(())
}
