/*
pub enum ClusterTarget {
    Local(LocalKubernetesFlavor),
    Gcp(GcpKubernetesFlavor),
    Aws(AwsKubernetesFlavor),
    Azure(AzureKubernetesFlavor),
}
*/

pub mod local_k8s;

pub struct ClusterConfig {
    pub cluster_name: String,
    pub kube_version: String,
}

pub trait ClusterTargetBuilder {
    fn generate(&self, config: &ClusterConfig, variables: Vec<(String, String)>); // variables: (key, value)
    fn build(&self, config: &ClusterConfig);
    fn replace_variables(&self, content: String, variables: Vec<(String, String)>) -> String {
        let mut new_content = content.clone();
        for (key, value) in variables {
            new_content = new_content.replace(&format!("{{{{{}}}}}", key), &value);
        }
        new_content
    }
}

pub struct Infra {
    pub cluster_type: Box<dyn ClusterTargetBuilder>,
}

impl Infra {
    pub fn new(cluster_type: Box<dyn ClusterTargetBuilder>) -> Infra {
        Infra { cluster_type }
    }

    pub fn read_config(&self, env_name: String) -> ClusterConfig {
        // @Note: This should be reading from a file
        ClusterConfig {
            cluster_name: env_name,
            kube_version: "v1.28.3".to_string(),
        }
    }

    pub fn generate(&self, config: ClusterConfig) {
        let variables = self.get_cluster_variables(&config);
        self.cluster_type.generate(&config, variables)
    }

    pub fn build(&self, config: ClusterConfig) {
        self.cluster_type.build(&config);
    }

    pub fn get_cluster_variables(&self, config: &ClusterConfig) -> Vec<(String, String)> {
        vec![
            ("kube_version".to_string(), config.kube_version.clone()),
            ("cluster_name".to_string(), config.cluster_name.clone()),
        ]
    }
}
