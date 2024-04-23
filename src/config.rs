use std::path::Path;

use crate::filesystem;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Config {
    pub name: String,
    pub config_filenames: Vec<String>,
    pub content: String,
    pub root_dir: String,
    file_manger: filesystem::FileSystemManager,
}

impl Config {
    pub fn new(
        name: &String,
        config_filenames: &Vec<String>,
        content: &String,
        dir: &String,
    ) -> Config {
        let config_path = Path::new(&dir).join(&name);

        Config {
            name: name.to_string(),
            config_filenames: config_filenames.to_vec(),
            content: content.to_string(),
            root_dir: dir.to_string(),
            file_manger: filesystem::FileSystemManager::new(
                config_path.to_str().unwrap().to_string(),
            ),
        }
    }

    pub fn create_config_map(&self) -> Result<(), String> {
        Ok(())
    }
}
