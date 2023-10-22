use scribe_rust::{log, Color};

use anyhow::Result;

use crate::{utils::inject_env_values_templates, errors::GenerateError};

pub async fn generate(env_name: &str) -> Result<(), GenerateError> {
    log(
        Color::Yellow,
        "Generating",
        &format!("Generating k8s resources for {}", env_name),
    );
    inject_env_values_templates(&env_name)?;
    Ok(())
}
