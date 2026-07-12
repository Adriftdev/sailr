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
pub struct ImageProvenance {
    pub build_fingerprint: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_revision: Option<String>,
}

impl ImageProvenance {
    pub fn validate(&self) -> Result<(), ArtifactError> {
        if self.build_fingerprint.trim().is_empty() {
            return Err(ArtifactError::Validation(
                "build_fingerprint cannot be empty".to_string(),
            ));
        }
        if let Some(rev) = &self.source_revision {
            if rev.trim().is_empty() {
                return Err(ArtifactError::Validation(
                    "source_revision cannot be empty".to_string(),
                ));
            }
            if rev.chars().any(|c| c.is_whitespace()) {
                return Err(ArtifactError::Validation(
                    "source_revision cannot contain whitespace".to_string(),
                ));
            }
        }
        Ok(())
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
    pub provenance: ImageProvenance,
    pub published_at: String,
}

impl PublishedImageArtifact {
    pub fn from_push_result(
        environment: &str,
        item: &ImagePushPlanItem,
        digest: &str,
        published_at: &str,
    ) -> Result<Self, ArtifactError> {
        let artifact = Self {
            service: item.service.clone(),
            environment: environment.to_string(),
            registry: item.registry.clone(),
            repository: item.repository.clone(),
            tag: item.tag.clone(),
            digest: digest.to_string(),
            image_ref: format!("{}/{}@{}", item.registry, item.repository, digest),
            provenance: item.provenance.clone(),
            published_at: published_at.to_string(),
        };

        artifact.validate_against_plan_item(environment, item)?;
        Ok(artifact)
    }
    pub fn validate(&self) -> Result<(), ArtifactError> {
        crate::oci::validate_sha256_digest(&self.digest)?;
        crate::oci::validate_repository_component(&self.service)?;
        crate::oci::validate_tag(&self.tag)?;
        self.provenance.validate()?;

        let expected_ref = format!("{}/{}@{}", self.registry, self.repository, self.digest);
        if self.image_ref != expected_ref {
            return Err(ArtifactError::Validation(format!(
                "image_ref mismatch: expected {}, got {}",
                expected_ref, self.image_ref
            )));
        }
        if self.service.trim().is_empty() {
            return Err(ArtifactError::Validation(
                "service cannot be empty".to_string(),
            ));
        }
        if self.environment.trim().is_empty() {
            return Err(ArtifactError::Validation(
                "environment cannot be empty".to_string(),
            ));
        }
        if self.registry.trim().is_empty() {
            return Err(ArtifactError::Validation(
                "registry cannot be empty".to_string(),
            ));
        }
        if self.repository.trim().is_empty() {
            return Err(ArtifactError::Validation(
                "repository cannot be empty".to_string(),
            ));
        }
        if self.tag.trim().is_empty() {
            return Err(ArtifactError::Validation("tag cannot be empty".to_string()));
        }

        chrono::DateTime::parse_from_rfc3339(&self.published_at).map_err(|e| {
            ArtifactError::Validation(format!("published_at is not valid RFC 3339: {}", e))
        })?;

        Ok(())
    }

    pub fn validate_against_plan_item(
        &self,
        report_environment: &str,
        item: &ImagePushPlanItem,
    ) -> Result<(), ArtifactError> {
        self.validate()?;
        item.validate()?;
        if self.environment != report_environment {
            return Err(ArtifactError::Validation(format!(
                "artifact environment '{}' does not match report environment '{}'",
                self.environment, report_environment
            )));
        }
        if self.service != item.service
            || self.registry != item.registry
            || self.repository != item.repository
            || self.tag != item.tag
            || self.provenance != item.provenance
        {
            return Err(ArtifactError::Validation(format!(
                "published artifact for '{}' does not match its push-plan item",
                self.service
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct DigestEvidence {
    pub push_output_digest: Option<String>,
    pub inspected_digest: Option<String>,
}

pub fn resolve_digest(evidence: DigestEvidence) -> Result<String, ArtifactError> {
    match (evidence.push_output_digest, evidence.inspected_digest) {
        (Some(push), Some(inspected)) => {
            crate::oci::validate_sha256_digest(&push)?;
            crate::oci::validate_sha256_digest(&inspected)?;
            if push != inspected {
                return Err(ArtifactError::DigestMismatch {
                    expected: inspected,
                    actual: push,
                });
            }
            Ok(inspected)
        }
        (Some(push), None) => {
            crate::oci::validate_sha256_digest(&push)?;
            Ok(push)
        }
        (None, Some(inspected)) => {
            crate::oci::validate_sha256_digest(&inspected)?;
            Ok(inspected)
        }
        (None, None) => Err(ArtifactError::MissingDigest(
            "No digest provided".to_string(),
        )),
    }
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
    #[serde(alias = "image_ref")]
    pub target_image_ref: String,
    pub local_image_ref: String,
    pub tag: String,
    pub provenance: ImageProvenance,
    pub action: ImagePushPlanAction,
}

impl ImagePushPlanItem {
    pub fn validate(&self) -> Result<(), ArtifactError> {
        if self.service.trim().is_empty() {
            return Err(ArtifactError::Validation(
                "push-plan service cannot be blank".to_string(),
            ));
        }
        if self.registry.trim().is_empty() {
            return Err(ArtifactError::Validation(
                "push-plan registry cannot be blank".to_string(),
            ));
        }
        if self.repository.trim().is_empty() {
            return Err(ArtifactError::Validation(
                "push-plan repository cannot be blank".to_string(),
            ));
        }
        if self.tag.trim().is_empty() {
            return Err(ArtifactError::Validation(
                "push-plan tag cannot be blank".to_string(),
            ));
        }
        if self.local_image_ref.trim().is_empty() {
            return Err(ArtifactError::Validation(
                "local image reference cannot be blank".to_string(),
            ));
        }
        crate::oci::validate_repository_component(&self.service)?;
        crate::oci::validate_tag(&self.tag)?;
        self.provenance.validate()?;
        let expected_target = format!("{}/{}:{}", self.registry, self.repository, self.tag);
        if self.target_image_ref != expected_target {
            return Err(ArtifactError::Validation(format!(
                "target_image_ref mismatch: expected {}, got {}",
                expected_target, self.target_image_ref
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImagePushPlanReport {
    pub environment: String,
    pub mutates_registry: bool,
    pub items: Vec<ImagePushPlanItem>,
}

impl ImagePushPlanReport {
    pub fn validate(&self) -> Result<(), ArtifactError> {
        if self.environment.trim().is_empty() {
            return Err(ArtifactError::Validation(
                "push-plan environment cannot be blank".to_string(),
            ));
        }
        if self.items.is_empty() && self.mutates_registry {
            return Err(ArtifactError::Validation(
                "empty push plan cannot mutate the registry".to_string(),
            ));
        }
        let mut services = std::collections::BTreeSet::new();
        let mut targets = std::collections::BTreeSet::new();
        for item in &self.items {
            item.validate()?;
            if !services.insert(item.service.as_str()) {
                return Err(ArtifactError::Validation(format!(
                    "duplicate push-plan service: {}",
                    item.service
                )));
            }
            if !targets.insert(item.target_image_ref.as_str()) {
                return Err(ArtifactError::Validation(format!(
                    "duplicate target image reference: {}",
                    item.target_image_ref
                )));
            }
        }
        Ok(())
    }
}

pub fn derive_image_tag(build_fingerprint: &str) -> Result<String, ArtifactError> {
    if build_fingerprint.trim().is_empty() {
        return Err(ArtifactError::Validation(
            "build fingerprint cannot be blank".to_string(),
        ));
    }
    let tag = build_fingerprint.chars().take(7).collect::<String>();
    crate::oci::validate_tag(&tag)?;
    Ok(tag)
}

pub fn parse_pushed_digest(output: &str) -> Result<Option<String>, ArtifactError> {
    let mut digests = std::collections::HashSet::new();
    let mut has_marker = false;
    let mut has_invalid_marker = false;

    for line in output.lines() {
        let marker = "digest:";
        if let Some(idx) = line.find(marker) {
            has_marker = true;
            let rest = line[idx + marker.len()..].trim();
            if let Some(digest) = rest.split_whitespace().next() {
                if crate::oci::validate_sha256_digest(digest).is_ok() {
                    digests.insert(digest.to_string());
                } else {
                    has_invalid_marker = true;
                }
            } else {
                has_invalid_marker = true;
            }
        }
    }

    if has_invalid_marker {
        return Err(ArtifactError::Validation(
            "push output contained an invalid digest marker".to_string(),
        ));
    }

    if digests.is_empty() {
        if has_marker {
            return Err(ArtifactError::Validation(
                "push output contained digest markers but no valid digest".to_string(),
            ));
        }
        return Ok(None);
    }

    if digests.len() > 1 {
        return Err(ArtifactError::Validation(
            "ambiguous push output contained multiple digests".to_string(),
        ));
    }

    Ok(Some(digests.into_iter().next().unwrap()))
}

pub fn pushed_artifact_from_output(
    environment: &str,
    item: &ImagePushPlanItem,
    output: &str,
    structured_digest: Option<&str>,
) -> Result<PublishedImageArtifact, String> {
    let parsed_digest = parse_pushed_digest(output).map_err(|e| e.to_string())?;

    let evidence = DigestEvidence {
        push_output_digest: parsed_digest,
        inspected_digest: structured_digest.map(|s| s.to_string()),
    };

    let digest = resolve_digest(evidence)
        .map_err(|e| format!("digest error for {}: {}", item.target_image_ref, e))?;

    let published_at = chrono::Utc::now().to_rfc3339();

    PublishedImageArtifact::from_push_result(environment, item, &digest, &published_at)
        .map_err(|e| format!("invalid published artifact: {:?}", e))
}

#[cfg(test)]
mod tests_derive {
    use super::*;
    #[test]
    fn test_derive_image_tag() {
        assert_eq!(
            derive_image_tag("2bcc3f70984bb6d33d93bbcbb9eb3539ce033dc8").unwrap(),
            "2bcc3f7"
        );
        assert_eq!(derive_image_tag("abc").unwrap(), "abc");
        assert!(derive_image_tag(" ").is_err());
    }

    #[test]
    fn parses_digest_from_stdout_style_output() {
        let output = "latest: digest: sha256:d3f443b7e71c6628b030c6a53fef1c9b6f87452140416cd64b547285227fbd87 size: 1234";
        assert_eq!(
            parse_pushed_digest(output).unwrap().as_deref(),
            Some("sha256:d3f443b7e71c6628b030c6a53fef1c9b6f87452140416cd64b547285227fbd87")
        );
    }

    #[test]
    fn parses_digest_from_stderr_style_combined_output() {
        let output = "some stdout\nlatest: digest: sha256:d3f443b7e71c6628b030c6a53fef1c9b6f87452140416cd64b547285227fbd87 size: 1234";
        assert_eq!(
            parse_pushed_digest(output).unwrap().as_deref(),
            Some("sha256:d3f443b7e71c6628b030c6a53fef1c9b6f87452140416cd64b547285227fbd87")
        );
    }

    #[test]
    fn test_parse_pushed_digest_not_found() {
        let output = "Some output without digest";
        assert_eq!(parse_pushed_digest(output).unwrap(), None);
    }

    #[test]
    fn test_pushed_artifact_from_output_success() {
        let item = ImagePushPlanItem {
            service: "api".to_string(),
            registry: "ghcr.io".to_string(),
            repository: "org/api".to_string(),
            tag: "latest".to_string(),
            target_image_ref: "ghcr.io/org/api:latest".to_string(),
            local_image_ref: "ghcr.io/org/api:latest".to_string(),
            provenance: ImageProvenance {
                build_fingerprint: "abc12345".to_string(),
                source_revision: Some("abc12345".to_string()),
            },
            action: ImagePushPlanAction::WouldPush,
        };
        let output =
            "digest: sha256:0000000000000000000000000000000000000000000000000000000000000000";
        let artifact = pushed_artifact_from_output("prod", &item, output, None).unwrap();
        assert_eq!(
            artifact.digest,
            "sha256:0000000000000000000000000000000000000000000000000000000000000000"
        );
        assert_eq!(artifact.image_ref, "ghcr.io/org/api@sha256:0000000000000000000000000000000000000000000000000000000000000000");
    }

    #[test]
    fn test_pushed_artifact_from_output_failure() {
        let item = ImagePushPlanItem {
            service: "api".to_string(),
            registry: "ghcr.io".to_string(),
            repository: "org/api".to_string(),
            tag: "latest".to_string(),
            target_image_ref: "ghcr.io/org/api:latest".to_string(),
            local_image_ref: "ghcr.io/org/api:latest".to_string(),
            provenance: ImageProvenance {
                build_fingerprint: "dev".to_string(),
                source_revision: Some("dev".to_string()),
            },
            action: ImagePushPlanAction::WouldPush,
        };
        let output = "no digest here";
        let err = pushed_artifact_from_output("prod", &item, output, None).unwrap_err();
        assert!(err.contains("digest error"));
    }

    #[test]
    fn pushed_digest_parser_rejects_invalid_and_conflicting_evidence() {
        let a = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let b = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        assert_eq!(
            parse_pushed_digest(&format!("digest: {a}\ndigest: {a}"))
                .unwrap()
                .as_deref(),
            Some(a)
        );
        assert!(parse_pushed_digest(&format!("digest: {a}\ndigest: {b}")).is_err());
        for invalid in [
            "sha256:abc",
            "sha256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "sha256:１２３４５６７８９０１２３４５６７８９０１２３４５６７８９０１２３４５６７８９０１２３４",
        ] {
            assert!(parse_pushed_digest(&format!("digest: {invalid}")).is_err());
            assert!(parse_pushed_digest(&format!("digest: {a}\ndigest: {invalid}")).is_err());
        }
    }

    #[test]
    fn digest_evidence_accepts_single_or_matching_sources_and_rejects_mismatch() {
        let a = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let b = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        for evidence in [
            DigestEvidence {
                push_output_digest: Some(a.to_string()),
                inspected_digest: None,
            },
            DigestEvidence {
                push_output_digest: None,
                inspected_digest: Some(a.to_string()),
            },
            DigestEvidence {
                push_output_digest: Some(a.to_string()),
                inspected_digest: Some(a.to_string()),
            },
        ] {
            assert_eq!(resolve_digest(evidence).unwrap(), a);
        }
        assert!(resolve_digest(DigestEvidence {
            push_output_digest: Some(a.to_string()),
            inspected_digest: Some(b.to_string()),
        })
        .is_err());
        assert!(resolve_digest(DigestEvidence {
            push_output_digest: None,
            inspected_digest: None,
        })
        .is_err());
    }
}

#[cfg(test)]
mod tests_addendum {
    use super::*;

    fn valid_item(service: &str, target: &str) -> ImagePushPlanItem {
        ImagePushPlanItem {
            service: service.to_string(),
            registry: "ghcr.io".to_string(),
            repository: format!("acme/{service}"),
            tag: "abc1234".to_string(),
            target_image_ref: target.to_string(),
            local_image_ref: format!("ghcr.io/acme/{service}:1.0.0"),
            provenance: ImageProvenance {
                build_fingerprint: "abc123456789".to_string(),
                source_revision: Some("revision".to_string()),
            },
            action: ImagePushPlanAction::WouldPush,
        }
    }

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
                target_image_ref: "ghcr.io/adriftdev/sailr/ci-build-hello:61eaa8b".to_string(),
                local_image_ref: "ghcr.io/adriftdev/sailr/ci-build-hello:61eaa8b".to_string(),
                provenance: ImageProvenance {
                    build_fingerprint: "61eaa8bb0e52f5bb1d5a621760b0a2eae601ccd3".to_string(),
                    source_revision: Some("61eaa8bb0e52f5bb1d5a621760b0a2eae601ccd3".to_string()),
                },
                action: ImagePushPlanAction::WouldPush,
            }],
        };

        let json = serde_json::to_value(report).unwrap();

        assert_eq!(json["environment"], "staging");
        assert_eq!(json["mutates_registry"], false);
        assert_eq!(json["items"][0]["action"], "would_push");
    }

    #[test]
    fn validates_push_plan_items_and_duplicate_contracts() {
        let item = valid_item("api", "ghcr.io/acme/api:abc1234");
        item.validate().unwrap();

        let mut invalid_target = item.clone();
        invalid_target.target_image_ref = "docker.io/other/api:abc1234".to_string();
        assert!(invalid_target.validate().is_err());

        assert!(ImagePushPlanReport {
            environment: "staging".to_string(),
            mutates_registry: true,
            items: vec![item.clone(), item.clone()],
        }
        .validate()
        .is_err());

        let mut same_target = valid_item("web", "ghcr.io/acme/web:abc1234");
        same_target.target_image_ref = item.target_image_ref.clone();
        same_target.repository = item.repository.clone();
        assert!(ImagePushPlanReport {
            environment: "staging".to_string(),
            mutates_registry: true,
            items: vec![item, same_target],
        }
        .validate()
        .is_err());

        assert!(ImagePushPlanReport {
            environment: "staging".to_string(),
            mutates_registry: true,
            items: vec![],
        }
        .validate()
        .is_err());
    }
}
