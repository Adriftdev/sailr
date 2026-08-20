pub const VALIDATE_CONFIG: &str = "workflow:validate-config";
pub const BUILD_PLAN: &str = "workflow:build-plan";
pub const PUSH_PLAN: &str = "workflow:push-plan";
pub const IMAGE_REPORT: &str = "workflow:image-report";
pub const BUILD_BEFORE_ALL: &str = "build:before-all";
pub const BUILD_AFTER_ALL: &str = "build:after-all";
pub const GENERATE: &str = "workflow:generate";
pub const PRE_DEPLOY_HOOKS: &str = "workflow:pre-deploy-hooks";
pub const DEPLOYMENT_BUNDLE: &str = "workflow:deployment-bundle";
pub const DEPLOYMENT_PLAN: &str = "workflow:deployment-plan";
pub const APPROVAL: &str = "workflow:approval";
pub const VERIFICATION_GATE: &str = "workflow:verification-gate";
pub const DEPLOY: &str = "workflow:deploy";
pub const ROLLOUT_VERIFICATION: &str = "workflow:rollout-verification";
pub const POST_DEPLOY_HOOKS: &str = "workflow:post-deploy-hooks";
pub const REPORT_ARTIFACTS: &str = "workflow:image-report";
pub const WRITE_REPORT_FINALIZER: &str = "finalizer:write-workflow-report";
pub const WRITE_BUILD_CACHE_FINALIZER: &str = "finalizer:write-build-cache";

pub fn service_build(service: &str) -> String {
    format!("service:{service}:build")
}

pub fn service_push(service: &str) -> String {
    format!("service:{service}:push")
}

pub fn service_phase(service: &str, phase: &str, index: usize) -> String {
    format!("service:{service}:{phase}:{index}")
}

pub fn service_finally(service: &str) -> String {
    format!("finalizer:service:{service}:finally")
}
