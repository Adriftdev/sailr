use crate::errors::DeployError;
use kube::core::DynamicObject;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub const DEPLOYMENT_BUNDLE_SCHEMA: &str = "sailr.audit/v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeploymentTarget {
    pub context: String,
    pub namespace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
}
