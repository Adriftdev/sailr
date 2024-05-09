use std::path::Path;

use scribe_rust::log;

use crate::{filesystem::FileSystemManager, load_global_vars, utils::ENV_DIR};

use super::{ClusterConfig, ClusterTargetBuilder, Infra};

pub struct AwsEks {
    pub region: String,
    pub files: Vec<(String, String)>, // (filename, content)
    file_manager: FileSystemManager,
}

impl AwsEks {
    pub fn new(name: String, region: String) -> AwsEks {
        AwsEks {
            files: vec![
                ("main.tf".to_string(), include_str!("main.tf").to_string()),
                (
                    "outputs.tf".to_string(),
                    include_str!("outputs.tf").to_string(),
                ),
                (
                    "terraform.tf".to_string(),
                    include_str!("terraform.tf").to_string(),
                ),
            ],
            file_manager: FileSystemManager::new(
                Path::new(ENV_DIR).join(name).to_str().unwrap().to_string(),
            ),
            region,
        }
    }

    pub fn get_variables(&self) -> Vec<(String, String)> {
        vec![("region".to_string(), self.region.to_string())]
    }
}

impl ClusterTargetBuilder for AwsEks {
    fn generate(&self, config: &ClusterConfig, variables: Vec<(String, String)>) {
        log(
            scribe_rust::Color::Blue,
            "Started",
            "Generating local kubernetes cluster",
        );

        let mut vars = load_global_vars().unwrap();
        vars.extend(variables.clone());
        for (filename, content) in &self.files {
            let generated_content = Infra::replace_variables(content.clone(), vars.clone());
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
