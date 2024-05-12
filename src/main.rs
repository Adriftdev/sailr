use std::{io, sync::Arc};

use sailr::{
    builder::{split_matches, Builder},
    cli::{Cli, Commands, InfraCommands, Provider},
    create_default_env_config, create_default_env_infra,
    environment::Environment,
    errors::CliError,
    generate,
    infra::{aws_eks::AwsEks, local_k8s::LocalK8, Infra},
    templates::TemplateManager,
    LOGGER,
};

use anyhow::Result;

use clap::{CommandFactory, Parser};

#[tokio::main]
async fn main() -> Result<(), CliError> {
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    match cli.commands {
        Commands::Init(arg) => {
            TemplateManager::new().copy_base_templates().unwrap();

            create_default_env_config(
                arg.name.clone(),
                arg.config_template_path,
                arg.default_registry.clone(),
            );

            if let Some(template_path) = arg.infra_template_path {
                create_default_env_infra(arg.name, Some(template_path), arg.default_registry)
            } else if let Some(provider) = arg.provider {
                let infra = match provider {
                    Provider::Local => Infra::new(Box::new(LocalK8::new(arg.name.clone()))),
                    Provider::Aws => Infra::new(Box::new(AwsEks::new(
                        arg.name.clone(),
                        arg.region.unwrap_or("eu-west-2".to_string()),
                    ))),
                    _ => {
                        LOGGER.error(&format!("Provider {:?} not supported", provider));
                        std::process::exit(1);
                    }
                };
                infra.generate(Infra::read_config(arg.name.clone()));
                infra.build(Infra::read_config(arg.name.clone()));
            } else {
                let infra = Infra::new(Box::new(LocalK8::new(arg.name.clone())));
                infra.generate(Infra::read_config(arg.name.clone()));
                infra.build(Infra::read_config(arg.name.clone()));
            }
        }
        Commands::Completions(arg) => {
            clap_complete::generate(arg.shell, &mut Cli::command(), "sailr", &mut io::stdout());
        }
        Commands::Infra(a) => match a.command {
            InfraCommands::Create(arg) => {
                LOGGER.info(&format!("Creating a new environment"));
                if let Some(template_path) = arg.infra_template_path {
                    create_default_env_infra(arg.name, Some(template_path), arg.default_registry);
                } else if let Some(provider) = arg.provider {
                    let infra = match provider {
                        Provider::Local => Infra::new(Box::new(LocalK8::new(arg.name.clone()))),
                        Provider::Aws => Infra::new(Box::new(AwsEks::new(
                            arg.name.clone(),
                            arg.region.unwrap_or("eu-west-2".to_string()),
                        ))),
                        _ => {
                            LOGGER.error(&format!("Provider {:?} not supported", provider));
                            std::process::exit(1);
                        }
                    };
                    infra.generate(Infra::read_config(arg.name.clone()));
                } else {
                    let infra = Infra::new(Box::new(LocalK8::new(arg.name.clone())));
                    infra.generate(Infra::read_config(arg.name.clone()));
                }
            }
            InfraCommands::Apply(arg) => Infra::apply(Infra::read_config(arg.name)),
            InfraCommands::Destroy(arg) => Infra::destroy(Infra::read_config(arg.name)),
        },
        Commands::Deploy(arg) => {
            LOGGER.info(&format!("Deploying an environment"));
            sailr::deployment::deploy(arg.context.to_string(), &arg.name).await?;
        }
        Commands::Generate(arg) => {
            LOGGER.info(&format!("Generating an environment"));

            let env = match Environment::load_from_file(&arg.name) {
                Ok(env) => env,
                Err(e) => {
                    LOGGER.error(&format!("Failed to load environment: {}", e));
                    std::process::exit(1);
                }
            };

            generate(&arg.name, &env);

            LOGGER.info(&format!("Generation Complete"));
        }
        Commands::Build(arg) => {
            let env = match Environment::load_from_file(&arg.name) {
                Ok(env) => env,
                Err(e) => {
                    LOGGER.error(&format!("Failed to load environment: {}", e));
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

            match builder.build(&env) {
                Ok(_) => (),
                Err(e) => {
                    LOGGER.error(&format!("Failed to build environment: {}", e));
                    std::process::exit(1);
                }
            };
        }
        Commands::Go(arg) => {
            LOGGER.info(&format!("Generating and deploying an environment"));

            let env = match Environment::load_from_file(&arg.name) {
                Ok(env) => env,
                Err(e) => {
                    LOGGER.error(&format!("Failed to load environment: {}", e));
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

            match builder.build(&env) {
                Ok(_) => (),
                Err(e) => {
                    LOGGER.error(&format!("Failed to build environment: {}", e));
                    std::process::exit(1);
                }
            };

            generate(&arg.name, &env);

            sailr::deployment::deploy(arg.context.to_string(), &arg.name).await?;
        }
    }

    Ok(())
}
