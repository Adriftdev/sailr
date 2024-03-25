use std::io;

use clap::{clap_derive::Args, command, Command, Parser, Subcommand, ValueEnum};
use clap_complete::{generate, Generator, Shell};

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
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
    Env(EnvArgs),
    /// Deploy an environment
    Deploy(DeployArgs),
    /// Generate an environment
    Generate(GenerateArgs),
    /// Generate and deploy an environment
    Go(GoArgs),
}

#[derive(Debug, Args)]
pub struct CompletionsArgs {
    #[arg(help = "Shell to generate completions for", value_enum)]
    pub shell: Shell,
}

#[derive(Debug, Args)]
pub struct InitArgs {}

#[derive(Debug, Args)]
pub struct EnvArgs {
    #[command(subcommand)]
    pub command: EnvCommands,
}

#[derive(Debug, Subcommand)]
pub enum EnvCommands {
    /// Create a new environment
    Create(CreateArgs),
}

#[derive(Debug, Args)]
pub struct CreateArgs {
    /// Name of the environment
    #[arg(name = "name", help = "Name of the environment")]
    pub name: String,

    /// Enable local postgres pod
    #[arg(
        name = "postgres",
        short = 'p',
        long = "postgres",
        help = "Enable local postgres pod (usually development only)"
    )]
    pub postresql: bool,

    /// Enable local redis pod
    #[arg(
        name = "redis",
        short = 'r',
        long = "redis",
        help = "Enable local redis pod (usually development only / only small caches)"
    )]
    pub redis: bool,

    /// Enable system registry pod
    #[arg(
        name = "registry",
        short = 'g',
        long = "registry",
        help = "Enable system registry pod"
    )]
    pub registry: bool,

    #[arg(help = "Provider to use", value_enum)]
    pub provider: Provider,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum Provider {
    GCP,
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
}

#[derive(Debug, Args)]
pub struct ArchiveArgs {}
