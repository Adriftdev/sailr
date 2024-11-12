use std::{collections::BTreeSet, error::Error, path::Path};

use scribe_rust::{log, Color};

use crate::{config::Config, environment::Environment, filesystem::FileSystemManager};

#[derive(Clone, Debug)]
pub struct Template {
    pub name: String,
    pub file_name: String,
    pub content: String,
    pub full_path: String,
}

impl Template {
    pub fn new(name: String, file_name: String, content: String) -> Template {
        Template {
            name: name.clone(),
            file_name: file_name.clone(),
            content,
            full_path: format!("{}/{}", name.clone(), file_name.clone()),
        }
    }
}

pub struct TemplateManager {
    filemanager: FileSystemManager,
    templates: Vec<(String, String)>,
}

impl TemplateManager {
    // Creates a new TemplateManager instance.
    // The `./k8s/templates` directory is used to store Sailr's base templates.
    pub fn new() -> TemplateManager {
        TemplateManager {
            filemanager: FileSystemManager::new("./k8s/templates".to_string()),
            templates: Vec::new(),
        }
    }

    // Copies the base templates embedded in the binary to the `./k8s/templates` directory.
    // This is used to provide boilerplate resource definitions for generating Kubernetes resources.
    pub fn copy_base_templates(&mut self) -> Result<(), Box<dyn Error>> {
        self.templates.push((
            "aux/redis/deployment.yaml".to_string(),
            include_str!("k8s/redis/deployment.yaml").to_string(),
        ));
        self.templates.push((
            "aux/redis/service.yaml".to_string(),
            include_str!("k8s/redis/service.yaml").to_string(),
        ));
        self.templates.push((
            "aux/postgres/deployment.yaml".to_string(),
            include_str!("k8s/postgres/deployment.yaml").to_string(),
        ));
        self.templates.push((
            "aux/postgres/service.yaml".to_string(),
            include_str!("k8s/postgres/service.yaml").to_string(),
        ));
        self.templates.push((
            "aux/postgres/pvc.yaml".to_string(),
            include_str!("k8s/postgres/pvc.yaml").to_string(),
        ));
        self.templates.push((
            "aux/registry/deployment.yaml".to_string(),
            include_str!("k8s/registry/deployment.yaml").to_string(),
        ));
        self.templates.push((
            "aux/registry/service.yaml".to_string(),
            include_str!("k8s/registry/service.yaml").to_string(),
        ));
        self.templates.push((
            "aux/registry/pvc.yaml".to_string(),
            include_str!("k8s/registry/pvc.yaml").to_string(),
        ));

        for (name, template) in &self.templates {
            self.filemanager
                .create_file(&name.to_string(), &template.to_string())?;
        }

        log(
            Color::Green,
            "Success",
            "Copied Sailr base templates to ./k8s/templates",
        );

        Ok(())
    }

    // Each tuple in the returned vector consists of:
    // - The directory name of the template
    // - The file name of the template
    // - The content of the template
    pub fn read_templates(
        &mut self,
        env: Option<&Environment>,
    ) -> Result<(Vec<Template>, Vec<Config>), Box<dyn Error>> {
        let mut template_dirs: BTreeSet<String> = BTreeSet::new();
        //read the templates from the environment
        //if path is specified in path, read the templates from the path instead and append to the template_dirs
        if let Some(env) = &env {
            for service in &env.service_whitelist {
                if service.path == None || service.path.clone().unwrap() == "".to_string() {
                    template_dirs.insert(service.name.clone());
                    continue;
                }

                let path = service
                    .path
                    .clone()
                    .unwrap()
                    .split("/")
                    .into_iter()
                    .fold(Path::new(&"".to_string()).to_owned(), |acc, x| acc.join(x));

                let parent = path.parent().unwrap().to_str().unwrap().to_string();

                let service_template_dir = self.filemanager.read_dir(&parent)?;
                template_dirs.remove(&path.parent().unwrap().to_str().unwrap().to_string());

                let templates = service_template_dir.into_iter().map(|x| {
                    Path::new(path.parent().unwrap())
                        .join(x)
                        .to_str()
                        .unwrap()
                        .to_string()
                });
                template_dirs.extend(templates);
            }
        }

        let mut templates = Vec::new();
        let mut config_maps = Vec::new();

        for template_name in template_dirs {
            let template_dir = self.filemanager.read_dir(&template_name)?;

            if let Some(env) = &env {
                if !env.service_whitelist.iter().any(|x| {
                    if template_name.contains(x.name.as_str()) {
                        return true;
                    }
                    return false;
                }) {
                    continue;
                }
            }

            for template_file in template_dir {
                if template_file == "config" {
                    let (config_name, config_map_content) =
                        self.read_config_files(&template_name)?;
                    config_maps.push(Config::new(
                        &template_name.clone(),
                        &config_map_content,
                        &config_name,
                        &"./k8s/templates".to_string(),
                    ));
                    continue;
                }

                let path = Path::new(&template_name)
                    .join(&template_file)
                    .to_str()
                    .unwrap()
                    .to_string();

                let template = self.filemanager.read_file(&path, None)?;
                templates.push(Template::new(
                    template_name.clone(),
                    template_file,
                    template,
                ));
            }
        }

        Ok((templates, config_maps))
    }

    pub fn read_config_files(
        &self,
        path: &String,
    ) -> Result<(String, Vec<String>), Box<dyn Error>> {
        let config_files = self.filemanager.read_dir(
            &Path::new(&path.to_string())
                .join("config")
                .to_str()
                .unwrap()
                .to_string(),
        )?;

        let config_map = config_files.into_iter().fold(
            (
                format!(
                    "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: {}\ndata:\n",
                    path
                )
                .to_string(),
                Vec::new(),
            ),
            |mut acc: (String, Vec<String>), config| {
                let config_file = &Path::new(&path.to_string())
                    .join("config")
                    .join(&config)
                    .to_str()
                    .unwrap()
                    .to_string();

                let config_content = self
                    .filemanager
                    .read_file(&config_file, Some(&"./k8s/templates".to_string()))
                    .unwrap();

                acc.0.push_str(&format!("\n  {:?}: |", &config));

                for line in config_content.lines() {
                    acc.0.push_str(&format!("\n    {}", &line))
                }

                acc.1.push(config);
                acc
            },
        );

        println!("{:?}", config_map);

        Ok(config_map)
    }

    // Replaces variables in the template. Any missing variable will be left untouched,
    // and the processed template is returned on success.
    pub fn replace_variables(
        &self,
        template: &Template,
        variables: &Vec<(String, String)>,
    ) -> Result<String, Box<dyn Error>> {
        let mut content = template.content.clone();

        for (key, value) in variables {
            content = content.replace(&format!("{{{{{}}}}}", key), &value);
        }

        match self.validate_yaml(content.clone()) {
            Ok(_) => log(
                Color::Green,
                "Passed Check",
                &format!("{}", template.full_path),
            ),
            Err(e) => {
                log(
                    Color::Red,
                    "Failed Check",
                    &format!("YAML Validation Error: {}\n {}", template.full_path, e,),
                );
                std::process::exit(1);
            }
        };
        Ok(content)
    }

    // Performs basic syntax validation on the provided YAML string using `serde_yaml`.
    // This checks for correct formatting and structure but does not involve schema validation.
    // An error is returned if the YAML string is invalid.
    pub fn validate_yaml(&self, yaml: String) -> Result<(), Box<dyn Error>> {
        let _ = match serde_yaml::from_str::<serde_yaml::Value>(&yaml) {
            Ok(_) => (),
            Err(e) => {
                let location = e.location().unwrap();
                let line = read_line_number(&yaml, location.line());
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{} \n{}", e, line),
                )
                .into());
            }
        };
        Ok(())
    }
}

fn read_line_number(input: &str, line: usize) -> String {
    let mut line_count = 0;
    let lines = input.lines();

    let window_size = 5; // Fixed window size of 5 lines
    let mut result = Vec::with_capacity(window_size * 2 + 1);

    for line_content in lines {
        line_count += 1;
        if line_count > line + window_size {
            break; // Reached window limit
        }
        if line_count >= line - window_size && line_count <= line + window_size {
            if line_count == line {
                result.push(format!(
                    "{}: {} {}<< Validation error occurred here{}",
                    line_count, line_content, "\x1b[1;31m", "\x1b[0m"
                ));
            // Highlight line
            } else {
                result.push(format!("{}: {}", line_count, line_content)); // Include lines within window
            }
        }
    }

    result.join("\n")
}
