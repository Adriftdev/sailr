use std::{fs::File, io, io::Write, path::Path, sync::Mutex};

use anyhow::Result;

use chrono::Utc;
use toml::{from_str, to_string};

use crate::environment::{Environment, EnvironmentVariable, Service};
use crate::errors::SailrError;

pub const ENV_DIR: &str = "./k8s/environments/";

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
    let mut env = Environment::new(env_name);

    if redis {
        let redis = Service::new(
            "redis",
            "default",
            None,
            None,
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
            None,
            None,
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
            None,
            None,
            None,
            None,
            None,
            Some("latest".to_string()),
        );
        env.add_service(registry);
    }

    env.save_to_file().unwrap();
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

pub fn replace_variables(content: String, variables: Vec<(String, String)>) -> String {
    let mut new_content = content.clone();
    for (key, value) in variables {
        new_content = new_content.replace(&format!("{{{{{}}}}}", key), &value);
    }
    new_content
}
