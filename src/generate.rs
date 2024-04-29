use std::path::Path;

use scribe_rust::{log, Color};

use anyhow::Result;

use crate::{
    config::Config, errors::GenerateError, filesystem::FileSystemManager, templates::Template,
};

pub struct Generator {
    filemanager: FileSystemManager,
    templates: Vec<Template>,
    config_maps: Vec<Config>,
}

impl Generator {
    pub fn new() -> Generator {
        Generator {
            filemanager: FileSystemManager::new("./k8s/generated".to_string()),
            templates: Vec::new(),
            config_maps: Vec::new(),
        }
    }

    pub fn add_template(&mut self, original_template: &Template, new_content: String) {
        self.templates.push(Template::new(
            original_template.name.clone(),
            original_template.file_name.clone(),
            new_content,
        ));
    }

    pub fn add_config_map(&mut self, config_map: &Config) {
        self.config_maps.push(Config::new(
            &config_map.name,
            &config_map.config_filenames,
            &config_map.content,
            &config_map.root_dir,
        ));
    }

    pub fn generate(&mut self, name: &String) -> Result<(), GenerateError> {
        self.filemanager.delete_dir(name).unwrap();

        for config_map in &self.config_maps {
            let path = Path::new(name)
                .join(&config_map.name)
                .join(&"configMap.yaml".to_string())
                .to_str()
                .unwrap()
                .to_string();
            match self
                .filemanager
                .create_file(&path, &config_map.content.to_string())
            {
                Ok(_) => log(Color::Blue, "Generated", &path),
                Err(_) => return Err(GenerateError::K8sResourceGenerationFailed(path)),
            };
        }

        println!(
            "templates: {:?}",
            self.templates
                .clone()
                .into_iter()
                .map(|x| x.name)
                .collect::<Vec<String>>()
        );

        for template in &self.templates {
            let path = Path::new(name)
                .join(&template.name)
                .join(&template.file_name)
                .to_str()
                .unwrap()
                .to_string();

            match self
                .filemanager
                .create_file(&path, &template.content.to_string())
            {
                Ok(_) => log(Color::Blue, "Generated", &path),
                Err(_) => return Err(GenerateError::K8sResourceGenerationFailed(path)),
            };
        }
        Ok(())
    }
}
