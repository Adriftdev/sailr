use crate::deployment::bundle::{ApprovalArtifact, DeploymentBundle};
use base64::Engine;
use ring::signature::{UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const APPROVAL_SIGNATURE_ENV: &str = "DEPLOY_APPROVAL_SIG";
pub const SIGNING_DOMAIN: &str = "sailr-deployment-plan-v1";
pub const PLAN_HASH_OUTPUT: &str = "plan_hash";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentVerificationStatus {
    NotRequired,
    #[default]
    AwaitingSignature,
    Verified,
    Failed,
}

#[derive(Debug, Clone)]
pub struct DeploymentVerificationEvidence {
    pub plan_hash: String,
    pub signer_key_fingerprint: String,
    pub status: DeploymentVerificationStatus,
    pub error: Option<String>,
}

#[derive(Debug, Default)]
struct DeploymentRunStateInner {
    bundle: Option<DeploymentBundle>,
    verification: Option<DeploymentVerificationEvidence>,
    journal: Option<crate::deployment::SharedDeploymentJournal>,
}

#[derive(Debug, Clone, Default)]
pub struct DeploymentRunState {
    inner: Arc<Mutex<DeploymentRunStateInner>>,
}

impl DeploymentRunState {
    pub fn set_bundle(
        &self,
        bundle: DeploymentBundle,
        signer_key_fingerprint: Option<String>,
    ) -> anyhow::Result<()> {
        let verification =
            signer_key_fingerprint.map(|fingerprint| DeploymentVerificationEvidence {
                plan_hash: bundle.plan_hash.clone(),
                signer_key_fingerprint: fingerprint,
                status: DeploymentVerificationStatus::AwaitingSignature,
                error: None,
            });
        let mut state = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("Deployment run state lock is poisoned"))?;
        state.bundle = Some(bundle);
        state.verification = verification;
        Ok(())
    }

    pub fn bundle(&self) -> anyhow::Result<DeploymentBundle> {
        self.inner
            .lock()
            .map_err(|_| anyhow::anyhow!("Deployment run state lock is poisoned"))?
            .bundle
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Deployment bundle has not been constructed"))
    }

    pub fn bundle_optional(&self) -> anyhow::Result<Option<DeploymentBundle>> {
        self.inner
            .lock()
            .map_err(|_| anyhow::anyhow!("Deployment run state lock is poisoned"))
            .map(|state| state.bundle.clone())
    }

    pub fn verification(&self) -> anyhow::Result<Option<DeploymentVerificationEvidence>> {
        self.inner
            .lock()
            .map_err(|_| anyhow::anyhow!("Deployment run state lock is poisoned"))
            .map(|state| state.verification.clone())
    }

    pub fn set_verification(
        &self,
        status: DeploymentVerificationStatus,
        error: Option<String>,
    ) -> anyhow::Result<()> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("Deployment run state lock is poisoned"))?;
        let verification = state
            .verification
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Signed deployment verification evidence is missing"))?;
        verification.status = status;
        verification.error = error;
        Ok(())
    }

    pub fn set_journal(
        &self,
        journal: crate::deployment::SharedDeploymentJournal,
    ) -> anyhow::Result<()> {
        self.inner
            .lock()
            .map_err(|_| anyhow::anyhow!("Deployment run state lock is poisoned"))?
            .journal = Some(journal);
        Ok(())
    }

    pub fn journal(&self) -> anyhow::Result<Option<crate::deployment::SharedDeploymentJournal>> {
        self.inner
            .lock()
            .map_err(|_| anyhow::anyhow!("Deployment run state lock is poisoned"))
            .map(|state| state.journal.clone())
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

pub fn decode_trusted_public_key(public_key_base64: &str) -> anyhow::Result<Vec<u8>> {
    let public_key = base64::engine::general_purpose::STANDARD
        .decode(public_key_base64.trim())
        .map_err(|_| anyhow::anyhow!("trusted_public_key is not valid base64"))?;
    if public_key.len() != 32 {
        anyhow::bail!("trusted_public_key must decode to exactly 32 bytes");
    }
    Ok(public_key)
}

pub fn trusted_key_fingerprint(public_key_base64: &str) -> anyhow::Result<String> {
    let public_key = decode_trusted_public_key(public_key_base64)?;
    Ok(format!(
        "sha256:{}",
        hex::encode(Sha256::digest(public_key))
    ))
}

pub fn signing_message(plan_hash: &str) -> String {
    format!("{SIGNING_DOMAIN}:{plan_hash}")
}

pub fn verify_signature(
    bundle: &DeploymentBundle,
    public_key_base64: &str,
    signature_base64: &str,
) -> anyhow::Result<()> {
    let public_key = decode_trusted_public_key(public_key_base64)?;
    let signature = base64::engine::general_purpose::STANDARD
        .decode(signature_base64.trim())
        .map_err(|_| anyhow::anyhow!("DEPLOY_APPROVAL_SIG is not valid base64"))?;
    if signature.len() != 64 {
        anyhow::bail!("DEPLOY_APPROVAL_SIG must decode to exactly 64 bytes");
    }

    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(signing_message(&bundle.plan_hash).as_bytes(), &signature)
        .map_err(|_| {
            anyhow::anyhow!("AUDIT FAILURE: Signature does not match the immutable plan hash.")
        })
}

pub fn build_verification_task(
    dependencies: &[String],
    trusted_public_key: String,
    state: DeploymentRunState,
) -> runkernel::Task {
    let dependency_refs = dependencies.iter().map(String::as_str).collect::<Vec<_>>();
    runkernel::Task::new(crate::workflow::task_id::VERIFICATION_GATE)
        .description("Verifies an Ed25519 signature using the configured trusted signer.")
        .depends_on(&dependency_refs)
        .env_vars(&[APPROVAL_SIGNATURE_ENV])
        .cache_disabled()
        .exec_fn(move |ctx| {
            let trusted_public_key = trusted_public_key.clone();
            let state = state.clone();
            async move {
                let bundle = state.bundle()?;
                let signature = match ctx.env(APPROVAL_SIGNATURE_ENV) {
                    Ok(signature) => signature,
                    Err(_) => {
                        state.set_verification(
                            DeploymentVerificationStatus::AwaitingSignature,
                            None,
                        )?;
                        anyhow::bail!(
                            "DEPLOYMENT LOCKED: Missing {}.\nPlan Hash: {}\nSign '{}' offline and inject the signature to proceed.",
                            APPROVAL_SIGNATURE_ENV,
                            bundle.plan_hash,
                            signing_message(&bundle.plan_hash)
                        );
                    }
                };

                if let Err(error) =
                    verify_signature(&bundle, &trusted_public_key, &signature)
                {
                    state.set_verification(
                        DeploymentVerificationStatus::Failed,
                        Some(error.to_string()),
                    )?;
                    return Err(error);
                }

                state.set_verification(DeploymentVerificationStatus::Verified, None)?;
                crate::LOGGER.info(&format!(
                    "[GATE] verified immutable deployment plan {}",
                    bundle.plan_hash
                ));
                Ok(())
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deployment::bundle::build_deployment_bundle;
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    fn bundle() -> DeploymentBundle {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            root.path().join("config.yaml"),
            "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: approved\n",
        )
        .expect("manifest");
        build_deployment_bundle(root.path(), "prod", "prod", "ctx", "ns").expect("bundle")
    }

    #[test]
    fn configured_signer_verifies_and_wrong_signer_fails() {
        let rng = SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).expect("key");
        let key = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("keypair");
        let bundle = bundle();
        let signature = key.sign(signing_message(&bundle.plan_hash).as_bytes());
        let public_key =
            base64::engine::general_purpose::STANDARD.encode(key.public_key().as_ref());
        let signature = base64::engine::general_purpose::STANDARD.encode(signature.as_ref());
        verify_signature(&bundle, &public_key, &signature).expect("signature");

        let wrong = Ed25519KeyPair::generate_pkcs8(&rng).expect("wrong key");
        let wrong = Ed25519KeyPair::from_pkcs8(wrong.as_ref()).expect("wrong pair");
        let wrong_public =
            base64::engine::general_purpose::STANDARD.encode(wrong.public_key().as_ref());
        assert!(verify_signature(&bundle, &wrong_public, &signature).is_err());
        assert!(verify_signature(&bundle, &public_key, "not-base64").is_err());
        assert!(verify_signature(
            &bundle,
            &public_key,
            &base64::engine::general_purpose::STANDARD.encode([0_u8; 63]),
        )
        .is_err());
    }

    #[test]
    fn key_validation_and_fingerprint_are_stable() {
        let key = base64::engine::general_purpose::STANDARD.encode([7_u8; 32]);
        let fingerprint = trusted_key_fingerprint(&key).expect("fingerprint");
        assert!(fingerprint.starts_with("sha256:"));
        assert_eq!(fingerprint.len(), 71);
        assert!(decode_trusted_public_key("not-base64").is_err());
        assert!(decode_trusted_public_key(
            &base64::engine::general_purpose::STANDARD.encode([1_u8; 31])
        )
        .is_err());
    }
}
