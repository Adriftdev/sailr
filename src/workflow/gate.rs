use base64::Engine;
use ring::signature::{UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub const APPROVAL_PUBLIC_KEY_ENV: &str = "DEPLOY_APPROVAL_PUBKEY";
pub const APPROVAL_SIGNATURE_ENV: &str = "DEPLOY_APPROVAL_SIG";
pub const APPROVAL_SCHEMA: &str = "sailr.audit/v1";
pub const SIGNING_DOMAIN: &str = "sailr-deployment-plan-v1";
pub const MANIFEST_HASH_OUTPUT: &str = "manifest_sha256";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestDigest {
    pub relative_path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalPayload {
    pub schema: String,
    pub profile: String,
    pub environment: String,
    pub context: String,
    pub namespace: String,
    pub manifests: Vec<ManifestDigest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalArtifact {
    pub payload: ApprovalPayload,
    pub plan_hash: String,
}

impl ApprovalArtifact {
    pub fn signing_message(&self) -> String {
        format!("{SIGNING_DOMAIN}:{}", self.plan_hash)
    }
}

pub fn audit_artifact_path(profile: &str) -> PathBuf {
    Path::new(".sailr")
        .join("audit")
        .join(profile)
        .join("deployment-plan.json")
}

pub fn generated_manifest_root(environment: &str) -> PathBuf {
    Path::new("k8s").join("generated").join(environment)
}

pub fn build_artifact(
    root: &Path,
    profile: &str,
    environment: &str,
    context: &str,
    namespace: &str,
) -> anyhow::Result<ApprovalArtifact> {
    let manifests = collect_manifest_digests(root)?;
    let payload = ApprovalPayload {
        schema: APPROVAL_SCHEMA.to_string(),
        profile: profile.to_string(),
        environment: environment.to_string(),
        context: context.to_string(),
        namespace: namespace.to_string(),
        manifests,
    };
    let bytes = serde_json::to_vec(&payload)?;
    let plan_hash = hex::encode(Sha256::digest(bytes));
    Ok(ApprovalArtifact { payload, plan_hash })
}

pub fn write_artifact(path: &Path, artifact: &ApprovalArtifact) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Audit artifact path has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(artifact)?;
    let result = (|| -> anyhow::Result<()> {
        use std::io::Write;
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

pub fn build_and_write_artifact(
    profile: &str,
    environment: &str,
    context: &str,
    namespace: &str,
) -> anyhow::Result<ApprovalArtifact> {
    let artifact = build_artifact(
        &generated_manifest_root(environment),
        profile,
        environment,
        context,
        namespace,
    )?;
    write_artifact(&audit_artifact_path(profile), &artifact)?;
    Ok(artifact)
}

pub fn verify_signature(
    artifact: &ApprovalArtifact,
    public_key_base64: &str,
    signature_base64: &str,
) -> anyhow::Result<()> {
    let public_key = base64::engine::general_purpose::STANDARD
        .decode(public_key_base64.trim())
        .map_err(|_| anyhow::anyhow!("DEPLOY_APPROVAL_PUBKEY is not valid base64"))?;
    if public_key.len() != 32 {
        anyhow::bail!("DEPLOY_APPROVAL_PUBKEY must decode to exactly 32 bytes");
    }

    let signature = base64::engine::general_purpose::STANDARD
        .decode(signature_base64.trim())
        .map_err(|_| anyhow::anyhow!("DEPLOY_APPROVAL_SIG is not valid base64"))?;
    if signature.len() != 64 {
        anyhow::bail!("DEPLOY_APPROVAL_SIG must decode to exactly 64 bytes");
    }

    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(artifact.signing_message().as_bytes(), &signature)
        .map_err(|_| {
            anyhow::anyhow!("AUDIT FAILURE: Signature does not match the immutable plan hash.")
        })
}

pub fn build_verification_task(
    dependencies: &[String],
    profile: String,
    environment: String,
    context: String,
    namespace: String,
) -> runkernel::Task {
    let dependency_refs = dependencies.iter().map(String::as_str).collect::<Vec<_>>();
    runkernel::Task::new(crate::workflow::task_id::VERIFICATION_GATE)
        .description("Verifies an Ed25519 signature over the immutable deployment plan.")
        .depends_on(&dependency_refs)
        .cache_disabled()
        .exec_fn(move |ctx| {
            let profile = profile.clone();
            let environment = environment.clone();
            let context = context.clone();
            let namespace = namespace.clone();
            async move {
                let expected_hash: String = ctx.output_from(
                    crate::workflow::task_id::GENERATE,
                    MANIFEST_HASH_OUTPUT,
                )?;
                let current = build_artifact(
                    &generated_manifest_root(&environment),
                    &profile,
                    &environment,
                    &context,
                    &namespace,
                )?;

                if current.plan_hash != expected_hash {
                    anyhow::bail!(
                        "AUDIT FAILURE: Generated manifests or deployment target changed after planning.\nPlan Hash: {}",
                        expected_hash
                    );
                }

                let public_key = ctx.env(APPROVAL_PUBLIC_KEY_ENV).map_err(|_| {
                    anyhow::anyhow!(
                        "DEPLOYMENT LOCKED: Missing {}.\nPlan Hash: {}",
                        APPROVAL_PUBLIC_KEY_ENV,
                        expected_hash
                    )
                })?;
                let signature = ctx.env(APPROVAL_SIGNATURE_ENV).map_err(|_| {
                    anyhow::anyhow!(
                        "DEPLOYMENT LOCKED: Missing {}.\nPlan Hash: {}\nSign '{}' offline and inject the signature to proceed.",
                        APPROVAL_SIGNATURE_ENV,
                        expected_hash,
                        current.signing_message()
                    )
                })?;
                verify_signature(&current, &public_key, &signature)?;
                crate::LOGGER.info(&format!(
                    "[GATE] verified immutable deployment plan {}",
                    expected_hash
                ));
                Ok(())
            }
        })
}

fn collect_manifest_digests(root: &Path) -> anyhow::Result<Vec<ManifestDigest>> {
    if !root.exists() {
        anyhow::bail!(
            "Generated manifest directory does not exist: {}",
            root.display()
        );
    }

    let mut paths = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| matches!(extension, "yaml" | "yml"))
        })
        .collect::<Vec<_>>();
    paths.sort();

    let mut manifests = Vec::with_capacity(paths.len());
    for path in paths {
        let relative = path
            .strip_prefix(root)
            .map_err(|_| anyhow::anyhow!("Manifest escaped generated root: {}", path.display()))?
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = std::fs::read(&path)?;
        manifests.push(ManifestDigest {
            relative_path: relative,
            sha256: hex::encode(Sha256::digest(bytes)),
        });
    }
    Ok(manifests)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    #[test]
    fn artifact_hash_is_order_independent_and_target_bound() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("b.yaml"), "kind: Service\n").unwrap();
        std::fs::write(dir.path().join("a.yml"), "kind: Deployment\n").unwrap();

        let first = build_artifact(dir.path(), "prod", "production", "cluster-a", "app").unwrap();
        let second = build_artifact(dir.path(), "prod", "production", "cluster-a", "app").unwrap();
        assert_eq!(first, second);
        assert_eq!(first.payload.manifests[0].relative_path, "a.yml");

        let other = build_artifact(dir.path(), "prod", "production", "cluster-b", "app").unwrap();
        assert_ne!(first.plan_hash, other.plan_hash);

        std::fs::write(dir.path().join("a.yml"), "kind: Secret\n").unwrap();
        let tampered =
            build_artifact(dir.path(), "prod", "production", "cluster-a", "app").unwrap();
        assert_ne!(first.plan_hash, tampered.plan_hash);
    }

    #[test]
    fn signature_verification_accepts_only_matching_artifact() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("app.yaml"), "kind: Service\n").unwrap();
        let artifact = build_artifact(dir.path(), "prod", "prod", "cluster", "app").unwrap();

        let rng = SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let key = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let signature = key.sign(artifact.signing_message().as_bytes());
        let public = base64::engine::general_purpose::STANDARD.encode(key.public_key().as_ref());
        let encoded_sig = base64::engine::general_purpose::STANDARD.encode(signature.as_ref());

        verify_signature(&artifact, &public, &encoded_sig).unwrap();

        let changed = build_artifact(dir.path(), "prod", "prod", "other-cluster", "app").unwrap();
        assert!(verify_signature(&changed, &public, &encoded_sig).is_err());
        assert!(verify_signature(&artifact, "not-base64", &encoded_sig).is_err());
        assert!(verify_signature(&artifact, &public, "AA==").is_err());
    }
}
