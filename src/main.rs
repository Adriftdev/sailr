use sailr::{
    cli::{Cli, Commands, EnvCommands},
    errors::CliError,
    generate, provider,
    utils::ensure_dir,
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

            ensure_dir("./k8s/environments")?;
            ensure_dir("./k8s/templates")?;
            ensure_dir("./k8s/generated")?;

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

            sailr::utils::copy_templates()?;
        }
        Commands::Env(a) => match a.command {
            EnvCommands::Create(arg) => {
                logger.info(&format!("Creating a new environment"));
                sailr::utils::create_env_toml(
                    &arg.name, 
                    arg.redis, 
                    arg.postresql, 
                    arg.registry
                )?;
            }
        },
        Commands::Deploy(arg) => {
            logger.info(&format!("Deploying an environment"));
            sailr::deploy::deploy(arg.context.to_string(), &arg.name).await?;
        }
        Commands::Generate(arg) => {
            logger.info(&format!("Generating an environment"));
            generate::generate(&arg.name).await?;
        }
        Commands::Go(arg) => {
            logger.info(&format!("Generating and deploying an environment"));
            generate::generate(&arg.name).await?;
            sailr::deploy::deploy(arg.context.to_string(), &arg.name).await?;
        }
        _ => {}
    }

    Ok(())
}
