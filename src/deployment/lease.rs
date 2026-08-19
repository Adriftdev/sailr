use crate::deployment::bundle::DeploymentTarget;
use crate::environment::ReleaseLockPolicy;
use crate::errors::DeployError;
use chrono::{Duration as ChronoDuration, Utc};
use k8s_openapi::api::coordination::v1::{Lease, LeaseSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::MicroTime;
use kube::api::PostParams;
use kube::Api;
use sha2::{Digest, Sha256};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub struct ReleaseLease {
    api: Api<Lease>,
    name: String,
    namespace: String,
    holder: String,
    lost: Arc<AtomicBool>,
    renewal: tokio::task::JoinHandle<()>,
}

pub struct LeaseCheckedBackend<'a> {
    pub inner: &'a dyn crate::deployment::DeploymentBackend,
    pub lease: &'a ReleaseLease,
}

#[async_trait::async_trait]
impl crate::deployment::DeploymentBackend for LeaseCheckedBackend<'_> {
    async fn get(
        &self,
        identity: &crate::deployment::bundle::ResourceIdentity,
    ) -> Result<Option<kube::core::DynamicObject>, DeployError> {
        self.lease.ensure_held()?;
        self.inner.get(identity).await
    }

    async fn apply(
        &self,
        resource: &crate::deployment::bundle::DeploymentResource,
    ) -> Result<kube::core::DynamicObject, DeployError> {
        self.lease.ensure_held()?;
        self.inner.apply(resource).await
    }

    async fn restore(
        &self,
        identity: &crate::deployment::bundle::ResourceIdentity,
        previous: &kube::core::DynamicObject,
    ) -> Result<(), DeployError> {
        self.lease.ensure_held()?;
        self.inner.restore(identity, previous).await
    }

    async fn delete(
        &self,
        identity: &crate::deployment::bundle::ResourceIdentity,
    ) -> Result<(), DeployError> {
        self.lease.ensure_held()?;
        self.inner.delete(identity).await
    }
}

impl ReleaseLease {
    pub async fn acquire(
        client: kube::Client,
        environment: &str,
        target: &DeploymentTarget,
        release_id: &str,
        policy: &ReleaseLockPolicy,
    ) -> Result<Self, DeployError> {
        if policy.lease_duration_seconds == 0
            || policy.lease_duration_seconds > i32::MAX as u32
            || policy.renew_interval_seconds == 0
            || policy.renew_interval_seconds >= u64::from(policy.lease_duration_seconds)
        {
            return Err(lock_error(
                "release lock renewal interval must be positive and shorter than its duration",
            ));
        }
        let namespace = policy
            .lease_namespace
            .clone()
            .unwrap_or_else(|| target.namespace.clone());
        let name = policy.lease_name.clone().unwrap_or_else(|| {
            let key = format!("{environment}\0{}\0{}", target.context, target.namespace);
            format!(
                "sailr-release-{}",
                &hex::encode(Sha256::digest(key.as_bytes()))[..16]
            )
        });
        let api: Api<Lease> = Api::namespaced(client, &namespace);
        let now = Utc::now();
        let desired = Lease {
            metadata: kube::core::ObjectMeta {
                name: Some(name.clone()),
                namespace: Some(namespace.clone()),
                labels: Some(std::collections::BTreeMap::from([(
                    "app.kubernetes.io/managed-by".to_string(),
                    "sailr".to_string(),
                )])),
                annotations: Some(std::collections::BTreeMap::from([
                    ("sailr.dev/environment".to_string(), environment.to_string()),
                    (
                        "sailr.dev/target-context".to_string(),
                        target.context.clone(),
                    ),
                    (
                        "sailr.dev/target-namespace".to_string(),
                        target.namespace.clone(),
                    ),
                    ("sailr.dev/release-id".to_string(), release_id.to_string()),
                ])),
                ..Default::default()
            },
            spec: Some(LeaseSpec {
                acquire_time: Some(MicroTime(now)),
                renew_time: Some(MicroTime(now)),
                holder_identity: Some(release_id.to_string()),
                lease_duration_seconds: Some(policy.lease_duration_seconds as i32),
                lease_transitions: Some(0),
                ..Default::default()
            }),
        };

        match api.get_opt(&name).await.map_err(kube_error)? {
            None => {
                api.create(&PostParams::default(), &desired)
                    .await
                    .map_err(kube_error)?;
            }
            Some(mut existing) => {
                if !is_stale(&existing, now) {
                    let holder = existing
                        .spec
                        .as_ref()
                        .and_then(|spec| spec.holder_identity.as_deref())
                        .unwrap_or("unknown");
                    return Err(lock_error(&format!(
                        "release lock {namespace}/{name} is held by {holder}"
                    )));
                }
                let transitions = existing
                    .spec
                    .as_ref()
                    .and_then(|spec| spec.lease_transitions)
                    .unwrap_or(0)
                    + 1;
                existing.spec = desired.spec.clone();
                if let Some(spec) = existing.spec.as_mut() {
                    spec.lease_transitions = Some(transitions);
                }
                existing.metadata.labels = desired.metadata.labels.clone();
                existing.metadata.annotations = desired.metadata.annotations.clone();
                api.replace(&name, &PostParams::default(), &existing)
                    .await
                    .map_err(kube_error)?;
            }
        }

        let lost = Arc::new(AtomicBool::new(false));
        let renewal_api = api.clone();
        let renewal_name = name.clone();
        let renewal_holder = release_id.to_string();
        let renewal_lost = lost.clone();
        let interval = policy.renew_interval_seconds;
        let duration = policy.lease_duration_seconds as i32;
        let renewal = tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
                let Ok(mut lease) = renewal_api.get(&renewal_name).await else {
                    renewal_lost.store(true, Ordering::SeqCst);
                    break;
                };
                if lease
                    .spec
                    .as_ref()
                    .and_then(|spec| spec.holder_identity.as_deref())
                    != Some(renewal_holder.as_str())
                {
                    renewal_lost.store(true, Ordering::SeqCst);
                    break;
                }
                let spec = lease.spec.get_or_insert_with(LeaseSpec::default);
                spec.renew_time = Some(MicroTime(Utc::now()));
                spec.lease_duration_seconds = Some(duration);
                if renewal_api
                    .replace(&renewal_name, &PostParams::default(), &lease)
                    .await
                    .is_err()
                {
                    renewal_lost.store(true, Ordering::SeqCst);
                    break;
                }
            }
        });

        Ok(Self {
            api,
            name,
            namespace,
            holder: release_id.to_string(),
            lost,
            renewal,
        })
    }

    pub fn ensure_held(&self) -> Result<(), DeployError> {
        if self.lost.load(Ordering::SeqCst) {
            Err(lock_error("release lock ownership was lost"))
        } else {
            Ok(())
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    pub async fn release(self) -> Result<(), DeployError> {
        self.renewal.abort();
        let Some(mut lease) = self.api.get_opt(&self.name).await.map_err(kube_error)? else {
            return Ok(());
        };
        if lease
            .spec
            .as_ref()
            .and_then(|spec| spec.holder_identity.as_deref())
            != Some(self.holder.as_str())
        {
            return Ok(());
        }
        let spec = lease.spec.get_or_insert_with(LeaseSpec::default);
        spec.holder_identity = None;
        spec.renew_time = Some(MicroTime(Utc::now()));
        self.api
            .replace(&self.name, &PostParams::default(), &lease)
            .await
            .map_err(kube_error)?;
        Ok(())
    }
}

fn is_stale(lease: &Lease, now: chrono::DateTime<Utc>) -> bool {
    let Some(spec) = lease.spec.as_ref() else {
        return true;
    };
    if spec.holder_identity.as_deref().is_none_or(str::is_empty) {
        return true;
    }
    let Some(duration) = spec.lease_duration_seconds else {
        return true;
    };
    let timestamp = spec
        .renew_time
        .as_ref()
        .or(spec.acquire_time.as_ref())
        .map(|time| time.0);
    timestamp.is_none_or(|time| time + ChronoDuration::seconds(i64::from(duration)) <= now)
}

fn lock_error(message: &str) -> DeployError {
    DeployError::EnvironmentDeploymentFailed(message.to_string())
}

fn kube_error(error: kube::Error) -> DeployError {
    lock_error(&format!("release lock Kubernetes error: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lease(holder: Option<&str>, renewed_seconds_ago: i64, duration: i32) -> Lease {
        Lease {
            spec: Some(LeaseSpec {
                holder_identity: holder.map(str::to_string),
                renew_time: Some(MicroTime(
                    Utc::now() - ChronoDuration::seconds(renewed_seconds_ago),
                )),
                lease_duration_seconds: Some(duration),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn stale_detection_requires_expiry_or_an_empty_holder() {
        let now = Utc::now();
        assert!(!is_stale(&lease(Some("release-a"), 10, 60), now));
        assert!(is_stale(&lease(Some("release-a"), 61, 60), now));
        assert!(is_stale(&lease(None, 1, 60), now));
    }
}
