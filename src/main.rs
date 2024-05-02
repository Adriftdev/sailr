use std::{io, path::Path};

use sailr::{
    builder::{split_matches, Builder},
    cli::{Cli, Commands, EnvCommands},
    environment::Environment,
    errors::CliError,
    infra::{local_k8s::LocalK8, Infra},
    templates::TemplateManager,
    utils::replace_variables,
};

use anyhow::Result;

use clap::{CommandFactory, Parser};

use scribe_rust;

fn generate(name: &str, env: &Environment) {
    let mut template_manager = TemplateManager::new();
    let (templates, config_maps) = &template_manager.read_templates(Some(&env)).unwrap();

    let services = env.list_services();

    let mut generator = sailr::generate::Generator::new();

    for service in services {
        let variables = &env.get_variables(service);
        for template in templates {
            if template.name != service.name && template.name != service.path.clone().unwrap() {
                println!("Skipping template: {}", template.name);
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

fn create_default_env_config(
    name: String,
    config_template: Option<String>,
    registry: Option<String>,
) {
    if config_template.is_some() {
        let file_manager =
            sailr::filesystem::FileSystemManager::new("./k8s/environments".to_string());

        let content = file_manager
            .read_file(&config_template.clone().unwrap(), Some(&"".to_string()))
            .unwrap();

        let generated_config = replace_variables(
            content.clone(),
            vec![
                ("name".to_string(), name.clone()),
                (
                    "registry".to_string(),
                    registry.unwrap_or("docker.io".to_string()),
                ),
            ],
        );

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

        let file_manager =
            sailr::filesystem::FileSystemManager::new("./k8s/environments".to_string());

        file_manager
            .create_file(
                &std::path::Path::new(&name)
                    .join(default_env_config.0)
                    .to_str()
                    .unwrap()
                    .to_string(),
                &default_env_config.1,
            )
            .unwrap();
    }
}
#[tokio::main]
async fn main() -> Result<(), CliError> {
    dotenvy::dotenv().ok();
    let logger = scribe_rust::Logger::default();

    let cli = Cli::parse();

    match cli.commands {
        Commands::Init(arg) => {
            TemplateManager::new().copy_base_templates().unwrap();
            create_default_env_config(
                arg.name.clone(),
                arg.config_template_path,
                arg.default_registry,
            );
            let infra = Infra::new(Box::new(LocalK8::new(arg.name, 1)));
            infra.generate(infra.read_config("local".to_string()));
        }
        Commands::Completions(arg) => {
            clap_complete::generate(arg.shell, &mut Cli::command(), "sailr", &mut io::stdout());
        }
        Commands::Env(a) => match a.command {
            EnvCommands::Create(arg) => {
                logger.info(&format!("Creating a new environment"));
                sailr::utils::create_env_toml(&arg.name, arg.redis, arg.postresql, arg.registry)?;
            }
        },
        Commands::Deploy(arg) => {
            logger.info(&format!("Deploying an environment"));
            sailr::deployment::deploy(arg.context.to_string(), &arg.name).await?;
        }
        Commands::Generate(arg) => {
            logger.info(&format!("Generating an environment"));

            let env = match Environment::load_from_file(&arg.name) {
                Ok(env) => env,
                Err(e) => {
                    logger.error(&format!("Failed to load environment: {}", e));
                    std::process::exit(1);
                }
            };

            generate(&arg.name, &env);

            logger.info(&format!("Generation Complete"));
        }
        Commands::Build(arg) => {
            let env = match Environment::load_from_file(&arg.name) {
                Ok(env) => env,
                Err(e) => {
                    logger.error(&format!("Failed to load environment: {}", e));
                    std::process::exit(1);
                }
            };

            let services = env
                .list_services()
                .into_iter()
                .filter(|s| s.build.is_some());

            let mut builder = Builder::new(
                ".roomservice".to_string(),
                arg.force.unwrap_or(false),
                services.into_iter().map(|s| s.name.clone()).collect(),
                split_matches(arg.ignore),
            );

            builder.build(&env);
        }
        Commands::Go(arg) => {
            logger.info(&format!("Generating and deploying an environment"));

            let env = match Environment::load_from_file(&arg.name) {
                Ok(env) => env,
                Err(e) => {
                    logger.error(&format!("Failed to load environment: {}", e));
                    std::process::exit(1);
                }
            };

            generate(&arg.name, &env);

            let services = env
                .list_services()
                .into_iter()
                .filter(|s| s.build.is_some());

            let mut builder = Builder::new(
                ".roomservice".to_string(),
                arg.force.unwrap_or(false),
                services.into_iter().map(|s| s.name.clone()).collect(),
                split_matches(arg.ignore),
            );

            builder.build(&env);

            sailr::deployment::deploy(arg.context.to_string(), &arg.name).await?;
        }
    }

    Ok(())
}
