use sailr::{
    cli::{Cli, Commands, EnvCommands},
    environment::Environment,
    errors::CliError,
    generate::Generator,
    provider,
    templates::TemplateManager,
};

use anyhow::Result;

use clap::Parser;

use scribe_rust;

#[tokio::main]
async fn main() -> Result<(), CliError> {
    dotenvy::dotenv().ok();
    let logger = scribe_rust::Logger::default();

    let cli = Cli::parse();

    match cli.commands {
        Commands::Init(arg) => {
            if arg.provider != "aws"
                && arg.provider != "gcp"
                && arg.provider != "docker-desktop"
                && arg.provider != "k3s"
            {
                logger.error(&format!("Invalid provider: {}", arg.provider));
                std::process::exit(1);
            }

            if arg.provider == "aws" {
                logger.info(&format!("Initializing a new Sailr project on AWS"));
                provider::Provider::new(provider::AwsProvider).initialize_project()?;
            } else if arg.provider == "gcp" {
                logger.info(&format!("Initializing a new Sailr project on GCP"));
                provider::Provider::new(provider::GcpProvider).initialize_project()?;
            } else if arg.provider == "docker-desktop" {
                logger.info(&format!(
                    "Initializing a new Sailr project on Docker Desktop"
                ));
                provider::Provider::new(provider::DockerDesktopProvider).initialize_project()?;
            } else if arg.provider == "k3s" {
                logger.info(&format!("Initializing a new Sailr project on k3s"));
                provider::Provider::new(provider::K3SProvider).initialize_project()?;
            }

            TemplateManager::new().copy_base_templates().unwrap();
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

            let mut template_manager = TemplateManager::new();
            let templates = &template_manager.read_templates(Some(&env)).unwrap();

            let services = env.list_services();

            let mut generator = Generator::new();

            for service in services {
                let variables = &env.get_variables(service);
                for template in templates {
                    if template.name != service.name {
                        continue;
                    }
                    let content = template_manager
                        .replace_variables(template, &variables)
                        .unwrap();

                    generator.add_template(&template, content)
                }
            }
            generator.generate(&arg.name)?;
            logger.info(&format!("Generation Complete"));
        }
        Commands::Go(_arg) => {
            logger.info(&format!("Generating and deploying an environment"));
            //generate::generate(&arg.name).await?;
            //sailr::deploy::deploy(arg.context.to_string(), &arg.name).await?;
        }
        _ => {}
    }

    Ok(())
}
