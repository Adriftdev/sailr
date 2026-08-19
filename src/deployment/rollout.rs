use crate::deployment::bundle::{DeploymentBundle, ResourceIdentity};
use crate::deployment::DeploymentBackend;
use crate::errors::DeployError;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RolloutStatus {
    Ready,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RolloutResult {
    pub identity: ResourceIdentity,
    pub status: RolloutStatus,
    pub observed_generation: Option<i64>,
    pub desired: Option<i64>,
    pub ready: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

enum Readiness {
    Ready(RolloutResult),
    Pending,
    Failed(String),
}

pub async fn verify_bundle_rollout(
    bundle: &DeploymentBundle,
    backend: &dyn DeploymentBackend,
    timeout: Duration,
) -> Result<Vec<RolloutResult>, DeployError> {
    let workloads = bundle
        .resources
        .iter()
        .filter(|resource| {
            matches!(
                resource.identity.kind.as_str(),
                "Deployment" | "StatefulSet" | "DaemonSet"
            )
        })
        .map(|resource| resource.identity.clone())
        .collect::<Vec<_>>();
    if workloads.is_empty() {
        return Ok(Vec::new());
    }

    let started = Instant::now();
    let mut results = Vec::new();
    loop {
        results.clear();
        let mut all_ready = true;
        for identity in &workloads {
            let object = backend.get(identity).await?.ok_or_else(|| {
                DeployError::EnvironmentDeploymentFailed(format!(
                    "{} {} disappeared during rollout verification",
                    identity.kind, identity.name
                ))
            })?;
            let value = serde_json::to_value(&object).map_err(|error| {
                DeployError::EnvironmentDeploymentFailed(format!(
                    "failed to inspect {} {} status: {error}",
                    identity.kind, identity.name
                ))
            })?;
            match readiness(identity, &value) {
                Readiness::Ready(result) => results.push(result),
                Readiness::Pending => all_ready = false,
                Readiness::Failed(error) => {
                    return Err(DeployError::EnvironmentDeploymentFailed(error));
                }
            }
        }
        if all_ready {
            return Ok(results.clone());
        }
        if started.elapsed() >= timeout {
            return Err(DeployError::EnvironmentDeploymentFailed(format!(
                "rollout verification timed out after {} seconds",
                timeout.as_secs()
            )));
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

fn number(value: &serde_json::Value, pointer: &str) -> Option<i64> {
    value.pointer(pointer).and_then(serde_json::Value::as_i64)
}

fn readiness(identity: &ResourceIdentity, value: &serde_json::Value) -> Readiness {
    let generation = number(value, "/metadata/generation").unwrap_or(0);
    let observed = number(value, "/status/observedGeneration");
    if observed.unwrap_or(-1) < generation {
        return Readiness::Pending;
    }

    match identity.kind.as_str() {
        "Deployment" => {
            if value
                .pointer("/status/conditions")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|conditions| {
                    conditions.iter().any(|condition| {
                        condition.get("type").and_then(serde_json::Value::as_str)
                            == Some("Progressing")
                            && condition.get("status").and_then(serde_json::Value::as_str)
                                == Some("False")
                            && condition.get("reason").and_then(serde_json::Value::as_str)
                                == Some("ProgressDeadlineExceeded")
                    })
                })
            {
                return Readiness::Failed(format!(
                    "Deployment {} exceeded its progress deadline",
                    identity.name
                ));
            }
            let desired = number(value, "/spec/replicas").unwrap_or(1);
            let updated = number(value, "/status/updatedReplicas").unwrap_or(0);
            let available = number(value, "/status/availableReplicas").unwrap_or(0);
            let unavailable = number(value, "/status/unavailableReplicas").unwrap_or(0);
            if updated == desired && available >= desired && unavailable == 0 {
                Readiness::Ready(result(identity, observed, desired, available))
            } else {
                Readiness::Pending
            }
        }
        "StatefulSet" => {
            let desired = number(value, "/spec/replicas").unwrap_or(1);
            let ready = number(value, "/status/readyReplicas").unwrap_or(0);
            let strategy = value
                .pointer("/spec/updateStrategy/type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("RollingUpdate");
            let revisions_ready = if strategy == "OnDelete" {
                true
            } else {
                let partition =
                    number(value, "/spec/updateStrategy/rollingUpdate/partition").unwrap_or(0);
                let updated = number(value, "/status/updatedReplicas").unwrap_or(0);
                let revision_matches = partition > 0
                    || value.pointer("/status/currentRevision")
                        == value.pointer("/status/updateRevision");
                updated >= desired.saturating_sub(partition) && revision_matches
            };
            if ready == desired && revisions_ready {
                Readiness::Ready(result(identity, observed, desired, ready))
            } else {
                Readiness::Pending
            }
        }
        "DaemonSet" => {
            let desired = number(value, "/status/desiredNumberScheduled").unwrap_or(0);
            let updated = number(value, "/status/updatedNumberScheduled").unwrap_or(0);
            let available = number(value, "/status/numberAvailable").unwrap_or(0);
            let unavailable = number(value, "/status/numberUnavailable").unwrap_or(0);
            if updated == desired && available >= desired && unavailable == 0 {
                Readiness::Ready(result(identity, observed, desired, available))
            } else {
                Readiness::Pending
            }
        }
        _ => Readiness::Ready(result(identity, observed, 0, 0)),
    }
}

fn result(
    identity: &ResourceIdentity,
    observed_generation: Option<i64>,
    desired: i64,
    ready: i64,
) -> RolloutResult {
    RolloutResult {
        identity: identity.clone(),
        status: RolloutStatus::Ready,
        observed_generation,
        desired: Some(desired),
        ready: Some(ready),
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(kind: &str) -> ResourceIdentity {
        ResourceIdentity {
            api_version: "apps/v1".to_string(),
            kind: kind.to_string(),
            namespace: Some("default".to_string()),
            name: "workload".to_string(),
        }
    }

    #[test]
    fn workload_specific_readiness_rules() {
        let deployment = serde_json::json!({
            "metadata": {"generation": 2},
            "spec": {"replicas": 3},
            "status": {"observedGeneration": 2, "updatedReplicas": 3, "availableReplicas": 3, "unavailableReplicas": 0}
        });
        assert!(matches!(
            readiness(&identity("Deployment"), &deployment),
            Readiness::Ready(_)
        ));

        let stateful = serde_json::json!({
            "metadata": {"generation": 1},
            "spec": {"replicas": 2, "updateStrategy": {"type": "RollingUpdate", "rollingUpdate": {"partition": 0}}},
            "status": {"observedGeneration": 1, "readyReplicas": 2, "updatedReplicas": 2, "currentRevision": "r2", "updateRevision": "r2"}
        });
        assert!(matches!(
            readiness(&identity("StatefulSet"), &stateful),
            Readiness::Ready(_)
        ));

        let daemon = serde_json::json!({
            "metadata": {"generation": 1},
            "status": {"observedGeneration": 1, "desiredNumberScheduled": 4, "updatedNumberScheduled": 4, "numberAvailable": 4, "numberUnavailable": 0}
        });
        assert!(matches!(
            readiness(&identity("DaemonSet"), &daemon),
            Readiness::Ready(_)
        ));
    }

    #[test]
    fn rollout_waits_for_observation_and_detects_terminal_deployment_failure() {
        let unobserved = serde_json::json!({
            "metadata": {"generation": 3},
            "spec": {"replicas": 1},
            "status": {"observedGeneration": 2, "updatedReplicas": 1, "availableReplicas": 1}
        });
        assert!(matches!(
            readiness(&identity("Deployment"), &unobserved),
            Readiness::Pending
        ));

        let failed = serde_json::json!({
            "metadata": {"generation": 3},
            "spec": {"replicas": 1},
            "status": {
                "observedGeneration": 3,
                "conditions": [{
                    "type": "Progressing",
                    "status": "False",
                    "reason": "ProgressDeadlineExceeded"
                }]
            }
        });
        assert!(matches!(
            readiness(&identity("Deployment"), &failed),
            Readiness::Failed(_)
        ));
    }

    #[test]
    fn statefulset_partition_requires_only_unpartitioned_replicas_to_update() {
        let stateful = serde_json::json!({
            "metadata": {"generation": 4},
            "spec": {
                "replicas": 5,
                "updateStrategy": {"type": "RollingUpdate", "rollingUpdate": {"partition": 2}}
            },
            "status": {
                "observedGeneration": 4,
                "readyReplicas": 5,
                "updatedReplicas": 3,
                "currentRevision": "old",
                "updateRevision": "new"
            }
        });
        assert!(matches!(
            readiness(&identity("StatefulSet"), &stateful),
            Readiness::Ready(_)
        ));
    }
}
