/*
pub enum ClusterTarget {
    Local(LocalKubernetesFlavor),
    Gcp(GcpKubernetesFlavor),
    Aws(AwsKubernetesFlavor),
    Azure(AzureKubernetesFlavor),
}
*/

use std::{collections::BTreeMap, path::Path};

use scribe_rust::log;

use crate::{filesystem::FileSystemManager, load_global_vars, utils::ENV_DIR};

pub mod aws_eks;

pub mod local_k8s;

pub struct ClusterConfig {
    pub cluster_name: String,
    pub kube_version: String,
}

pub trait ClusterTargetBuilder {
    fn generate(&self, config: &ClusterConfig, variables: Vec<(String, String)>); // variables: (key, value)
    fn build(&self, config: &ClusterConfig);
}

pub struct Infra {
    pub cluster_type: Box<dyn ClusterTargetBuilder>,
}

impl Infra {
    pub fn new(cluster_type: Box<dyn ClusterTargetBuilder>) -> Infra {
        Infra { cluster_type }
    }

    pub fn read_config(env_name: String) -> ClusterConfig {
        // @Note: This should be reading from a file
        ClusterConfig {
            cluster_name: env_name,
            kube_version: "v1.30.0".to_string(),
        }
    }

    pub fn generate(&self, config: ClusterConfig) {
        let variables = Infra::get_cluster_variables(&config);
        self.cluster_type.generate(&config, variables)
    }

    pub fn build(&self, config: ClusterConfig) {
        self.cluster_type.build(&config);
    }

    pub fn get_cluster_variables(config: &ClusterConfig) -> Vec<(String, String)> {
        vec![
            ("kube_version".to_string(), config.kube_version.clone()),
            ("cluster_name".to_string(), config.cluster_name.clone()),
        ]
    }

    fn replace_variables(content: String, variables: BTreeMap<String, String>) -> String {
        let mut new_content = content.clone();
        for (key, value) in variables {
            new_content = new_content.replace(&format!("{{{{{}}}}}", key), &value);
        }
        new_content
    }

    pub fn use_template(
        name: &String,
        template_path: &String,
        vars: &mut BTreeMap<String, String>,
    ) {
        log(
            scribe_rust::Color::Blue,
            "Started",
            "Generating local kubernetes cluster",
        );
        let config = &Infra::read_config(name.to_string());

        let file_manager =
            FileSystemManager::new(Path::new(ENV_DIR).join(name).to_str().unwrap().to_string());

        let files = FileSystemManager::new(Path::new(template_path).to_str().unwrap().to_string())
            .read_dir(&"".to_string())
            .unwrap()
            .into_iter()
            .map(|f| {
                (
                    f.clone(),
                    Path::new(template_path)
                        .join(f)
                        .to_str()
                        .unwrap()
                        .to_string(),
                )
            })
            .collect::<Vec<(String, String)>>();

        vars.extend(load_global_vars().unwrap());
        vars.extend(Infra::get_cluster_variables(config));

        for (filename, file_path) in files.into_iter() {
            println!("{}", file_path);
            let content = file_manager
                .read_file(&file_path, Some(&"".to_string()))
                .unwrap();

            let generated_content = Infra::replace_variables(content, vars.clone());
            let path = Path::new(ENV_DIR)
                .join(&config.cluster_name)
                .join(filename.clone());
            log(
                scribe_rust::Color::Gray,
                "Infra initialize",
                path.to_str().unwrap(),
            );
            file_manager
                .create_file(&filename, &generated_content)
                .unwrap();
        }

        log(
            scribe_rust::Color::Green,
            "Finished",
            "Generating local kubernetes cluster",
        );

        log(
            scribe_rust::Color::Blue,
            "Started",
            "Building local kubernetes cluster",
        );

        let handle = std::process::Command::new("tofu")
            .arg("init")
            .current_dir(Path::new(ENV_DIR).join(name))
            .spawn()
            .expect("Failed to execute terraform init");

        handle.wait_with_output().unwrap();

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

    pub fn apply(config: ClusterConfig) {
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

    pub fn destroy(config: ClusterConfig) {
        let handle = std::process::Command::new("tofu")
            .arg("destroy")
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
