use std::path::Path;

use scribe_rust::{log, Color};

use anyhow::Result;

use crate::{errors::GenerateError, filesystem::FileSystemManager, templates::Template};

pub struct Generator {
    filemanager: FileSystemManager,
    templates: Vec<Template>,
}

impl Generator {
    pub fn new() -> Generator {
        Generator {
            filemanager: FileSystemManager::new("./k8s/generated".to_string()),
            templates: Vec::new(),
        }
    }

    pub fn add_template(&mut self, original_template: &Template, new_content: String) {
        self.templates.push(Template::new(
            original_template.name.clone(),
            original_template.file_name.clone(),
            new_content,
        ));
    }

    pub fn generate(&mut self, name: &String) -> Result<(), GenerateError> {
        self.filemanager.delete_dir(name).unwrap();
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
