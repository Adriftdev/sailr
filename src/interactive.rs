use scribe_rust::log;

use crate::{cli::InteractiveArgs, deployment::k8sm8, errors::CliError};
use inquire::{MultiSelect, Select};

pub async fn main_menu(args: InteractiveArgs) -> Result<(), CliError> {
    let selection = Select::new(
        "Select the command",
        vec![
            "Log Merger",
            "Log Streamer",
            "Display ConfigMaps",
            "Display Events",
            "Display Secrets",
            "Delete ConfigMaps",
            "Delete Deployments",
            "Delete Pods",
            "Delete Services",
            "Delete Secrets",
            "Exit",
        ],
    );

    loop {
        let selected_command = selection
            .clone()
            .prompt()
            .map_err(|e| CliError::Other(format!("Failed to select command: {}", e)))?;

        if selected_command == "Exit" {
            println!("Exiting...");
            break;
        }

        if let Err(e) = execute(args.clone(), &selected_command).await {
            log(
                scribe_rust::Color::Red,
                "Error",
                &format!("Failed to execute command '{}': {}", selected_command, e),
            );
        } else {
            log(
                scribe_rust::Color::Green,
                "Success",
                &format!("Command '{}' executed successfully", selected_command),
            );
        }
    }

    Ok(())
}

pub async fn execute(args: InteractiveArgs, selected_command: &str) -> Result<(), CliError> {
    match selected_command {
        "Log Merger" => {
            let client = k8sm8::create_client(args.context.to_string())
                .await
                .map_err(|e| {
                    CliError::Other(format!("Failed to create Kubernetes client: {}", e))
                })?;

            let pods = k8sm8::pods::get_all_pods(client.clone(), "default")
                .await
                .map_err(|e| CliError::Other(format!("Failed to get all pods: {}", e)))?;

            let selected_pods = MultiSelect::new(
                "Select pods to stream logs from",
                pods.iter()
                    .map(|p| p.metadata.name.clone().unwrap_or_default())
                    .collect::<Vec<_>>(),
            )
            .prompt()
            .map_err(|e| CliError::Other(format!("Failed to select pods: {}", e)))?;

            k8sm8::logs::log_merger(client.clone(), "default", selected_pods)
                .await
                .map_err(|e| CliError::Other(format!("Failed to merge logs: {}", e)))?;
        }
        "Log Streamer" => {
            let client = k8sm8::create_client(args.context.to_string())
                .await
                .map_err(|e| {
                    CliError::Other(format!("Failed to create Kubernetes client: {}", e))
                })?;

            let pods = k8sm8::pods::get_all_pods(client.clone(), "default")
                .await
                .map_err(|e| CliError::Other(format!("Failed to get all pods: {}", e)))?;

            let selected_pods = MultiSelect::new(
                "Select pods to stream logs from",
                pods.iter()
                    .map(|p| p.metadata.name.clone().unwrap_or_default())
                    .collect::<Vec<_>>(),
            )
            .prompt()
            .map_err(|e| CliError::Other(format!("Failed to select pods: {}", e)))?;

            k8sm8::logs::log_streamer(client.clone(), "default", selected_pods)
                .await
                .map_err(|e| CliError::Other(format!("Failed to stream logs: {}", e)))?;
        }
        "Delete Pods" => {
            let client = k8sm8::create_client(args.context.to_string())
                .await
                .map_err(|e| {
                    CliError::Other(format!("Failed to create Kubernetes client: {}", e))
                })?;

            let pods = k8sm8::pods::get_all_pods(client.clone(), "default")
                .await
                .map_err(|e| CliError::Other(format!("Failed to get all pods: {}", e)))?;

            let selected_pods = MultiSelect::new(
                "Select pods to delete",
                pods.iter()
                    .map(|p| p.metadata.name.clone().unwrap_or_default())
                    .collect::<Vec<_>>(),
            )
            .prompt()
            .map_err(|e| CliError::Other(format!("Failed to select pods: {}", e)))?;

            for pod_name in selected_pods {
                k8sm8::pods::delete_pod(client.clone(), "default", &pod_name)
                    .await
                    .map_err(|e| {
                        CliError::Other(format!("Failed to delete pod {}: {}", pod_name, e))
                    })?;
            }
        }
        "Display ConfigMaps" => {
            let client = k8sm8::create_client(args.context.to_string())
                .await
                .map_err(|e| {
                    CliError::Other(format!("Failed to create Kubernetes client: {}", e))
                })?;

            let configmaps = k8sm8::configmaps::get_all_configmaps(client.clone(), "default")
                .await
                .map_err(|e| CliError::Other(format!("Failed to get all ConfigMaps: {}", e)))?;

            let selected_configmaps = MultiSelect::new(
                "Select ConfigMaps to display",
                configmaps
                    .iter()
                    .map(|cm| cm.metadata.name.clone().unwrap_or_default())
                    .collect::<Vec<_>>(),
            )
            .prompt()
            .map_err(|e| CliError::Other(format!("Failed to select ConfigMaps: {}", e)))?;

            for cm_name in selected_configmaps {
                let cm = k8sm8::configmaps::get_configmap(client.clone(), "default", &cm_name)
                    .await
                    .map_err(|e| {
                        CliError::Other(format!("Failed to get ConfigMap {}: {}", cm_name, e))
                    })?;
                //pretty print config map
                println!(
                    "ConfigMap: {}\nData: {:?}\n",
                    cm.metadata.name.unwrap_or_default(),
                    cm.data
                );
            }
        }
        "Display Events" => {
            let client = k8sm8::create_client(args.context.to_string())
                .await
                .map_err(|e| {
                    CliError::Other(format!("Failed to create Kubernetes client: {}", e))
                })?;

            let events = k8sm8::events::get_all_events(client.clone(), "default")
                .await
                .map_err(|e| CliError::Other(format!("Failed to get all events: {}", e)))?;

            for event in events {
                log(
                    scribe_rust::Color::Green,
                    &event.metadata.name.unwrap_or_default(),
                    &format!(
                        "\nReason: {}\nMessage: {}\n",
                        event.reason.unwrap_or_default(),
                        event.message.unwrap_or_default()
                    ),
                );
            }
        }
        "Display Secrets" => {
            let client = k8sm8::create_client(args.context.to_string())
                .await
                .map_err(|e| {
                    CliError::Other(format!("Failed to create Kubernetes client: {}", e))
                })?;

            let secrets = k8sm8::secrets::get_all_secrets(client.clone(), "default")
                .await
                .map_err(|e| CliError::Other(format!("Failed to get all secrets: {}", e)))?;

            let selected_secrets = MultiSelect::new(
                "Select Secrets to display",
                secrets
                    .iter()
                    .map(|s| s.metadata.name.clone().unwrap_or_default())
                    .collect::<Vec<_>>(),
            )
            .prompt()
            .map_err(|e| CliError::Other(format!("Failed to select Secrets: {}", e)))?;

            for secret_name in selected_secrets {
                let secret = k8sm8::secrets::get_secret(client.clone(), "default", &secret_name)
                    .await
                    .map_err(|e| {
                        CliError::Other(format!("Failed to get Secret {}: {}", secret_name, e))
                    })?;

                // Pretty print secret data
                secret.data.unwrap().iter().for_each(|(k, v)| {
                    println!(
                        "{}",
                        &format!("{}: {}", k, String::from_utf8_lossy(v.0.as_ref()))
                    );
                });
            }
        }
        "Delete ConfigMaps" => {
            let client = k8sm8::create_client(args.context.to_string())
                .await
                .map_err(|e| {
                    CliError::Other(format!("Failed to create Kubernetes client: {}", e))
                })?;

            let configmaps = k8sm8::configmaps::get_all_configmaps(client.clone(), "default")
                .await
                .map_err(|e| CliError::Other(format!("Failed to get all ConfigMaps: {}", e)))?;

            let selected_configmaps = MultiSelect::new(
                "Select ConfigMaps to delete",
                configmaps
                    .iter()
                    .map(|cm| cm.metadata.name.clone().unwrap_or_default())
                    .collect::<Vec<_>>(),
            )
            .prompt()
            .map_err(|e| CliError::Other(format!("Failed to select ConfigMaps: {}", e)))?;

            for cm_name in selected_configmaps {
                k8sm8::configmaps::delete_configmap(client.clone(), "default", &cm_name)
                    .await
                    .map_err(|e| {
                        CliError::Other(format!("Failed to delete ConfigMap {}: {}", cm_name, e))
                    })?;
            }
        }
        "Delete Deployments" => {
            let client = k8sm8::create_client(args.context.to_string())
                .await
                .map_err(|e| {
                    CliError::Other(format!("Failed to create Kubernetes client: {}", e))
                })?;

            let deployments = k8sm8::deployments::get_all_deployments(client.clone(), "default")
                .await
                .map_err(|e| CliError::Other(format!("Failed to get all deployments: {}", e)))?;

            let selected_deployments = MultiSelect::new(
                "Select Deployments to delete",
                deployments
                    .iter()
                    .map(|d| d.metadata.name.clone().unwrap_or_default())
                    .collect::<Vec<_>>(),
            )
            .prompt()
            .map_err(|e| CliError::Other(format!("Failed to select Deployments: {}", e)))?;

            for dep_name in selected_deployments {
                k8sm8::deployments::delete_deployment(client.clone(), "default", &dep_name)
                    .await
                    .map_err(|e| {
                        CliError::Other(format!("Failed to delete Deployment {}: {}", dep_name, e))
                    })?;
            }
        }
        "Delete Services" => {
            let client = k8sm8::create_client(args.context.to_string())
                .await
                .map_err(|e| {
                    CliError::Other(format!("Failed to create Kubernetes client: {}", e))
                })?;

            let services = k8sm8::services::get_all_services(client.clone(), "default")
                .await
                .map_err(|e| CliError::Other(format!("Failed to get all services: {}", e)))?;

            let selected_services = MultiSelect::new(
                "Select Services to delete",
                services
                    .iter()
                    .map(|s| s.metadata.name.clone().unwrap_or_default())
                    .collect::<Vec<_>>(),
            )
            .prompt()
            .map_err(|e| CliError::Other(format!("Failed to select Services: {}", e)))?;

            for svc_name in selected_services {
                k8sm8::services::delete_service(client.clone(), "default", &svc_name)
                    .await
                    .map_err(|e| {
                        CliError::Other(format!("Failed to delete Service {}: {}", svc_name, e))
                    })?;
            }
        }

        "Delete Secrets" => {
            let client = k8sm8::create_client(args.context.to_string())
                .await
                .map_err(|e| {
                    CliError::Other(format!("Failed to create Kubernetes client: {}", e))
                })?;

            let secrets = k8sm8::secrets::get_all_secrets(client.clone(), "default")
                .await
                .map_err(|e| CliError::Other(format!("Failed to get all secrets: {}", e)))?;

            let selected_secrets = MultiSelect::new(
                "Select Secrets to delete",
                secrets
                    .iter()
                    .map(|s| s.metadata.name.clone().unwrap_or_default())
                    .collect::<Vec<_>>(),
            )
            .prompt()
            .map_err(|e| CliError::Other(format!("Failed to select Secrets: {}", e)))?;

            for secret_name in selected_secrets {
                k8sm8::secrets::delete_secret(client.clone(), "default", &secret_name)
                    .await
                    .map_err(|e| {
                        CliError::Other(format!("Failed to delete Secret {}: {}", secret_name, e))
                    })?;
            }
        }

        &_ => todo!(),
    };

    Ok(())
}
