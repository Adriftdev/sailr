use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::workflow::error::ArtifactError;

#[derive(Debug, Clone, Default, Serialize)]
pub struct WorkflowReportData {
    pub published_artifacts: Vec<PublishedImageArtifact>,
}

#[derive(Debug, Clone, Default)]
pub struct WorkflowReportAccumulator {
    inner: Arc<tokio::sync::Mutex<WorkflowReportData>>,
}

impl WorkflowReportAccumulator {
    pub async fn add_image(&self, artifact: PublishedImageArtifact) {
        let mut inner = self.inner.lock().await;
        inner.published_artifacts.push(artifact);
    }

    pub async fn snapshot(&self) -> WorkflowReportData {
        self.inner.lock().await.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageArtifact {
    pub service: String,
    pub environment: String,
    pub registry: String,
    pub repository: String,
    pub tag: String,
    pub digest: Option<String>,
    pub image_ref: String,
    pub source_sha: Option<String>,
    pub built_at: Option<String>,
}

impl ImageArtifact {
    pub fn tagged(
        service: impl Into<String>,
        environment: impl Into<String>,
        registry: impl Into<String>,
        repository: impl Into<String>,
        tag: impl Into<String>,
        source_sha: Option<String>,
    ) -> Self {
        let registry = registry.into();
        let repository = repository.into();
        let tag = tag.into();

        let image_ref = format!("{}/{}:{}", registry, repository, tag);

        Self {
            service: service.into(),
            environment: environment.into(),
            registry,
            repository,
            tag,
            digest: None,
            image_ref,
            source_sha,
            built_at: None,
        }
    }

    pub fn with_digest(mut self, digest: impl Into<String>) -> Self {
        let digest = digest.into();
        self.image_ref = format!("{}/{}@{}", self.registry, self.repository, digest);
        self.digest = Some(digest);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedImageArtifact {
    pub service: String,
    pub environment: String,
    pub registry: String,
    pub repository: String,
    pub tag: String,
    pub digest: String,
    pub image_ref: String,
    pub source_sha: String,
    pub published_at: String,
}

impl PublishedImageArtifact {
    pub fn from_push_result(
        environment: &str,
        item: &ImagePushPlanItem,
        digest: &str,
        source_sha: &str,
        published_at: &str,
    ) -> Result<Self, ArtifactError> {
        validate_digest(digest)?;
        
        let image_ref = format!("{}/{}@{}", item.registry, item.repository, digest);
        
        if source_sha.is_empty() {
            return Err(ArtifactError::Validation("source_sha cannot be empty".to_string()));
        }
        if item.service.is_empty() {
            return Err(ArtifactError::Validation("service cannot be empty".to_string()));
        }
        if item.registry.is_empty() {
            return Err(ArtifactError::Validation("registry cannot be empty".to_string()));
        }
        if item.repository.is_empty() {
            return Err(ArtifactError::Validation("repository cannot be empty".to_string()));
        }

        Ok(Self {
            service: item.service.clone(),
            environment: environment.to_string(),
            registry: item.registry.clone(),
            repository: item.repository.clone(),
            tag: item.tag.clone(),
            digest: digest.to_string(),
            image_ref,
            source_sha: source_sha.to_string(),
            published_at: published_at.to_string(),
        })
    }
}

pub fn validate_digest(value: &str) -> Result<(), ArtifactError> {
    if !value.starts_with("sha256:") {
        return Err(ArtifactError::Validation("digest must start with sha256:".to_string()));
    }
    let hex_part = &value[7..];
    if hex_part.len() != 64 {
        return Err(ArtifactError::Validation("digest hex part must be exactly 64 characters long".to_string()));
    }
    if !hex_part.chars().all(|c| c.is_ascii_hexdigit() && c.is_lowercase() || c.is_numeric()) {
        return Err(ArtifactError::Validation("digest must contain only lowercase hexadecimal characters".to_string()));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ImageArtifactReport {
    pub published_artifacts: Vec<PublishedImageArtifact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImagePushPlanAction {
    WouldPush,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImagePushPlanItem {
    pub service: String,
    pub registry: String,
    pub repository: String,
    pub tag: String,
    pub image_ref: String,
    pub source_sha: Option<String>,
    pub action: ImagePushPlanAction,
}

impl ImageArtifact {
    pub fn from_push_plan_item(environment: impl Into<String>, item: &ImagePushPlanItem) -> Self {
        ImageArtifact::tagged(
            item.service.clone(),
            environment,
            item.registry.clone(),
            item.repository.clone(),
            item.tag.clone(),
            item.source_sha.clone(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImagePushPlanReport {
    pub environment: String,
    pub mutates_registry: bool,
    pub items: Vec<ImagePushPlanItem>,
}

pub fn derive_image_tag(source_sha: Option<&str>) -> String {
    match source_sha {
        Some(sha) if sha.len() >= 7 => sha[0..7].to_string(),
        Some(sha) => sha.to_string(),
        None => "dev".to_string(),
    }
}

pub fn parse_pushed_digest(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let marker = "digest:";
        let idx = line.find(marker)?;
        let rest = line[idx + marker.len()..].trim();
        rest.split_whitespace().next().map(str::to_string)
    })
}

pub fn pushed_artifact_from_output(
    environment: &str,
    item: &ImagePushPlanItem,
    output: &str,
    structured_digest: Option<&str>,
) -> Result<PublishedImageArtifact, String> {
    let parsed_digest = parse_pushed_digest(output).ok_or_else(|| {
        format!(
            "image push succeeded but digest could not be captured for {}",
            item.image_ref
        )
    })?;

    if let Some(structured) = structured_digest {
        if structured != parsed_digest {
            return Err(format!("digest mismatch for {}: parsed '{}' != structured '{}'", item.image_ref, parsed_digest, structured));
        }
    }

    let source_sha = item.source_sha.as_deref().unwrap_or("dev");
    let published_at = chrono::Utc::now().to_rfc3339();

    PublishedImageArtifact::from_push_result(
        environment,
        item,
        &parsed_digest,
        source_sha,
        &published_at,
    ).map_err(|e| format!("invalid published artifact: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_artifact_tagged_builds_tag_ref() {
        let artifact = ImageArtifact::tagged(
            "ci-build-hello",
            "staging",
            "ghcr.io",
            "adriftdev/sailr/ci-build-hello",
            "2bcc3f7",
            Some("2bcc3f70984bb6d33d93bbcbb9eb3539ce033dc8".to_string()),
        );

        assert_eq!(artifact.service, "ci-build-hello");
        assert_eq!(artifact.environment, "staging");
        assert_eq!(artifact.registry, "ghcr.io");
        assert_eq!(artifact.repository, "adriftdev/sailr/ci-build-hello");
        assert_eq!(artifact.tag, "2bcc3f7");
        assert_eq!(artifact.digest, None);
        assert_eq!(
            artifact.image_ref,
            "ghcr.io/adriftdev/sailr/ci-build-hello:2bcc3f7"
        );
    }

    #[test]
    fn image_artifact_with_digest_builds_digest_ref() {
        let artifact = ImageArtifact::tagged(
            "ci-build-hello",
            "staging",
            "ghcr.io",
            "adriftdev/sailr/ci-build-hello",
            "2bcc3f7",
            None,
        )
        .with_digest("sha256:abc123");

        assert_eq!(artifact.digest.as_deref(), Some("sha256:abc123"));
        assert_eq!(
            artifact.image_ref,
            "ghcr.io/adriftdev/sailr/ci-build-hello@sha256:abc123"
        );
    }

    #[test]
    fn empty_image_artifact_report_serializes() {
        let report = ImageArtifactReport::default();
        let json = serde_json::to_value(report).unwrap();

        assert_eq!(json["published_artifacts"], serde_json::json!([]));
    }

    #[test]
    fn image_artifact_serializes_expected_shape() {
        let artifact = ImageArtifact::tagged(
            "ci-build-hello",
            "staging",
            "ghcr.io",
            "adriftdev/sailr/ci-build-hello",
            "2bcc3f7",
            Some("2bcc3f70984bb6d33d93bbcbb9eb3539ce033dc8".to_string()),
        )
        .with_digest("sha256:abc123");

        let json = serde_json::to_value(artifact).unwrap();

        assert_eq!(json["service"], "ci-build-hello");
        assert_eq!(json["environment"], "staging");
        assert_eq!(json["registry"], "ghcr.io");
        assert_eq!(json["repository"], "adriftdev/sailr/ci-build-hello");
        assert_eq!(json["tag"], "2bcc3f7");
        assert_eq!(json["digest"], "sha256:abc123");
        assert_eq!(
            json["image_ref"],
            "ghcr.io/adriftdev/sailr/ci-build-hello@sha256:abc123"
        );
    }
}

#[cfg(test)]
mod tests_derive {
    use super::*;
    #[test]
    fn test_derive_image_tag() {
        assert_eq!(
            derive_image_tag(Some("2bcc3f70984bb6d33d93bbcbb9eb3539ce033dc8")),
            "2bcc3f7"
        );
        assert_eq!(derive_image_tag(Some("abc")), "abc");
        assert_eq!(derive_image_tag(None), "dev");
    }

    #[test]
    fn parses_digest_from_stdout_style_output() {
        let output = "latest: digest: sha256:abc123 size: 1234";
        assert_eq!(
            parse_pushed_digest(output).as_deref(),
            Some("sha256:abc123")
        );
    }

    #[test]
    fn parses_digest_from_stderr_style_combined_output() {
        let output = "some stdout\nlatest: digest: sha256:def456 size: 1234";
        assert_eq!(
            parse_pushed_digest(output).as_deref(),
            Some("sha256:def456")
        );
    }

    #[test]
    fn returns_none_when_digest_missing() {
        let output = "pushed some layers";
        assert_eq!(parse_pushed_digest(output), None);
    }

    #[test]
    fn test_pushed_artifact_from_output_success() {
        let item = ImagePushPlanItem {
            service: "api".to_string(),
            registry: "ghcr.io".to_string(),
            repository: "org/api".to_string(),
            tag: "latest".to_string(),
            image_ref: "ghcr.io/org/api:latest".to_string(),
            source_sha: Some("abc12345".to_string()),
            action: ImagePushPlanAction::WouldPush,
        };
        let output = "digest: sha256:0000000000000000000000000000000000000000000000000000000000000000";
        let artifact = pushed_artifact_from_output("prod", &item, output, None).unwrap();
        assert_eq!(artifact.digest, "sha256:0000000000000000000000000000000000000000000000000000000000000000");
        assert_eq!(artifact.image_ref, "ghcr.io/org/api@sha256:0000000000000000000000000000000000000000000000000000000000000000");
    }

    #[test]
    fn test_pushed_artifact_from_output_failure() {
        let item = ImagePushPlanItem {
            service: "api".to_string(),
            registry: "ghcr.io".to_string(),
            repository: "org/api".to_string(),
            tag: "latest".to_string(),
            image_ref: "ghcr.io/org/api:latest".to_string(),
            source_sha: None,
            action: ImagePushPlanAction::WouldPush,
        };
        let output = "no digest here";
        let err = pushed_artifact_from_output("prod", &item, output, None).unwrap_err();
        assert_eq!(
            err,
            "image push succeeded but digest could not be captured for ghcr.io/org/api:latest"
        );
    }

    #[test]
    fn test_validate_digest_valid() {
        assert!(validate_digest("sha256:0000000000000000000000000000000000000000000000000000000000000000").is_ok());
    }

    #[test]
    fn test_validate_digest_missing_prefix() {
        assert!(validate_digest("0000000000000000000000000000000000000000000000000000000000000000").is_err());
    }

    #[test]
    fn test_validate_digest_truncated() {
        assert!(validate_digest("sha256:000").is_err());
    }

    #[test]
    fn test_validate_digest_non_hex() {
        assert!(validate_digest("sha256:zzzz000000000000000000000000000000000000000000000000000000000000").is_err());
    }
}

#[cfg(test)]
mod tests_addendum {
    use super::*;

    #[test]
    fn image_push_plan_report_serializes() {
        let report = ImagePushPlanReport {
            environment: "staging".to_string(),
            mutates_registry: false,
            items: vec![ImagePushPlanItem {
                service: "ci-build-hello".to_string(),
                registry: "ghcr.io".to_string(),
                repository: "adriftdev/sailr/ci-build-hello".to_string(),
                tag: "61eaa8b".to_string(),
                image_ref: "ghcr.io/adriftdev/sailr/ci-build-hello:61eaa8b".to_string(),
                source_sha: Some("61eaa8bb0e52f5bb1d5a621760b0a2eae601ccd3".to_string()),
                action: ImagePushPlanAction::WouldPush,
            }],
        };

        let json = serde_json::to_value(report).unwrap();

        assert_eq!(json["environment"], "staging");
        assert_eq!(json["mutates_registry"], false);
        assert_eq!(json["items"][0]["action"], "would_push");
    }
}
