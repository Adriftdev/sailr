use std::path::Path;

use serde;
use serde::ser::Serialize;
use serde::{Deserialize, Deserializer, Serializer};
use toml::Value;

use crate::filesystem;
use crate::utils::get_current_timestamp;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Environment {
    pub schema_version: String,
    pub name: String,
    pub log_level: String,
    pub service_whitelist: Vec<Service>,
    pub domain: String,
    pub default_replicas: u32,
    pub registry: String,
    pub environment_variables: Option<Vec<EnvironmentVariable>>,
    #[serde(skip)]
    file_manager: filesystem::FileSystemManager,
}

impl Environment {
    // Creates a new Sailr environment instance for the specified name.
    // This searches for an environment configuration file at `./k8s/environments/<name>/config.toml`.
    // Default values are used for properties like log level, domain, and default replicas unless overridden in the config file.
    // A `FileSystemManager` is used to manage access and manipulation of environment configuration files.
    pub fn new(name: &str) -> Self {
        Self {
            schema_version: "0.1.0".to_string(),
            name: name.to_string(),
            log_level: "INFO".to_string(),
            service_whitelist: Vec::new(),
            domain: "localhost".to_string(),
            default_replicas: 1,
            registry: "docker.io".to_string(),
            environment_variables: Some(Vec::new()),
            file_manager: filesystem::FileSystemManager::new(
                Path::new("./k8s/environments")
                    .join(name)
                    .to_str()
                    .unwrap()
                    .to_string(),
            ),
        }
    }

    pub fn get_service(&self, name: &str) -> Option<&Service> {
        self.service_whitelist.iter().find(|s| s.name == name)
    }

    // Returns a list of services in the environment.
    pub fn list_services(&self) -> Vec<&Service> {
        self.service_whitelist.iter().collect()
    }

    // Adds a service to the environment.
    pub fn add_service(&mut self, service: Service) {
        self.service_whitelist.push(service);
    }

    // Removes a service from the environment.
    pub fn remove_service(&mut self, name: &str) {
        self.service_whitelist.retain(|s| s.name != name);
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

    // Loads the environment configuration from the `./k8s/environments/<name>/config.toml` file, overriding default values set in the constructor.
    // An error is returned if the file is missing, cannot be read, or contains an incompatible schema version.
    pub fn load_from_file(name: &String) -> Result<Self, Box<dyn std::error::Error>> {
        let filemanager = filesystem::FileSystemManager::new(
            Path::new("./k8s/environments")
                .join(name)
                .to_str()
                .unwrap()
                .to_string(),
        );

        let contents = filemanager.read_file(&"config.toml".to_string())?;
        let env = toml::from_str::<Self>(&contents)?; // Use destructuring assignment

        if env.schema_version != "0.1.0" {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!(
                    "Invalid schema version: expected {}, found {}",
                    "0.1.0", env.schema_version
                ),
            )));
        }

        Ok(env)
    }

    pub fn save_to_file(&self) -> Result<(), Box<dyn std::error::Error>> {
        let contents = toml::to_string(&self)?;
        self.file_manager
            .create_file(&"config.toml".to_string(), &contents)?;
        Ok(())
    }

    pub fn get_variables(&self, service: &Service) -> Vec<(String, String)> {
        let mut variables = Vec::new();
        variables.push(("name".to_string(), self.name.clone()));
        variables.push(("log_level".to_string(), self.log_level.clone()));
        variables.push(("domain".to_string(), self.domain.clone()));

        variables.push(("deployment_date".to_string(), get_current_timestamp()));

        variables.push((
            "default_replicas".to_string(),
            self.default_replicas.to_string(),
        ));
        variables.push(("registry".to_string(), self.registry.clone()));
        variables.push(("schema_version".to_string(), self.schema_version.clone()));

        variables.push(("service_name".to_string(), service.name.clone()));

        variables.push(("service_namespace".to_string(), service.namespace.clone()));

        if let Some(path) = &service.path {
            variables.push(("service_path".to_string(), path.clone()));
        }

        variables.push(("service_version".to_string(), service.get_version()));

        if let Some(env_vars) = &self.environment_variables {
            env_vars.iter().for_each(|e| {
                variables.push((
                    e.name.clone(),
                    e.value
                        .clone()
                        .unwrap_or(Value::String("".to_string()))
                        .to_string(),
                ))
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

#[derive(Debug, Clone)]
pub struct Service {
    pub name: String,
    pub namespace: String,
    pub path: Option<String>,
    pub major_version: Option<i32>,
    pub minor_version: Option<i32>,
    pub patch_version: Option<i32>,
    pub tag: Option<String>,
}

impl Service {
    pub fn new(
        name: &str,
        namespace: &str,
        path: Option<&str>,
        major_version: Option<i32>,
        minor_version: Option<i32>,
        patch_version: Option<i32>,
        tag: Option<String>,
    ) -> Self {
        Self {
            name: name.to_string(),
            namespace: namespace.to_string(),
            path: path.map(|p| p.to_string()),
            major_version,
            minor_version,
            patch_version,
            tag,
        }
    }
}

impl Serialize for Service {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut service = std::collections::HashMap::new();
        service.insert(
            "name".to_string(),
            format!("{}/{}", self.namespace, self.name),
        );
        if self.path.is_some() {
            service.insert("path".to_string(), self.path.clone().unwrap());
        }

        if self.tag.is_none()
            && self.major_version.is_none()
            && self.minor_version.is_none()
            && self.patch_version.is_none()
        {
            service.insert("version".to_string(), "latest".to_string());
        } else if self.major_version.is_none()
            || self.minor_version.is_none()
            || self.patch_version.is_none()
        {
            service.insert("version".to_string(), self.tag.clone().unwrap());
        } else if self.tag.is_some() {
            service.insert(
                "version".to_string(),
                format!(
                    "{}.{}.{}-{}",
                    self.major_version.unwrap(),
                    self.minor_version.unwrap(),
                    self.patch_version.unwrap(),
                    self.tag.as_ref().unwrap()
                ),
            );
        } else {
            service.insert(
                "version".to_string(),
                format!(
                    "{}.{}.{}",
                    self.major_version.unwrap(),
                    self.minor_version.unwrap(),
                    self.patch_version.unwrap()
                ),
            );
        }
        service.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Service {
    // I know know this is not using typical serde deserialization,
    // but I couldn't be bother reading the docs. This works, so I'm happy.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let service: std::collections::HashMap<String, String> =
            match serde::Deserialize::deserialize(deserializer) {
                Ok(service) => service,
                Err(e) => {
                    println!("Error deserializing service: {}", e);
                    return Err(e);
                }
            };

        let name = service.get("name").unwrap();
        let path = match service.get("path") {
            Some(path) => path,
            None => "",
        };
        let version = service.get("version").unwrap();

        let mut name = name.split('/');
        let namespace = name.next().unwrap();
        let name = name.next().unwrap();
        if !version.contains('-') && !version.contains('.') {
            let tag = version.to_string();
            return Ok(Self::new(
                name,
                namespace,
                Some(path),
                None,
                None,
                None,
                Some(tag),
            ));
        }
        if version.contains('-') {
            let mut version_tag_split = version.split('-');
            let tag = version_tag_split.next().unwrap().to_string();
            let mut version = version.split('-').next().unwrap().split('.');

            let major_version: Option<i32> = match version.next() {
                Some(major_version) => match major_version.parse::<i32>() {
                    Ok(major_version) => Some(major_version),
                    Err(_) => None,
                },
                None => None,
            };
            let minor_version: Option<i32> = match version.next() {
                Some(minor_version) => match minor_version.parse::<i32>() {
                    Ok(minor_version) => Some(minor_version),
                    Err(_) => None,
                },
                None => None,
            };

            let patch_version: Option<i32> = match version.next() {
                Some(patch_version) => match patch_version.parse::<i32>() {
                    Ok(patch_version) => Some(patch_version),
                    Err(_) => None,
                },
                None => None,
            };

            Ok(Self {
                name: name.to_string(),
                namespace: namespace.to_string(),
                path: Some(path.to_string()),
                major_version,
                minor_version,
                patch_version,
                tag: Some(tag),
            })
        } else {
            let mut version = version.split('.');
            let major_version: Option<i32> = match version.next() {
                Some(major_version) => match major_version.parse::<i32>() {
                    Ok(major_version) => Some(major_version),
                    Err(_) => None,
                },
                None => None,
            };
            let minor_version: Option<i32> = match version.next() {
                Some(minor_version) => match minor_version.parse::<i32>() {
                    Ok(minor_version) => Some(minor_version),
                    Err(_) => None,
                },
                None => None,
            };

            let patch_version: Option<i32> = match version.next() {
                Some(patch_version) => match patch_version.parse::<i32>() {
                    Ok(patch_version) => Some(patch_version),
                    Err(_) => None,
                },
                None => None,
            };

            let tag = match version.next() {
                Some(tag) => Some(tag.to_string()),
                None => None,
            };
            Ok(Self {
                name: name.to_string(),
                namespace: namespace.to_string(),
                path: Some(path.to_string()),
                major_version,
                minor_version,
                patch_version,
                tag,
            })
        }
    }
}

impl PartialEq for Service {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.namespace == other.namespace
            && self.path == other.path
            && self.major_version == other.major_version
            && self.minor_version == other.minor_version
            && self.patch_version == other.patch_version
            && self.tag == other.tag
    }
}

impl Service {
    pub fn get_version(&self) -> String {
        if None == self.major_version && None == self.minor_version && None == self.patch_version {
            return self.tag.as_ref().unwrap().to_string();
        }

        if let Some(tag) = &self.tag {
            format!(
                "{}.{}.{}-{}",
                match self.major_version {
                    Some(major_version) => major_version.to_string(),
                    None => "0".to_string(),
                },
                match self.minor_version {
                    Some(minor_version) => minor_version.to_string(),
                    None => "0".to_string(),
                },
                match self.patch_version {
                    Some(patch_version) => patch_version.to_string(),
                    None => "0".to_string(),
                },
                tag
            )
        } else {
            format!(
                "{}.{}.{}",
                self.major_version.unwrap_or(0),
                self.minor_version.unwrap_or(0),
                self.patch_version.unwrap_or(0)
            )
        }
    }

    pub fn get_version_without_tag(&self) -> String {
        format!(
            "{}.{}.{}",
            self.major_version.unwrap(),
            self.minor_version.unwrap(),
            self.patch_version.unwrap()
        )
    }

    pub fn get_path(&self) -> String {
        self.path.clone().unwrap_or("".to_string())
    }

    pub fn get_full_name(&self) -> String {
        format!("{}/{}", self.namespace, self.name)
    }

    pub fn get_full_name_with_version(&self) -> String {
        format!("{}/{}:{}", self.namespace, self.name, self.get_version())
    }

    pub fn get_full_name_with_path(&self) -> String {
        format!("{}/{}:{}", self.namespace, self.name, self.get_path())
    }

    pub fn bump_major_version(&mut self) {
        if let Some(major_version) = self.major_version {
            self.major_version = Some(major_version + 1);
        } else {
            self.major_version = Some(1);
        }
    }

    pub fn bump_minor_version(&mut self) {
        if let Some(minor_version) = self.minor_version {
            self.minor_version = Some(minor_version + 1);
        } else {
            self.minor_version = Some(1);
        }
    }

    pub fn bump_patch_version(&mut self) {
        if let Some(patch_version) = self.patch_version {
            self.patch_version = Some(patch_version + 1);
        } else {
            self.patch_version = Some(1);
        }
    }

    pub fn set_tag(&mut self, tag: String) {
        self.tag = Some(tag);
    }
}
