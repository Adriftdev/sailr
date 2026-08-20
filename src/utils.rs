use std::collections::BTreeMap;
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
        let redis = Service::new("redis", Some("default"), "latest");
        env.add_service(redis);

        let redis_host =
            EnvironmentVariable::new("REDIS_HOST", Some(toml::Value::String("redis".to_string())));

        let redis_port =
            EnvironmentVariable::new("REDIS_PORT", Some(toml::Value::String("6379".to_string())));

        env.add_environment_variable(redis_host);
        env.add_environment_variable(redis_port);
    }

    if postgres {
        let postgres = Service::new("postgres", Some("default"), "latest");
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
        let registry = Service::new("registry", Some("kube-system"), "latest");
        env.add_service(registry);
    }

    env.save_to_file().unwrap();
    Ok(())
}

pub fn get_env_toml(env_name: &str) -> Result<Environment, SailrError> {
    let env_name = env_name.to_lowercase().trim().replace(" ", "-");
    let env = std::fs::read_to_string(format!("{}{}.toml", ENV_DIR, env_name))?;
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

pub fn replace_variables(content: String, variables: BTreeMap<String, String>) -> String {
    let mut new_content = content;
    for (key, value) in variables {
        new_content = replace_template_variable(&new_content, &key, &value);
    }
    new_content
}

pub fn contains_template_variable(content: &str, key: &str) -> bool {
    template_variable_ranges(content).any(|(start, end)| content[start + 2..end - 2].trim() == key)
}

pub fn replace_template_variable(content: &str, key: &str, value: &str) -> String {
    let mut output = String::with_capacity(content.len());
    let mut cursor = 0;
    for (start, end) in template_variable_ranges(content) {
        output.push_str(&content[cursor..start]);
        if content[start + 2..end - 2].trim() == key {
            output.push_str(value);
        } else {
            output.push_str(&content[start..end]);
        }
        cursor = end;
    }
    output.push_str(&content[cursor..]);
    output
}

fn template_variable_ranges(content: &str) -> impl Iterator<Item = (usize, usize)> + '_ {
    let mut cursor = 0;
    std::iter::from_fn(move || {
        let start = content[cursor..].find("{{").map(|offset| cursor + offset)?;
        let body_start = start + 2;
        let Some(close_offset) = content[body_start..].find("}}") else {
            cursor = content.len();
            return None;
        };
        let end = body_start + close_offset + 2;
        cursor = end;
        Some((start, end))
    })
}

#[cfg(test)]
mod template_variable_tests {
    use super::*;

    #[test]
    fn template_variables_allow_whitespace_without_replacing_unknown_keys() {
        let input = "{{service_image}} {{ service_image }} {{  service_image\t}} {{other}}";
        assert!(contains_template_variable(input, "service_image"));
        assert_eq!(
            replace_template_variable(input, "service_image", "image@sha256:digest"),
            "image@sha256:digest image@sha256:digest image@sha256:digest {{other}}"
        );
    }
}
