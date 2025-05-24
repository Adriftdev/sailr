// tests/cli_integration_tests.rs

use sailr::cli::{Commands, InitArgs, AddServiceArgs, Provider}; // Cli removed
use sailr::environment::Environment;
use sailr::errors::CliError;
use sailr::templates::scaffolding::{generate_config_map, generate_deployment, generate_service};
use sailr::{create_default_env_config, create_default_env_infra, LOGGER}; // Access to main's logic handlers
use std::fs;
use std::path::{Path, PathBuf};
// std::io::Write removed

// Helper function to simulate parts of main's command handling.
// This will need to be adapted based on how `main.rs` is structured or refactored.
// For now, it's a placeholder.
async fn handle_command(command: Commands, _test_workspace_root: &Path) -> Result<(), CliError> {
    // In a real scenario, this function would need to:
    // 1. Set the current working directory to test_workspace_root
    // 2. Call the specific logic block from main.rs based on the command variant.
    // This might involve refactoring main.rs to expose these logic blocks.

    // Placeholder: Directly calling the logic from main.rs for Init
    // This is a simplified approach and might need significant adjustments
    // depending on the actual structure of main.rs.
    // The actual implementation will be done in a later step.

    match command {
        Commands::Init(args) => {
            sailr::templates::TemplateManager::new().copy_base_templates().unwrap();

            create_default_env_config(
                args.name.clone(),
                args.config_template_path,
                args.default_registry.clone(),
            );

            if let Some(template_path) = args.infra_template_path {
                create_default_env_infra(args.name.clone(), Some(template_path), args.default_registry)
            } else if let Some(provider) = args.provider {
                let infra = match provider {
                    Provider::Local => sailr::infra::Infra::new(Box::new(sailr::infra::local_k8s::LocalK8::new(args.name.clone()))),
                    _ => {
                        LOGGER.error(&format!("Provider {:?} not supported", provider));
                        // In a test, we might panic or return a specific error
                        return Err(CliError::Other("Unsupported provider".to_string()));
                    }
                };
                infra.generate(sailr::infra::Infra::read_config(args.name.clone()));
                infra.build(sailr::infra::Infra::read_config(args.name.clone()));
            } else {
                // Reflects the updated logic in main.rs: no default infra provisioning
                LOGGER.info("No infrastructure provider specified in test, skipping default infrastructure setup.");
            }

            // Add default "sample-app" service (logic copied from main.rs)
            let sample_service_name = "sample-app".to_string();
            let sample_app_type = "web-app".to_string();
            let sample_image = "nginx:latest".to_string();
            let sample_replicas = 1;
            let sample_port = 80;

            let sample_service_template_path_str =
                format!("k8s/templates/{}", sample_service_name);
            let sample_service_template_path = Path::new(&sample_service_template_path_str);

            fs::create_dir_all(sample_service_template_path).map_err(|e| CliError::Other(format!("Failed to create sample-app dir: {}", e)))?;
            LOGGER.info(&format!(
                "Created directory for sample-app: {}",
                sample_service_template_path.display()
            ));


            let deployment_content = generate_deployment(
                &sample_service_name,
                &sample_app_type,
                &sample_image,
                sample_replicas,
            );
            let service_content =
                generate_service(&sample_service_name, &sample_app_type, sample_port);
            let config_map_content =
                generate_config_map(&sample_service_name, &sample_app_type);

            let deployment_file_path =
                sample_service_template_path.join("deployment.yaml");
            let service_file_path = sample_service_template_path.join("service.yaml");
            let config_map_file_path =
                sample_service_template_path.join("configmap.yaml");

            for (path, content) in &[
                (&deployment_file_path, deployment_content),
                (&service_file_path, service_content),
                (&config_map_file_path, config_map_content),
            ] {
                fs::write(path, content).map_err(|e| CliError::Other(format!("Failed to write sample-app manifest: {}", e)))?;
                LOGGER.info(&format!(
                    "Created sample-app manifest: {}",
                    path.display()
                ));
            }

            let env_name = args.name.clone();
            match Environment::load_from_file(&env_name) {
                Ok(mut env) => {
                    let sample_service_entry = sailr::environment::Service::new(
                        &sample_service_name, // name: &str
                        "default",            // namespace: &str
                        Some(&sample_service_name), // path: Option<&str>
                        None,                 // build: Option<String>
                        None,                 // major_version: Option<u32>
                        None,                 // minor_version: Option<u32>
                        None,                 // patch_version: Option<u32>
                        Some("latest".to_string()), // tag: Option<String>
                    );

                    if !env.service_whitelist.iter().any(|s| s.name == sample_service_entry.name) {
                        env.service_whitelist.push(sample_service_entry);
                        env.save_to_file().map_err(|e| CliError::Other(format!("Failed to save updated config for sample-app: {}", e)))?;
                        LOGGER.info(&format!(
                            "Added sample-app service to environment {} config.",
                            env_name
                        ));
                    } else {
                         LOGGER.warn(&format!(
                            "Sample service {} already exists in environment {}, skipping addition.",
                            sample_service_name, env_name
                        ));
                    }
                }
                Err(e) => {
                    return Err(CliError::Other(format!("Failed to load env to add sample-app: {}", e)));
                }
            }
        }
        Commands::AddService(args) => {
            LOGGER.info(&format!(
                "Adding new service: {} of type {}",
                args.service_name, args.app_type
            ));

            let service_template_path_str = format!("k8s/templates/{}", args.service_name);
            let service_template_path = Path::new(&service_template_path_str);

            fs::create_dir_all(service_template_path).map_err(|e| CliError::Other(format!("Failed to create service dir: {}", e)))?;
            LOGGER.info(&format!(
                "Directory {} created/existed.",
                service_template_path.display()
            ));

            let image = "nginx:latest";
            let replicas = 1;
            let port = 80;

            let deployment_content = generate_deployment(&args.service_name, &args.app_type, image, replicas);
            let service_content = generate_service(&args.service_name, &args.app_type, port);
            let config_map_content = generate_config_map(&args.service_name, &args.app_type);

            let deployment_file_path = service_template_path.join("deployment.yaml");
            let service_file_path = service_template_path.join("service.yaml");
            let config_map_file_path = service_template_path.join("configmap.yaml");

            fs::write(&deployment_file_path, deployment_content).map_err(|e| CliError::Other(format!("Failed to write deployment: {}", e)))?;
            fs::write(&service_file_path, service_content).map_err(|e| CliError::Other(format!("Failed to write service: {}", e)))?;
            fs::write(&config_map_file_path, config_map_content).map_err(|e| CliError::Other(format!("Failed to write configmap: {}", e)))?;

            LOGGER.info("Generated template files for new service.");

            // For tests, we will use "develop" as the environment name for add-service
            let env_name = "develop".to_string();
            match Environment::load_from_file(&env_name) {
                Ok(mut env) => {
                    let new_service = sailr::environment::Service::new(
                        &args.service_name, // name: &str
                        "default",          // namespace: &str
                        Some(&args.service_name), // path: Option<&str>
                        None,               // build: Option<String>
                        None,               // major_version: Option<u32>
                        None,               // minor_version: Option<u32>
                        None,               // patch_version: Option<u32>
                        Some("latest".to_string()), // tag: Option<String>
                    );
                    if !env.service_whitelist.iter().any(|s| s.name == new_service.name) {
                        env.service_whitelist.push(new_service);
                        env.save_to_file().map_err(|e| CliError::Other(format!("Failed to save env for new service: {}", e)))?;
                        LOGGER.info(&format!("Updated env {} for service {}", env_name, args.service_name));
                    } else {
                        LOGGER.warn(&format!("Service {} already in env {}", args.service_name, env_name));
                    }
                }
                Err(e) => {
                    return Err(CliError::Other(format!("Failed to load env {} to add service: {}", env_name, e)));
                }
            }
        }
        // Other commands would be handled here
        _ => {
            unimplemented!("This command is not yet handled by the test helper.");
        }
    }
    Ok(())
}


const TEST_WORKSPACE_DIR: &str = "./test_workspace";

fn get_workspace_path() -> PathBuf {
    PathBuf::from(TEST_WORKSPACE_DIR)
}

fn setup_test_workspace() {
    let workspace_path = get_workspace_path();
    if workspace_path.exists() {
        fs::remove_dir_all(&workspace_path)
            .expect("Failed to remove existing test workspace.");
    }
    fs::create_dir_all(workspace_path.join("k8s/environments"))
        .expect("Failed to create environments directory in test workspace.");
    fs::create_dir_all(workspace_path.join("k8s/templates"))
        .expect("Failed to create templates directory in test workspace.");
}

fn cleanup_test_workspace() {
    let workspace_path = get_workspace_path();
    if workspace_path.exists() {
        fs::remove_dir_all(&workspace_path)
            .expect("Failed to remove test workspace.");
    }
}

#[tokio::test]
async fn test_init_creates_sample_app() {
    setup_test_workspace();
    let workspace_root = get_workspace_path();
    let original_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(&workspace_root).expect("Failed to set CWD to workspace root");

    let init_args = InitArgs {
        name: "testenv".to_string(),
        config_template_path: None,
        default_registry: None,
        provider: None, // Test the case where no provider is specified
        infra_template_path: None,
        region: None,
    };

    match handle_command(Commands::Init(init_args), &workspace_root).await {
        Ok(_) => (),
        Err(e) => {
            // Restore CWD before panicking
            std::env::set_current_dir(&original_cwd).expect("Failed to restore CWD");
            panic!("test_init_creates_sample_app failed during command execution: {:?}", e);
        }
    }
    
    // Assertions
    let config_path = workspace_root.join("k8s/environments/testenv/config.toml");
    assert!(config_path.exists(), "Config.toml was not created at {:?}", config_path);

    let env_name_for_load = "testenv".to_string();
    let env = Environment::load_from_file(&env_name_for_load) // Pass the environment name as &String
        .expect("Failed to load config.toml for testenv");
    assert!(
        env.service_whitelist.iter().any(|s| s.name == "sample-app"),
        "sample-app service not found in config.toml's service_whitelist"
    );

    let sample_app_template_path = workspace_root.join("k8s/templates/sample-app");
    assert!(sample_app_template_path.join("deployment.yaml").exists(), "sample-app deployment.yaml missing");
    assert!(sample_app_template_path.join("service.yaml").exists(), "sample-app service.yaml missing");
    assert!(sample_app_template_path.join("configmap.yaml").exists(), "sample-app configmap.yaml missing");

    std::env::set_current_dir(&original_cwd).expect("Failed to restore CWD");
    cleanup_test_workspace();
}

#[tokio::test]
async fn test_add_service_creates_service_files_and_updates_config() {
    setup_test_workspace();
    let workspace_root = get_workspace_path();
    let original_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(&workspace_root).expect("Failed to set CWD to workspace root");

    // 1. Initialize a "develop" environment first
    let init_args_develop = InitArgs {
        name: "develop".to_string(), // Environment name add_service expects
        config_template_path: None,
        default_registry: None,
        provider: Some(Provider::Local),
        infra_template_path: None,
        region: None,
    };

    if let Err(e) = handle_command(Commands::Init(init_args_develop), &workspace_root).await {
        std::env::set_current_dir(&original_cwd).expect("Failed to restore CWD");
        panic!("test_add_service setup (init) failed: {:?}", e);
    }

    // 2. Add the new service
    let add_service_args = AddServiceArgs {
        service_name: "new-service".to_string(),
        app_type: "web-app".to_string(),
    };

    if let Err(e) = handle_command(Commands::AddService(add_service_args), &workspace_root).await {
        std::env::set_current_dir(&original_cwd).expect("Failed to restore CWD");
        panic!("test_add_service (add service command) failed: {:?}", e);
    }

    // Assertions
    let new_service_template_path = workspace_root.join("k8s/templates/new-service");
    assert!(new_service_template_path.join("deployment.yaml").exists(), "new-service deployment.yaml missing");
    assert!(new_service_template_path.join("service.yaml").exists(), "new-service service.yaml missing");
    assert!(new_service_template_path.join("configmap.yaml").exists(), "new-service configmap.yaml missing");
    
    let config_path_develop = workspace_root.join("k8s/environments/develop/config.toml");
    assert!(config_path_develop.exists(), "Develop config.toml missing after add service");

    let develop_env_name_for_load = "develop".to_string();
    let env_develop = Environment::load_from_file(&develop_env_name_for_load) // Pass the environment name as &String
        .expect("Failed to load develop config.toml after adding service");
    assert!(
        env_develop.service_whitelist.iter().any(|s| s.name == "new-service"),
        "new-service not found in develop config.toml's service_whitelist"
    );

    std::env::set_current_dir(&original_cwd).expect("Failed to restore CWD");
    cleanup_test_workspace();
}
