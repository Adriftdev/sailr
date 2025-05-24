use std::io;

use sailr::{
    builder::{split_matches, Builder},
    cli::{Cli, Commands, InfraCommands, K8sCommands, Provider},
    create_default_env_config,
    create_default_env_infra,
    environment::{Environment, Service}, // Added Service
    errors::CliError,
    generate,
    infra::{local_k8s::LocalK8, Infra},
    templates::{
        scaffolding::{generate_config_map, generate_deployment, generate_service}, // Added scaffolding functions
        TemplateManager,
    },
    LOGGER,
    // filesystem::FileSystemManager, // FileSystemManager is not directly used here, fs is used.
};
use std::fs;
use std::path::Path; // Path was already here

use anyhow::Result;

use clap::{CommandFactory, Parser};

#[tokio::main]
async fn main() -> Result<(), CliError> {
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    match cli.commands {
        Commands::Init(arg) => {
            TemplateManager::new().copy_base_templates().unwrap();

            create_default_env_config(
                arg.name.clone(),
                arg.config_template_path,
                arg.default_registry.clone(),
            );

            if let Some(template_path) = arg.infra_template_path {
                create_default_env_infra(
                    arg.name.clone(),
                    Some(template_path),
                    arg.default_registry,
                )
            } else if let Some(provider) = arg.provider {
                let infra = match provider {
                    Provider::Local => Infra::new(Box::new(LocalK8::new(arg.name.clone()))),
                    _ => {
                        LOGGER.error(&format!("Provider {:?} not supported", provider));
                        std::process::exit(1);
                    }
                };
                infra.generate(Infra::read_config(arg.name.clone()));
                infra.build(Infra::read_config(arg.name.clone()));
            } else {
                let infra = Infra::new(Box::new(LocalK8::new(arg.name.clone())));
                infra.generate(Infra::read_config(arg.name.clone()));
                infra.build(Infra::read_config(arg.name.clone()));
            }

            // Add default "sample-app" service
            let sample_service_name = "sample-app".to_string();
            let sample_app_type = "web-app".to_string();
            let sample_image = "nginx:latest".to_string();
            let sample_replicas = 1;
            let sample_port = 80;

            let sample_service_template_path_str = format!("k8s/templates/{}", sample_service_name);
            let sample_service_template_path = Path::new(&sample_service_template_path_str);

            match fs::create_dir_all(sample_service_template_path) {
                Ok(_) => LOGGER.info(&format!(
                    "Created directory for sample-app: {}",
                    sample_service_template_path.display()
                )),
                Err(e) => {
                    LOGGER.error(&format!(
                        "Failed to create directory for sample-app {}: {}",
                        sample_service_template_path.display(),
                        e
                    ));
                    return Err(CliError::Other(format!(
                        "Failed to create directory for sample-app: {}",
                        e
                    )));
                }
            }

            let deployment_content = generate_deployment(
                &sample_service_name,
                &sample_app_type,
                &sample_image,
                sample_replicas,
            );
            let service_content =
                generate_service(&sample_service_name, &sample_app_type, sample_port);
            let config_map_content = generate_config_map(&sample_service_name, &sample_app_type);

            let deployment_file_path = sample_service_template_path.join("deployment.yaml");
            let service_file_path = sample_service_template_path.join("service.yaml");
            let config_map_file_path = sample_service_template_path.join("configmap.yaml");

            for (path, content) in &[
                (&deployment_file_path, deployment_content),
                (&service_file_path, service_content),
                (&config_map_file_path, config_map_content),
            ] {
                match fs::write(path, content) {
                    Ok(_) => {
                        LOGGER.info(&format!("Created sample-app manifest: {}", path.display()))
                    }
                    Err(e) => {
                        LOGGER.error(&format!(
                            "Failed to write sample-app manifest {}: {}",
                            path.display(),
                            e
                        ));
                        return Err(CliError::Other(format!(
                            "Failed to write sample-app manifest: {}",
                            e
                        )));
                    }
                }
            }

            // Update the new environment's config.toml with sample-app
            let env_name = arg.name.clone();
            match Environment::load_from_file(&env_name) {
                Ok(mut env) => {
                    let sample_service_entry = Service::new(
                        "sample-app",
                        "default",
                        Some("sample-app"),
                        None,                       // build
                        None,                       // major_version
                        None,                       // minor_version
                        None,                       // patch_version
                        Some("latest".to_string()), // tag
                    );

                    if env
                        .service_whitelist
                        .iter()
                        .any(|s| s.name == sample_service_entry.name)
                    {
                        LOGGER.warn(&format!(
                            "Sample service {} already exists in environment {}, skipping addition.",
                            sample_service_name, env_name
                        ));
                    } else {
                        env.service_whitelist.push(sample_service_entry);
                        match env.save_to_file() {
                            Ok(_) => LOGGER.info(&format!(
                                "Added sample-app service to environment {} config.",
                                env_name
                            )),
                            Err(e) => {
                                LOGGER.error(&format!(
                                    "Failed to save updated config for environment {}: {}",
                                    env_name, e
                                ));
                                return Err(CliError::Other(format!(
                                    "Failed to save config for sample-app: {}",
                                    e
                                )));
                            }
                        }
                    }
                }
                Err(e) => {
                    LOGGER.error(&format!(
                        "Failed to load environment {} to add sample-app: {}",
                        env_name, e
                    ));
                    return Err(CliError::Other(format!(
                        "Failed to load environment config for sample-app: {}",
                        e
                    )));
                }
            }
        }
        Commands::Completions(arg) => {
            clap_complete::generate(arg.shell, &mut Cli::command(), "sailr", &mut io::stdout());
        }
        Commands::Infra(a) => match a {
            InfraCommands::Up(arg) => {
                LOGGER.info(&format!("Creating a new environment"));
                if let Some(template_path) = arg.infra_template_path {
                    create_default_env_infra(arg.name, Some(template_path), arg.default_registry);
                } else if let Some(provider) = arg.provider {
                    let infra = match provider {
                        Provider::Local => Infra::new(Box::new(LocalK8::new(arg.name.clone()))),

                        _ => {
                            LOGGER.error(&format!("Provider {:?} not supported", provider));
                            std::process::exit(1);
                        }
                    };
                    infra.generate(Infra::read_config(arg.name.clone()));
                } else {
                    let infra = Infra::new(Box::new(LocalK8::new(arg.name.clone())));
                    infra.generate(Infra::read_config(arg.name.clone()));
                }
            }
            InfraCommands::Down(arg) => Infra::destroy(Infra::read_config(arg.name)),
        },
        Commands::Deploy(arg) => {
            LOGGER.info(&format!("Deploying an environment"));

            sailr::deployment::deploy(arg.context.to_string(), &arg.name).await?;
        }
        Commands::Generate(arg) => {
            LOGGER.info(&format!("Generating an environment"));

            let env = match Environment::load_from_file(&arg.name) {
                Ok(env) => env,
                Err(e) => {
                    LOGGER.error(&format!("Failed to load environment: {}", e));
                    std::process::exit(1);
                }
            };

            let mut services = env.list_services();

            if let Some(only_services) = arg.only {
                services = services
                    .into_iter()
                    .filter(|s| only_services.contains(&s.name))
                    .collect();
            }

            if let Some(ignored_services) = arg.ignore {
                services = services
                    .into_iter()
                    .filter(|s| !ignored_services.contains(&s.name))
                    .collect();
            }

            generate(&arg.name, &env, services);

            LOGGER.info(&format!("Generation Complete"));
        }
        Commands::Build(arg) => {
            let env = match Environment::load_from_file(&arg.name) {
                Ok(env) => env,
                Err(e) => {
                    LOGGER.error(&format!("Failed to load environment: {}", e));
                    std::process::exit(1);
                }
            };

            let services = env
                .list_services()
                .into_iter()
                .filter(|s| s.build.is_some());

            let mut builder = Builder::new(
                ".roomservice".to_string(),
                arg.force.unwrap_or(false),
                services.into_iter().map(|s| s.name.clone()).collect(),
                split_matches(arg.ignore),
            );

            match builder.build(&env) {
                Ok(_) => (),
                Err(e) => {
                    LOGGER.error(&format!("Failed to build environment: {}", e));
                    std::process::exit(1);
                }
            };
        }
        Commands::Go(arg) => {
            LOGGER.info(&format!("Generating and deploying an environment"));

            let env = match Environment::load_from_file(&arg.name) {
                Ok(env) => env,
                Err(e) => {
                    LOGGER.error(&format!("Failed to load environment: {}", e));
                    std::process::exit(1);
                }
            };

            let mut services = env.list_services();

            if let Some(only_services) = arg.only {
                services = services
                    .into_iter()
                    .filter(|s| only_services.contains(&s.name))
                    .collect();
            }

            if let Some(ref ignored_services) = arg.ignore {
                services = services
                    .into_iter()
                    .filter(|s| !ignored_services.contains(&s.name))
                    .collect();
            }

            let mut builder = Builder::new(
                ".roomservice".to_string(),
                arg.force.unwrap_or(false),
                services
                    .clone()
                    .into_iter()
                    .map(|s| s.name.clone())
                    .collect(),
                split_matches(arg.ignore),
            );

            match builder.build(&env) {
                Ok(_) => (),
                Err(e) => {
                    LOGGER.error(&format!("Failed to build environment: {}", e));
                    std::process::exit(1);
                }
            };

            generate(&arg.name, &env, services);

            sailr::deployment::deploy(arg.context.to_string(), &arg.name).await?;
        }
        Commands::K8s(args) => {
            match args.command {
                K8sCommands::Pod(pod_args) => {
                    match pod_args.command {
                        sailr::cli::ResourceCommands::Delete(arg) => {
                            LOGGER.info(&format!("Deleting a pod"));

                            let client =
                                sailr::deployment::k8sm8::create_client(arg.context.to_string())
                                    .await
                                    .map_err(|e| {
                                        CliError::Other(format!(
                                            "Failed to create Kubernetes client: {}",
                                            e
                                        ))
                                    })?;

                            sailr::deployment::k8sm8::pods::delete_pod(
                                client,
                                arg.namespace.as_deref().unwrap_or("default"),
                                &arg.name,
                            )
                            .await
                            .map_err(|e| CliError::Other(format!("Failed to delete pod: {}", e)))?;
                        }
                        sailr::cli::ResourceCommands::Get(arg) => {
                            let client =
                                sailr::deployment::k8sm8::create_client(arg.context.to_string())
                                    .await
                                    .map_err(|e| {
                                        CliError::Other(format!(
                                            "Failed to create Kubernetes client: {}",
                                            e
                                        ))
                                    })?;

                            let pods = sailr::deployment::k8sm8::pods::get_all_pods(
                                client.clone(),
                                client.clone().default_namespace(),
                            )
                            .await
                            .map_err(|e| {
                                CliError::Other(format!("Failed to get all pods: {}", e))
                            })?;

                            for pod in pods {
                                let pod_name = pod.metadata.name.clone().unwrap_or_default();
                                let namespace = pod.metadata.namespace.clone().unwrap_or_default();

                                let phase = pod
                                    .status
                                    .as_ref()
                                    .and_then(|s| s.phase.clone())
                                    .unwrap_or_else(|| "Unknown".into());

                                println!(
                                    "{}, Namespace: {}, Phase: {}",
                                    pod_name, namespace, phase
                                );

                                if let Some(status) = pod.status.as_ref() {
                                    if let Some(container_statuses) =
                                        status.container_statuses.as_ref()
                                    {
                                        for container_status in container_statuses {
                                            let container_name = &container_status.name;
                                            if let Some(waiting) = container_status
                                                .state
                                                .as_ref()
                                                .and_then(|s| s.waiting.as_ref())
                                            {
                                                println!(
                                                    "  Container {}: Waiting - Reason: {:?}",
                                                    container_name, waiting.reason
                                                );
                                            } else if let Some(terminated) = container_status
                                                .state
                                                .as_ref()
                                                .and_then(|s| s.terminated.as_ref())
                                            {
                                                println!("  Container {}: Terminated - Reason: {:?}, Exit Code: {}", container_name, terminated.reason, terminated.exit_code);
                                            }
                                        }
                                    }

                                    if let Some(conditions) = &status.conditions {
                                        if !conditions
                                            .iter()
                                            .any(|c| c.type_ == "Ready" && c.status == "True")
                                            || conditions.iter().any(|c| c.status != "True")
                                        {
                                            println!("  Conditions:");
                                            for condition in conditions {
                                                if condition.status != "True"
                                                    || condition.type_ != "Ready"
                                                {
                                                    // Only show non-ready or non-true conditions
                                                    println!(
                                                        "    Type: {}, Status: {}",
                                                        condition.type_, condition.status
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                                println!("--------------------");
                            }
                        }
                        sailr::cli::ResourceCommands::DeleteAll(arg) => {
                            LOGGER.info(&format!("Deleting all pods"));

                            let client =
                                sailr::deployment::k8sm8::create_client(arg.context.to_string())
                                    .await
                                    .map_err(|e| {
                                        CliError::Other(format!(
                                            "Failed to create Kubernetes client: {}",
                                            e
                                        ))
                                    })?;

                            sailr::deployment::k8sm8::pods::delete_all_pods(client, &arg.namespace)
                                .await
                                .map_err(|e| {
                                    CliError::Other(format!("Failed to delete all pods: {}", e))
                                })?;
                        }
                    }
                }
                K8sCommands::Service(service_args) => match service_args.command {
                    sailr::cli::ResourceCommands::Get(args) => {
                        LOGGER.info(&format!("Getting all services"));

                        let client =
                            sailr::deployment::k8sm8::create_client(args.context.to_string())
                                .await
                                .map_err(|e| {
                                    CliError::Other(format!(
                                        "Failed to create Kubernetes client: {}",
                                        e
                                    ))
                                })?;

                        let services = sailr::deployment::k8sm8::services::get_all_services(
                            client.clone(),
                            client.clone().default_namespace(),
                        )
                        .await
                        .map_err(|e| {
                            CliError::Other(format!("Failed to get all services: {}", e))
                        })?;

                        for service in services {
                            let service_name = service.metadata.name.clone().unwrap_or_default();
                            let namespace = service.metadata.namespace.clone().unwrap_or_default();

                            println!("{}, Namespace: {}", service_name, namespace);
                            println!("--------------------");
                        }
                    }
                    sailr::cli::ResourceCommands::Delete(args) => {
                        LOGGER.info(&format!("Deleting a service"));

                        let client =
                            sailr::deployment::k8sm8::create_client(args.context.to_string())
                                .await
                                .map_err(|e| {
                                    CliError::Other(format!(
                                        "Failed to create Kubernetes client: {}",
                                        e
                                    ))
                                })?;

                        sailr::deployment::k8sm8::services::delete_service(
                            client.clone(),
                            args.namespace
                                .as_deref()
                                .unwrap_or(&client.default_namespace()),
                            &args.name,
                        )
                        .await
                        .map_err(|e| CliError::Other(format!("Failed to delete service: {}", e)))?;
                    }
                    sailr::cli::ResourceCommands::DeleteAll(args) => {
                        LOGGER.info(&format!("Deleting all services"));

                        let client =
                            sailr::deployment::k8sm8::create_client(args.context.to_string())
                                .await
                                .map_err(|e| {
                                    CliError::Other(format!(
                                        "Failed to create Kubernetes client: {}",
                                        e
                                    ))
                                })?;

                        sailr::deployment::k8sm8::services::delete_all_services(
                            client,
                            &args.namespace,
                        )
                        .await
                        .map_err(|e| {
                            CliError::Other(format!("Failed to delete all services: {}", e))
                        })?;
                    }
                },
                K8sCommands::Deployment(deployment_args) => match deployment_args.command {
                    sailr::cli::ResourceCommands::Get(args) => {
                        LOGGER.info(&format!("Getting all deployments"));

                        let client =
                            sailr::deployment::k8sm8::create_client(args.context.to_string())
                                .await
                                .map_err(|e| {
                                    CliError::Other(format!(
                                        "Failed to create Kubernetes client: {}",
                                        e
                                    ))
                                })?;

                        let deployments = sailr::deployment::k8sm8::get_all_deployments(client)
                            .await
                            .map_err(|e| {
                                CliError::Other(format!("Failed to get all deployments: {}", e))
                            })?;

                        for deployment in deployments {
                            let deployment_name =
                                deployment.metadata.name.clone().unwrap_or_default();
                            let namespace =
                                deployment.metadata.namespace.clone().unwrap_or_default();

                            println!("{}, Namespace: {}", deployment_name, namespace);
                            println!("--------------------");
                        }
                    }
                    sailr::cli::ResourceCommands::Delete(args) => {
                        LOGGER.info(&format!("Deleting a deployment"));

                        let client =
                            sailr::deployment::k8sm8::create_client(args.context.to_string())
                                .await
                                .map_err(|e| {
                                    CliError::Other(format!(
                                        "Failed to create Kubernetes client: {}",
                                        e
                                    ))
                                })?;

                        sailr::deployment::k8sm8::delete_deployment(
                            client.clone(),
                            &args
                                .namespace
                                .unwrap_or(client.default_namespace().to_string()),
                            &args.name,
                        )
                        .await
                        .map_err(|e| {
                            CliError::Other(format!("Failed to delete deployment: {}", e))
                        })?;
                    }
                    sailr::cli::ResourceCommands::DeleteAll(args) => {
                        LOGGER.info(&format!("Deleting all deployments"));

                        let client =
                            sailr::deployment::k8sm8::create_client(args.context.to_string())
                                .await
                                .map_err(|e| {
                                    CliError::Other(format!(
                                        "Failed to create Kubernetes client: {}",
                                        e
                                    ))
                                })?;

                        sailr::deployment::k8sm8::delete_all_deployments(client, &args.namespace)
                            .await
                            .map_err(|e| {
                                CliError::Other(format!("Failed to delete all deployments: {}", e))
                            })?;
                    }
                },
            }
        }
        Commands::AddService(args) => {
            LOGGER.info(&format!(
                "Adding new service: {} of type {}",
                args.service_name, args.app_type
            ));

            let service_template_path_str = format!("k8s/templates/{}", args.service_name);
            let service_template_path = Path::new(&service_template_path_str);

            match fs::create_dir_all(service_template_path) {
                Ok(_) => {
                    if service_template_path.exists() {
                        LOGGER.info(&format!(
                            "Directory {} already existed or was created successfully.",
                            service_template_path.display()
                        ));
                    } else {
                        // This case should ideally not be reached if create_dir_all is successful
                        // but fs::create_dir_all doesn't error if path already exists.
                        // We log it just in case.
                        LOGGER.info(&format!(
                            "Directory {} created successfully.",
                            service_template_path.display()
                        ));
                    }
                }
                Err(e) => {
                    LOGGER.error(&format!(
                        "Failed to create directory {}: {}",
                        service_template_path.display(),
                        e
                    ));
                    return Err(CliError::Other(format!(
                        "Failed to create directory {}: {}",
                        service_template_path.display(),
                        e
                    )));
                }
            }

            // Template Generation
            let image = "nginx:latest"; // Default image
            let replicas = 1; // Default replicas
            let port = 80; // Default port

            let deployment_content =
                generate_deployment(&args.service_name, &args.app_type, image, replicas);
            let service_content = generate_service(&args.service_name, &args.app_type, port);
            let config_map_content = generate_config_map(&args.service_name, &args.app_type);

            let deployment_file_path = service_template_path.join("deployment.yaml");
            let service_file_path = service_template_path.join("service.yaml");
            let config_map_file_path = service_template_path.join("configmap.yaml");

            match fs::write(&deployment_file_path, deployment_content) {
                Ok(_) => LOGGER.info(&format!(
                    "Created deployment manifest: {}",
                    deployment_file_path.display()
                )),
                Err(e) => {
                    LOGGER.error(&format!(
                        "Failed to write deployment manifest {}: {}",
                        deployment_file_path.display(),
                        e
                    ));
                    return Err(CliError::Other(format!(
                        "Failed to write deployment manifest: {}",
                        e
                    )));
                }
            }

            match fs::write(&service_file_path, service_content) {
                Ok(_) => LOGGER.info(&format!(
                    "Created service manifest: {}",
                    service_file_path.display()
                )),
                Err(e) => {
                    LOGGER.error(&format!(
                        "Failed to write service manifest {}: {}",
                        service_file_path.display(),
                        e
                    ));
                    return Err(CliError::Other(format!(
                        "Failed to write service manifest: {}",
                        e
                    )));
                }
            }

            match fs::write(&config_map_file_path, config_map_content) {
                Ok(_) => LOGGER.info(&format!(
                    "Created configmap manifest: {}",
                    config_map_file_path.display()
                )),
                Err(e) => {
                    LOGGER.error(&format!(
                        "Failed to write configmap manifest {}: {}",
                        config_map_file_path.display(),
                        e
                    ));
                    return Err(CliError::Other(format!(
                        "Failed to write configmap manifest: {}",
                        e
                    )));
                }
            }

            // config.toml update
            let env_name = args.env_name.to_string(); // Fixed environment name for now
            match Environment::load_from_file(&env_name) {
                Ok(mut env) => {
                    let new_service = Service::new(
                        &args.service_name,
                        "default",
                        Some(args.service_name.as_str()), // path
                        None,                             // build
                        None,                             // major_version
                        None,                             // minor_version
                        None,                             // patch_version
                        Some("latest".to_string()),       // tag
                    );

                    // Check if service already exists to prevent duplicates
                    if env
                        .service_whitelist
                        .iter()
                        .any(|s| s.name == new_service.name)
                    {
                        LOGGER.warn(&format!(
                            "Service {} already exists in environment {}, skipping addition to config.toml.",
                            args.service_name, env_name
                        ));
                    } else {
                        env.service_whitelist.push(new_service);
                        match env.save_to_file() {
                            Ok(_) => LOGGER.info(&format!(
                                "Updated config.toml for environment {} with new service {}.",
                                env_name, args.service_name
                            )),
                            Err(e) => {
                                LOGGER.error(&format!(
                                    "Failed to save updated config.toml for environment {}: {}",
                                    env_name, e
                                ));
                                return Err(CliError::Other(format!(
                                    "Failed to save config.toml: {}",
                                    e
                                )));
                            }
                        }
                    }
                }
                Err(e) => {
                    // If the develop.toml doesn't exist, we might want to create it
                    // or instruct the user. For now, just error out.
                    LOGGER.error(&format!(
                        "Failed to load environment {}.toml: {}. Please ensure it exists or run 'sailr init {}' first.",
                        env_name, e, env_name
                    ));
                    return Err(CliError::Other(format!(
                        "Failed to load environment {}.toml: {}",
                        env_name, e
                    )));
                }
            }
        }
    }

    Ok(())
}
