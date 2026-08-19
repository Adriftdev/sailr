use crate::errors::DeployError;
use base64::Engine;
use kube::core::DynamicObject;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub const DEPLOYMENT_BUNDLE_SCHEMA: &str = "sailr.deployment-bundle/v1";
pub const LEGACY_DEPLOYMENT_AUDIT_SCHEMA: &str = "sailr.audit/v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeploymentTarget {
    pub context: String,
    pub namespace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(deny_unknown_fields)]
pub struct ResourceIdentity {
    pub api_version: String,
    pub kind: String,
    pub namespace: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct DeploymentResource {
    pub source_path: String,
    pub document_index: usize,
    pub identity: ResourceIdentity,
    pub raw_bytes: Vec<u8>,
    pub object: DynamicObject,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct DeploymentBundle {
    pub schema: String,
    pub profile: String,
    pub environment: String,
    pub target: DeploymentTarget,
    pub resources: Vec<DeploymentResource>,
    pub plan_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BoundPostDeployHook {
    pub service: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PortableDeploymentResource {
    pub source_path: String,
    pub document_index: usize,
    pub identity: ResourceIdentity,
    pub canonical_json_base64: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PortableBundlePayload {
    pub schema_version: String,
    pub profile: String,
    pub environment: String,
    pub target: DeploymentTarget,
    pub approval: crate::workflow::profile::ApprovalMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signer_key_fingerprint: Option<String>,
    pub promotion_plan_digest: String,
    pub publication_report_digests: Vec<String>,
    pub services: Vec<crate::workflow::promotion::PromotionService>,
    #[serde(default)]
    pub post_deploy_hooks: Vec<BoundPostDeployHook>,
    pub resources: Vec<PortableDeploymentResource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PortableDeploymentBundle {
    #[serde(flatten)]
    pub payload: PortableBundlePayload,
    pub plan_hash: String,
}

impl PortableDeploymentBundle {
    pub fn from_runtime(
        bundle: &DeploymentBundle,
        approval: crate::workflow::profile::ApprovalMode,
        signer_key_fingerprint: Option<String>,
        promotion: &crate::workflow::promotion::PromotionPlan,
        post_deploy_hooks: Vec<BoundPostDeployHook>,
    ) -> Result<Self, DeployError> {
        let payload = PortableBundlePayload {
            schema_version: DEPLOYMENT_BUNDLE_SCHEMA.to_string(),
            profile: bundle.profile.clone(),
            environment: bundle.environment.clone(),
            target: bundle.target.clone(),
            approval,
            signer_key_fingerprint,
            promotion_plan_digest: promotion
                .canonical_digest()
                .map_err(|error| DeployError::ManifestApplicationFailed(error.to_string()))?,
            publication_report_digests: promotion
                .source_reports
                .iter()
                .map(|source| source.digest.clone())
                .collect(),
            services: promotion.services.clone(),
            post_deploy_hooks,
            resources: bundle
                .resources
                .iter()
                .map(|resource| PortableDeploymentResource {
                    source_path: resource.source_path.clone(),
                    document_index: resource.document_index,
                    identity: resource.identity.clone(),
                    canonical_json_base64: base64::engine::general_purpose::STANDARD
                        .encode(&resource.raw_bytes),
                    sha256: resource.sha256.clone(),
                })
                .collect(),
        };
        let plan_hash = hash_portable_payload(&payload)?;
        let portable = Self { payload, plan_hash };
        portable.validate()?;
        Ok(portable)
    }

    pub fn validate(&self) -> Result<(), DeployError> {
        let fail = |message: String| DeployError::ManifestApplicationFailed(message);
        if self.payload.schema_version != DEPLOYMENT_BUNDLE_SCHEMA {
            return Err(fail(format!(
                "Unsupported deployment bundle schema: {}",
                self.payload.schema_version
            )));
        }
        if self.payload.profile.trim().is_empty()
            || self.payload.environment.trim().is_empty()
            || self.payload.target.context.trim().is_empty()
            || self.payload.target.namespace.trim().is_empty()
        {
            return Err(fail(
                "Deployment bundle bindings cannot be blank".to_string(),
            ));
        }
        validate_prefixed_sha256(&self.payload.promotion_plan_digest, "promotion plan digest")?;
        if self.payload.publication_report_digests.is_empty() {
            return Err(fail(
                "Deployment bundle contains no publication report digests".to_string(),
            ));
        }
        let mut previous_report_digest: Option<&str> = None;
        for digest in &self.payload.publication_report_digests {
            validate_prefixed_sha256(digest, "publication report digest")?;
            if previous_report_digest.is_some_and(|previous| previous >= digest.as_str()) {
                return Err(fail(
                    "Publication report digests must be uniquely sorted".to_string(),
                ));
            }
            previous_report_digest = Some(digest);
        }
        match self.payload.approval {
            crate::workflow::profile::ApprovalMode::Signature => {
                let fingerprint =
                    self.payload
                        .signer_key_fingerprint
                        .as_deref()
                        .ok_or_else(|| {
                            fail(
                                "Signed deployment bundle is missing signer fingerprint"
                                    .to_string(),
                            )
                        })?;
                validate_prefixed_sha256(fingerprint, "signer fingerprint")?;
            }
            crate::workflow::profile::ApprovalMode::External => {
                if self.payload.signer_key_fingerprint.is_some() {
                    return Err(fail(
                        "External-approval bundle cannot contain a signer fingerprint".to_string(),
                    ));
                }
            }
            _ => {
                return Err(fail(
                    "Portable deployment bundle requires external or signature approval"
                        .to_string(),
                ));
            }
        }
        if self.payload.resources.is_empty() {
            return Err(fail("Deployment bundle contains no resources".to_string()));
        }
        if self.payload.services.is_empty() {
            return Err(fail(
                "Deployment bundle contains no promoted services".to_string(),
            ));
        }
        let mut service_names = BTreeSet::new();
        let mut previous_service: Option<&str> = None;
        let mut promoted_images = BTreeSet::new();
        for service in &self.payload.services {
            if service.service.trim().is_empty()
                || service.registry.trim().is_empty()
                || service.repository.trim().is_empty()
                || service.build_fingerprint.trim().is_empty()
                || service.source_revision.trim().is_empty()
                || service.source_revision.chars().any(char::is_whitespace)
            {
                return Err(fail(format!(
                    "Invalid promoted service metadata for '{}'",
                    service.service
                )));
            }
            crate::oci::validate_sha256_digest(&service.digest)
                .map_err(|error| fail(error.to_string()))?;
            let expected_image = format!(
                "{}/{}@{}",
                service.registry, service.repository, service.digest
            );
            if service.image_ref != expected_image {
                return Err(fail(format!(
                    "Invalid promoted image reference for '{}'",
                    service.service
                )));
            }
            chrono::DateTime::parse_from_rfc3339(&service.published_at).map_err(|error| {
                fail(format!(
                    "Invalid publication timestamp for '{}': {error}",
                    service.service
                ))
            })?;
            if !service_names.insert(service.service.as_str()) {
                return Err(fail(format!(
                    "Duplicate promoted service '{}'",
                    service.service
                )));
            }
            if previous_service.is_some_and(|previous| previous >= service.service.as_str()) {
                return Err(fail(
                    "Promoted services must be sorted by service name".to_string(),
                ));
            }
            previous_service = Some(service.service.as_str());
            promoted_images.insert(service.image_ref.as_str());
        }
        for hook in &self.payload.post_deploy_hooks {
            if !service_names.contains(hook.service.as_str()) || hook.command.trim().is_empty() {
                return Err(fail(format!(
                    "Invalid bound post-deployment hook for '{}'",
                    hook.service
                )));
            }
        }
        let mut identities = BTreeSet::new();
        let mut locations = BTreeSet::new();
        let mut bound_images = BTreeSet::new();
        let mut previous_location: Option<(&str, usize)> = None;
        for resource in &self.payload.resources {
            let path = Path::new(&resource.source_path);
            if resource.source_path.trim().is_empty()
                || path.is_absolute()
                || path
                    .components()
                    .any(|component| matches!(component, std::path::Component::ParentDir))
            {
                return Err(fail(format!(
                    "Invalid resource source path: {}",
                    resource.source_path
                )));
            }
            if !identities.insert(resource.identity.clone()) {
                return Err(fail(format!(
                    "Duplicate Kubernetes resource identity: {} {} {}",
                    resource.identity.api_version, resource.identity.kind, resource.identity.name
                )));
            }
            if !locations.insert((resource.source_path.as_str(), resource.document_index)) {
                return Err(fail(format!(
                    "Duplicate resource location: {} document {}",
                    resource.source_path, resource.document_index
                )));
            }
            if previous_location.is_some_and(|previous| {
                previous >= (resource.source_path.as_str(), resource.document_index)
            }) {
                return Err(fail(
                    "Deployment resources must be sorted by source path and document index"
                        .to_string(),
                ));
            }
            previous_location = Some((resource.source_path.as_str(), resource.document_index));
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&resource.canonical_json_base64)
                .map_err(|_| {
                    fail(format!(
                        "Invalid canonical bytes for {}",
                        resource.source_path
                    ))
                })?;
            let actual = hex::encode(Sha256::digest(&bytes));
            if actual != resource.sha256 {
                return Err(fail(format!(
                    "Canonical resource digest mismatch for {} document {}",
                    resource.source_path, resource.document_index
                )));
            }
            let object: DynamicObject = serde_json::from_slice(&bytes).map_err(|error| {
                fail(format!(
                    "Invalid canonical Kubernetes resource {} document {}: {error}",
                    resource.source_path, resource.document_index
                ))
            })?;
            let canonical = serde_json::to_vec(&object).map_err(|error| {
                fail(format!(
                    "Failed to canonicalize {} document {}: {error}",
                    resource.source_path, resource.document_index
                ))
            })?;
            if canonical != bytes {
                return Err(fail(format!(
                    "Canonical resource bytes mismatch for {} document {}",
                    resource.source_path, resource.document_index
                )));
            }
            let value = serde_json::to_value(&object).map_err(|error| {
                fail(format!(
                    "Failed to inspect images in {} document {}: {error}",
                    resource.source_path, resource.document_index
                ))
            })?;
            if let Some(pod_spec) = workload_pod_spec(&value) {
                collect_image_references(pod_spec, &mut bound_images);
            }
            let identity = identity_from_object(&object, &self.payload.target.namespace)?;
            if identity != resource.identity {
                return Err(fail(format!(
                    "Resource identity mismatch for {} document {}",
                    resource.source_path, resource.document_index
                )));
            }
        }
        for image_ref in promoted_images {
            if !bound_images.contains(image_ref) {
                return Err(fail(format!(
                    "Promoted image '{image_ref}' is not bound to a bundled workload"
                )));
            }
        }
        let expected = hash_portable_payload(&self.payload)?;
        if self.plan_hash != expected {
            return Err(fail("Deployment bundle plan hash mismatch".to_string()));
        }
        Ok(())
    }

    pub fn to_runtime(&self) -> Result<DeploymentBundle, DeployError> {
        self.validate()?;
        let resources = self
            .payload
            .resources
            .iter()
            .map(|resource| {
                let raw_bytes = base64::engine::general_purpose::STANDARD
                    .decode(&resource.canonical_json_base64)
                    .map_err(|_| {
                        DeployError::ManifestApplicationFailed(format!(
                            "Invalid canonical bytes for {}",
                            resource.source_path
                        ))
                    })?;
                let object = serde_json::from_slice(&raw_bytes).map_err(|error| {
                    DeployError::ManifestApplicationFailed(format!(
                        "Invalid canonical Kubernetes resource: {error}"
                    ))
                })?;
                Ok(DeploymentResource {
                    source_path: resource.source_path.clone(),
                    document_index: resource.document_index,
                    identity: resource.identity.clone(),
                    raw_bytes,
                    object,
                    sha256: resource.sha256.clone(),
                })
            })
            .collect::<Result<Vec<_>, DeployError>>()?;
        Ok(DeploymentBundle {
            schema: self.payload.schema_version.clone(),
            profile: self.payload.profile.clone(),
            environment: self.payload.environment.clone(),
            target: self.payload.target.clone(),
            resources,
            plan_hash: self.plan_hash.clone(),
        })
    }
}

fn collect_image_references(value: &serde_json::Value, images: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                if key == "image" {
                    if let Some(image) = child.as_str() {
                        images.insert(image.to_string());
                    }
                }
                collect_image_references(child, images);
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                collect_image_references(child, images);
            }
        }
        _ => {}
    }
}

fn workload_pod_spec(value: &serde_json::Value) -> Option<&serde_json::Value> {
    match value.get("kind").and_then(serde_json::Value::as_str) {
        Some("Pod") => value.pointer("/spec"),
        Some("CronJob") => value.pointer("/spec/jobTemplate/spec/template/spec"),
        Some(
            "Deployment"
            | "StatefulSet"
            | "DaemonSet"
            | "ReplicaSet"
            | "Job"
            | "ReplicationController",
        ) => value.pointer("/spec/template/spec"),
        _ => None,
    }
}

fn hash_portable_payload(payload: &PortableBundlePayload) -> Result<String, DeployError> {
    let bytes = serde_json::to_vec(payload).map_err(|error| {
        DeployError::ManifestApplicationFailed(format!(
            "Failed to serialize portable deployment bundle: {error}"
        ))
    })?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn validate_prefixed_sha256(value: &str, label: &str) -> Result<(), DeployError> {
    let Some(hex_value) = value.strip_prefix("sha256:") else {
        return Err(DeployError::ManifestApplicationFailed(format!(
            "{label} must use sha256:<hex>"
        )));
    };
    if hex_value.len() != 64
        || !hex_value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(DeployError::ManifestApplicationFailed(format!(
            "{label} must use sha256:<64 lowercase hex>"
        )));
    }
    Ok(())
}

fn identity_from_object(
    object: &DynamicObject,
    default_namespace: &str,
) -> Result<ResourceIdentity, DeployError> {
    let types = object.types.as_ref().ok_or_else(|| {
        DeployError::ManifestApplicationFailed(
            "Canonical resource is missing apiVersion or kind".to_string(),
        )
    })?;
    let name = object.metadata.name.clone().ok_or_else(|| {
        DeployError::ManifestApplicationFailed(
            "Canonical resource is missing metadata.name".to_string(),
        )
    })?;
    Ok(ResourceIdentity {
        api_version: types.api_version.clone(),
        kind: types.kind.clone(),
        namespace: object
            .metadata
            .namespace
            .clone()
            .or_else(|| Some(default_namespace.to_string())),
        name,
    })
}

pub fn read_portable(path: &Path) -> Result<PortableDeploymentBundle, DeployError> {
    let bytes = std::fs::read(path).map_err(|error| {
        DeployError::ManifestApplicationFailed(format!(
            "Failed to read deployment bundle '{}': {error}",
            path.display()
        ))
    })?;
    let bundle: PortableDeploymentBundle = serde_json::from_slice(&bytes).map_err(|error| {
        DeployError::ManifestApplicationFailed(format!(
            "Failed to parse deployment bundle '{}': {error}",
            path.display()
        ))
    })?;
    bundle.validate()?;
    Ok(bundle)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestDigest {
    pub relative_path: String,
    pub document_index: usize,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalPayload {
    pub schema: String,
    pub profile: String,
    pub environment: String,
    pub target: DeploymentTarget,
    pub manifests: Vec<ManifestDigest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalArtifact {
    pub payload: ApprovalPayload,
    pub plan_hash: String,
}

impl DeploymentBundle {
    pub fn approval_artifact(&self) -> ApprovalArtifact {
        ApprovalArtifact {
            payload: ApprovalPayload {
                schema: self.schema.clone(),
                profile: self.profile.clone(),
                environment: self.environment.clone(),
                target: self.target.clone(),
                manifests: self
                    .resources
                    .iter()
                    .map(|resource| ManifestDigest {
                        relative_path: resource.source_path.clone(),
                        document_index: resource.document_index,
                        sha256: resource.sha256.clone(),
                    })
                    .collect(),
            },
            plan_hash: self.plan_hash.clone(),
        }
    }
}

pub fn build_deployment_bundle(
    root: &Path,
    profile: &str,
    environment: &str,
    context: &str,
    namespace: &str,
) -> Result<DeploymentBundle, DeployError> {
    if !root.exists() {
        return Err(DeployError::ManifestApplicationFailed(format!(
            "Generated manifest directory does not exist: {}",
            root.display()
        )));
    }

    let mut paths = yaml_paths(root)?;
    paths.sort();
    let target = DeploymentTarget {
        context: context.to_string(),
        namespace: namespace.to_string(),
    };
    let mut resources = Vec::new();
    let mut identities = BTreeSet::new();

    for path in paths {
        let relative_path = path
            .strip_prefix(root)
            .map_err(|_| {
                DeployError::ManifestApplicationFailed(format!(
                    "Manifest escaped generated root: {}",
                    path.display()
                ))
            })?
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = std::fs::read(&path).map_err(|error| {
            DeployError::ManifestApplicationFailed(format!(
                "Failed to read {}: {error}",
                path.display()
            ))
        })?;

        for (document_index, document) in serde_yaml::Deserializer::from_slice(&bytes).enumerate() {
            let value = serde_yaml::Value::deserialize(document).map_err(|error| {
                DeployError::ManifestApplicationFailed(format!(
                    "Failed to parse {} document {}: {error}",
                    path.display(),
                    document_index
                ))
            })?;
            if value.is_null() {
                continue;
            }
            let object: DynamicObject = serde_yaml::from_value(value).map_err(|error| {
                DeployError::ManifestApplicationFailed(format!(
                    "Failed to decode {} document {} as a Kubernetes object: {error}",
                    path.display(),
                    document_index
                ))
            })?;
            let types = object.types.as_ref().ok_or_else(|| {
                DeployError::ManifestApplicationFailed(format!(
                    "{} document {} is missing apiVersion or kind",
                    path.display(),
                    document_index
                ))
            })?;
            if types.api_version.trim().is_empty() || types.kind.trim().is_empty() {
                return Err(DeployError::ManifestApplicationFailed(format!(
                    "{} document {} is missing apiVersion or kind",
                    path.display(),
                    document_index
                )));
            }
            let name = object.metadata.name.clone().ok_or_else(|| {
                DeployError::ManifestApplicationFailed(format!(
                    "{} document {} is missing metadata.name",
                    path.display(),
                    document_index
                ))
            })?;
            let identity = ResourceIdentity {
                api_version: types.api_version.clone(),
                kind: types.kind.clone(),
                namespace: object
                    .metadata
                    .namespace
                    .clone()
                    .or_else(|| Some(namespace.to_string())),
                name,
            };
            if !identities.insert(identity.clone()) {
                return Err(DeployError::ManifestApplicationFailed(format!(
                    "Duplicate Kubernetes resource identity: {} {} {}/{}",
                    identity.api_version,
                    identity.kind,
                    identity.namespace.as_deref().unwrap_or("<cluster>"),
                    identity.name
                )));
            }

            let raw_bytes = serde_json::to_vec(&object).map_err(|error| {
                DeployError::ManifestApplicationFailed(format!(
                    "Failed to canonicalize {} document {}: {error}",
                    path.display(),
                    document_index
                ))
            })?;
            let sha256 = hex::encode(Sha256::digest(&raw_bytes));
            resources.push(DeploymentResource {
                source_path: relative_path.clone(),
                document_index,
                identity,
                raw_bytes,
                object,
                sha256,
            });
        }
    }

    if resources.is_empty() {
        return Err(DeployError::ManifestApplicationFailed(
            "Deployment bundle contains no Kubernetes resources".to_string(),
        ));
    }

    let payload = ApprovalPayload {
        schema: DEPLOYMENT_BUNDLE_SCHEMA.to_string(),
        profile: profile.to_string(),
        environment: environment.to_string(),
        target: target.clone(),
        manifests: resources
            .iter()
            .map(|resource| ManifestDigest {
                relative_path: resource.source_path.clone(),
                document_index: resource.document_index,
                sha256: resource.sha256.clone(),
            })
            .collect(),
    };
    let canonical = serde_json::to_vec(&payload).map_err(|error| {
        DeployError::ManifestApplicationFailed(format!(
            "Failed to serialize deployment bundle: {error}"
        ))
    })?;
    let plan_hash = hex::encode(Sha256::digest(canonical));

    Ok(DeploymentBundle {
        schema: DEPLOYMENT_BUNDLE_SCHEMA.to_string(),
        profile: profile.to_string(),
        environment: environment.to_string(),
        target,
        resources,
        plan_hash,
    })
}

fn yaml_paths(root: &Path) -> Result<Vec<PathBuf>, DeployError> {
    WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            DeployError::ManifestApplicationFailed(format!(
                "Failed to traverse {}: {error}",
                root.display()
            ))
        })
        .map(|entries| {
            entries
                .into_iter()
                .filter(|entry| entry.file_type().is_file())
                .map(|entry| entry.into_path())
                .filter(|path| {
                    path.extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| matches!(extension, "yaml" | "yml"))
                })
                .collect()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().expect("fixture parent")).expect("create fixture");
        std::fs::write(path, contents).expect("write fixture");
    }

    #[test]
    fn bundle_is_deterministic_ordered_and_target_bound() {
        let root = tempfile::tempdir().expect("tempdir");
        write(
            &root.path().join("b.yaml"),
            "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: b\n",
        );
        write(
            &root.path().join("a.yaml"),
            "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: a\n",
        );
        let first =
            build_deployment_bundle(root.path(), "prod", "prod", "ctx", "ns").expect("bundle");
        let second =
            build_deployment_bundle(root.path(), "prod", "prod", "ctx", "ns").expect("bundle");
        assert_eq!(first.plan_hash, second.plan_hash);
        assert_eq!(first.resources[0].source_path, "a.yaml");
        assert_eq!(first.resources[1].source_path, "b.yaml");

        let changed =
            build_deployment_bundle(root.path(), "prod", "prod", "other", "ns").expect("bundle");
        assert_ne!(first.plan_hash, changed.plan_hash);
    }

    #[test]
    fn bundle_rejects_empty_missing_metadata_and_duplicate_identity() {
        let empty = tempfile::tempdir().expect("tempdir");
        assert!(build_deployment_bundle(empty.path(), "p", "e", "c", "n").is_err());

        let missing = tempfile::tempdir().expect("tempdir");
        write(
            &missing.path().join("bad.yaml"),
            "apiVersion: v1\nkind: ConfigMap\nmetadata: {}\n",
        );
        assert!(build_deployment_bundle(missing.path(), "p", "e", "c", "n").is_err());

        let duplicate = tempfile::tempdir().expect("tempdir");
        write(
            &duplicate.path().join("objects.yaml"),
            "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: same\n---\napiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: same\n",
        );
        assert!(build_deployment_bundle(duplicate.path(), "p", "e", "c", "n").is_err());
    }

    #[test]
    fn portable_bundle_round_trip_detects_resource_and_plan_tampering() {
        let root = tempfile::tempdir().expect("tempdir");
        let digest = format!("sha256:{}", "a".repeat(64));
        write(
            &root.path().join("a.yaml"),
            &format!(
                "apiVersion: v1\nkind: Pod\nmetadata:\n  name: a\nspec:\n  containers:\n    - name: api\n      image: docker.io/api@{digest}\n"
            ),
        );
        let runtime =
            build_deployment_bundle(root.path(), "prod", "prod", "ctx", "ns").expect("bundle");
        let promotion = crate::workflow::promotion::PromotionPlan {
            schema_version: crate::workflow::promotion::PROMOTION_PLAN_SCHEMA.to_string(),
            target_environment: "prod".to_string(),
            source_reports: vec![crate::workflow::promotion::PromotionSourceReport {
                schema_version: "sailr.workflow-report/v1".to_string(),
                profile: "publish".to_string(),
                environment: "staging".to_string(),
                digest: format!("sha256:{}", "b".repeat(64)),
            }],
            services: vec![crate::workflow::promotion::PromotionService {
                service: "api".to_string(),
                registry: "docker.io".to_string(),
                repository: "api".to_string(),
                digest: digest.clone(),
                image_ref: format!("docker.io/api@{digest}"),
                build_fingerprint: "fingerprint".to_string(),
                source_revision: "revision".to_string(),
                published_at: "2026-08-18T12:00:00Z".to_string(),
            }],
        };
        let portable = PortableDeploymentBundle::from_runtime(
            &runtime,
            crate::workflow::profile::ApprovalMode::External,
            None,
            &promotion,
            Vec::new(),
        )
        .expect("portable bundle");
        assert_eq!(
            portable.to_runtime().expect("runtime").resources[0].raw_bytes,
            runtime.resources[0].raw_bytes
        );

        let mut resource_tamper = portable.clone();
        resource_tamper.payload.resources[0]
            .canonical_json_base64
            .push('A');
        assert!(resource_tamper.validate().is_err());

        let mut provenance_tamper = portable.clone();
        provenance_tamper
            .payload
            .publication_report_digests
            .push(format!("sha256:{}", "a".repeat(64)));
        assert!(provenance_tamper.validate().is_err());

        let mut hash_tamper = portable;
        hash_tamper.plan_hash = "0".repeat(64);
        assert!(hash_tamper.validate().is_err());
    }
}
