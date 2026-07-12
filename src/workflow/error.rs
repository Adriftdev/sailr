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
    #[error("Missing digest: {0}")]
    MissingDigest(String),
    #[error("Digest mismatch. expected: {expected}, actual: {actual}")]
    DigestMismatch { expected: String, actual: String },
}

impl From<crate::oci::OciError> for ArtifactError {
    fn from(error: crate::oci::OciError) -> Self {
        Self::Validation(error.to_string())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RegistryConfigError {
    #[error("Empty host")]
    EmptyHost,
    #[error("Invalid host: {0}")]
    InvalidHost(String),
    #[error("Invalid namespace: {0}")]
    InvalidNamespace(String),
    #[error("Invalid service: {0}")]
    InvalidService(String),
    #[error("Invalid tag: {0}")]
    InvalidTag(String),
    #[error("Invalid digest: {0}")]
    InvalidDigest(String),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProvenanceError {
    #[error("Source revision is unavailable")]
    MissingSourceRevision,

    #[error("Invalid source revision: {0}")]
    InvalidSourceRevision(String),

    #[error("Failed to read Git revision: {0}")]
    Git(String),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WorkflowReportError {
    #[error("Workflow report validation error: {0}")]
    Validation(String),
}
