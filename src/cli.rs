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
    Infra(InfraArgs),
    /// Deploy an environment
    Deploy(DeployArgs),
    /// Generate an environment
    Generate(GenerateArgs),
    /// Build related projects
    Build(BuildArgs),
    /// Generate and deploy an environment
    Go(GoArgs),
    /// Kubernetes resources commands
    K8s(K8sArgs),
}

#[derive(Debug, Args)]
pub struct K8sArgs {
    #[command(subcommand)]
    pub command: K8sCommands,
}

#[derive(Debug, Subcommand)]
pub enum K8sCommands {
    Pod(PodArgs),
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
}

#[derive(Debug, Args)]
pub struct InfraArgs {
    #[command(subcommand)]
    pub command: InfraCommands,
}

#[derive(Debug, Subcommand)]
pub enum InfraCommands {
    Create(CreateArgs),
    Apply(ApplyArgs),
    Destroy(DestroyArgs),
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

    #[arg(long, short)]
    pub only: Option<String>,
}

#[derive(Debug, Args)]
pub struct PodArgs {
    #[command(subcommand)]
    pub command: PodCommands,
}

#[derive(Debug, Subcommand)]
pub enum PodCommands {
    Delete(DeleteArgs),
    Get(GetArgs),
}

#[derive(Debug, Args)]
pub struct DeleteArgs {
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
        help = "Name of the pod to delete"
    )]
    pub name: String,
}

#[derive(Debug, Args)]
pub struct GetArgs {
    /// Kubernetes context to use
    #[arg(
        name = "context",
        short = 'c',
        long = "context",
        help = "Kubernetes context to use"
    )]
    pub context: String,
}
