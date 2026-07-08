use std::process::Command;
use std::env;
use crate::LOGGER;

pub struct RemoteBuilderContext {
    builder_name: String,
}

impl RemoteBuilderContext {
    pub fn setup(profile_name: &str, endpoint: &str) -> Result<Option<Self>, String> {
        if endpoint.is_empty() {
            return Ok(None);
        }

        let builder_name = format!("sailr-remote-{}", profile_name);

        LOGGER.info(&format!(
            "🔧 Setting up remote docker builder '{}' at {}",
            builder_name, endpoint
        ));

        // Create the remote builder
        let output = Command::new("docker")
            .args([
                "buildx",
                "create",
                "--name",
                &builder_name,
                "--driver",
                "docker-container",
                endpoint,
            ])
            .output()
            .map_err(|e| format!("Failed to execute docker buildx create: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "Failed to create remote docker builder: {}",
                stderr
            ));
        }

        // Set the environment variable so all subsequent docker buildx commands use it
        env::set_var("BUILDX_BUILDER", &builder_name);

        Ok(Some(RemoteBuilderContext { builder_name }))
    }
}

impl Drop for RemoteBuilderContext {
    fn drop(&mut self) {
        LOGGER.info(&format!(
            "🧹 Tearing down remote docker builder '{}'",
            self.builder_name
        ));

        let output = Command::new("docker")
            .args(["buildx", "rm", "-f", &self.builder_name])
            .output();

        if let Err(e) = output {
            LOGGER.warn(&format!(
                "Failed to remove remote builder '{}': {}",
                self.builder_name, e
            ));
        } else if let Ok(out) = output {
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                LOGGER.warn(&format!(
                    "Failed to remove remote builder '{}': {}",
                    self.builder_name, stderr
                ));
            }
        }
        
        // Unset the environment variable
        env::remove_var("BUILDX_BUILDER");
    }
}
