use std::{collections::BTreeMap, path::Path, sync::Arc};

use environment::Environment;
use filesystem::FileSystemManager;
use generate::Generator;
use infra::Infra;
use serde::Deserialize;
use templates::TemplateManager;
use utils::replace_variables;

use once_cell::sync::Lazy;
use scribe_rust::{self, Logger};

pub mod builder;
pub mod cli;
pub mod config;
pub mod deployment;
pub mod environment;
pub mod errors;
pub mod filesystem;
pub mod generate;
pub mod infra;
pub mod provider;
pub mod roomservice;
pub mod templates;
pub mod utils;

pub static LOGGER: Lazy<Arc<Logger>> = Lazy::new(|| Logger::default());

#[derive(Debug, Deserialize)]
pub struct GlobalVars {
    pub default_registry: Option<String>,
    pub default_domain: Option<String>,
    pub default_config_template: Option<String>,
    pub custom_vars: Option<BTreeMap<String, String>>,
}

pub fn load_global_vars() -> Result<BTreeMap<String, String>, Box<dyn std::error::Error>> {
    let filemanager =
        filesystem::FileSystemManager::new(Path::new("./k8s").to_str().unwrap().to_string());

    let contents = filemanager.read_file(&"default.toml".to_string(), None)?;
    let global_vars = toml::from_str::<GlobalVars>(&contents)?; // Use destructuring assignment

    let mut vars = BTreeMap::new();

    if let Some(default_registry) = global_vars.default_registry {
        vars.insert("default_registry".to_string(), default_registry);
    }

    if let Some(default_domain) = global_vars.default_domain {
        vars.insert("default_domain".to_string(), default_domain);
    }

    if let Some(default_config_template) = global_vars.default_config_template {
        vars.insert(
            "default_config_template".to_string(),
            default_config_template,
        );
    }

    if let Some(custom_vars) = global_vars.custom_vars {
        for (key, value) in custom_vars {
            vars.insert(key, value);
        }
    }

    Ok(vars)
}

pub fn generate(name: &str, env: &Environment) {
    let mut template_manager = TemplateManager::new();
    let (templates, config_maps) = &template_manager.read_templates(Some(&env)).unwrap();

    let services = env.list_services();

    let mut generator = Generator::new();

    for service in services {
        let variables = &env.get_variables(service);
        for template in templates {
            if template.name != service.name
                && template.name != service.path.clone().unwrap_or("".to_string())
            {
                continue;
            }
            let content = template_manager
                .replace_variables(template, &variables)
                .unwrap();

            generator.add_template(&template, content)
        }
        for config in config_maps {
            if config.name != service.name {
                continue;
            }

            generator.add_config_map(config);
        }
    }
    let _ = generator.generate(&name.to_string());
}

pub fn create_default_env_config(
    name: String,
    config_template: Option<String>,
    registry: Option<String>,
) {
    let mut vars = load_global_vars().unwrap();

    if vars.len() == 0 {
        vars.insert("default_registry".to_string(), "docker.io".to_string());
        vars.insert("default_domain".to_string(), "example.com".to_string());
    }

    vars.insert("name".to_string(), name.clone());

    if registry.is_some() {
        vars.insert("default_registry".to_string(), registry.unwrap());
    }

    let file_manager = FileSystemManager::new("./k8s/environments".to_string());

    if let Some(config) = vars
        .clone()
        .into_iter()
        .find(|v| v.0 == "default_config_template")
    {
        let content = file_manager
            .read_file(&config.1, Some(&"".to_string()))
            .unwrap();

        let generated_config = replace_variables(content.clone(), vars);

        file_manager
            .create_file(
                &std::path::Path::new(&name)
                    .join("config.toml")
                    .to_str()
                    .unwrap()
                    .to_string(),
                &generated_config,
            )
            .unwrap();
        return;
    } else if let Some(config_template) = config_template {
        let content = file_manager
            .read_file(&config_template.clone(), Some(&"".to_string()))
            .unwrap();

        let generated_config = replace_variables(content.clone(), vars);

        file_manager
            .create_file(
                &std::path::Path::new(&name)
                    .join("config.toml")
                    .to_str()
                    .unwrap()
                    .to_string(),
                &generated_config,
            )
            .unwrap();
        return;
    } else {
        let default_env_config = (
            "config.toml".to_string(),
            include_str!("default_config.toml").to_string(),
        );
        let generated_config = replace_variables(default_env_config.1, vars);

        file_manager
            .create_file(
                &std::path::Path::new(&name)
                    .join(default_env_config.0)
                    .to_str()
                    .unwrap()
                    .to_string(),
                &generated_config,
            )
            .unwrap();
    }
}

pub fn create_default_env_infra(
    name: String,
    infra_template: Option<String>,
    registry: Option<String>,
) {
    let mut vars = load_global_vars().unwrap();

    if vars.len() == 0 {
        vars.insert("default_registry".to_string(), "docker.io".to_string());
        vars.insert("default_domain".to_string(), "example.com".to_string());
    }

    vars.insert("name".to_string(), name.clone());

    if registry.is_some() {
        println!("Registry: {:?}", registry);
        vars.insert("default_registry".to_string(), registry.unwrap());
    }

    println!("Vars: {:?}", vars);

    if let Some(config_template) = infra_template {
        Infra::use_template(&name, &config_template, &mut vars);
        return;
    }
}
