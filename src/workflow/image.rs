use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ImageArtifactReport {
    pub artifacts: Vec<ImageArtifact>,
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
            "Adriftdev/sailr/ci-build-hello",
            "2bcc3f7",
            Some("2bcc3f70984bb6d33d93bbcbb9eb3539ce033dc8".to_string()),
        );

        assert_eq!(artifact.service, "ci-build-hello");
        assert_eq!(artifact.environment, "staging");
        assert_eq!(artifact.registry, "ghcr.io");
        assert_eq!(artifact.repository, "Adriftdev/sailr/ci-build-hello");
        assert_eq!(artifact.tag, "2bcc3f7");
        assert_eq!(artifact.digest, None);
        assert_eq!(
            artifact.image_ref,
            "ghcr.io/Adriftdev/sailr/ci-build-hello:2bcc3f7"
        );
    }

    #[test]
    fn image_artifact_with_digest_builds_digest_ref() {
        let artifact = ImageArtifact::tagged(
            "ci-build-hello",
            "staging",
            "ghcr.io",
            "Adriftdev/sailr/ci-build-hello",
            "2bcc3f7",
            None,
        )
        .with_digest("sha256:abc123");

        assert_eq!(artifact.digest.as_deref(), Some("sha256:abc123"));
        assert_eq!(
            artifact.image_ref,
            "ghcr.io/Adriftdev/sailr/ci-build-hello@sha256:abc123"
        );
    }

    #[test]
    fn empty_image_artifact_report_serializes() {
        let report = ImageArtifactReport::default();
        let json = serde_json::to_value(report).unwrap();

        assert_eq!(json["artifacts"], serde_json::json!([]));
    }

    #[test]
    fn image_artifact_serializes_expected_shape() {
        let artifact = ImageArtifact::tagged(
            "ci-build-hello",
            "staging",
            "ghcr.io",
            "Adriftdev/sailr/ci-build-hello",
            "2bcc3f7",
            Some("2bcc3f70984bb6d33d93bbcbb9eb3539ce033dc8".to_string()),
        )
        .with_digest("sha256:abc123");

        let json = serde_json::to_value(artifact).unwrap();

        assert_eq!(json["service"], "ci-build-hello");
        assert_eq!(json["environment"], "staging");
        assert_eq!(json["registry"], "ghcr.io");
        assert_eq!(json["repository"], "Adriftdev/sailr/ci-build-hello");
        assert_eq!(json["tag"], "2bcc3f7");
        assert_eq!(json["digest"], "sha256:abc123");
        assert_eq!(
            json["image_ref"],
            "ghcr.io/Adriftdev/sailr/ci-build-hello@sha256:abc123"
        );
    }
}
