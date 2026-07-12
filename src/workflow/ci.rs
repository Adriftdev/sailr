use std::fs;
use std::path::PathBuf;

use super::error::WorkflowError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CiProvider {
    GitHub,
    CircleCi,
    Travis,
    Generic,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CiEnvironment {
    pub provider: CiProvider,
    pub run_id: Option<String>,
}

impl std::str::FromStr for CiProvider {
    type Err = WorkflowError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "github" => Ok(CiProvider::GitHub),
            "circleci" => Ok(CiProvider::CircleCi),
            "travis" => Ok(CiProvider::Travis),
            _ => Err(WorkflowError::ConfigError(format!(
                "Unsupported CI provider: {}",
                s
            ))),
        }
    }
}

pub struct CiTemplateGenerator;

impl CiTemplateGenerator {
    pub fn generate(profile_name: &str, provider: &CiProvider) -> String {
        match provider {
            CiProvider::GitHub => Self::generate_github(profile_name),
            CiProvider::CircleCi => Self::generate_circleci(profile_name),
            CiProvider::Travis => Self::generate_travis(profile_name),
            CiProvider::Generic => String::new(),
        }
    }

    pub fn default_output_path(profile_name: &str, provider: &CiProvider) -> PathBuf {
        match provider {
            CiProvider::GitHub => {
                PathBuf::from(format!(".github/workflows/sailr-{}.yml", profile_name))
            }
            CiProvider::CircleCi => PathBuf::from(".circleci/config.yml"),
            CiProvider::Travis => PathBuf::from(".travis.yml"),
            CiProvider::Generic => PathBuf::from("sailr-workflow.sh"),
        }
    }

    pub fn write_template(
        profile_name: &str,
        provider: &CiProvider,
        custom_path: Option<&str>,
    ) -> Result<PathBuf, WorkflowError> {
        let path = custom_path
            .map(PathBuf::from)
            .unwrap_or_else(|| Self::default_output_path(profile_name, provider));

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                WorkflowError::ConfigError(format!(
                    "Failed to create directories for CI template: {}",
                    e
                ))
            })?;
        }

        let content = Self::generate(profile_name, provider);
        fs::write(&path, content).map_err(|e| {
            WorkflowError::ConfigError(format!("Failed to write CI template: {}", e))
        })?;

        Ok(path)
    }

    fn generate_github(profile_name: &str) -> String {
        format!(
            r#"name: Sailr Workflow - {profile_name}

on:
  pull_request:
  push:
    branches: [main]

jobs:
  sailr-workflow:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Sailr
        run: cargo install --path .

      - name: Run Workflow
        run: sailr workflow run {profile_name} --non-interactive
"#,
            profile_name = profile_name
        )
    }

    fn generate_circleci(profile_name: &str) -> String {
        format!(
            r#"version: 2.1
jobs:
  sailr-workflow:
    docker:
      - image: cimg/base:current
    steps:
      - checkout
      - run:
          name: Install Sailr
          command: curl -sSL https://sailr.dev/install.sh | bash
      - run:
          name: Run Workflow
          command: sailr workflow run {profile_name}

workflows:
  sailr-pipeline:
    jobs:
      - sailr-workflow
"#,
            profile_name = profile_name
        )
    }

    fn generate_travis(profile_name: &str) -> String {
        format!(
            r#"language: minimal
os: linux
dist: jammy

install:
  - curl -sSL https://sailr.dev/install.sh | bash

script:
  - sailr workflow run {profile_name}
"#,
            profile_name = profile_name
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_parse_ci_provider() {
        assert!(matches!(
            CiProvider::from_str("github").unwrap(),
            CiProvider::GitHub
        ));
        assert!(matches!(
            CiProvider::from_str("CIRCLECI").unwrap(),
            CiProvider::CircleCi
        ));
        assert!(matches!(
            CiProvider::from_str("Travis").unwrap(),
            CiProvider::Travis
        ));
        assert!(CiProvider::from_str("jenkins").is_err());
    }

    #[test]
    fn test_generate_github() {
        let profile_name = "edge";
        let yaml = CiTemplateGenerator::generate(profile_name, &CiProvider::GitHub);
        assert!(yaml.contains("name: Sailr Workflow - edge"));
        assert!(yaml.contains("sailr workflow run edge --non-interactive"));
        assert!(yaml.contains("cargo install --path ."));
    }

    #[test]
    fn test_generate_circleci() {
        let profile_name = "edge";
        let yaml = CiTemplateGenerator::generate(profile_name, &CiProvider::CircleCi);
        assert!(yaml.contains("sailr workflow run edge"));
        assert!(yaml.contains("cimg/base:current"));
    }

    #[test]
    fn test_generate_travis() {
        let profile_name = "edge";
        let yaml = CiTemplateGenerator::generate(profile_name, &CiProvider::Travis);
        assert!(yaml.contains("sailr workflow run edge"));
        assert!(yaml.contains("language: minimal"));
    }

    #[test]
    fn test_default_paths() {
        let profile_name = "edge";
        assert_eq!(
            CiTemplateGenerator::default_output_path(profile_name, &CiProvider::GitHub),
            PathBuf::from(".github/workflows/sailr-edge.yml")
        );
        assert_eq!(
            CiTemplateGenerator::default_output_path(profile_name, &CiProvider::CircleCi),
            PathBuf::from(".circleci/config.yml")
        );
        assert_eq!(
            CiTemplateGenerator::default_output_path(profile_name, &CiProvider::Travis),
            PathBuf::from(".travis.yml")
        );
    }
}
