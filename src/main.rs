use std::{io, process::exit};

use sailr::{
    builder::{split_matches, Builder},
    cli::{Cli, Commands, EnvType, InfraCommands, Provider},
    create_default_env_config,
    create_default_env_infra,
    deployment::k8sm8::{
        logs::{log_merger, log_streamer},
        pods::get_all_pods,
    },
    environment::{Environment, Service},
    errors::CliError,
    generate,
    infra::{local_k8s::LocalK8, Infra},
    plan::{generate_deployment_plan, validate_plan_safety},
    templates::{
        scaffolding::{generate_secret_template, get_service_template}, // Added scaffolding functions
        TemplateManager,
    },
    LOGGER, // filesystem::FileSystemManager, // FileSystemManager is not directly used here, fs is used.
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
            LOGGER.info(&format!(
                "🚀 Initializing new Sailr environment: {}",
                arg.name
            ));

            TemplateManager::new().copy_base_templates().unwrap();

            // Create environment configuration
            create_default_env_config(
                arg.name.clone(),
                arg.config_template_path,
                arg.default_registry.clone(),
            );

            // Handle infrastructure setup
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
                LOGGER.info("No infrastructure provider specified, skipping default infrastructure setup. Use 'sailr infra up' to provision later if needed.");
            }

            // Enhanced sample service creation
            let should_create_sample = arg.with_sample && !arg.no_sample;

            if should_create_sample {
                LOGGER.info("📦 Creating sample service for immediate testing...");

                let sample_service_name = "hello-sailr".to_string();
                let sample_app_type = match arg.env_type {
                    Some(EnvType::Development) => "web-app".to_string(),
                    Some(EnvType::Staging) => "api".to_string(),
                    Some(EnvType::Production) => "api".to_string(),
                    None => "web-app".to_string(),
                };

                let sample_image = match sample_app_type.as_str() {
                    "web-app" => "nginx:latest".to_string(),
                    "api" => "node:16-alpine".to_string(),
                    _ => "nginx:latest".to_string(),
                };
                let sample_port = 80;

                let sample_service_template_path_str =
                    format!("k8s/templates/{}", sample_service_name);
                let sample_service_template_path = Path::new(&sample_service_template_path_str);

                match fs::create_dir_all(sample_service_template_path) {
                    Ok(_) => LOGGER.info(&format!(
                        "✓ Created directory for sample service: {}",
                        sample_service_template_path.display()
                    )),
                    Err(e) => {
                        LOGGER.error(&format!(
                            "Failed to create directory for sample service {}: {}",
                            sample_service_template_path.display(),
                            e
                        ));
                        return Err(CliError::Other(format!(
                            "Failed to create directory for sample service: {}",
                            e
                        )));
                    }
                }

                // Use enhanced scaffolding system
                let template = get_service_template(
                    &sample_app_type,
                    &sample_service_name,
                    &sample_image,
                    sample_port,
                );

                // Write all template files
                let files_to_write = vec![
                    ("deployment.yaml", &template.deployment),
                    ("service.yaml", &template.service),
                    ("configmap.yaml", &template.config_map),
                ];

                for (filename, content) in files_to_write {
                    if !content.is_empty() {
                        let file_path = sample_service_template_path.join(filename);
                        match fs::write(&file_path, content) {
                            Ok(_) => LOGGER.info(&format!("✓ Created {}", filename)),
                            Err(e) => {
                                LOGGER.error(&format!(
                                    "Failed to write {} manifest {}: {}",
                                    filename,
                                    file_path.display(),
                                    e
                                ));
                                return Err(CliError::Other(format!(
                                    "Failed to write {} manifest: {}",
                                    filename, e
                                )));
                            }
                        }
                    }
                }

                // Write optional files
                if let Some(ingress_content) = &template.ingress {
                    let ingress_file_path = sample_service_template_path.join("ingress.yaml");
                    match fs::write(&ingress_file_path, ingress_content) {
                        Ok(_) => LOGGER.info("✓ Created ingress.yaml"),
                        Err(e) => {
                            LOGGER.error(&format!(
                                "Failed to write ingress manifest {}: {}",
                                ingress_file_path.display(),
                                e
                            ));
                        }
                    }
                }

                if let Some(hpa_content) = &template.hpa {
                    let hpa_file_path = sample_service_template_path.join("hpa.yaml");
                    match fs::write(&hpa_file_path, hpa_content) {
                        Ok(_) => LOGGER.info("✓ Created hpa.yaml"),
                        Err(e) => {
                            LOGGER.error(&format!(
                                "Failed to write HPA manifest {}: {}",
                                hpa_file_path.display(),
                                e
                            ));
                        }
                    }
                }

                // Update environment configuration with sample service
                let env_name = arg.name.clone();
                match Environment::load_from_file(&env_name) {
                    Ok(mut env) => {
                        let sample_service_entry = Service::new(
                            &sample_service_name,
                            "default",
                            Some(sample_service_name.as_str()),
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
                                    "✓ Added {} service to environment {} config.",
                                    sample_service_name, env_name
                                )),
                                Err(e) => {
                                    LOGGER.error(&format!(
                                        "Failed to save updated config for environment {}: {}",
                                        env_name, e
                                    ));
                                    return Err(CliError::Other(format!(
                                        "Failed to save config for sample service: {}",
                                        e
                                    )));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        LOGGER.error(&format!(
                            "Failed to load environment {} to add sample service: {}",
                            env_name, e
                        ));
                        return Err(CliError::Other(format!(
                            "Failed to load environment config for sample service: {}",
                            e
                        )));
                    }
                }

                // Provide next steps guidance
                LOGGER.info("🎉 Environment initialized successfully!");
                LOGGER.info("");
                LOGGER.info("Next steps:");
                LOGGER.info(&format!(
                    "  1. Deploy your environment: sailr go <context> {}",
                    arg.name
                ));
                LOGGER.info("  2. Check deployment status: kubectl get pods");
                LOGGER.info(&format!(
                    "  3. Add more services: sailr add-service <name> --type <type> --name {}",
                    arg.name
                ));
                LOGGER.info("");
                LOGGER.info(&format!(
                    "Your sample '{}' service is ready to deploy!",
                    sample_service_name
                ));

                if sample_app_type == "web-app" {
                    LOGGER.info("  - Access via: kubectl port-forward svc/hello-sailr 8080:80");
                    LOGGER.info("  - Then visit: http://localhost:8080");
                }
            } else {
                LOGGER.info("🎉 Environment initialized successfully!");
                LOGGER.info("");
                LOGGER.info("Next steps:");
                LOGGER.info(&format!(
                    "  1. Add a service: sailr add-service <name> --type <type> --name {}",
                    arg.name
                ));
                LOGGER.info(&format!(
                    "  2. Deploy your environment: sailr go <context> {}",
                    arg.name
                ));
                LOGGER.info("  3. Check deployment status: kubectl get pods");
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
            if !arg.apply {
                LOGGER.info("🔍 Generating deployment plan...");

                match generate_deployment_plan(&arg.name, &arg.context).await {
                    Ok(plan) => {
                        validate_plan_safety(&plan).map_err(|e| {
                            CliError::Other(format!("Plan validation failed: {}", e))
                        })?;
                        plan.display();
                    }
                    Err(e) => {
                        LOGGER.error(&format!("Failed to generate deployment plan: {}", e));
                        return Err(CliError::Other(format!("Plan generation failed: {}", e)));
                    }
                }
            } else {
                LOGGER.info(&format!("Deploying environment '{}'", arg.name));
                sailr::deployment::deploy(arg.context.to_string(), &arg.name, arg.strategy).await?;
            }
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
            LOGGER.info(&format!(
                "🚀 Building, generating and deploying environment '{}'",
                arg.name
            ));

            let env = match Environment::load_from_file(&arg.name) {
                Ok(env) => env,
                Err(e) => {
                    LOGGER.error(&format!("Failed to load environment: {}", e));
                    std::process::exit(1);
                }
            };

            let mut services = env.list_services();

            if let Some(ref ignored_services) = arg.ignore {
                services = services
                    .into_iter()
                    .filter(|s| !ignored_services.contains(&s.name))
                    .collect();
            }

            if let Some(ref only_services) = arg.only {
                services = services
                    .into_iter()
                    .filter(|s| only_services.contains(&s.name))
                    .collect();
            }

            if !arg.skip_build {
                let mut builder = Builder::new(
                    ".roomservice".to_string(),
                    arg.force,
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
            }

            generate(&arg.name, &env, services);

            if !arg.apply {
                LOGGER.info("🔍 Generating deployment plan for build-generate-deploy workflow...");

                match generate_deployment_plan(&arg.name, &arg.context).await {
                    Ok(plan) => {
                        validate_plan_safety(&plan).map_err(|e| {
                            CliError::Other(format!("Plan validation failed: {}", e))
                        })?;
                        plan.display();
                        LOGGER.info("");
                        LOGGER.info("Note: This plan shows the final deployment state.");

                        // inquire for confirmation to proceed with deployment
                        let confirm = inquire::Confirm::new("Proceed with deployment?")
                            .with_default(true)
                            .prompt()
                            .map_err(|e| {
                                CliError::Other(format!("Failed to confirm deployment: {}", e))
                            })?;

                        if !confirm {
                            LOGGER.info("Deployment cancelled by user.");
                            return Ok(());
                        }
                    }
                    Err(e) => {
                        LOGGER.error(&format!("Failed to generate deployment plan: {}", e));
                        return Err(CliError::Other(format!("Plan generation failed: {}", e)));
                    }
                }
            }

            sailr::deployment::deploy(arg.context.to_string(), &arg.name, arg.strategy).await?;
        }
        Commands::AddService(args) => {
            LOGGER.info(&format!(
                "Adding new service: {} of type {}",
                args.service_name, args.app_type
            ));

            // Validate service type
            let valid_types = vec!["web-app", "worker", "database-client", "api"];
            if !valid_types.contains(&args.app_type.as_str()) {
                LOGGER.warn(&format!(
                    "Unknown service type '{}'. Using default template. Valid types: {}",
                    args.app_type,
                    valid_types.join(", ")
                ));
            }

            let service_template_path_str = format!("k8s/templates/{}", args.service_name);
            let service_template_path = Path::new(&service_template_path_str);

            match fs::create_dir_all(service_template_path) {
                Ok(_) => {
                    LOGGER.info(&format!(
                        "Created directory: {}",
                        service_template_path.display()
                    ));
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

            // Enhanced Template Generation
            let image = args.image.unwrap_or_else(|| match args.app_type.as_str() {
                "web-app" => "nginx:latest".to_string(),
                "worker" => "ubuntu:latest".to_string(),
                "database-client" => "postgres:13".to_string(),
                "api" => "node:16-alpine".to_string(),
                _ => "nginx:latest".to_string(),
            });
            let port = args.port.unwrap_or(80);

            let template = get_service_template(&args.app_type, &args.service_name, &image, port);

            // Write deployment manifest
            let deployment_file_path = service_template_path.join("deployment.yaml");
            match fs::write(&deployment_file_path, &template.deployment) {
                Ok(_) => LOGGER.info(&format!(
                    "✓ Created deployment manifest: {}",
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

            // Write service manifest (if not empty)
            if !template.service.is_empty() {
                let service_file_path = service_template_path.join("service.yaml");
                match fs::write(&service_file_path, &template.service) {
                    Ok(_) => LOGGER.info(&format!(
                        "✓ Created service manifest: {}",
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
            }

            // Write configmap manifest
            let config_map_file_path = service_template_path.join("configmap.yaml");
            match fs::write(&config_map_file_path, &template.config_map) {
                Ok(_) => LOGGER.info(&format!(
                    "✓ Created configmap manifest: {}",
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

            // Write ingress manifest (if provided)
            if let Some(ingress_content) = &template.ingress {
                let ingress_file_path = service_template_path.join("ingress.yaml");
                match fs::write(&ingress_file_path, ingress_content) {
                    Ok(_) => LOGGER.info(&format!(
                        "✓ Created ingress manifest: {}",
                        ingress_file_path.display()
                    )),
                    Err(e) => {
                        LOGGER.error(&format!(
                            "Failed to write ingress manifest {}: {}",
                            ingress_file_path.display(),
                            e
                        ));
                        return Err(CliError::Other(format!(
                            "Failed to write ingress manifest: {}",
                            e
                        )));
                    }
                }
            }

            // Write HPA manifest (if provided)
            if let Some(hpa_content) = &template.hpa {
                let hpa_file_path = service_template_path.join("hpa.yaml");
                match fs::write(&hpa_file_path, hpa_content) {
                    Ok(_) => LOGGER.info(&format!(
                        "✓ Created HPA manifest: {}",
                        hpa_file_path.display()
                    )),
                    Err(e) => {
                        LOGGER.error(&format!(
                            "Failed to write HPA manifest {}: {}",
                            hpa_file_path.display(),
                            e
                        ));
                        return Err(CliError::Other(format!(
                            "Failed to write HPA manifest: {}",
                            e
                        )));
                    }
                }
            }

            // Write secrets manifest for certain service types
            if matches!(args.app_type.as_str(), "database-client" | "api" | "worker") {
                let secret_content = generate_secret_template(&args.service_name, &args.app_type);
                let secret_file_path = service_template_path.join("secret.yaml");
                match fs::write(&secret_file_path, secret_content) {
                    Ok(_) => LOGGER.info(&format!(
                        "✓ Created secret manifest: {}",
                        secret_file_path.display()
                    )),
                    Err(e) => {
                        LOGGER.error(&format!(
                            "Failed to write secret manifest {}: {}",
                            secret_file_path.display(),
                            e
                        ));
                        return Err(CliError::Other(format!(
                            "Failed to write secret manifest: {}",
                            e
                        )));
                    }
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
        Commands::Interactive(args) => {
            // Handle interactive commands
            sailr::interactive::main_menu(args)
                .await
                .map_err(|e| CliError::Other(format!("Interactive mode failed: {}", e)))?;
            exit(0)
        }
    }

    Ok(())
}
