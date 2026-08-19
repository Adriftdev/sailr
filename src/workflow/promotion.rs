use crate::environment::Environment;
use crate::workflow::publication;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const PROMOTION_PLAN_SCHEMA: &str = "sailr.promotion-plan/v1";

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
    pub source_report: PromotionSourceReport,
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
        if self.source_report.schema_version != "sailr.workflow-report/v1"
            || self.source_report.profile.trim().is_empty()
            || self.source_report.environment.trim().is_empty()
        {
            return Err("promotion source report metadata is invalid".to_string());
        }
        validate_sha256_identity(&self.source_report.digest, "source report digest")?;
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

pub fn create(from_report: &Path, target_environment: &str) -> Result<PromotionPlan, String> {
    let (report, validation) = publication::load_and_validate(from_report)?;
    let environment = Environment::load_from_file(target_environment).map_err(|error| {
        format!("failed to load target environment '{target_environment}': {error}")
    })?;
    let mut services = report
        .artifacts
        .published_images
        .iter()
        .map(|artifact| {
            Ok(PromotionService {
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
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    services.sort_by(|left, right| left.service.cmp(&right.service));
    let plan = PromotionPlan {
        schema_version: PROMOTION_PLAN_SCHEMA.to_string(),
        target_environment: target_environment.to_string(),
        source_report: PromotionSourceReport {
            schema_version: report.schema_version,
            profile: report.profile,
            environment: report.environment,
            digest: validation.report_digest,
        },
        services,
    };
    plan.validate_for_environment(&environment)?;
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
            source_report: PromotionSourceReport {
                schema_version: "sailr.workflow-report/v1".to_string(),
                profile: "publish".to_string(),
                environment: "staging".to_string(),
                digest: digest('a'),
            },
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
    }
}
