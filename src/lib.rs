use std::{collections::BTreeMap, path::Path};

use environment::{Environment, Service};
use filesystem::FileSystemManager;
use generate::Generator;
use infra::Infra;
use serde::Deserialize;
use templates::TemplateManager;
use utils::replace_variables;

use once_cell::sync::Lazy;

pub mod builder;
pub mod cli;
pub mod config;
pub mod deployment;
pub mod environment;
pub mod errors;
pub mod filesystem;
pub mod generate;
pub mod infra;
pub mod interactive;
pub mod oci;
pub mod orchestrator;
pub mod plan;
pub mod provider;
pub mod roomservice;
pub mod templates;
pub mod tui;
pub mod ui;
pub mod utils;
pub mod workflow;

pub static LOGGER: Lazy<ui::SailrUI> = Lazy::new(|| ui::SailrUI::new(false, false));
pub const RUNKERNEL_CACHE_ROOT: &str = ".sailr/cache/runkernel";

pub(crate) fn new_runkernel_pipeline(name: impl Into<String>) -> runkernel::Pipeline {
    runkernel::Pipeline::new(name).cache_root(RUNKERNEL_CACHE_ROOT)
}

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

    if !filemanager.file_exists(&"default.toml".to_string()) {
        return Ok(BTreeMap::new());
    }

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

pub fn generate(name: &str, env: &Environment, services: Vec<&Service>) -> anyhow::Result<()> {
    generate_with_context(name, env, services, &GenerationContext::default())
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct GenerationContext {
    pub service_images: std::collections::BTreeMap<String, String>,
    pub deployment_date: Option<String>,
    pub default_namespace: Option<String>,
}

pub fn generate_with_image_overrides(
    name: &str,
    env: &Environment,
    services: Vec<&Service>,
    image_overrides: &std::collections::BTreeMap<String, String>,
) -> anyhow::Result<()> {
    generate_with_context(
        name,
        env,
        services,
        &GenerationContext {
            service_images: image_overrides.clone(),
            deployment_date: None,
            default_namespace: None,
        },
    )
}

pub fn generate_with_context(
    name: &str,
    env: &Environment,
    services: Vec<&Service>,
    context: &GenerationContext,
) -> anyhow::Result<()> {
    let services_needing_images = services
        .iter()
        .copied()
        .filter(|service| {
            service.build.is_some()
                && !service.has_explicit_version()
                && !context.service_images.contains_key(&service.name)
        })
        .collect::<Vec<_>>();
    let mut resolved_images = context.service_images.clone();
    if !services_needing_images.is_empty() {
        resolved_images.extend(
            crate::builder::generation_image_overrides(env, &services_needing_images)
                .map_err(anyhow::Error::msg)?,
        );
    }

    for service in services
        .iter()
        .filter(|service| service.build.is_none() && !service.has_explicit_version())
    {
        LOGGER.warn(&format!(
            "External service '{}' has no explicit version; {{service_version}} and {{service_image}} use the legacy 'latest' fallback",
            service.name
        ));
    }

    let mut template_manager = TemplateManager::new();
    let (templates, config_maps) = template_manager
        .read_templates(Some(env))
        .map_err(|e| anyhow::anyhow!("Failed to read templates: {:?}", e))?;

    let mut generator = Generator::new();

    for service in services {
        let variables = &env
            .get_variables_with_context_overrides(
                service,
                resolved_images.get(&service.name).map(String::as_str),
                context.deployment_date.as_deref(),
                context.default_namespace.as_deref(),
            )
            .map_err(|e| anyhow::anyhow!("Registry config error: {}", e))?;
        for template in &templates {
            if template.name != service.name && template.name != service.get_path() {
                continue;
            }
            let content = template_manager
                .replace_variables(template, variables)
                .map_err(|e| anyhow::anyhow!("Failed to replace variables: {:?}", e))?;

            generator.add_template(template, content)
        }
        for config in &config_maps {
            if config.name.split("/").last().unwrap() != service.name {
                continue;
            }

            generator.add_config_map(config);
        }
    }
    generator
        .generate(&name.to_string())
        .map_err(|e| anyhow::anyhow!("Failed to generate templates: {:?}", e))?;
    Ok(())
}

pub fn create_default_env_config(
    name: String,
    config_template: Option<String>,
    registry: Option<String>,
    engine: Option<environment::BuildEngine>,
) {
    let mut vars = load_global_vars().unwrap();

    if vars.is_empty() {
        vars.insert("default_registry".to_string(), "docker.io".to_string());
        vars.insert("default_domain".to_string(), "example.com".to_string());
    }

    vars.insert("name".to_string(), name.clone());

    if let Some(r) = registry {
        vars.insert("default_registry".to_string(), r);
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

        let generated_config =
            configure_build_engine(&replace_variables(content.clone(), vars), engine);

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
    } else if let Some(config_template) = config_template {
        let content = file_manager
            .read_file(&config_template.clone(), Some(&"".to_string()))
            .unwrap();

        let generated_config =
            configure_build_engine(&replace_variables(content.clone(), vars), engine);

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
    } else {
        let default_env_config = (
            "config.toml".to_string(),
            include_str!("default_config.toml").to_string(),
        );
        let generated_config =
            configure_build_engine(&replace_variables(default_env_config.1, vars), engine);

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

fn configure_build_engine(contents: &str, engine: Option<environment::BuildEngine>) -> String {
    let Some(engine) = engine else {
        return contents.to_string();
    };
    let mut document = contents
        .parse::<toml_edit::DocumentMut>()
        .expect("generated environment config must be valid TOML");
    document["build"]["engine"] = toml_edit::value(match engine {
        environment::BuildEngine::Roomservice => "roomservice",
        environment::BuildEngine::Runkernel => "runkernel",
    });
    document.to_string()
}

pub fn create_default_env_infra(
    name: String,
    infra_template: Option<String>,
    registry: Option<String>,
) {
    let mut vars = load_global_vars().unwrap();

    if vars.is_empty() {
        vars.insert("default_registry".to_string(), "docker.io".to_string());
        vars.insert("default_domain".to_string(), "example.com".to_string());
    }

    vars.insert("name".to_string(), name.clone());

    if let Some(r) = registry {
        vars.insert("default_registry".to_string(), r);
    }

    if let Some(config_template) = infra_template {
        Infra::use_template(&name, &config_template, &mut vars);
    }
}

#[cfg(test)]
mod runkernel_cache_contract_tests {
    #[test]
    fn sailr_pipelines_keep_runkernel_cache_under_sailr() {
        let pipeline = super::new_runkernel_pipeline("cache-contract");
        assert_eq!(
            pipeline.cache_root,
            std::path::PathBuf::from(super::RUNKERNEL_CACHE_ROOT)
        );
        assert!(super::RUNKERNEL_CACHE_ROOT.starts_with(".sailr/"));
        assert!(!super::RUNKERNEL_CACHE_ROOT.starts_with(".runkernel/"));
    }
}
