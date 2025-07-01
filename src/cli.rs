use std::io;

use clap::{clap_derive::Args, command, Command, Parser, Subcommand, ValueEnum};
use clap_complete::{generate, Generator, Shell};

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
#[command(
    name = "sailr",
    about = "A CLI tool for managing environments and deployments"
)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub commands: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Initialize a new project
    Init(InitArgs),
    /// Generate shell completions
    Completions(CompletionsArgs),
    /// Manage environments
    #[command(subcommand)]
    Infra(InfraCommands),
    /// Deploy an environment
    Deploy(DeployArgs),
    /// Generate an environment
    Generate(GenerateArgs),
    /// Build related projects
    Build(BuildArgs),
    /// Generate and deploy an environment
    Go(GoArgs),
    /// Add a new service to the project
    AddService(AddServiceArgs),
    /// Enter interactive terminal interface cli mode
    Interactive(InteractiveArgs),
}

#[derive(Debug, Args, Clone)]
pub struct InteractiveArgs {
    /// Kubernetes context to use
    #[arg(
        name = "context",
        short = 'c',
        long = "context",
        help = "Kubernetes context to use"
    )]
    pub context: String,
}

#[derive(Debug, Args)]
pub struct CompletionsArgs {
    #[arg(help = "Shell to generate completions for", value_enum)]
    pub shell: Shell,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    #[arg(
        name = "name",
        short = 'n',
        long = "name",
        help = "Name of the environment"
    )]
    pub name: String,

    #[arg(
        name = "Config Template Path",
        short = 'c',
        long = "config-template",
        help = "sailr config template path to use instead of the default one."
    )]
    pub config_template_path: Option<String>,

    #[arg(
        name = "Default Registry",
        short = 'r',
        long = "registry",
        help = "Default registry to use for images"
    )]
    pub default_registry: Option<String>,

    #[arg(
        help = "Provider to use",
        value_enum,
        short = 'p',
        long = "provider",
        help = "Provider to use"
    )]
    pub provider: Option<Provider>,

    #[arg(
        name = "Infrastructure Template",
        short = 'i',
        long = "infra-templates",
        help = "Template path for infrastruture templates"
    )]
    pub infra_template_path: Option<String>,

    #[arg(
        name = "Region",
        short = 'R',
        long = "region",
        help = "Region to use for the provider"
    )]
    pub region: Option<String>,

    #[arg(
        long = "with-sample",
        help = "Include a sample service for immediate testing (default: true)",
        default_value = "true"
    )]
    pub with_sample: bool,

    #[arg(
        long = "no-sample",
        help = "Skip creating sample service",
        conflicts_with = "with_sample"
    )]
    pub no_sample: bool,

    #[arg(
        long = "env-type",
        help = "Environment type template to use",
        value_enum
    )]
    pub env_type: Option<EnvType>,
}

#[derive(Debug, Subcommand)]
pub enum InfraCommands {
    Up(CreateArgs),
    Down(DestroyArgs),
}

#[derive(Debug, Args)]
pub struct ApplyArgs {
    /// Name of the environment
    #[arg(
        name = "name",
        short = 'n',
        long = "name",
        help = "Name of the environment"
    )]
    pub name: String,
}

#[derive(Debug, Args)]
pub struct CreateArgs {
    /// Name of the environment
    #[arg(name = "name", help = "Name of the environment")]
    pub name: String,

    #[arg(help = "Provider to use", value_enum)]
    pub provider: Option<Provider>,

    #[arg(
        name = "Default Registry",
        short = 'r',
        long = "registry",
        help = "Default registry to use for images"
    )]
    pub default_registry: Option<String>,

    #[arg(
        name = "Infrastructure Template",
        short = 'i',
        long = "infra-templates",
        help = "Template path for infrastruture templates"
    )]
    pub infra_template_path: Option<String>,

    #[arg(
        name = "Region",
        short = 'r',
        long = "region",
        help = "Region to use for the provider"
    )]
    pub region: Option<String>,
}

#[derive(Debug, Args)]
pub struct DestroyArgs {
    /// Name of the environment
    #[arg(
        name = "name",
        short = 'n',
        long = "name",
        help = "Name of the environment"
    )]
    pub name: String,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum Provider {
    Local,
    Aws,
    Gcp,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum EnvType {
    Development,
    Staging,
    Production,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum DeploymentStrategy {
    Restart,
    Rolling,
}

pub fn print_completions<G: Generator>(gen: G, cmd: &mut Command) {
    generate(gen, cmd, cmd.get_name().to_string(), &mut io::stdout());
}

#[derive(Debug, Args)]
pub struct DeployArgs {
    /// Kubernetes context to use
    #[arg(
        name = "context",
        short = 'c',
        long = "context",
        help = "Kubernetes context to use"
    )]
    pub context: String,

    /// Name of the environment
    #[arg(
        name = "name",
        short = 'n',
        long = "name",
        help = "Name of the environment"
    )]
    pub name: String,

    #[arg(long = "strategy", help = "Deployment strategy to use", default_value_t = DeploymentStrategy::Rolling, value_enum)]
    pub strategy: DeploymentStrategy,

    #[arg(long = "apply", help = "Apply the deployment without planning first")]
    pub apply: bool,
}

#[derive(Debug, Args)]
pub struct GenerateArgs {
    /// Name of the environment
    #[arg(
        name = "name",
        short = 'n',
        long = "name",
        help = "Name of the environment"
    )]
    pub name: String,

    #[arg(long, short)]
    pub only: Option<String>,

    #[arg(long, short)]
    pub ignore: Option<String>,
}

#[derive(Debug, Args)]
pub struct BuildArgs {
    /// Name of the environment
    #[arg(
        name = "name",
        short = 'n',
        long = "name",
        help = "Name of the environment"
    )]
    pub name: String,

    #[arg(
        name = "force",
        short = 'f',
        long = "force",
        help = "Force all rooms to build, ignore the cache"
    )]
    pub force: Option<bool>,

    /// Name of the environment
    #[arg(
        name = "ignore",
        short = 'i',
        long = "ignore",
        help = "rooms to ignore from the build of the environment"
    )]
    pub ignore: Option<String>,
}

#[derive(Debug, Args)]
pub struct GoArgs {
    /// Kubernetes context to use
    #[arg(
        name = "context",
        short = 'c',
        long = "context",
        help = "Kubernetes context to use"
    )]
    pub context: String,

    /// Name of the environment
    #[arg(
        name = "name",
        short = 'n',
        long = "name",
        help = "Name of the environment"
    )]
    pub name: String,

    #[arg(
        name = "skip build",
        short = 's',
        long = "skip-build",
        help = "Skip the build step and run only generate and deploy steps"
    )]
    pub skip_build: bool,

    #[arg(
        name = "force",
        short = 'f',
        long = "force",
        help = "Force all rooms to build, ignore the cache"
    )]
    pub force: bool,

    /// Name of the environment
    #[arg(
        name = "ignore",
        short = 'i',
        long = "ignore",
        help = "rooms to ignore from the build of the environment"
    )]
    pub ignore: Option<String>,

    #[arg(long, short)]
    pub only: Option<String>,

    #[arg(long = "strategy", help = "Deployment strategy to use for the deploy step", default_value_t = DeploymentStrategy::Rolling, value_enum)]
    pub strategy: DeploymentStrategy,

    /// Skip plan step
    #[arg(long = "apply", help = "Apply the deployment without planning first")]
    pub apply: bool,
}

#[derive(Debug, Args)]
pub struct AddServiceArgs {
    #[arg(help = "Name of the service")]
    pub service_name: String,

    #[arg(
        short = 't',
        long = "type",
        help = "Type of the application (e.g., web-app, worker)"
    )]
    pub app_type: String,

    #[arg(
        short = 'p',
        long = "port",
        help = "Port for the service (default is 80)"
    )]
    pub port: Option<u16>,

    #[arg(
        short = 'i',
        long = "image",
        help = "Docker image for the service (default is 'nginx:latest')"
    )]
    pub image: Option<String>,

    #[arg(short = 'n', long = "name", help = "Environment to add the service to")]
    pub env_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deploy_args_strategy_restart() {
        let cli = Cli::try_parse_from(&[
            "sailr",
            "deploy",
            "--context",
            "test-context",
            "--name",
            "test-env",
            "--strategy",
            "Restart",
        ])
        .unwrap();
        match cli.commands {
            Commands::Deploy(args) => {
                assert_eq!(args.strategy, DeploymentStrategy::Restart);
                assert_eq!(args.context, "test-context");
                assert_eq!(args.name, "test-env");
            }
            _ => panic!("Expected Deploy command"),
        }
    }

    #[test]
    fn test_deploy_args_strategy_rolling() {
        let cli = Cli::try_parse_from(&[
            "sailr",
            "deploy",
            "--context",
            "test-context",
            "--name",
            "test-env",
            "--strategy",
            "Rolling",
        ])
        .unwrap();
        match cli.commands {
            Commands::Deploy(args) => {
                assert_eq!(args.strategy, DeploymentStrategy::Rolling);
                assert_eq!(args.context, "test-context");
                assert_eq!(args.name, "test-env");
            }
            _ => panic!("Expected Deploy command"),
        }
    }

    #[test]
    fn test_deploy_args_strategy_default() {
        // Assumes DeploymentStrategy::Restart is the default
        let cli = Cli::try_parse_from(&[
            "sailr",
            "deploy",
            "--context",
            "test-context",
            "--name",
            "test-env",
        ])
        .unwrap();
        match cli.commands {
            Commands::Deploy(args) => {
                assert_eq!(args.strategy, DeploymentStrategy::Restart);
                assert_eq!(args.context, "test-context");
                assert_eq!(args.name, "test-env");
            }
            _ => panic!("Expected Deploy command"),
        }
    }

    #[test]
    fn test_deploy_args_strategy_invalid() {
        let result = Cli::try_parse_from(&[
            "sailr",
            "deploy",
            "--context",
            "test-context",
            "--name",
            "test-env",
            "--strategy",
            "InvalidStrategy",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn test_go_args_strategy_restart() {
        let cli = Cli::try_parse_from(&[
            "sailr",
            "go",
            "--context",
            "test-context",
            "--name",
            "test-env",
            "--strategy",
            "Restart",
        ])
        .unwrap();
        match cli.commands {
            Commands::Go(args) => {
                assert_eq!(args.strategy, DeploymentStrategy::Restart);
                assert_eq!(args.context, "test-context");
                assert_eq!(args.name, "test-env");
            }
            _ => panic!("Expected Go command"),
        }
    }

    #[test]
    fn test_go_args_strategy_rolling() {
        let cli = Cli::try_parse_from(&[
            "sailr",
            "go",
            "--context",
            "test-context",
            "--name",
            "test-env",
            "--strategy",
            "Rolling",
        ])
        .unwrap();
        match cli.commands {
            Commands::Go(args) => {
                assert_eq!(args.strategy, DeploymentStrategy::Rolling);
                assert_eq!(args.context, "test-context");
                assert_eq!(args.name, "test-env");
            }
            _ => panic!("Expected Go command"),
        }
    }

    #[test]
    fn test_go_args_strategy_default() {
        // Assumes DeploymentStrategy::Restart is the default
        let cli = Cli::try_parse_from(&[
            "sailr",
            "go",
            "--context",
            "test-context",
            "--name",
            "test-env",
        ])
        .unwrap();
        match cli.commands {
            Commands::Go(args) => {
                assert_eq!(args.strategy, DeploymentStrategy::Restart);
                assert_eq!(args.context, "test-context");
                assert_eq!(args.name, "test-env");
            }
            _ => panic!("Expected Go command"),
        }
    }

    #[test]
    fn test_go_args_strategy_invalid() {
        let result = Cli::try_parse_from(&[
            "sailr",
            "go",
            "--context",
            "test-context",
            "--name",
            "test-env",
            "--strategy",
            "InvalidStrategy",
        ]);
        assert!(result.is_err());
    }
}
