use thiserror::Error;

#[derive(Error, Debug)]
pub enum CliError {
    #[error("Invalid provider: {0}")]
    InvalidProvider(String),

    #[error("Failed to initialize project: {0}")]
    InitializationFailed(String),

    #[error("Failed to create environment: {0}")]
    EnvironmentCreationFailed(String),

    #[error("Failed to deploy environment: {0}")]
    EnvironmentDeploymentFailed(String),

    #[error("Failed to generate environment: {0}")]
    EnvironmentGenerationFailed(String),

    #[error("Invalid command")]
    InvalidCommand,

    #[error("Directory does not exist")]
    DirectoryNotFound,

    #[error("Failed to create directory")]
    DirectoryCreationFailed,

    #[error("Failed to ensure directory")]
    DirectoryEnsuringFailed,

    #[error("Failed to copy templates")]
    TemplateCopyFailed,

    #[error("Failed to parse CLI arguments")]
    CliArgumentParsingFailed,

    #[error("Other error: {0}")]
    Other(String),

    #[error("Sailr error: {0}")]
    SailrError(#[from] SailrError),

    #[error("Provider error: {0}")]
    ProviderError(#[from] ProviderError),

    #[error("Generate error: {0}")]
    GenerateError(#[from] GenerateError),

    #[error("Deploy error: {0}")]
    DeployError(#[from] DeployError),
}

#[derive(Error, Debug)]
pub enum KubeError {
    #[error("Failed to create Kubernetes client: {0}")]
    ClientCreationFailed(String),

    #[error("Failed to initialize Kubernetes Discovery: {0}")]
    DiscoveryInitializationFailed(String),

    #[error("Failed to read or apply Kubernetes manifest: {0}")]
    ManifestApplicationFailed(String),

    // Add more error variants as needed for your specific use case.
    #[error("Invalid Kubernetes context: {0}")]
    InvalidKubernetesContext(String),

    #[error("Kubernetes API error: {0}")]
    KubernetesApiError(String),

    #[error("Failed to retrieve Kubernetes resource: {0}")]
    ResourceRetrievalFailed(String),

    #[error("Failed to update Kubernetes resource: {0}")]
    ResourceUpdateFailed(String),

    #[error("Failed to delete Kubernetes resource: {0}")]
    ResourceDeletionFailed(String),

    // Generic catch-all error variant for any unhandled error.
    #[error("An unexpected error occurred: {0}")]
    UnexpectedError(String),
}

#[derive(Error, Debug)]
pub enum GenerateError {
    #[error("Failed to generate k8s resources: {0}")]
    K8sResourceGenerationFailed(String),

    #[error("Sailr error: {0}")]
    SailrError(#[from] SailrError),
}

#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("Invalid provider: {0}")]
    InvalidProvider(String),
}

#[derive(Error, Debug)]
pub enum SailrError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML serialization error: {0}")]
    TomlSerialization(#[from] toml::ser::Error),

    #[error("TOML deserialization error: {0}")]
    TomlDeserialization(#[from] toml::de::Error),

    #[error("YAML serialization/deserialization error: {0}")]
    YamlError(#[from] serde_yaml::Error),

    #[error("Invalid YAML")]
    InvalidYaml,

    #[error("Template validation error: {0}")]
    TemplateValidation(#[from] anyhow::Error),

    #[error("Environment not found")]
    EnvironmentNotFound,

    #[error("Service not found in environment")]
    ServiceNotFound,

    #[error("Service already exists in environment")]
    ServiceAlreadyExists,

    #[error("Directory does not exist")]
    DirectoryNotFound,

    #[error("Failed to create directory")]
    DirectoryCreationFailed,

    #[error("Failed to remove directory")]
    DirectoryRemovalFailed,

    #[error("Failed to read file")]
    FileReadFailed,

    #[error("Failed to write file")]
    FileWriteFailed,
}

#[derive(Error, Debug)]
pub enum FileSystemManagerError {
    #[error("Directory does not exist: {0}")]
    DirectoryNotFound(String),
    #[error("Failed to create directory: {0}")]
    DirectoryCreationFailed(String),
    #[error("Failed to remove directory: {0}")]
    DirectoryRemovalFailed(String),
    #[error("Failed to read file!: {0}")]
    FileReadFailed(String),
    #[error("Failed to write file!: {0}")]
    FileWriteFailed(String),
}

#[derive(Error, Debug)]
pub enum DeployError {
    #[error("Failed to deploy environment: {0}")]
    EnvironmentDeploymentFailed(String),

    #[error("Failed to create Kubernetes client: {0}")]
    ClientCreationFailed(String),

    #[error("Failed to initialize Kubernetes Discovery: {0}")]
    DiscoveryInitializationFailed(String),

    #[error("Failed to read or apply Kubernetes manifest: {0}")]
    ManifestApplicationFailed(String),

    // Add more error variants as needed for your specific use case.
    #[error("Invalid Kubernetes context: {0}")]
    InvalidKubernetesContext(String),

    #[error("Kubernetes API error: {0}")]
    KubernetesApiError(#[from] KubeError),
}
