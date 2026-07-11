use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorkflowError {
    #[error("Workflow config error: {0}")]
    ConfigError(String),

    #[error("Workflow profile not found: {0}")]
    ProfileNotFound(String),

    #[error("Workflow config parse error: {0}")]
    ParseError(#[from] toml::de::Error),

    #[error("IO error reading workflow config: {0}")]
    IoError(#[from] std::io::Error),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ArtifactError {
    #[error("Artifact validation error: {0}")]
    Validation(String),
}
