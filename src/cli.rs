use std::io;

use crate::environment::BuildEngine;
use clap::{clap_derive::Args, Command, Parser, Subcommand, ValueEnum};
use clap_complete::{generate, Generator, Shell};

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
#[command(
    name = "sailr",
    about = "A CLI tool for managing environments and deployments"
)]
#[command(propagate_version = true)]
pub struct Cli {
    #[arg(
        short = 'q',
        long = "quiet",
        global = true,
        help = "Do not print log messages"
    )]
    pub quiet: bool,

    #[arg(
        short = 'v',
        long = "verbose",
        global = true,
        help = "Use verbose output"
    )]
    pub verbose: bool,

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
    /// Migrate an environment configuration to schema 0.5.0
    Migrate(MigrateArgs),
    /// Bump the version of a service
    #[command(disable_version_flag = true)]
    Bump(BumpArgs),
    /// Lint an environment configuration
    Lint(LintArgs),
    /// Manage workflow profiles
    #[command(subcommand)]
    Workflow(WorkflowCommands),
    /// Delivery flow management and validation
    #[command(subcommand)]
    Flow(FlowCommands),
    /// Validate immutable publication reports
    #[command(subcommand)]
    Publication(PublicationCommands),
    /// Plan immutable artifact promotion
    #[command(subcommand)]
    Promote(PromoteCommands),
    /// Show machine-readable Sailr feature support
    Capabilities(CapabilitiesArgs),
}

#[derive(Debug, Subcommand)]
pub enum WorkflowCommands {
    /// Create a workflow profile from an existing environment
    Init(WorkflowInitArgs),
    /// List available workflow profiles
    List,
    /// Show details of a workflow profile
    Show(WorkflowShowArgs),
    /// Run a workflow profile
    Run(WorkflowRunArgs),
    /// Generate a CI template for a workflow profile
    GenerateCi(WorkflowGenerateCiArgs),
    /// Plan a workflow profile
    Plan(WorkflowPlanArgs),
    /// View workflow graph
    Graph(WorkflowGraphArgs),
    /// Explain a workflow task
    Explain(WorkflowExplainArgs),
    /// Inspect workflow diagnostic configuration
    Inspect(WorkflowInspectArgs),
    /// Prepare and serialize an immutable deployment bundle
    Prepare(WorkflowPrepareArgs),
    /// Apply a previously prepared immutable deployment bundle
    Apply(WorkflowApplyArgs),
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum WorkflowInitPreset {
    Build,
    Deploy,
    PortableRelease,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum WorkflowInitApproval {
    External,
    Signature,
}

#[derive(Debug, Args)]
pub struct WorkflowInitArgs {
    /// Name of the workflow profile to create
    pub profile: String,

    /// Existing Sailr environment to use
    #[arg(long)]
    pub environment: String,

    /// Safe workflow profile preset
    #[arg(long, value_enum, default_value = "deploy")]
    pub preset: WorkflowInitPreset,

    /// Kubernetes context for deploy profiles
    #[arg(long)]
    pub context: Option<String>,

    /// Kubernetes namespace override
    #[arg(long)]
    pub namespace: Option<String>,

    /// Portable-release approval mechanism
    #[arg(long, value_enum)]
    pub approval: Option<WorkflowInitApproval>,

    /// File containing a base64-encoded raw Ed25519 public key
    #[arg(long = "trusted-public-key-file")]
    pub trusted_public_key_file: Option<std::path::PathBuf>,

    /// Print the complete resulting configuration without writing it
    #[arg(long)]
    pub print: bool,

    /// Workflow configuration to create or update
    #[arg(long, default_value = "sailr.workflow.toml")]
    pub config: std::path::PathBuf,
}

#[derive(Debug, Subcommand)]
pub enum PublicationCommands {
    /// Validate a workflow publication report
    Validate(PublicationValidateArgs),
}

#[derive(Debug, Args)]
pub struct PublicationValidateArgs {
    pub report: std::path::PathBuf,
}

#[derive(Debug, Subcommand)]
pub enum PromoteCommands {
    /// Create a deterministic promotion plan
    Plan(PromotePlanArgs),
}

#[derive(Debug, Args)]
pub struct PromotePlanArgs {
    #[arg(long = "from-report", action = clap::ArgAction::Append, conflicts_with = "from_manifest", required_unless_present = "from_manifest")]
    pub from_reports: Vec<std::path::PathBuf>,
    #[arg(long = "from-manifest", conflicts_with = "from_reports")]
    pub from_manifest: Option<std::path::PathBuf>,
    #[arg(long = "to")]
    pub target_environment: String,
    #[arg(long)]
    pub out: std::path::PathBuf,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum CapabilitiesFormat {
    Json,
}

#[derive(Debug, Args)]
pub struct CapabilitiesArgs {
    #[arg(long, default_value = "json", value_enum)]
    pub format: CapabilitiesFormat,
}

#[derive(Debug, Subcommand)]
pub enum FlowCommands {
    /// Inspect repository and output machine-readable JSON
    Inspect,
    /// Validate TOML/YAML syntax, duplicate workflow names, missing references, mutable image refs
    Validate,
    /// Generate or merge CI configuration
    GenerateCi(FlowGenerateCiArgs),
    /// Validate production invariants
    CheckRelease,
    /// Validate development invariants
    CheckGitops,
}

#[derive(Debug, Args)]
pub struct FlowGenerateCiArgs {
    /// Named flow; may be omitted when exactly one flow exists
    pub flow: Option<String>,
    #[arg(long, value_enum)]
    pub mode: FlowGenerationMode,
    #[arg(long)]
    pub output: Option<std::path::PathBuf>,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum FlowGenerationMode {
    Print,
    Fragment,
    Create,
    Merge,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum WorkflowOutputFormat {
    Text,
    Json,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum WorkflowGraphFormat {
    Text,
    Mermaid,
}

#[derive(Debug, Args)]
pub struct WorkflowPlanArgs {
    /// Name of the workflow profile to plan
    pub profile: String,

    #[arg(long, default_value = "text", value_enum)]
    pub format: WorkflowOutputFormat,

    #[arg(long)]
    pub only: Option<String>,

    #[arg(long)]
    pub ignore: Option<String>,
}

#[derive(Debug, Args)]
pub struct WorkflowInspectArgs {
    /// Name of the workflow profile to inspect
    pub profile: String,
}

#[derive(Debug, Args)]
pub struct WorkflowGraphArgs {
    /// Name of the workflow profile to graph
    pub profile: String,

    #[arg(long, default_value = "text", value_enum)]
    pub format: WorkflowGraphFormat,

    #[arg(long)]
    pub only: Option<String>,

    #[arg(long)]
    pub ignore: Option<String>,
}

#[derive(Debug, Args)]
pub struct WorkflowExplainArgs {
    /// Name of the workflow profile
    pub profile: String,

    /// ID of the task to explain
    pub task: String,
}

#[derive(Debug, Args)]
pub struct WorkflowRunArgs {
    /// Name of the workflow profile to run
    pub profile: String,

    #[arg(long)]
    pub only: Option<String>,

    #[arg(long)]
    pub ignore: Option<String>,

    #[arg(long)]
    pub non_interactive: bool,

    #[arg(long)]
    pub plan: bool,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(long)]
    pub apply: bool,

    #[arg(long = "release-id")]
    pub release_id: Option<String>,
}

#[derive(Debug, Args)]
pub struct WorkflowPrepareArgs {
    pub profile: String,
    #[arg(long = "promotion-plan")]
    pub promotion_plan: std::path::PathBuf,
    #[arg(long)]
    pub out: std::path::PathBuf,
}

#[derive(Debug, Args)]
pub struct WorkflowApplyArgs {
    pub profile: String,
    #[arg(long)]
    pub bundle: std::path::PathBuf,
    #[arg(long)]
    pub non_interactive: bool,
    #[arg(long)]
    pub apply: bool,
    #[arg(long = "release-id")]
    pub release_id: Option<String>,
}

#[derive(Debug, Args)]
pub struct WorkflowGenerateCiArgs {
    /// Name of the workflow profile to run
    pub profile: String,

    /// CI provider to generate for (github, circleci, travis)
    #[arg(long)]
    pub provider: String,

    /// Optional output file path
    #[arg(long)]
    pub output: Option<String>,
}

#[derive(Debug, Args)]
pub struct WorkflowShowArgs {
    /// Name of the workflow profile to show
    pub profile: String,
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

    /// Sailr environment to use for interactive deploy
    #[arg(
        name = "environment",
        short = 'e',
        long = "environment",
        help = "Sailr environment to use for interactive deploy"
    )]
    pub environment: Option<String>,

    /// Namespace to use
    #[arg(
        name = "namespace",
        short = 'n',
        long = "namespace",
        help = "Namespace to use",
        default_value = "default"
    )]
    pub namespace: String,
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

    #[arg(long, value_enum, help = "Build engine to configure")]
    pub engine: Option<BuildEngine>,
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
        short = 'R',
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

    ///namespace to deploy to
    #[arg(
        name = "namespace",
        short = 'N',
        long = "namespace",
        help = "Namespace to deploy to"
    )]
    pub namespace: Option<String>,

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

    #[arg(long)]
    pub only: Option<String>,

    /// Name of the environment
    #[arg(
        name = "ignore",
        short = 'i',
        long = "ignore",
        help = "rooms to ignore from the build of the environment"
    )]
    pub ignore: Option<String>,

    #[arg(long, help = "Plan the build without executing commands")]
    pub plan: bool,

    #[arg(
        long,
        help = "Print the commands that would run without executing them"
    )]
    pub dry_run: bool,

    #[arg(long, help = "Explain why each room is dirty or clean")]
    pub explain: bool,

    #[arg(long, help = "Dump the resolved file scope for each room")]
    pub dump_scope: bool,

    #[arg(long, value_enum, help = "Build engine to use")]
    pub engine: Option<BuildEngine>,
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

    ///namespace to deploy to
    #[arg(
        name = "namespace",
        short = 'N',
        long = "namespace",
        help = "Namespace to deploy to"
    )]
    pub namespace: Option<String>,

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

    #[arg(long, help = "Plan the build step without executing commands")]
    pub plan: bool,

    #[arg(long, help = "Print the build-step commands without executing them")]
    pub dry_run: bool,

    #[arg(long, help = "Explain why each build room is dirty or clean")]
    pub explain: bool,

    #[arg(long, help = "Dump the resolved file scope for each room")]
    pub dump_scope: bool,

    #[arg(long, value_enum, help = "Build engine to use")]
    pub engine: Option<BuildEngine>,

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
        let cli = Cli::try_parse_from([
            "sailr",
            "deploy",
            "--context",
            "test-context",
            "--name",
            "test-env",
            "--strategy",
            "restart",
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
        let cli = Cli::try_parse_from([
            "sailr",
            "deploy",
            "--context",
            "test-context",
            "--name",
            "test-env",
            "--strategy",
            "rolling",
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
        // Assumes DeploymentStrategy::Rolling is the default
        let cli = Cli::try_parse_from([
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
                assert_eq!(args.strategy, DeploymentStrategy::Rolling);
                assert_eq!(args.context, "test-context");
                assert_eq!(args.name, "test-env");
            }
            _ => panic!("Expected Deploy command"),
        }
    }

    #[test]
    fn test_deploy_args_strategy_invalid() {
        let result = Cli::try_parse_from([
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
    fn test_migrate_args_parse() {
        let cli = Cli::try_parse_from([
            "sailr",
            "migrate",
            "--name",
            "edge",
            "--engine",
            "runkernel",
        ])
        .unwrap();
        match cli.commands {
            Commands::Migrate(args) => {
                assert_eq!(args.name, "edge");
                assert_eq!(args.engine, Some(BuildEngine::Runkernel));
            }
            _ => panic!("Expected Migrate command"),
        }
    }

    #[test]
    fn test_init_args_parse_engine() {
        let cli = Cli::try_parse_from([
            "sailr",
            "init",
            "--name",
            "edge",
            "--engine",
            "runkernel",
            "--no-sample",
        ])
        .unwrap();
        match cli.commands {
            Commands::Init(args) => {
                assert_eq!(args.name, "edge");
                assert_eq!(args.engine, Some(BuildEngine::Runkernel));
            }
            _ => panic!("Expected Init command"),
        }
    }

    #[test]
    fn test_build_args_parse_plan_flags() {
        let cli = Cli::try_parse_from([
            "sailr",
            "build",
            "--name",
            "edge",
            "--plan",
            "--dry-run",
            "--explain",
            "--dump-scope",
            "--only",
            "api,web",
            "--engine",
            "runkernel",
        ])
        .unwrap();
        match cli.commands {
            Commands::Build(args) => {
                assert_eq!(args.name, "edge");
                assert!(args.plan);
                assert!(args.dry_run);
                assert!(args.explain);
                assert!(args.dump_scope);
                assert_eq!(args.only.as_deref(), Some("api,web"));
                assert_eq!(args.engine, Some(BuildEngine::Runkernel));
            }
            _ => panic!("Expected Build command"),
        }
    }

    #[test]
    fn test_workflow_init_args_parse_portable_release() {
        let cli = Cli::try_parse_from([
            "sailr",
            "workflow",
            "init",
            "release-production",
            "--environment",
            "production",
            "--preset",
            "portable-release",
            "--context",
            "production-cluster",
            "--namespace",
            "production",
            "--approval",
            "external",
            "--print",
        ])
        .expect("workflow init arguments");
        match cli.commands {
            Commands::Workflow(WorkflowCommands::Init(args)) => {
                assert_eq!(args.profile, "release-production");
                assert_eq!(args.environment, "production");
                assert_eq!(args.preset, WorkflowInitPreset::PortableRelease);
                assert_eq!(args.context.as_deref(), Some("production-cluster"));
                assert_eq!(args.namespace.as_deref(), Some("production"));
                assert_eq!(args.approval, Some(WorkflowInitApproval::External));
                assert!(args.print);
            }
            _ => panic!("Expected workflow init command"),
        }
    }

    #[test]
    fn promotion_accepts_repeated_reports_or_one_manifest() {
        let direct = Cli::try_parse_from([
            "sailr",
            "promote",
            "plan",
            "--from-report",
            "api.json",
            "--from-report",
            "worker.json",
            "--to",
            "prod",
            "--out",
            "promotion.json",
        ])
        .expect("repeated reports");
        match direct.commands {
            Commands::Promote(PromoteCommands::Plan(args)) => {
                assert_eq!(args.from_reports.len(), 2);
                assert!(args.from_manifest.is_none());
            }
            _ => panic!("Expected promote plan command"),
        }

        assert!(Cli::try_parse_from([
            "sailr",
            "promote",
            "plan",
            "--from-report",
            "api.json",
            "--from-manifest",
            "candidates.json",
            "--to",
            "prod",
            "--out",
            "promotion.json",
        ])
        .is_err());
    }
}

#[derive(Debug, Args, Clone)]
pub struct MigrateArgs {
    #[arg(short, long)]
    pub name: String,

    #[arg(long, value_enum, help = "Build engine to configure after migration")]
    pub engine: Option<BuildEngine>,
}

#[derive(Debug, Args, Clone)]
pub struct BumpArgs {
    #[arg(short, long)]
    pub name: String,
    #[arg(short, long)]
    pub service: String,
    #[arg(long)]
    pub version: String,
}

#[derive(Debug, Args, Clone)]
pub struct LintArgs {
    #[arg(short, long)]
    pub name: String,
}
