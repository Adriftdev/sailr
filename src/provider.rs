use std::env;

use scribe_rust::{log, Color};
use serde::{Deserialize, Serialize};

use anyhow::Result;

use crate::{errors::ProviderError, LOGGER};

#[derive(Deserialize, Serialize)]
pub struct GcpProviderKey {
    project_id: String,
    r#type: String,
    private_key_id: String,
    private_key: String,
    client_email: String,
    client_id: String,
    auth_uri: String,
    token_uri: String,
    auth_provider_x509_cert_url: String,
    client_x509_cert_url: String,
    universe_domain: String,
}

impl GcpProviderKey {
    pub async fn authenticate(&self) -> Result<(), ProviderError> {
        println!("Authenticating with GCP...");

        Ok(())
    }
}

pub trait ProviderStrategy {
    fn initialize_project(&self) -> Result<(), ProviderError>;
}

// Implement the trait for different providers
pub struct AwsProvider;
pub struct GcpProvider;
pub struct DockerDesktopProvider;
pub struct K3SProvider;

impl ProviderStrategy for AwsProvider {
    fn initialize_project(&self) -> Result<(), ProviderError> {
        LOGGER.info("Initializin gAWS provider...");
        let _provider_key = env::var("PROVIDER_KEY").expect("PROVIDER_KEY must be set");
        // TODO: Create a new AWS project

        LOGGER.info("AWS provider initialized");
        Ok(())
    }
}

impl ProviderStrategy for GcpProvider {
    fn initialize_project(&self) -> Result<(), ProviderError> {
        LOGGER.info("GCP provider...");
        let provider_key = serde_json::from_str::<GcpProviderKey>(
            &env::var("PROVIDER_KEY").expect("PROVIDER_KEY must be set"),
        );

        tokio::task::spawn_blocking(move || async {
            provider_key.unwrap().authenticate().await.unwrap();
        });

        // TODO: Create a new GCP project

        LOGGER.info("GCP provider initialized");
        Ok(())
    }
}

impl ProviderStrategy for DockerDesktopProvider {
    fn initialize_project(&self) -> Result<(), ProviderError> {
        LOGGER.info("Docker Desktop provider...");
        // TODO: Create a new Docker Desktop project

        log(
            Color::Green,
            "Success",
            "Docker Desktop provider initialized",
        );
        Ok(())
    }
}

impl ProviderStrategy for K3SProvider {
    fn initialize_project(&self) -> Result<(), ProviderError> {
        LOGGER.info("k3s provider...");
        // TODO: Create a new k3s project

        LOGGER.info("k3s provider initialized");
        Ok(())
    }
}

// Provider struct that holds the selected strategy
pub struct Provider<T: ProviderStrategy> {
    strategy: T,
}

impl<T: ProviderStrategy> Provider<T> {
    pub fn new(strategy: T) -> Self {
        Provider { strategy }
    }

    pub fn initialize_project(&self) -> Result<(), ProviderError> {
        self.strategy.initialize_project()
    }
}
