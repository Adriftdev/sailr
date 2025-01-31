use std::path::Path;

use crate::{filesystem::FileSystemManager, load_global_vars, utils::ENV_DIR, LOGGER};

use super::{ClusterConfig, ClusterTargetBuilder, Infra};

pub struct LocalK8 {
    pub files: Vec<(String, String)>, // (filename, content)
    file_manager: FileSystemManager,
}

impl LocalK8 {
    pub fn new(name: String) -> LocalK8 {
        LocalK8 {
            files: vec![
                ("main.tf".to_string(), include_str!("main.tf").to_string()),
                (
                    "outputs.tf".to_string(),
                    include_str!("outputs.tf").to_string(),
                ),
                (
                    "providers.tf".to_string(),
                    include_str!("variables.tf").to_string(),
                ),
            ],
            file_manager: FileSystemManager::new(
                Path::new(ENV_DIR).join(name).to_str().unwrap().to_string(),
            ),
        }
    }
}

impl ClusterTargetBuilder for LocalK8 {
    fn generate(&self, config: &ClusterConfig, variables: Vec<(String, String)>) {
        LOGGER.info("Generating local kubernetes cluster");

        let mut vars = load_global_vars().unwrap();
        vars.extend(variables.clone());
        for (filename, content) in &self.files {
            let generated_content = Infra::replace_variables(content.clone(), vars.clone());
            let path = Path::new(ENV_DIR).join(&config.cluster_name).join(filename);
            LOGGER.trace(path.to_str().unwrap());
            self.file_manager
                .create_file(filename, &generated_content)
                .unwrap();
        }
    }

    fn build(&self, config: &ClusterConfig) {
        LOGGER.info("Building local kubernetes cluster");
        // execute system process `tofu apply` in the directory
        //
        let handle = std::process::Command::new("tofu")
            .arg("init")
            .current_dir(Path::new(ENV_DIR).join(&config.cluster_name))
            .spawn()
            .expect("Failed to execute terraform apply");

        let output = handle.wait_with_output().unwrap();

        println!("{:?}", output.stdout);

        let handle = std::process::Command::new("tofu")
            .arg("apply")
            .arg("-auto-approve")
            .current_dir(Path::new(ENV_DIR).join(&config.cluster_name))
            .spawn()
            .expect("Failed to execute terraform apply");

        let result = handle.wait_with_output().unwrap();

        if result.status.success() {
            println!("Cluster built successfully");
        } else {
            println!("Cluster build failed");
            println!("{}", String::from_utf8_lossy(&result.stderr));
        }
    }
}
