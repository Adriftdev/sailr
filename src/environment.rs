use std::{error::Error, path::Path};

use serde::{Deserialize, Deserializer};
use toml::{map::Map, Value};

use crate::filesystem;
use crate::roomservice::config::Config;
use crate::utils::get_current_timestamp;
use crate::LOGGER;

const SCHEMA_V02: &str = "0.2.0";
const SCHEMA_V03: &str = "0.3.0";
const SCHEMA_V04: &str = "0.4.0";
const SCHEMA_V05: &str = "0.5.0";

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum RegistryConfig {
    Simple(String),
    Detailed {
        host: String,
        namespace: Option<String>,
    },
}

#[cfg(test)]
mod registry_contract_tests {
    use super::{RegistryConfig, ResolvedRegistry};

    #[test]
    fn detailed_registry_separates_host_and_namespace_validation() {
        let valid = RegistryConfig::Detailed {
            host: "ghcr.io".to_string(),
            namespace: Some("acme/platform".to_string()),
        }
        .resolve()
        .unwrap();
        assert_eq!(valid.prefix(), "ghcr.io/acme/platform");

        for host in [
            "",
            "https://ghcr.io",
            "/ghcr.io",
            "ghcr.io/",
            "ghcr.io/acme",
            "ghcr .io",
        ] {
            assert!(RegistryConfig::Detailed {
                host: host.to_string(),
                namespace: None,
            }
            .resolve()
            .is_err());
        }
        for namespace in [
            "",
            "/acme",
            "acme/",
            "acme//platform",
            "acme platform",
            "https://acme",
        ] {
            assert!(RegistryConfig::Detailed {
                host: "ghcr.io".to_string(),
                namespace: Some(namespace.to_string()),
            }
            .resolve()
            .is_err());
        }
    }

    #[test]
    fn repository_and_tag_inputs_reject_obvious_malformed_values() {
        let registry = ResolvedRegistry::parse("ghcr.io/acme").unwrap();
        for service in ["", "api worker", "/api", "api/", "api//worker"] {
            assert!(registry.repository_for(service).is_err());
        }
        for tag in ["", "release candidate", ":latest", "release/latest"] {
            assert!(registry.tagged_ref("api", tag).is_err());
        }
        assert_eq!(
            registry.tagged_ref("api", "1.2.0").unwrap(),
            "ghcr.io/acme/api:1.2.0"
        );
    }
}

impl Default for RegistryConfig {
    fn default() -> Self {
        RegistryConfig::Simple("docker.io".to_string())
    }
}

impl RegistryConfig {
    pub fn host(&self) -> Result<String, crate::workflow::error::RegistryConfigError> {
        Ok(self.resolve()?.host)
    }

    pub fn namespace(&self) -> Result<Option<String>, crate::workflow::error::RegistryConfigError> {
        Ok(self.resolve()?.namespace)
    }

    pub fn prefix(&self) -> Result<String, crate::workflow::error::RegistryConfigError> {
        Ok(self.resolve()?.prefix())
    }

    pub fn resolve(&self) -> Result<ResolvedRegistry, crate::workflow::error::RegistryConfigError> {
        match self {
            Self::Simple(s) => ResolvedRegistry::parse(s),
            Self::Detailed { host, namespace } => {
                let parsed_host = host.trim();
                if parsed_host.is_empty() {
                    return Err(crate::workflow::error::RegistryConfigError::EmptyHost);
                }
                if parsed_host.contains("://")
                    || parsed_host.starts_with('/')
                    || parsed_host.ends_with('/')
                    || parsed_host.contains('/')
                    || parsed_host.contains(|c: char| c.is_whitespace())
                {
                    return Err(crate::workflow::error::RegistryConfigError::InvalidHost(
                        parsed_host.to_string(),
                    ));
                }

                if let Some(ns) = namespace {
                    let parsed_ns = ns.trim();
                    if parsed_ns.is_empty()
                        || parsed_ns.contains("://")
                        || parsed_ns.starts_with('/')
                        || parsed_ns.ends_with('/')
                        || parsed_ns.contains(|c: char| c.is_whitespace())
                        || parsed_ns.contains("//")
                    {
                        return Err(
                            crate::workflow::error::RegistryConfigError::InvalidNamespace(
                                parsed_ns.to_string(),
                            ),
                        );
                    }
                    Ok(ResolvedRegistry {
                        host: parsed_host.to_string(),
                        namespace: Some(parsed_ns.to_string()),
                    })
                } else {
                    Ok(ResolvedRegistry {
                        host: parsed_host.to_string(),
                        namespace: None,
                    })
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRegistry {
    pub host: String,
    pub namespace: Option<String>,
}

impl ResolvedRegistry {
    pub fn parse(s: &str) -> Result<Self, crate::workflow::error::RegistryConfigError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(crate::workflow::error::RegistryConfigError::EmptyHost);
        }
        if s.contains("://")
            || s.starts_with('/')
            || s.ends_with('/')
            || s.contains(|c: char| c.is_whitespace())
            || s.contains("//")
        {
            return Err(crate::workflow::error::RegistryConfigError::InvalidHost(
                s.to_string(),
            ));
        }

        let parts: Vec<&str> = s.splitn(2, '/').collect();
        let host = parts[0].to_string();
        if host.is_empty() {
            return Err(crate::workflow::error::RegistryConfigError::InvalidHost(
                s.to_string(),
            ));
        }

        let namespace = if parts.len() > 1 {
            let ns = parts[1].to_string();
            if ns.is_empty()
                || ns.contains("://")
                || ns.starts_with('/')
                || ns.ends_with('/')
                || ns.contains("//")
                || ns.chars().any(char::is_whitespace)
            {
                return Err(
                    crate::workflow::error::RegistryConfigError::InvalidNamespace(s.to_string()),
                );
            }
            Some(ns)
        } else {
            None
        };

        Ok(Self { host, namespace })
    }

    pub fn prefix(&self) -> String {
        match &self.namespace {
            Some(ns) => format!("{}/{}", self.host, ns),
            None => self.host.clone(),
        }
    }

    pub fn repository_for(
        &self,
        service: &str,
    ) -> Result<String, crate::workflow::error::RegistryConfigError> {
        if service.trim().is_empty()
            || service.chars().any(char::is_whitespace)
            || service.starts_with('/')
            || service.ends_with('/')
            || service.contains("//")
        {
            return Err(crate::workflow::error::RegistryConfigError::InvalidService(
                service.to_string(),
            ));
        }
        match &self.namespace {
            Some(ns) => Ok(format!("{}/{}", ns, service)),
            None => Ok(service.to_string()),
        }
    }

    pub fn tagged_ref(
        &self,
        service: &str,
        tag: &str,
    ) -> Result<String, crate::workflow::error::RegistryConfigError> {
        if tag.trim().is_empty()
            || tag.chars().any(char::is_whitespace)
            || tag.starts_with(':')
            || tag.contains('/')
        {
            return Err(crate::workflow::error::RegistryConfigError::InvalidTag(
                tag.to_string(),
            ));
        }
        let repo = self.repository_for(service)?;
        Ok(format!("{}/{}:{}", self.host, repo, tag))
    }

    pub fn digest_ref(
        &self,
        service: &str,
        digest: &str,
    ) -> Result<String, crate::workflow::error::RegistryConfigError> {
        if crate::oci::validate_sha256_digest(digest).is_err() {
            return Err(crate::workflow::error::RegistryConfigError::InvalidDigest(
                digest.to_string(),
            ));
        }
        let repo = self.repository_for(service)?;
        Ok(format!("{}/{}@{}", self.host, repo, digest))
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Environment {
    pub schema_version: String,
    pub name: String,
    pub log_level: String,
    #[serde(rename = "service", alias = "service_whitelist", default)]
    pub services: Vec<Service>,
    pub domain: String,
    pub default_replicas: u8,
    #[serde(default)]
    pub registry: RegistryConfig,

    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<BuildPolicy>,
    pub environment_variables: Option<Vec<EnvironmentVariable>>,
}

impl Environment {
    // Creates a new Sailr environment instance for the specified name.
    // This searches for an environment configuration file at `./k8s/environments/<name>/config.toml`.
    // Default values are used for properties like log level, domain, and default replicas unless overridden in the config file.
    // A `FileSystemManager` is used to manage access and manipulation of environment configuration files.
    pub fn new(name: &str) -> Self {
        Self {
            schema_version: SCHEMA_V05.to_string(),
            name: name.to_string(),
            log_level: "INFO".to_string(),
            services: Vec::new(),
            domain: "localhost".to_string(),
            default_replicas: 1,
            registry: RegistryConfig::default(),
            platform: None,
            build: None,
            environment_variables: Some(Vec::new()),
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
        if schema_version == SCHEMA_V04 || schema_version == SCHEMA_V05 {
            if raw.get("build").is_some() {
                let is_legacy_rooms = raw
                    .get("build")
                    .and_then(Value::as_table)
                    .is_some_and(|table| table.contains_key("rooms"));

                if schema_version == SCHEMA_V04 || is_legacy_rooms {
                    return Err(Box::new(std::io::Error::other(
                        "Schema 0.4.0+ does not allow legacy [build.rooms]. Move build config to [[service]].build and top-level [build] policy fields.",
                    )));
                }
            }

            if raw.get("service_whitelist").is_some() {
                return Err(Box::new(std::io::Error::other(
                    "Schema 0.4.0+ requires [[service]] instead of [[service_whitelist]].",
                )));
            }

            if let Some(services) = raw.get("service").and_then(|v| v.as_array()) {
                for service in services {
                    if schema_version == SCHEMA_V04
                        && service.get("build").is_some_and(Value::is_str)
                    {
                        return Err(Box::new(std::io::Error::other(
                            "Schema 0.4.0 requires [service.build] table syntax; string shorthand is legacy.",
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    fn extract_legacy_build(raw: &Value) -> Result<Option<Config>, Box<dyn std::error::Error>> {
        let Some(build_value) = raw.get("build") else {
            return Ok(None);
        };

        if build_value
            .as_table()
            .is_some_and(|table| table.contains_key("rooms"))
        {
            return build_value
                .clone()
                .try_into()
                .map(Some)
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>);
        }

        Ok(None)
    }

    fn apply_legacy_build_fallback(&mut self, legacy_build: Option<Config>) {
        let Some(legacy_build) = legacy_build else {
            return;
        };

        let build_policy = self.build.get_or_insert_with(BuildPolicy::default);
        if build_policy.before_all.is_none() {
            build_policy.before_all =
                command_spec_from_vec(legacy_build.before_all.into_iter().collect());
        }
        if build_policy.after_all.is_none() {
            build_policy.after_all =
                command_spec_from_vec(legacy_build.after_all.into_iter().collect());
        }

        for (name, room_config) in legacy_build.rooms {
            let crate::roomservice::config::RoomConfig {
                path,
                include,
                before_synchronous,
                before,
                run_synchronous,
                run_parallel,
                after,
                finally,
            } = room_config;

            let mapped_build = ServiceBuildConfig {
                path,
                include: Some(vec![include]),
                relies_on: None,
                before_synchronous: before_synchronous.map(CommandSpec::Single),
                before: None,
                run_parallel: run_parallel.map(CommandSpec::Single),
                run_synchronous: run_synchronous.map(CommandSpec::Single),
                after: None,
                finally: finally.map(CommandSpec::Single),
                dockerfile: None,
                build_command: before,
                push_command: after,
            };

            match self.services.iter_mut().find(|s| s.name == name) {
                Some(service) => {
                    if let Some(existing_build) = service.build.as_mut() {
                        if existing_build.path.trim().is_empty() {
                            existing_build.path = mapped_build.path.clone();
                        }
                        if existing_build.include.is_none() {
                            existing_build.include = mapped_build.include.clone();
                        }
                        if existing_build.before_synchronous.is_none() {
                            existing_build.before_synchronous =
                                mapped_build.before_synchronous.clone();
                        }
                        if existing_build.run_parallel.is_none() {
                            existing_build.run_parallel = mapped_build.run_parallel.clone();
                        }
                        if existing_build.run_synchronous.is_none() {
                            existing_build.run_synchronous = mapped_build.run_synchronous.clone();
                        }
                        if existing_build.build_command.is_none() {
                            existing_build.build_command = mapped_build.build_command.clone();
                        }
                        if existing_build.push_command.is_none() {
                            existing_build.push_command = mapped_build.push_command.clone();
                        }
                        if existing_build.finally.is_none() {
                            existing_build.finally = mapped_build.finally.clone();
                        }
                    } else {
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
    pub fn load_from_file(name: &str) -> Result<Self, Box<dyn Error>> {
        let (raw, inherited) = Self::resolve_raw_environment(name, &mut Vec::new(), &|env_name| {
            Self::read_environment_contents(env_name)
        })?;

        Self::environment_from_raw(raw, name, inherited)
    }

    fn read_environment_contents(name: &str) -> Result<String, Box<dyn Error>> {
        let filemanager = filesystem::FileSystemManager::new(
            Path::new("./k8s/environments")
                .join(name)
                .to_str()
                .ok_or("Invalid path for environment name")?
                .to_string(),
        );

        filemanager
            .read_file(&"config.toml".to_string(), None)
            .map_err(|error| {
                Box::new(std::io::Error::other(format!(
                    "Failed to read environment config '{}' at ./k8s/environments/{}/config.toml: {}",
                    name, name, error
                ))) as Box<dyn Error>
            })
    }

    fn resolve_raw_environment(
        name: &str,
        stack: &mut Vec<String>,
        read_config: &EnvironmentReader<'_>,
    ) -> Result<(Value, bool), Box<dyn Error>> {
        if let Some(cycle_start) = stack.iter().position(|entry| entry == name) {
            let mut cycle = stack[cycle_start..].to_vec();
            cycle.push(name.to_string());
            return Err(Box::new(std::io::Error::other(format!(
                "Environment inheritance cycle detected: {}",
                cycle.join(" -> ")
            ))));
        }

        stack.push(name.to_string());
        let contents = read_config(name)?;
        let raw = toml::from_str::<Value>(&contents)?;
        let Some(base_name) = raw.get("extends").and_then(Value::as_str) else {
            stack.pop();
            return Ok((raw, false));
        };

        let child_defines_name = raw
            .as_table()
            .is_some_and(|table| table.contains_key("name"));
        let (mut resolved, _) = Self::resolve_raw_environment(base_name, stack, read_config)?;
        merge_environment_value(&mut resolved, raw)?;

        if let Some(table) = resolved.as_table_mut() {
            table.remove("extends");
            if !child_defines_name {
                table.insert("name".to_string(), Value::String(name.to_string()));
            }
        }

        stack.pop();
        Ok((resolved, true))
    }

    fn environment_from_raw(
        raw: Value,
        source_name: &str,
        inherited: bool,
    ) -> Result<Self, Box<dyn Error>> {
        let schema_version = raw
            .get("schema_version")
            .and_then(Value::as_str)
            .ok_or("Missing schema_version in environment config")?;

        if schema_version != SCHEMA_V02
            && schema_version != SCHEMA_V03
            && schema_version != SCHEMA_V04
            && schema_version != SCHEMA_V05
        {
            return Err(Box::new(std::io::Error::other(format!(
                "Invalid schema version: expected one of [{}, {}, {}, {}], found {}",
                SCHEMA_V02, SCHEMA_V03, SCHEMA_V04, SCHEMA_V05, schema_version
            ))));
        }

        if inherited && schema_version != SCHEMA_V05 {
            return Err(Box::new(std::io::Error::other(format!(
                "Inherited environment '{}' must resolve to schema_version = \"{}\"; found {}",
                source_name, SCHEMA_V05, schema_version
            ))));
        }

        Self::validate_schema_constraints(&raw, schema_version)?;
        let legacy_build = Self::extract_legacy_build(&raw)?;

        let mut env = raw.try_into::<Self>()?;

        if env.schema_version == SCHEMA_V02 || env.schema_version == SCHEMA_V03 {
            LOGGER.warn(&format!(
                "Schema version {} is legacy. Please migrate to {}.",
                env.schema_version, SCHEMA_V05
            ));
        }

        env.apply_legacy_build_fallback(legacy_build);

        Ok(env)
    }

    pub fn update_local_service_version_contents(
        contents: &str,
        service_name: &str,
        version: &str,
    ) -> Result<Option<String>, Box<dyn Error>> {
        let mut doc = contents.parse::<toml_edit::DocumentMut>()?;
        let service_key = local_service_array_key(&doc);
        let Some(services) = doc
            .get_mut(service_key)
            .and_then(toml_edit::Item::as_array_of_tables_mut)
        else {
            return Ok(None);
        };

        let mut updated = false;
        for service in services.iter_mut() {
            if service.get("name").and_then(|name| name.as_str()) == Some(service_name) {
                service["version"] = toml_edit::value(version);
                updated = true;
                break;
            }
        }

        if updated {
            Ok(Some(doc.to_string()))
        } else {
            Ok(None)
        }
    }

    pub fn append_service_version_override_contents(
        contents: &str,
        service_name: &str,
        version: &str,
    ) -> Result<String, Box<dyn Error>> {
        let service = Service::new(service_name, None, version);
        Self::append_service_override_contents(contents, &service)
    }

    pub fn append_service_override_contents(
        contents: &str,
        service: &Service,
    ) -> Result<String, Box<dyn Error>> {
        let mut doc = contents.parse::<toml_edit::DocumentMut>()?;
        append_service_to_document(&mut doc, service);
        Ok(doc.to_string())
    }

    pub fn migrate_contents_to_v04(contents: &str) -> Result<String, Box<dyn std::error::Error>> {
        let raw = toml::from_str::<Value>(contents)?;
        let schema_version = raw
            .get("schema_version")
            .and_then(Value::as_str)
            .ok_or("Missing schema_version in environment config")?;

        if schema_version != SCHEMA_V02
            && schema_version != SCHEMA_V03
            && schema_version != SCHEMA_V04
            && schema_version != SCHEMA_V05
        {
            return Err(Box::new(std::io::Error::other(format!(
                "Invalid schema version: expected one of [{}, {}, {}, {}], found {}",
                SCHEMA_V02, SCHEMA_V03, SCHEMA_V04, SCHEMA_V05, schema_version
            ))));
        }

        Self::validate_schema_constraints(&raw, schema_version)?;
        let legacy_build = Self::extract_legacy_build(&raw)?;

        let mut env = toml::from_str::<Self>(contents)?;
        env.apply_legacy_build_fallback(legacy_build);
        env.schema_version = SCHEMA_V04.to_string();
        env.build = None;

        Ok(toml::to_string_pretty(&env)?)
    }

    pub fn migrate_contents_to_v05(contents: &str) -> Result<String, Box<dyn std::error::Error>> {
        let raw = toml::from_str::<Value>(contents)?;
        let schema_version = raw
            .get("schema_version")
            .and_then(Value::as_str)
            .ok_or("Missing schema_version in environment config")?;

        if schema_version != SCHEMA_V02
            && schema_version != SCHEMA_V03
            && schema_version != SCHEMA_V04
            && schema_version != SCHEMA_V05
        {
            return Err(Box::new(std::io::Error::other(format!(
                "Invalid schema version: expected one of [{}, {}, {}, {}], found {}",
                SCHEMA_V02, SCHEMA_V03, SCHEMA_V04, SCHEMA_V05, schema_version
            ))));
        }

        let legacy_build = Self::extract_legacy_build(&raw)?;
        let mut env = toml::from_str::<Self>(contents)?;
        env.apply_legacy_build_fallback(legacy_build);
        env.schema_version = SCHEMA_V05.to_string();
        env.upgrade_builds_to_v05();

        Ok(toml::to_string_pretty(&env)?)
    }

    pub fn migrate_file_to_v04(name: &String) -> Result<String, Box<dyn std::error::Error>> {
        let filemanager = filesystem::FileSystemManager::new(
            Path::new("./k8s/environments")
                .join(name)
                .to_str()
                .ok_or("Invalid path for environment name")?
                .to_string(),
        );

        let contents = filemanager.read_file(&"config.toml".to_string(), None)?;
        let migrated = Self::migrate_contents_to_v04(&contents)?;
        filemanager.create_file(&"config.toml".to_string(), &migrated)?;
        Ok(migrated)
    }

    pub fn migrate_file_to_v05(name: &String) -> Result<String, Box<dyn std::error::Error>> {
        let filemanager = filesystem::FileSystemManager::new(
            Path::new("./k8s/environments")
                .join(name)
                .to_str()
                .ok_or("Invalid path for environment name")?
                .to_string(),
        );

        let contents = filemanager.read_file(&"config.toml".to_string(), None)?;
        let migrated = Self::migrate_contents_to_v05(&contents)?;
        filemanager.create_file(&"config.toml".to_string(), &migrated)?;
        Ok(migrated)
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

    pub fn get_variables(
        &self,
        service: &Service,
    ) -> Result<Vec<(String, String)>, crate::workflow::error::RegistryConfigError> {
        let mut variables = vec![
            ("name".to_string(), self.name.clone()),
            ("log_level".to_string(), self.log_level.clone()),
            ("replicas".to_string(), self.default_replicas.to_string()),
            ("registry".to_string(), self.registry.prefix()?),
            ("domain".to_string(), self.domain.clone()),
            ("deployment_date".to_string(), get_current_timestamp()),
            (
                "default_replicas".to_string(),
                self.default_replicas.to_string(),
            ),
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

        Ok(variables)
    }

    fn upgrade_builds_to_v05(&mut self) {
        for service in &mut self.services {
            let Some(build) = service.build.as_mut() else {
                continue;
            };

            if build.build_command.is_none()
                && build.before_synchronous.is_none()
                && build.run_parallel.is_none()
                && build.run_synchronous.is_none()
                && build.before.is_some()
            {
                build.build_command = build.before.take().map(command_spec_to_shell);
            }

            if build.push_command.is_none() && build.finally.is_none() && build.after.is_some() {
                build.push_command = build.after.take().map(command_spec_to_shell);
            }
        }
    }
}

type EnvironmentReader<'a> = dyn Fn(&str) -> Result<String, Box<dyn Error>> + 'a;

fn merge_environment_value(base: &mut Value, child: Value) -> Result<(), Box<dyn Error>> {
    let (Some(base_table), Some(child_table)) = (base.as_table_mut(), child.as_table()) else {
        *base = child;
        return Ok(());
    };

    merge_environment_table(base_table, child_table.clone());
    Ok(())
}

fn merge_environment_table(base: &mut Map<String, Value>, child: Map<String, Value>) {
    for (key, child_value) in child {
        if key == "extends" {
            continue;
        }

        if key == "service" || key == "environment_variables" {
            merge_named_array(base, key, child_value);
            continue;
        }

        match base.get_mut(&key) {
            Some(base_value) => merge_value(base_value, child_value),
            None => {
                base.insert(key, child_value);
            }
        }
    }
}

fn merge_value(base: &mut Value, child: Value) {
    match (base.as_table_mut(), child) {
        (Some(base_table), Value::Table(child_table)) => {
            merge_environment_table(base_table, child_table);
        }
        (_, child_value) => {
            *base = child_value;
        }
    }
}

fn merge_named_array(base: &mut Map<String, Value>, key: String, child_value: Value) {
    let Value::Array(child_items) = child_value else {
        base.insert(key, child_value);
        return;
    };

    let base_items = base
        .remove(&key)
        .and_then(|value| match value {
            Value::Array(items) => Some(items),
            _ => None,
        })
        .unwrap_or_default();

    let mut merged_items = base_items;

    for child_item in child_items {
        let Some(child_name) = named_array_item_name(&child_item) else {
            merged_items.push(child_item);
            continue;
        };

        if let Some(base_item) = merged_items
            .iter_mut()
            .find(|item| named_array_item_name(item) == Some(child_name))
        {
            merge_value(base_item, child_item);
        } else {
            merged_items.push(child_item);
        }
    }

    base.insert(key, Value::Array(merged_items));
}

fn named_array_item_name(item: &Value) -> Option<&str> {
    item.get("name").and_then(Value::as_str)
}

fn local_service_array_key(doc: &toml_edit::DocumentMut) -> &'static str {
    let has_service = doc
        .get("service")
        .is_some_and(toml_edit::Item::is_array_of_tables);
    let has_service_whitelist = doc
        .get("service_whitelist")
        .is_some_and(toml_edit::Item::is_array_of_tables);

    if has_service || !has_service_whitelist {
        "service"
    } else {
        "service_whitelist"
    }
}

fn append_service_to_document(doc: &mut toml_edit::DocumentMut, service: &Service) {
    let key = local_service_array_key(doc);
    if !doc
        .get(key)
        .is_some_and(toml_edit::Item::is_array_of_tables)
    {
        doc.as_table_mut().insert(
            key,
            toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()),
        );
    }

    let mut table = toml_edit::Table::new();
    table["name"] = toml_edit::value(service.name.clone());
    if let Some(namespace) = &service.namespace {
        table["namespace"] = toml_edit::value(namespace.clone());
    }
    table["version"] = toml_edit::value(service.version.clone());
    if let Some(template_path) = &service.template_path {
        table["path"] = toml_edit::value(template_path.clone());
    }

    doc[key]
        .as_array_of_tables_mut()
        .expect("service item should be an array of tables")
        .push(table);
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

fn deserialize_optional_string_vec<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let maybe_value = Option::<Value>::deserialize(deserializer)?;
    match maybe_value {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(vec![value])),
        Some(Value::Array(values)) => values
            .into_iter()
            .map(|value| match value {
                Value::String(item) => Ok(item),
                _ => Err(serde::de::Error::custom(
                    "expected string values in include list",
                )),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        Some(other) => Err(serde::de::Error::custom(format!(
            "expected string or array of strings, found {}",
            other.type_str()
        ))),
    }
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

fn command_spec_from_vec(commands: Vec<String>) -> Option<CommandSpec> {
    match commands.len() {
        0 => None,
        1 => commands.into_iter().next().map(CommandSpec::Single),
        _ => Some(CommandSpec::Multiple(commands)),
    }
}

fn command_spec_to_shell(command: CommandSpec) -> String {
    command.into_vec().join(" && ")
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Default)]
pub struct BuildPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<BuildEngine>,
    #[serde(default, alias = "beforeAll", skip_serializing_if = "Option::is_none")]
    pub before_all: Option<CommandSpec>,
    #[serde(default, alias = "afterAll", skip_serializing_if = "Option::is_none")]
    pub after_all: Option<CommandSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_parallelism: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fail_fast: Option<bool>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize, clap::ValueEnum,
)]
#[serde(rename_all = "kebab-case")]
pub enum BuildEngine {
    Roomservice,
    Runkernel,
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
    #[serde(
        default,
        rename = "path",
        alias = "template_path",
        skip_serializing_if = "Option::is_none"
    )]
    pub template_path: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct ServiceBuildConfig {
    pub path: String,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_string_vec",
        skip_serializing_if = "Option::is_none"
    )]
    pub include: Option<Vec<String>>,
    #[serde(default, alias = "depends_on", skip_serializing_if = "Option::is_none")]
    pub relies_on: Option<Vec<String>>,
    #[serde(
        default,
        alias = "beforeSynchronous",
        skip_serializing_if = "Option::is_none"
    )]
    pub before_synchronous: Option<CommandSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<CommandSpec>,
    #[serde(
        default,
        alias = "runParallel",
        skip_serializing_if = "Option::is_none"
    )]
    pub run_parallel: Option<CommandSpec>,
    #[serde(
        default,
        alias = "runSynchronous",
        skip_serializing_if = "Option::is_none"
    )]
    pub run_synchronous: Option<CommandSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<CommandSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finally: Option<CommandSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dockerfile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub push_command: Option<String>,
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
    use std::collections::BTreeMap;

    fn load_environment_from_sources(
        name: &str,
        sources: BTreeMap<&str, &str>,
    ) -> Result<Environment, Box<dyn Error>> {
        let (raw, inherited) =
            Environment::resolve_raw_environment(name, &mut Vec::new(), &|env_name| {
                sources
                    .get(env_name)
                    .map(|content| content.to_string())
                    .ok_or_else(|| {
                        Box::new(std::io::Error::other(format!(
                            "missing environment {}",
                            env_name
                        ))) as Box<dyn Error>
                    })
            })?;

        Environment::environment_from_raw(raw, name, inherited)
    }

    fn env_var_value(env: &Environment, name: &str) -> String {
        match env
            .get_environment_variable(name)
            .and_then(|env_var| env_var.value.as_ref())
        {
            Some(Value::String(value)) => value.clone(),
            Some(value) => value.to_string(),
            None => String::new(),
        }
    }

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
    fn test_deserialize_service_build_with_include_and_depends_on_alias() {
        let service_json = json!({
            "name": "api",
            "version": "1.2.3",
            "build": {
                "path": "./services/api",
                "include": "./src/**/*",
                "depends_on": ["kernel", "./services/shared"],
                "build_command": "docker build .",
                "push_command": "docker push api"
            }
        });

        let service: Service = serde_json::from_value(service_json).unwrap();
        let build = service.build.unwrap();
        assert_eq!(build.include, Some(vec!["./src/**/*".to_string()]));
        assert_eq!(
            build.relies_on,
            Some(vec!["kernel".to_string(), "./services/shared".to_string()])
        );
        assert_eq!(build.build_command.as_deref(), Some("docker build ."));
        assert_eq!(build.push_command.as_deref(), Some("docker push api"));
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
    fn test_environment_extends_merges_named_sections() {
        let base = r#"
schema_version = "0.5.0"
name = "base"
log_level = "INFO"
domain = "base.example.com"
default_replicas = 1
registry = "docker.io/base"
platform = "linux/amd64"

[build]
max_parallelism = 2
fail_fast = true

[[service]]
name = "api"
version = "1.0.0"
path = "api"
[service.build]
path = "./services/api"
include = ["src/**"]
build_command = "docker build api"

[[service]]
name = "worker"
version = "1.0.0"

[[environment_variables]]
name = "API_URL"
value = "https://base.example.com"

[[environment_variables]]
name = "SHARED"
value = "base"
"#;
        let child = r#"
schema_version = "0.5.0"
extends = "base"
domain = "child.example.com"
default_replicas = 3

[build]
fail_fast = false

[[service]]
name = "api"
version = "2.0.0"
[service.build]
push_command = "docker push api"

[[service]]
name = "web"
version = "latest"

[[environment_variables]]
name = "API_URL"
value = "https://child.example.com"

[[environment_variables]]
name = "NEW_VAR"
value = "enabled"
"#;

        let env = load_environment_from_sources(
            "child",
            BTreeMap::from([("base", base), ("child", child)]),
        )
        .unwrap();

        assert_eq!(env.name, "child");
        assert_eq!(env.domain, "child.example.com");
        assert_eq!(env.default_replicas, 3);
        assert_eq!(env.registry.prefix().unwrap(), "docker.io/base");
        assert_eq!(env.platform.as_deref(), Some("linux/amd64"));
        assert_eq!(env.build.as_ref().unwrap().max_parallelism, Some(2));
        assert_eq!(env.build.as_ref().unwrap().fail_fast, Some(false));

        let api = env.get_service("api").unwrap();
        assert_eq!(api.version, "2.0.0");
        assert_eq!(api.template_path.as_deref(), Some("api"));
        let api_build = api.build.as_ref().unwrap();
        assert_eq!(api_build.path, "./services/api");
        assert_eq!(api_build.include, Some(vec!["src/**".to_string()]));
        assert_eq!(api_build.build_command.as_deref(), Some("docker build api"));
        assert_eq!(api_build.push_command.as_deref(), Some("docker push api"));

        assert!(env.get_service("worker").is_some());
        assert!(env.get_service("web").is_some());
        assert_eq!(env_var_value(&env, "API_URL"), "https://child.example.com");
        assert_eq!(env_var_value(&env, "SHARED"), "base");
        assert_eq!(env_var_value(&env, "NEW_VAR"), "enabled");
    }

    #[test]
    fn test_environment_extends_supports_chains() {
        let base = r#"
schema_version = "0.5.0"
name = "base"
log_level = "INFO"
domain = "base.example.com"
default_replicas = 1
registry = "docker.io/base"
"#;
        let staging = r#"
schema_version = "0.5.0"
extends = "base"
registry = "docker.io/staging"
"#;
        let prod = r#"
schema_version = "0.5.0"
extends = "staging"
domain = "prod.example.com"
"#;

        let env = load_environment_from_sources(
            "prod",
            BTreeMap::from([("base", base), ("staging", staging), ("prod", prod)]),
        )
        .unwrap();

        assert_eq!(env.name, "prod");
        assert_eq!(env.domain, "prod.example.com");
        assert_eq!(env.registry.prefix().unwrap(), "docker.io/staging");
    }

    #[test]
    fn test_environment_extends_reports_missing_base() {
        let child = r#"
schema_version = "0.5.0"
extends = "missing"
domain = "child.example.com"
"#;

        let err =
            load_environment_from_sources("child", BTreeMap::from([("child", child)])).unwrap_err();
        assert!(err.to_string().contains("missing environment missing"));
    }

    #[test]
    fn test_environment_extends_detects_cycles() {
        let a = r#"
schema_version = "0.5.0"
extends = "b"
"#;
        let b = r#"
schema_version = "0.5.0"
extends = "a"
"#;

        let err =
            load_environment_from_sources("a", BTreeMap::from([("a", a), ("b", b)])).unwrap_err();
        assert!(err
            .to_string()
            .contains("Environment inheritance cycle detected: a -> b -> a"));
    }

    #[test]
    fn test_environment_extends_requires_resolved_v05_schema() {
        let base = r#"
schema_version = "0.4.0"
name = "base"
log_level = "INFO"
domain = "base.example.com"
default_replicas = 1
registry = "docker.io/base"
"#;
        let child = r#"
extends = "base"
domain = "child.example.com"
"#;

        let err = load_environment_from_sources(
            "child",
            BTreeMap::from([("base", base), ("child", child)]),
        )
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("must resolve to schema_version = \"0.5.0\""));
    }

    #[test]
    fn test_bump_helpers_append_minimal_inherited_service_override() {
        let child = r#"
schema_version = "0.5.0"
extends = "base"
domain = "child.example.com"
"#;

        assert!(
            Environment::update_local_service_version_contents(child, "api", "2.0.0")
                .unwrap()
                .is_none()
        );

        let updated =
            Environment::append_service_version_override_contents(child, "api", "2.0.0").unwrap();
        assert!(updated.contains("[[service]]"));
        assert!(updated.contains("name = \"api\""));
        assert!(updated.contains("version = \"2.0.0\""));

        let bumped = Environment::update_local_service_version_contents(&updated, "api", "3.0.0")
            .unwrap()
            .unwrap();
        assert!(bumped.contains("version = \"3.0.0\""));
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

        let raw = toml::from_str::<Value>(content).unwrap();
        let legacy_build = Environment::extract_legacy_build(&raw).unwrap();
        let mut env: Environment = toml::from_str(content).unwrap();
        env.apply_legacy_build_fallback(legacy_build);

        let api = env.get_service("api").unwrap();
        let build = api.build.as_ref().unwrap();
        assert_eq!(build.path, "./services/api");
        assert_eq!(build.build_command, Some("custom-build".to_string()));
        assert_eq!(build.push_command, Some("custom-push".to_string()));
    }

    #[test]
    fn test_legacy_build_rooms_merge_with_existing_service_build() {
        let content = r#"
schema_version = "0.3.0"
name = "edge"
log_level = "INFO"
domain = "example.com"
default_replicas = 1
registry = "docker.io"

[[service_whitelist]]
name = "kernel"
version = "edge-latest"
build = "./services/core/kernel"

[build.rooms.kernel]
path = "./services/core/kernel"
before = "docker buildx build -t kernel:edge-latest ."
after = "docker push kernel:edge-latest"
"#;

        let raw = toml::from_str::<Value>(content).unwrap();
        let legacy_build = Environment::extract_legacy_build(&raw).unwrap();
        let mut env: Environment = toml::from_str(content).unwrap();
        env.apply_legacy_build_fallback(legacy_build);

        let kernel = env.get_service("kernel").unwrap();
        let build = kernel.build.as_ref().unwrap();
        assert_eq!(build.path, "./services/core/kernel");
        assert_eq!(
            build.build_command,
            Some("docker buildx build -t kernel:edge-latest .".to_string())
        );
        assert_eq!(
            build.push_command,
            Some("docker push kernel:edge-latest".to_string())
        );
    }

    #[test]
    fn test_legacy_build_rooms_preserve_before_synchronous_and_before() {
        let content = r#"
schema_version = "0.2.0"
name = "edge"
log_level = "TRACE"
domain = "example.com"
default_replicas = 1
registry = "docker.io"

[[service_whitelist]]
path = "services/skyfleetv2-portal"
version = "edge-latest"
name = "skyfleetv2-portal"
build = "./services/extensions/skyfleetv2-portal"

[build.rooms.skyfleetv2-portal]
path = "./services/extensions/skyfleetv2-portal"
beforeSynchronous = "pnpm i && rm -rf ./dist && pnpm build"
before = "docker buildx build -t portal:edge-latest ."
after = "docker push portal:edge-latest"
"#;

        let migrated = Environment::migrate_contents_to_v04(content).unwrap();
        let env: Environment = toml::from_str(&migrated).unwrap();

        let portal = env.get_service("skyfleetv2-portal").unwrap();
        assert_eq!(
            portal.template_path.as_deref(),
            Some("services/skyfleetv2-portal")
        );
        assert_eq!(
            portal.build.as_ref().unwrap().before_synchronous,
            Some(CommandSpec::Single(
                "pnpm i && rm -rf ./dist && pnpm build".to_string()
            ))
        );
        assert_eq!(
            portal.build.as_ref().unwrap().build_command,
            Some("docker buildx build -t portal:edge-latest .".to_string())
        );
    }

    #[test]
    fn test_migrate_contents_to_v04_from_legacy_schema() {
        let content = r#"
schema_version = "0.3.0"
name = "edge"
log_level = "TRACE"
domain = "example.com"
default_replicas = 1
registry = "docker.io"
platform = "linux/amd64,linux/arm64"

[[service_whitelist]]
path = "./core/kernel"
name = "kernel"
version = "edge-latest"
build = "./services/core/kernel"

[build.rooms.kernel]
path = "./services/core/kernel"
before = "docker buildx build --platform linux/amd64,linux/arm64 -t docker.io/kernel:edge-latest ."
after = "docker push docker.io/kernel:edge-latest"
"#;

        let migrated = Environment::migrate_contents_to_v04(content).unwrap();
        assert!(migrated.contains("schema_version = \"0.4.0\""));
        assert!(!migrated.contains("service_whitelist"));
        assert!(!migrated.contains("[build.rooms"));
        assert!(migrated.contains("path = \"./core/kernel\""));

        let env: Environment = toml::from_str(&migrated).unwrap();
        assert_eq!(env.schema_version, "0.4.0");
        assert_eq!(env.platform.as_deref(), Some("linux/amd64,linux/arm64"));

        let kernel = env.get_service("kernel").unwrap();
        assert_eq!(kernel.template_path.as_deref(), Some("./core/kernel"));
        assert_eq!(kernel.get_path(), "core/kernel");
        let build = kernel.build.as_ref().unwrap();
        assert_eq!(build.path, "./services/core/kernel");
        assert!(build.build_command.is_some());
        assert!(build.push_command.is_some());
    }

    #[test]
    fn test_migrate_contents_to_v05_from_v04_schema() {
        let content = r#"
schema_version = "0.4.0"
name = "dev"
log_level = "INFO"
domain = "example.com"
default_replicas = 1
registry = "docker.io"

[[service]]
name = "api"
version = "latest"
[service.build]
path = "./services/api"
before = "docker buildx build -t api:latest ."
after = "docker push api:latest"
"#;

        let migrated = Environment::migrate_contents_to_v05(content).unwrap();
        assert!(migrated.contains("schema_version = \"0.5.0\""));
        assert!(migrated.contains("build_command = \"docker buildx build -t api:latest .\""));
        assert!(migrated.contains("push_command = \"docker push api:latest\""));

        let env: Environment = toml::from_str(&migrated).unwrap();
        let api = env.get_service("api").unwrap();
        let build = api.build.as_ref().unwrap();
        assert_eq!(
            build.build_command.as_deref(),
            Some("docker buildx build -t api:latest .")
        );
        assert_eq!(
            build.push_command.as_deref(),
            Some("docker push api:latest")
        );
    }

    #[test]
    fn test_migrate_preserves_template_paths_for_non_build_services() {
        let content = r#"
schema_version = "0.2.0"
name = "edge"
log_level = "TRACE"
domain = "example.com"
default_replicas = 1
registry = "docker.io"

[[service_whitelist]]
path = "valkey"
version = "latest"
name = "valkey"

[[service_whitelist]]
path = "aux/postgres"
version = "latest"
name = "postgres"
"#;

        let migrated = Environment::migrate_contents_to_v04(content).unwrap();
        assert!(migrated.contains("path = \"valkey\""));
        assert!(migrated.contains("path = \"aux/postgres\""));

        let env: Environment = toml::from_str(&migrated).unwrap();
        assert_eq!(
            env.get_service("valkey").unwrap().template_path.as_deref(),
            Some("valkey")
        );
        assert_eq!(
            env.get_service("postgres")
                .unwrap()
                .template_path
                .as_deref(),
            Some("aux/postgres")
        );
    }
}
