#[derive(Debug, serde::Serialize)]
pub struct CapabilitySchemas {
    pub environment: Vec<&'static str>,
    pub workflow_report: Vec<&'static str>,
    pub promotion_plan: Vec<&'static str>,
    pub deployment_bundle: Vec<&'static str>,
}

#[derive(Debug, serde::Serialize)]
pub struct CapabilityFeatures {
    pub signed_deployment: bool,
    pub transactional_rollback: bool,
    pub publication_consumption: bool,
    pub promotion: bool,
    pub rollout_verification: bool,
    pub locking: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct Capabilities {
    pub schema_version: &'static str,
    pub sailr_version: &'static str,
    pub schemas: CapabilitySchemas,
    pub ci_providers: Vec<&'static str>,
    pub flow_generation_modes: Vec<&'static str>,
    pub features: CapabilityFeatures,
}

pub fn current() -> Capabilities {
    Capabilities {
        schema_version: "sailr.capabilities/v1",
        sailr_version: env!("CARGO_PKG_VERSION"),
        schemas: CapabilitySchemas {
            environment: vec!["0.2.0", "0.3.0", "0.4.0", "0.5.0"],
            workflow_report: vec!["sailr.workflow-report/v1"],
            promotion_plan: vec![super::promotion::PROMOTION_PLAN_SCHEMA],
            deployment_bundle: vec![crate::deployment::bundle::DEPLOYMENT_BUNDLE_SCHEMA],
        },
        ci_providers: vec!["circleci", "github", "travis"],
        flow_generation_modes: vec!["print", "fragment", "create", "merge"],
        features: CapabilityFeatures {
            signed_deployment: true,
            transactional_rollback: true,
            publication_consumption: true,
            promotion: true,
            rollout_verification: true,
            locking: true,
        },
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn advanced_release_capabilities_are_machine_readable() {
        let value = serde_json::to_value(super::current()).expect("capabilities");
        assert_eq!(value["schema_version"], "sailr.capabilities/v1");
        assert_eq!(value["features"]["promotion"], true);
        assert_eq!(value["features"]["locking"], true);
        assert_eq!(
            value["schemas"]["deployment_bundle"][0],
            crate::deployment::bundle::DEPLOYMENT_BUNDLE_SCHEMA
        );
    }
}
