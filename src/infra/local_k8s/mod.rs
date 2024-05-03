use std::path::Path;

use scribe_rust::log;

use crate::{environment::Environment, filesystem::FileSystemManager, utils::ENV_DIR};

use super::{ClusterConfig, ClusterTargetBuilder};

pub struct LocalK8 {
    pub nodes: u8,
    pub files: Vec<(String, String)>, // (filename, content)
    file_manager: FileSystemManager,
}

impl LocalK8 {
    pub fn new(name: String, nodes: u8) -> LocalK8 {
        LocalK8 {
            nodes,
            files: vec![
                ("main.tf".to_string(), include_str!("main.tf").to_string()),
                (
                    "outputs.tf".to_string(),
                    include_str!("outputs.tf").to_string(),
                ),
                (
                    "providers.tf".to_string(),
                    include_str!("providers.tf").to_string(),
                ),
            ],
            file_manager: FileSystemManager::new(
                Path::new(ENV_DIR).join(name).to_str().unwrap().to_string(),
            ),
        }
    }

    pub fn get_variables(&self) -> Vec<(String, String)> {
        vec![("nodes".to_string(), self.nodes.to_string())]
    }
}

impl ClusterTargetBuilder for LocalK8 {
    fn generate(&self, config: &ClusterConfig, variables: Vec<(String, String)>) {
        let mut vars = self.get_variables();
        vars.extend(variables.clone());
        for (filename, content) in &self.files {
            let generated_content = self.replace_variables(content.clone(), vars.clone());
            let path = Path::new(ENV_DIR).join(&config.cluster_name).join(filename);
            log(
                scribe_rust::Color::Gray,
                "Infra initialize",
                path.to_str().unwrap(),
            );
            self.file_manager
                .create_file(filename, &generated_content)
                .unwrap();
        }
    }

    fn build(&self, config: &ClusterConfig) {
        println!("Building local kubernetes cluster");
        // execute system process `tofu apply` in the directory
        std::process::Command::new("tofu")
            .arg("init")
            .current_dir(Path::new(ENV_DIR).join(&config.cluster_name))
            .output()
            .expect("Failed to execute terraform apply");

        let result = std::process::Command::new("tofu")
            .arg("apply")
            .arg("-auto-approve")
            .current_dir(Path::new(ENV_DIR).join(&config.cluster_name))
            .output()
            .expect("Failed to execute terraform apply");

        if result.status.success() {
            println!("Cluster built successfully");
        } else {
            println!("Cluster build failed");
            println!("{}", String::from_utf8_lossy(&result.stderr));
        }
    }
}
