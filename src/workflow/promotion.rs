use crate::environment::Environment;
use crate::workflow::publication;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const PROMOTION_PLAN_SCHEMA: &str = "sailr.promotion-plan/v1";
pub const RELEASE_CANDIDATES_SCHEMA: &str = "sailr.release-candidates/v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CandidatePublicationReport {
    pub path: String,
    pub report_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReleaseCandidateManifest {
    pub schema_version: String,
    pub publication_reports: Vec<CandidatePublicationReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PromotionSourceReport {
    pub schema_version: String,
    pub profile: String,
    pub environment: String,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PromotionService {
    pub service: String,
    pub registry: String,
    pub repository: String,
    pub digest: String,
    pub image_ref: String,
    pub build_fingerprint: String,
    pub source_revision: String,
    pub published_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PromotionPlan {
    pub schema_version: String,
    pub target_environment: String,
    pub source_reports: Vec<PromotionSourceReport>,
    pub services: Vec<PromotionService>,
}

impl PromotionPlan {
    pub fn canonical_digest(&self) -> Result<String, String> {
        self.validate_shape()?;
        let bytes = serde_json::to_vec(self)
            .map_err(|error| format!("failed to canonicalize promotion plan: {error}"))?;
        Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
    }

    pub fn image_overrides(&self) -> BTreeMap<String, String> {
        self.services
            .iter()
            .map(|service| (service.service.clone(), service.image_ref.clone()))
            .collect()
    }

    pub fn validate_for_environment(&self, environment: &Environment) -> Result<(), String> {
        self.validate_shape()?;
        if self.target_environment != environment.name {
            return Err(format!(
                "promotion target '{}' does not match environment '{}'",
                self.target_environment, environment.name
            ));
        }
        let expected_services = environment
            .services
            .iter()
            .filter(|service| service.is_release_artifact())
            .map(|service| service.name.as_str())
            .collect::<BTreeSet<_>>();
        let actual_services = self
            .services
            .iter()
            .map(|service| service.service.as_str())
            .collect::<BTreeSet<_>>();
        if expected_services != actual_services {
            let missing = expected_services
                .difference(&actual_services)
                .copied()
                .collect::<Vec<_>>();
            let unknown = actual_services
                .difference(&expected_services)
                .copied()
                .collect::<Vec<_>>();
            return Err(format!(
                "promotion service coverage mismatch for build-backed services (missing: [{}], unknown: [{}])",
                missing.join(", "),
                unknown.join(", ")
            ));
        }
        let registry = environment
            .registry
            .resolve()
            .map_err(|error| format!("invalid target registry: {error}"))?;
        for service in &self.services {
            let expected = registry
                .digest_ref(&service.service, &service.digest)
                .map_err(|error| error.to_string())?;
            if service.image_ref != expected {
                return Err(format!(
                    "promotion image for '{}' is '{}', but target environment requires '{}' (cross-repository copying is not supported)",
                    service.service, service.image_ref, expected
                ));
            }
        }
        Ok(())
    }

    fn validate_shape(&self) -> Result<(), String> {
        if self.schema_version != PROMOTION_PLAN_SCHEMA {
            return Err(format!(
                "unsupported promotion schema '{}'",
                self.schema_version
            ));
        }
        if self.target_environment.trim().is_empty() || self.services.is_empty() {
            return Err("promotion target and services must be nonempty".to_string());
        }
        if self.source_reports.is_empty() {
            return Err("promotion source reports must be nonempty".to_string());
        }
        let mut previous_digest: Option<&str> = None;
        let mut source_environment: Option<&str> = None;
        for source in &self.source_reports {
            if source.schema_version != "sailr.workflow-report/v1"
                || source.profile.trim().is_empty()
                || source.environment.trim().is_empty()
            {
                return Err("promotion source report metadata is invalid".to_string());
            }
            validate_sha256_identity(&source.digest, "source report digest")?;
            if previous_digest.is_some_and(|digest| digest >= source.digest.as_str()) {
                return Err(
                    "promotion source reports must be uniquely sorted by digest".to_string()
                );
            }
            if source_environment.is_some_and(|environment| environment != source.environment) {
                return Err("promotion source reports must use one source environment".to_string());
            }
            previous_digest = Some(source.digest.as_str());
            source_environment = Some(source.environment.as_str());
        }
        let mut previous: Option<&str> = None;
        let mut names = BTreeSet::new();
        for service in &self.services {
            crate::oci::validate_sha256_digest(&service.digest)
                .map_err(|error| error.to_string())?;
            if service.image_ref
                != format!(
                    "{}/{}@{}",
                    service.registry, service.repository, service.digest
                )
            {
                return Err(format!(
                    "invalid digest-bearing image_ref for '{}'",
                    service.service
                ));
            }
            if service.build_fingerprint.trim().is_empty()
                || service.source_revision.trim().is_empty()
                || service.source_revision.chars().any(char::is_whitespace)
            {
                return Err(format!("invalid provenance for '{}'", service.service));
            }
            chrono::DateTime::parse_from_rfc3339(&service.published_at).map_err(|error| {
                format!("invalid published_at for '{}': {error}", service.service)
            })?;
            if !names.insert(service.service.as_str()) {
                return Err(format!("duplicate promotion service '{}'", service.service));
            }
            if previous.is_some_and(|name| name >= service.service.as_str()) {
                return Err("promotion services must be sorted by service name".to_string());
            }
            previous = Some(service.service.as_str());
        }
        Ok(())
    }
}

fn validate_sha256_identity(value: &str, label: &str) -> Result<(), String> {
    let digest = value
        .strip_prefix("sha256:")
        .ok_or_else(|| format!("{label} must use sha256:<hex>"))?;
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("{label} must use sha256:<64 lowercase hex>"));
    }
    Ok(())
}

pub fn load(path: &Path) -> Result<PromotionPlan, String> {
    let bytes = std::fs::read(path).map_err(|error| {
        format!(
            "failed to read promotion plan '{}': {error}",
            path.display()
        )
    })?;
    let plan: PromotionPlan = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "failed to parse promotion plan '{}': {error}",
            path.display()
        )
    })?;
    plan.validate_shape()?;
    Ok(plan)
}

pub fn create(
    from_reports: &[std::path::PathBuf],
    target_environment: &str,
) -> Result<PromotionPlan, String> {
    if from_reports.is_empty() {
        return Err("at least one publication report is required".to_string());
    }
    create_from_selected_reports(
        from_reports
            .iter()
            .map(|path| (path.clone(), None))
            .collect(),
        target_environment,
    )
}

pub fn create_from_manifest(
    path: &Path,
    target_environment: &str,
) -> Result<PromotionPlan, String> {
    create_from_selected_reports(load_candidate_manifest(path)?, target_environment)
}

fn load_candidate_manifest(
    path: &Path,
) -> Result<Vec<(std::path::PathBuf, Option<String>)>, String> {
    let bytes = std::fs::read(path).map_err(|error| {
        format!(
            "failed to read candidate manifest '{}': {error}",
            path.display()
        )
    })?;
    let manifest: ReleaseCandidateManifest = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "failed to parse candidate manifest '{}': {error}",
            path.display()
        )
    })?;
    if manifest.schema_version != RELEASE_CANDIDATES_SCHEMA {
        return Err(format!(
            "unsupported candidate manifest schema '{}'",
            manifest.schema_version
        ));
    }
    if manifest.publication_reports.is_empty() {
        return Err("candidate manifest must contain publication reports".to_string());
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut paths = BTreeSet::new();
    let mut digests = BTreeSet::new();
    let mut selected = Vec::new();
    for candidate in manifest.publication_reports {
        validate_sha256_identity(&candidate.report_digest, "candidate report digest")?;
        let relative = Path::new(&candidate.path);
        if candidate.path.trim().is_empty()
            || relative.is_absolute()
            || relative
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            return Err(format!(
                "candidate report path '{}' must be a safe relative path",
                candidate.path
            ));
        }
        if !paths.insert(candidate.path.clone()) {
            return Err(format!(
                "duplicate candidate report path '{}'",
                candidate.path
            ));
        }
        if !digests.insert(candidate.report_digest.clone()) {
            return Err(format!(
                "duplicate candidate report digest '{}'",
                candidate.report_digest
            ));
        }
        selected.push((parent.join(relative), Some(candidate.report_digest)));
    }
    Ok(selected)
}

fn create_from_selected_reports(
    selected: Vec<(std::path::PathBuf, Option<String>)>,
    target_environment: &str,
) -> Result<PromotionPlan, String> {
    let environment = Environment::load_from_file(target_environment).map_err(|error| {
        format!("failed to load target environment '{target_environment}': {error}")
    })?;
    create_from_selected_reports_for_environment(selected, target_environment, &environment)
}

fn create_from_selected_reports_for_environment(
    selected: Vec<(std::path::PathBuf, Option<String>)>,
    target_environment: &str,
    environment: &Environment,
) -> Result<PromotionPlan, String> {
    let mut source_reports = Vec::new();
    let mut services = Vec::new();
    let mut report_digests = BTreeSet::new();
    let mut service_names = BTreeSet::new();
    let mut source_environment: Option<String> = None;
    for (path, expected_digest) in selected {
        let (report, validation) = publication::load_and_validate(&path)?;
        if expected_digest
            .as_deref()
            .is_some_and(|expected| expected != validation.report_digest)
        {
            return Err(format!(
                "candidate report '{}' canonical digest does not match manifest",
                path.display()
            ));
        }
        if !report_digests.insert(validation.report_digest.clone()) {
            return Err(format!(
                "duplicate publication report digest '{}'",
                validation.report_digest
            ));
        }
        if source_environment
            .as_deref()
            .is_some_and(|environment| environment != report.environment)
        {
            return Err("publication reports must use one source environment".to_string());
        }
        source_environment = Some(report.environment.clone());
        source_reports.push(PromotionSourceReport {
            schema_version: report.schema_version.clone(),
            profile: report.profile.clone(),
            environment: report.environment.clone(),
            digest: validation.report_digest,
        });
        for artifact in &report.artifacts.published_images {
            if !service_names.insert(artifact.service.clone()) {
                return Err(format!(
                    "duplicate promotion service '{}'",
                    artifact.service
                ));
            }
            services.push(PromotionService {
                service: artifact.service.clone(),
                registry: artifact.registry.clone(),
                repository: artifact.repository.clone(),
                digest: artifact.digest.clone(),
                image_ref: artifact.image_ref.clone(),
                build_fingerprint: artifact.provenance.build_fingerprint.clone(),
                source_revision: artifact.provenance.source_revision.clone().ok_or_else(|| {
                    format!(
                        "published service '{}' is missing source_revision",
                        artifact.service
                    )
                })?,
                published_at: artifact.published_at.clone(),
            });
        }
    }
    source_reports.sort_by(|left, right| left.digest.cmp(&right.digest));
    services.sort_by(|left, right| left.service.cmp(&right.service));
    let plan = PromotionPlan {
        schema_version: PROMOTION_PLAN_SCHEMA.to_string(),
        target_environment: target_environment.to_string(),
        source_reports,
        services,
    };
    plan.validate_for_environment(environment)?;
    Ok(plan)
}

pub fn write(path: &Path, plan: &PromotionPlan) -> Result<(), String> {
    plan.validate_shape()?;
    if path.exists() {
        return Err(format!(
            "refusing to overwrite existing promotion plan '{}'",
            path.display()
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create '{}': {error}", parent.display()))?;
    }
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(plan)
        .map_err(|error| format!("failed to serialize promotion plan: {error}"))?;
    std::fs::write(&temporary, bytes)
        .map_err(|error| format!("failed to write '{}': {error}", temporary.display()))?;
    std::fs::rename(&temporary, path)
        .map_err(|error| format!("failed to install '{}': {error}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::{RegistryConfig, Service, ServiceBuildConfig};

    fn digest(character: char) -> String {
        format!("sha256:{}", character.to_string().repeat(64))
    }

    fn service(name: &str, character: char) -> PromotionService {
        let digest = digest(character);
        PromotionService {
            service: name.to_string(),
            registry: "docker.io".to_string(),
            repository: name.to_string(),
            image_ref: format!("docker.io/{name}@{digest}"),
            digest,
            build_fingerprint: format!("fingerprint-{name}"),
            source_revision: "0123456789abcdef".to_string(),
            published_at: "2026-08-18T12:00:00Z".to_string(),
        }
    }

    fn build_backed_service(name: &str) -> Service {
        let mut service = Service::new(name, None, "old");
        service.build = Some(ServiceBuildConfig {
            path: format!("services/{name}"),
            include: None,
            ignore_cache: None,
            relies_on: None,
            before_synchronous: None,
            before: None,
            run_parallel: None,
            run_synchronous: None,
            after: None,
            finally: None,
            dockerfile: None,
            build_command: None,
            push_command: None,
        });
        service
    }

    fn plan(services: Vec<PromotionService>) -> PromotionPlan {
        PromotionPlan {
            schema_version: PROMOTION_PLAN_SCHEMA.to_string(),
            target_environment: "prod".to_string(),
            source_reports: vec![PromotionSourceReport {
                schema_version: "sailr.workflow-report/v1".to_string(),
                profile: "publish".to_string(),
                environment: "staging".to_string(),
                digest: digest('a'),
            }],
            services,
        }
    }

    #[test]
    fn promotion_is_deterministic_and_requires_complete_target_coverage() {
        let mut environment = Environment::new("prod");
        environment.registry = RegistryConfig::Simple("docker.io".to_string());
        environment.services = vec![
            build_backed_service("api"),
            build_backed_service("worker"),
            Service::new("nanomq", None, "0.24.5-slim"),
        ];
        let complete = plan(vec![service("api", 'b'), service("worker", 'c')]);
        complete
            .validate_for_environment(&environment)
            .expect("valid promotion");
        assert_eq!(
            complete.canonical_digest().expect("digest"),
            complete.canonical_digest().expect("digest")
        );

        let incomplete = plan(vec![service("api", 'b')]);
        assert!(incomplete.validate_for_environment(&environment).is_err());

        let unsorted = plan(vec![service("worker", 'c'), service("api", 'b')]);
        assert!(unsorted.validate_for_environment(&environment).is_err());

        let external_dependency = plan(vec![
            service("api", 'b'),
            service("nanomq", 'd'),
            service("worker", 'c'),
        ]);
        assert!(external_dependency
            .validate_for_environment(&environment)
            .is_err());

        let mut duplicate_sources = complete.clone();
        duplicate_sources
            .source_reports
            .push(duplicate_sources.source_reports[0].clone());
        assert!(duplicate_sources
            .validate_for_environment(&environment)
            .is_err());
    }

    #[test]
    fn candidate_manifest_is_strict_and_rejects_unsafe_or_duplicate_selection() {
        let directory = tempfile::tempdir().expect("tempdir");
        let unsafe_manifest = ReleaseCandidateManifest {
            schema_version: RELEASE_CANDIDATES_SCHEMA.to_string(),
            publication_reports: vec![CandidatePublicationReport {
                path: "../api.json".to_string(),
                report_digest: digest('a'),
            }],
        };
        let unsafe_path = directory.path().join("unsafe.json");
        std::fs::write(
            &unsafe_path,
            serde_json::to_vec(&unsafe_manifest).expect("manifest"),
        )
        .expect("write");
        assert!(create_from_manifest(&unsafe_path, "unused").is_err());

        let duplicate_manifest = ReleaseCandidateManifest {
            schema_version: RELEASE_CANDIDATES_SCHEMA.to_string(),
            publication_reports: vec![
                CandidatePublicationReport {
                    path: "api.json".to_string(),
                    report_digest: digest('a'),
                },
                CandidatePublicationReport {
                    path: "worker.json".to_string(),
                    report_digest: digest('a'),
                },
            ],
        };
        let duplicate_path = directory.path().join("duplicate.json");
        std::fs::write(
            &duplicate_path,
            serde_json::to_vec(&duplicate_manifest).expect("manifest"),
        )
        .expect("write");
        assert!(create_from_manifest(&duplicate_path, "unused").is_err());

        let unknown = format!(
            r#"{{"schema_version":"{}","publication_reports":[],"extra":true}}"#,
            RELEASE_CANDIDATES_SCHEMA
        );
        assert!(serde_json::from_str::<ReleaseCandidateManifest>(&unknown).is_err());
    }

    #[test]
    fn direct_and_manifest_selection_are_identical_and_manifest_binds_report_bytes() {
        let directory = tempfile::tempdir().expect("tempdir");
        let report_path = directory.path().join("api.json");
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/reports/image-publication-success.json");
        let mut report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(fixture).expect("fixture")).expect("report");
        let repository = "org/repo/ci-build-hello";
        let digest = report["artifacts"]["published_images"][0]["digest"]
            .as_str()
            .expect("digest")
            .to_string();
        let tagged = format!("ghcr.io/{repository}:1.2.0");
        let immutable = format!("ghcr.io/{repository}@{digest}");
        report["plans"]["image_push"]["items"][0]["service"] = "ci-build-hello".into();
        report["plans"]["image_push"]["items"][0]["repository"] = repository.into();
        report["plans"]["image_push"]["items"][0]["target_image_ref"] = tagged.clone().into();
        report["plans"]["image_push"]["items"][0]["local_image_ref"] = tagged.into();
        report["artifacts"]["published_images"][0]["service"] = "ci-build-hello".into();
        report["artifacts"]["published_images"][0]["repository"] = repository.into();
        report["artifacts"]["published_images"][0]["image_ref"] = immutable.into();
        std::fs::write(
            &report_path,
            serde_json::to_vec_pretty(&report).expect("serialize report"),
        )
        .expect("write report");

        let (_, validation) = publication::load_and_validate(&report_path).expect("valid report");
        let mut environment = Environment::new("prod");
        environment.registry = RegistryConfig::Detailed {
            host: "ghcr.io".to_string(),
            namespace: Some("org/repo".to_string()),
        };
        environment.services = vec![build_backed_service("ci-build-hello")];
        let direct = create_from_selected_reports_for_environment(
            vec![(report_path.clone(), None)],
            "prod",
            &environment,
        )
        .expect("direct plan");

        let manifest_path = directory.path().join("candidates.json");
        let manifest = ReleaseCandidateManifest {
            schema_version: RELEASE_CANDIDATES_SCHEMA.to_string(),
            publication_reports: vec![CandidatePublicationReport {
                path: "api.json".to_string(),
                report_digest: validation.report_digest,
            }],
        };
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).expect("manifest"),
        )
        .expect("write manifest");
        let manifest_plan = create_from_selected_reports_for_environment(
            load_candidate_manifest(&manifest_path).expect("selection"),
            "prod",
            &environment,
        )
        .expect("manifest plan");
        assert_eq!(direct, manifest_plan);
        assert_eq!(
            direct.canonical_digest().expect("direct digest"),
            manifest_plan.canonical_digest().expect("manifest digest")
        );

        report["artifacts"]["published_images"][0]["published_at"] = "2026-08-19T12:00:00Z".into();
        std::fs::write(
            &report_path,
            serde_json::to_vec_pretty(&report).expect("serialize replacement"),
        )
        .expect("replace report");
        assert!(create_from_selected_reports_for_environment(
            load_candidate_manifest(&manifest_path).expect("selection"),
            "prod",
            &environment,
        )
        .is_err());
    }
}
