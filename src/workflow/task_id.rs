pub const VALIDATE_CONFIG: &str = "workflow:validate-config";
pub const BUILD_PLAN: &str = "workflow:build-plan";
pub const PUSH_PLAN: &str = "workflow:push-plan";
pub const IMAGE_REPORT: &str = "workflow:image-report";
pub const BUILD_BEFORE_ALL: &str = "build:before-all";
pub const BUILD_AFTER_ALL: &str = "build:after-all";

pub fn service_build(service: &str) -> String {
    format!("service:{service}:build")
}

pub fn service_push(service: &str) -> String {
    format!("service:{service}:push")
}
