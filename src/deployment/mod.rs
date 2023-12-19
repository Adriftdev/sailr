pub mod kube_api;
use anyhow::Result;

use std::path::Path;

use async_recursion::async_recursion;
use scribe_rust::{log, Color};

use crate::errors::DeployError;

#[async_recursion]
pub async fn apply_path_recursive(
    path: &Path,
    client: kube::Client,
    discovery: &kube::discovery::Discovery,
    manifest: &mut Option<Vec<(String, String)>>,
) -> Result<Vec<(String, String)>, DeployError> {
    let mut manifest: Vec<(String, String)> = match manifest {
        Some(m) => m.clone(),
        None => vec![],
    };

    if path.is_dir() {
        for entry in std::fs::read_dir(path).map_err(|e| {
            DeployError::ManifestApplicationFailed(format!(
                "Failed to read or apply Kubernetes manifest: {}",
                e
            ))
        })? {
            let entry = entry.map_err(|e| {
                DeployError::ManifestApplicationFailed(format!(
                    "Failed to read or apply Kubernetes manifest: {}",
                    e
                ))
            })?;
            let path = entry.path();
            manifest = apply_path_recursive(
                &path,
                client.clone(),
                discovery,
                &mut Some(manifest.clone()),
            )
            .await?;
        }
    } else {
        let path = path.to_path_buf();
        let res = kube_api::apply(Some(path), client, discovery).await?;
        manifest.push(res);
    }

    Ok(manifest)
}

pub async fn deploy(ctx: String, env_name: &str) -> Result<(), DeployError> {
    log(
        Color::Yellow,
        "Deploying",
        &format!("Deploying to {} for {}", ctx, env_name),
    );
    let client = kube_api::create_client(ctx).await?;
    let discovery = kube::Discovery::new(client.clone())
        .run()
        .await
        .map_err(|e| {
            DeployError::DiscoveryInitializationFailed(format!(
                "Failed to initialize Kubernetes Discovery: {}",
                e
            ))
        })?;
    let path = Path::new("./k8s/generated").join(env_name);

    // TODO: https://linear.app/adriftdev/issue/ADR-40/environment-version-management
    let _manifest = apply_path_recursive(path.as_path(), client, &discovery, &mut None).await?;
    log(Color::Green, "Done", "Deployed successfully!");

    Ok(())
}
