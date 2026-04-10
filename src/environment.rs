use std::path::Path;

use serde::{Deserialize, Deserializer};
use toml::Value;

use crate::filesystem;
use crate::roomservice::config::Config;
use crate::utils::get_current_timestamp;
use crate::LOGGER;

const SCHEMA_V02: &str = "0.2.0";
const SCHEMA_V03: &str = "0.3.0";
const SCHEMA_V04: &str = "0.4.0";

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Environment {
    pub schema_version: String,
    pub name: String,
    pub log_level: String,
    #[serde(rename = "service", alias = "service_whitelist", default)]
    pub services: Vec<Service>,
    pub domain: String,
    pub default_replicas: u8,
    pub registry: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    pub environment_variables: Option<Vec<EnvironmentVariable>>,
    #[serde(rename = "build", default, skip_serializing)]
    legacy_build: Option<Config>,
}

impl Environment {
    // Creates a new Sailr environment instance for the specified name.
    // This searches for an environment configuration file at `./k8s/environments/<name>/config.toml`.
    // Default values are used for properties like log level, domain, and default replicas unless overridden in the config file.
    // A `FileSystemManager` is used to manage access and manipulation of environment configuration files.
    pub fn new(name: &str) -> Self {
        Self {
            schema_version: SCHEMA_V04.to_string(),
            name: name.to_string(),
            log_level: "INFO".to_string(),
            services: Vec::new(),
            domain: "localhost".to_string(),
            default_replicas: 1,
            registry: "docker.io".to_string(),
            platform: None,
            environment_variables: Some(Vec::new()),
            legacy_build: None,
        }
    }

    pub fn get_service(&self, name: &str) -> Option<&Service> {
        self.services.iter().find(|s| s.name == name)
    }

    // Returns a list of services in the environment.
    pub fn list_services(&self) -> Vec<&Service> {
        self.services.iter().collect()
    }

    // Adds a service to the environment.
    pub fn add_service(&mut self, service: Service) {
        self.services.push(service);
    }

    // Removes a service from the environment.
    pub fn remove_service(&mut self, name: &str) {
        self.services.retain(|s| s.name != name);
    }

    // Returns an environment variable from the environment.
    pub fn get_environment_variable(&self, name: &str) -> Option<&EnvironmentVariable> {
        if let Some(env_vars) = &self.environment_variables {
            env_vars.iter().find(|e| e.name == name)
        } else {
            None
        }
    }

    // Adds an environment variable to the environment.
    pub fn add_environment_variable(&mut self, env_var: EnvironmentVariable) {
        if let Some(env_vars) = &mut self.environment_variables {
            env_vars.push(env_var);
        }
    }

    // Removes an environment variable from the environment.
    pub fn remove_environment_variable(&mut self, name: &str) {
        if let Some(env_vars) = &mut self.environment_variables {
            env_vars.retain(|e| e.name != name);
        }
    }

    fn validate_schema_constraints(
        raw: &Value,
        schema_version: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if schema_version == SCHEMA_V04 {
            if raw.get("build").is_some() {
                return Err(Box::new(std::io::Error::other(
                    "Schema 0.4.0 does not allow top-level [build.rooms]. Move build config to [[service]].[service.build].",
                )));
            }

            if raw.get("service_whitelist").is_some() {
                return Err(Box::new(std::io::Error::other(
                    "Schema 0.4.0 requires [[service]] instead of [[service_whitelist]].",
                )));
            }

            if let Some(services) = raw.get("service").and_then(|v| v.as_array()) {
                for service in services {
                    if service.get("path").is_some() {
                        return Err(Box::new(std::io::Error::other(
                            "Schema 0.4.0 does not allow top-level service.path. Use [service.build].relies_on instead.",
                        )));
                    }

                    if service.get("build").is_some_and(Value::is_str) {
                        return Err(Box::new(std::io::Error::other(
                            "Schema 0.4.0 requires [service.build] table syntax; string shorthand is legacy.",
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    fn apply_legacy_build_fallback(&mut self) {
        let Some(legacy_build) = self.legacy_build.take() else {
            return;
        };

        for (name, room_config) in legacy_build.rooms {
            let mapped_build = ServiceBuildConfig {
                path: room_config.path,
                relies_on: None,
                before: room_config
                    .before
                    .or(room_config.run_parallel)
                    .or(room_config.run_synchronous)
                    .or(room_config.before_synchronous)
                    .map(CommandSpec::Single),
                after: room_config
                    .after
                    .or(room_config.finally)
                    .map(CommandSpec::Single),
            };

            match self.services.iter_mut().find(|s| s.name == name) {
                Some(service) => {
                    if service.build.is_none() {
                        service.build = Some(mapped_build);
                    }
                }
                None => {
                    let mut service = Service::new(&name, None, "latest");
                    service.build = Some(mapped_build);
                    self.services.push(service);
                }
            }
        }
    }

    // Loads the environment configuration from the `./k8s/environments/<name>/config.toml` file, overriding default values set in the constructor.
    // An error is returned if the file is missing, cannot be read, or contains an incompatible schema version.
    pub fn load_from_file(name: &String) -> Result<Self, Box<dyn std::error::Error>> {
        let filemanager = filesystem::FileSystemManager::new(
            Path::new("./k8s/environments")
                .join(name)
                .to_str()
                .ok_or("Invalid path for environment name")?
                .to_string(),
        );

        let contents = filemanager.read_file(&"config.toml".to_string(), None)?;
        let raw = toml::from_str::<Value>(&contents)?;
        let schema_version = raw
            .get("schema_version")
            .and_then(Value::as_str)
            .ok_or("Missing schema_version in environment config")?;

        if schema_version != SCHEMA_V02
            && schema_version != SCHEMA_V03
            && schema_version != SCHEMA_V04
        {
            return Err(Box::new(std::io::Error::other(format!(
                "Invalid schema version: expected one of [{}, {}, {}], found {}",
                SCHEMA_V02, SCHEMA_V03, SCHEMA_V04, schema_version
            ))));
        }

        Self::validate_schema_constraints(&raw, schema_version)?;

        let mut env = toml::from_str::<Self>(&contents)?;

        if env.schema_version == SCHEMA_V02 || env.schema_version == SCHEMA_V03 {
            LOGGER.warn(&format!(
                "Schema version {} is legacy. Please migrate to {}.",
                env.schema_version, SCHEMA_V04
            ));
        }

        env.apply_legacy_build_fallback();

        Ok(env)
    }

    pub fn save_to_file(&self) -> Result<(), Box<dyn std::error::Error>> {
        let contents = toml::to_string(&self)?;

        let filemanager = filesystem::FileSystemManager::new(
            Path::new("./k8s/environments")
                .join(&self.name)
                .to_str()
                .ok_or("Invalid path for environment name")?
                .to_string(),
        );

        filemanager.create_file(&"config.toml".to_string(), &contents)?;
        Ok(())
    }

    pub fn get_variables(&self, service: &Service) -> Vec<(String, String)> {
        let mut variables = vec![
            ("name".to_string(), self.name.clone()),
            ("log_level".to_string(), self.log_level.clone()),
            ("domain".to_string(), self.domain.clone()),
            ("deployment_date".to_string(), get_current_timestamp()),
            (
                "default_replicas".to_string(),
                self.default_replicas.to_string(),
            ),
            ("registry".to_string(), self.registry.clone()),
            (
                "platform".to_string(),
                self.platform.clone().unwrap_or_default(),
            ),
            ("schema_version".to_string(), self.schema_version.clone()),
            ("service_name".to_string(), service.name.clone()),
            (
                "service_namespace".to_string(),
                service.namespace_or(&self.name).to_string(),
            ),
        ];

        if let Some(build) = &service.build {
            variables.push(("service_path".to_string(), build.path.clone()));
        } else if let Some(path) = &service.template_path {
            variables.push(("service_path".to_string(), path.clone()));
        }

        variables.push(("service_version".to_string(), service.get_version()));

        if let Some(env_vars) = &self.environment_variables {
            env_vars.iter().for_each(|e| {
                let rendered_value = match e.value.clone() {
                    Some(Value::String(s)) => s,
                    Some(v) => v.to_string(),
                    None => String::new(),
                };

                variables.push((e.name.clone(), rendered_value));
            })
        }

        variables
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct EnvironmentVariable {
    pub name: String,
    pub value: Option<Value>,
}

impl EnvironmentVariable {
    pub fn new(name: &str, value: Option<Value>) -> Self {
        Self {
            name: name.to_string(),
            value,
        }
    }

    pub fn set_value(&mut self, value: Value) {
        self.value = Some(value);
    }
}

fn default_service_version() -> String {
    "latest".to_string()
}

fn deserialize_build_config<'de, D>(deserializer: D) -> Result<Option<ServiceBuildConfig>, D::Error>
where
    D: Deserializer<'de>,
{
    let maybe_value = Option::<Value>::deserialize(deserializer)?;

    match maybe_value {
        None => Ok(None),
        Some(Value::String(path)) => Ok(Some(ServiceBuildConfig {
            path,
            relies_on: None,
            before: None,
            after: None,
        })),
        Some(value) => value
            .try_into::<ServiceBuildConfig>()
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq)]
#[serde(untagged)]
pub enum CommandSpec {
    Single(String),
    Multiple(Vec<String>),
}

impl CommandSpec {
    pub fn into_vec(self) -> Vec<String> {
        match self {
            Self::Single(cmd) => vec![cmd],
            Self::Multiple(cmds) => cmds,
        }
    }

    pub fn as_vec(&self) -> Vec<String> {
        match self {
            Self::Single(cmd) => vec![cmd.clone()],
            Self::Multiple(cmds) => cmds.clone(),
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct Service {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(default = "default_service_version")]
    pub version: String,
    #[serde(
        default,
        deserialize_with = "deserialize_build_config",
        skip_serializing_if = "Option::is_none"
    )]
    pub build: Option<ServiceBuildConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<ServiceHooks>,
    #[serde(default, alias = "path", skip_serializing)]
    pub template_path: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct ServiceBuildConfig {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relies_on: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<CommandSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<CommandSpec>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct ServiceHooks {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_deploy: Option<CommandSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_deploy: Option<CommandSpec>,
}

impl Service {
    pub fn new(name: &str, namespace: Option<&str>, version: &str) -> Self {
        Self {
            name: name.to_string(),
            namespace: namespace.map(|ns| ns.to_string()),
            version: version.to_string(),
            build: None,
            hooks: None,
            template_path: None,
        }
    }

    pub fn namespace_or<'a>(&'a self, default_namespace: &'a str) -> &'a str {
        self.namespace.as_deref().unwrap_or(default_namespace)
    }

    pub fn get_version(&self) -> String {
        self.version.clone()
    }

    pub fn get_version_without_tag(&self) -> String {
        self.version
            .split_once('-')
            .map(|(base, _)| base.to_string())
            .unwrap_or_else(|| self.version.clone())
    }

    pub fn get_path(&self) -> String {
        self.template_path
            .clone()
            .unwrap_or_else(|| self.name.clone())
            .replace("./", "")
    }

    pub fn get_full_name(&self) -> String {
        format!("{}/{}", self.namespace_or("default"), self.name)
    }

    pub fn get_full_name_with_version(&self) -> String {
        format!(
            "{}/{}:{}",
            self.namespace_or("default"),
            self.name,
            self.get_version()
        )
    }

    pub fn get_full_name_with_path(&self) -> String {
        format!(
            "{}/{}:{}",
            self.namespace_or("default"),
            self.name,
            self.get_path()
        )
    }

    pub fn bump_major_version(&mut self) {
        let mut parts = self
            .get_version_without_tag()
            .split('.')
            .map(|x| x.parse::<i32>().unwrap_or(0))
            .collect::<Vec<i32>>();

        if parts.is_empty() {
            parts = vec![1, 0, 0];
        } else {
            parts[0] += 1;
            if parts.len() < 3 {
                while parts.len() < 3 {
                    parts.push(0);
                }
            } else {
                parts[1] = 0;
                parts[2] = 0;
            }
        }

        self.version = format!("{}.{}.{}", parts[0], parts[1], parts[2]);
    }

    pub fn bump_minor_version(&mut self) {
        let mut parts = self
            .get_version_without_tag()
            .split('.')
            .map(|x| x.parse::<i32>().unwrap_or(0))
            .collect::<Vec<i32>>();

        while parts.len() < 3 {
            parts.push(0);
        }

        parts[1] += 1;
        parts[2] = 0;
        self.version = format!("{}.{}.{}", parts[0], parts[1], parts[2]);
    }

    pub fn bump_patch_version(&mut self) {
        let mut parts = self
            .get_version_without_tag()
            .split('.')
            .map(|x| x.parse::<i32>().unwrap_or(0))
            .collect::<Vec<i32>>();

        while parts.len() < 3 {
            parts.push(0);
        }

        parts[2] += 1;
        self.version = format!("{}.{}.{}", parts[0], parts[1], parts[2]);
    }

    pub fn set_tag(&mut self, tag: String) {
        self.version = format!("{}-{}", self.get_version_without_tag(), tag);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_deserialize_legacy_service_build_string() {
        let service_json = json!({
            "name": "api",
            "version": "1.2.3",
            "build": "./services/api"
        });

        let service: Service = serde_json::from_value(service_json).unwrap();
        assert_eq!(service.name, "api");
        assert_eq!(service.version, "1.2.3");
        assert_eq!(
            service.build,
            Some(ServiceBuildConfig {
                path: "./services/api".to_string(),
                relies_on: None,
                before: None,
                after: None
            })
        );
    }

    #[test]
    fn test_deserialize_service_build_table() {
        let service_json = json!({
            "name": "api",
            "version": "1.2.3",
            "build": {
                "path": "./services/api",
                "relies_on": ["./services/shared"],
                "before": "docker build .",
                "after": "docker push x"
            }
        });

        let service: Service = serde_json::from_value(service_json).unwrap();
        let build = service.build.unwrap();
        assert_eq!(build.path, "./services/api");
        assert_eq!(build.relies_on.unwrap(), vec!["./services/shared"]);
        assert_eq!(
            build.before.unwrap(),
            CommandSpec::Single("docker build .".to_string())
        );
        assert_eq!(
            build.after.unwrap(),
            CommandSpec::Single("docker push x".to_string())
        );
    }

    #[test]
    fn test_deserialize_service_build_with_command_arrays() {
        let service_json = json!({
            "name": "api",
            "version": "1.2.3",
            "build": {
                "path": "./services/api",
                "before": ["pnpm i", "pnpm build"],
                "after": ["docker push x", "echo done"]
            },
            "hooks": {
                "pre_deploy": ["echo pre", "scripts/check.sh"],
                "post_deploy": "echo post"
            }
        });

        let service: Service = serde_json::from_value(service_json).unwrap();
        let build = service.build.unwrap();
        assert_eq!(
            build.before.unwrap(),
            CommandSpec::Multiple(vec!["pnpm i".to_string(), "pnpm build".to_string()])
        );
        assert_eq!(
            build.after.unwrap(),
            CommandSpec::Multiple(vec!["docker push x".to_string(), "echo done".to_string()])
        );

        let hooks = service.hooks.unwrap();
        assert_eq!(
            hooks.pre_deploy.unwrap(),
            CommandSpec::Multiple(vec!["echo pre".to_string(), "scripts/check.sh".to_string()])
        );
        assert_eq!(
            hooks.post_deploy.unwrap(),
            CommandSpec::Single("echo post".to_string())
        );
    }

    #[test]
    fn test_service_path_falls_back_to_name() {
        let service = Service::new("worker", None, "latest");
        assert_eq!(service.get_path(), "worker");
    }

    #[test]
    fn test_environment_uses_service_aliases() {
        let content = r#"
schema_version = "0.3.0"
name = "dev"
log_level = "INFO"
domain = "example.com"
default_replicas = 1
registry = "docker.io"

[[service_whitelist]]
name = "api"
version = "latest"
"#;

        let env: Environment = toml::from_str(content).unwrap();
        assert_eq!(env.services.len(), 1);
        assert_eq!(env.services[0].name, "api");
    }

    #[test]
    fn test_environment_parses_top_level_platform() {
        let content = r#"
schema_version = "0.3.0"
name = "edge"
log_level = "INFO"
domain = "example.com"
default_replicas = 1
registry = "docker.io"
platform = "linux/amd64,linux/arm64"

[[service_whitelist]]
name = "api"
version = "latest"
"#;

        let env: Environment = toml::from_str(content).unwrap();
        assert_eq!(env.platform.as_deref(), Some("linux/amd64,linux/arm64"));
    }

    #[test]
    fn test_legacy_build_rooms_map_onto_services() {
        let content = r#"
schema_version = "0.3.0"
name = "dev"
log_level = "INFO"
domain = "example.com"
default_replicas = 1
registry = "docker.io"

[[service_whitelist]]
name = "api"
version = "latest"

[build.rooms.api]
path = "./services/api"
before = "custom-build"
after = "custom-push"
"#;

        let mut env: Environment = toml::from_str(content).unwrap();
        env.apply_legacy_build_fallback();

        let api = env.get_service("api").unwrap();
        let build = api.build.as_ref().unwrap();
        assert_eq!(build.path, "./services/api");
        assert_eq!(
            build.before,
            Some(CommandSpec::Single("custom-build".to_string()))
        );
        assert_eq!(
            build.after,
            Some(CommandSpec::Single("custom-push".to_string()))
        );
    }
}
