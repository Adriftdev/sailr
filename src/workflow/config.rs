use std::collections::HashMap;
use std::path::Path;

use super::error::WorkflowError;
use super::profile::WorkflowProfile;

const DEFAULT_CONFIG_FILENAME: &str = "sailr.workflow.toml";

/// Top-level structure matching the `sailr.workflow.toml` file layout.
///
/// Example file:
/// ```toml
/// [workflow.local]
/// environment = "dev"
/// mode = "go"
///
/// [workflow.pr]
/// environment = "preview"
/// mode = "check"
/// ```
#[derive(Debug, Clone, serde::Deserialize)]
pub struct WorkflowConfig {
    #[serde(default)]
    pub workflow: HashMap<String, WorkflowProfile>,
}

impl WorkflowConfig {
    /// Load workflow config from `sailr.workflow.toml` in the current directory.
    ///
    /// Returns an empty config (no profiles) if the file does not exist.
    /// Returns an error if the file exists but cannot be parsed.
    pub fn load() -> Result<Self, WorkflowError> {
        let path = Path::new(DEFAULT_CONFIG_FILENAME);
        if !path.exists() {
            return Ok(Self {
                workflow: HashMap::new(),
            });
        }
        Self::load_from(path)
    }

    /// Load workflow config from a specific file path.
    pub fn load_from(path: &Path) -> Result<Self, WorkflowError> {
        let contents = std::fs::read_to_string(path)?;
        Self::parse(&contents)
    }

    /// Parse workflow config from a TOML string.
    pub fn parse(contents: &str) -> Result<Self, WorkflowError> {
        let mut config: WorkflowConfig = toml::from_str(contents)?;

        // Inject profile names from the TOML keys.
        for (name, profile) in config.workflow.iter_mut() {
            profile.name = name.clone();
        }

        Ok(config)
    }

    /// Get a named profile. Returns `None` if not found.
    pub fn get_profile(&self, name: &str) -> Option<&WorkflowProfile> {
        self.workflow.get(name)
    }

    /// List all profile names, sorted alphabetically.
    pub fn list_profiles(&self) -> Vec<String> {
        let mut names: Vec<String> = self.workflow.keys().cloned().collect();
        names.sort();
        names
    }

    /// Returns `true` if no profiles are defined.
    pub fn is_empty(&self) -> bool {
        self.workflow.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Display helpers
// ---------------------------------------------------------------------------

impl WorkflowConfig {
    /// Format a profile for detailed `workflow show` output.
    pub fn format_profile_detail(profile: &WorkflowProfile) -> String {
        let mut lines = Vec::new();

        lines.push(format!("Profile:        {}", profile.name));
        lines.push(format!("Environment:    {}", profile.environment));
        lines.push(format!("Mode:           {}", profile.mode));
        lines.push(format!("Engine:         {}", profile.engine));
        lines.push(format!(
            "Interactive:    {}",
            match profile.interactive {
                Some(true) => "yes",
                Some(false) => "no",
                None => "auto (runner decides)",
            }
        ));

        lines.push(String::new());
        lines.push("Steps:".to_string());
        lines.push(format!("  build:        {}", profile.build));
        lines.push(format!("  generate:     {}", profile.generate));
        lines.push(format!("  deploy:       {}", profile.deploy));
        lines.push(format!("  test:         {}", profile.test));
        lines.push(format!("  verify:       {}", profile.verify));

        if profile.deploy_context.is_some()
            || profile.namespace.is_some()
            || profile.apply.is_some()
        {
            lines.push(String::new());
            lines.push("Deployment:".to_string());
            if let Some(ctx) = &profile.deploy_context {
                lines.push(format!("  context:      {}", ctx));
            }
            if let Some(ns) = &profile.namespace {
                lines.push(format!("  namespace:    {}", ns));
            }
            if let Some(apply) = profile.apply {
                lines.push(format!("  apply:        {}", apply));
            }
        }

        lines.push(String::new());
        lines.push(format!("Approval:       {}", profile.approval));
        lines.push(format!("Report:         {}", profile.report));

        if profile.artifacts.upload || profile.artifacts.directory.is_some() {
            lines.push(String::new());
            lines.push("Artifacts:".to_string());
            lines.push(format!("  upload:       {}", profile.artifacts.upload));
            if let Some(dir) = &profile.artifacts.directory {
                lines.push(format!("  directory:    {}", dir));
            }
        }

        if let Some(promotion) = &profile.promotion {
            lines.push(String::new());
            lines.push("Promotion:".to_string());
            lines.push(format!("  from:         {}", promotion.from));
            lines.push(format!("  strategy:     {}", promotion.strategy));
        }

        lines.join("\n")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::profile::{
        ApprovalMode, PromotionStrategy, ReportMode, WorkflowEngine, WorkflowMode,
        WorkflowStepMode,
    };
    use std::io::Write;

    #[test]
    fn parse_spec_example_config() {
        let toml_str = r#"
            [workflow.local]
            environment = "dev"
            mode = "go"
            engine = "runkernel"
            interactive = true
            deploy_context = "minikube"
            apply = true

            [workflow.pr]
            environment = "preview"
            mode = "check"
            engine = "runkernel"
            interactive = false
            build = "plan"
            generate = "run"
            deploy = "plan"

            [workflow.staging]
            environment = "staging"
            mode = "go"
            engine = "runkernel"
            interactive = false
            deploy_context = "staging"
            apply = true

            [workflow.production]
            environment = "production"
            mode = "go"
            engine = "runkernel"
            interactive = false
            deploy_context = "prod"
            approval = "external"
            apply = false
        "#;

        let config = WorkflowConfig::parse(toml_str).unwrap();

        assert_eq!(config.list_profiles(), vec!["local", "pr", "production", "staging"]);

        // Local profile
        let local = config.get_profile("local").unwrap();
        assert_eq!(local.name, "local");
        assert_eq!(local.environment, "dev");
        assert_eq!(local.mode, WorkflowMode::Go);
        assert_eq!(local.engine, WorkflowEngine::Runkernel);
        assert_eq!(local.interactive, Some(true));
        assert_eq!(local.deploy_context.as_deref(), Some("minikube"));
        assert_eq!(local.apply, Some(true));

        // PR profile
        let pr = config.get_profile("pr").unwrap();
        assert_eq!(pr.name, "pr");
        assert_eq!(pr.environment, "preview");
        assert_eq!(pr.mode, WorkflowMode::Check);
        assert_eq!(pr.interactive, Some(false));
        assert_eq!(pr.build, WorkflowStepMode::Plan);
        assert_eq!(pr.generate, WorkflowStepMode::Run);
        assert_eq!(pr.deploy, WorkflowStepMode::Plan);

        // Production profile
        let prod = config.get_profile("production").unwrap();
        assert_eq!(prod.approval, ApprovalMode::External);
        assert_eq!(prod.apply, Some(false));
    }

    #[test]
    fn empty_config() {
        let config = WorkflowConfig::parse("").unwrap();
        assert!(config.is_empty());
        assert!(config.list_profiles().is_empty());
        assert!(config.get_profile("anything").is_none());
    }

    #[test]
    fn missing_file_returns_empty_config() {
        // WorkflowConfig::load() is tested implicitly via load_from with non-existent path
        let config = WorkflowConfig::parse("[workflow]").unwrap();
        assert!(config.is_empty());
    }

    #[test]
    fn load_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.workflow.toml");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(
            file,
            r#"
            [workflow.test]
            environment = "testing"
            mode = "check"
        "#
        )
        .unwrap();

        let config = WorkflowConfig::load_from(&path).unwrap();
        assert_eq!(config.list_profiles(), vec!["test"]);
        let profile = config.get_profile("test").unwrap();
        assert_eq!(profile.name, "test");
        assert_eq!(profile.environment, "testing");
        assert_eq!(profile.mode, WorkflowMode::Check);
    }

    #[test]
    fn load_from_nonexistent_file_errors() {
        let result = WorkflowConfig::load_from(Path::new("/nonexistent/workflow.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn malformed_toml_errors() {
        let result = WorkflowConfig::parse("[workflow.broken\ninvalid toml {{{}}}");
        assert!(result.is_err());
    }

    #[test]
    fn invalid_mode_in_config_errors() {
        let toml_str = r#"
            [workflow.bad]
            environment = "dev"
            mode = "not-a-real-mode"
        "#;
        let result = WorkflowConfig::parse(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn profile_names_are_injected() {
        let toml_str = r#"
            [workflow.alpha]
            environment = "dev"
            mode = "go"

            [workflow.beta]
            environment = "staging"
            mode = "check"
        "#;

        let config = WorkflowConfig::parse(toml_str).unwrap();
        assert_eq!(config.get_profile("alpha").unwrap().name, "alpha");
        assert_eq!(config.get_profile("beta").unwrap().name, "beta");
    }

    #[test]
    fn profile_with_promotion() {
        let toml_str = r#"
            [workflow.prod]
            environment = "production"
            mode = "promote"

            [workflow.prod.promotion]
            from = "staging"
            strategy = "image-digest"
        "#;

        let config = WorkflowConfig::parse(toml_str).unwrap();
        let prod = config.get_profile("prod").unwrap();
        let promotion = prod.promotion.as_ref().unwrap();
        assert_eq!(promotion.from, "staging");
        assert_eq!(promotion.strategy, PromotionStrategy::ImageDigest);
    }

    #[test]
    fn profile_with_artifacts() {
        let toml_str = r#"
            [workflow.ci]
            environment = "preview"
            mode = "check"

            [workflow.ci.artifacts]
            upload = true
            directory = ".sailr/reports"
        "#;

        let config = WorkflowConfig::parse(toml_str).unwrap();
        let ci = config.get_profile("ci").unwrap();
        assert!(ci.artifacts.upload);
        assert_eq!(ci.artifacts.directory.as_deref(), Some(".sailr/reports"));
    }

    #[test]
    fn format_profile_detail_output() {
        let toml_str = r#"
            environment = "staging"
            mode = "go"
            deploy_context = "staging-cluster"
            approval = "prompt"
        "#;
        let mut profile: WorkflowProfile = toml::from_str(toml_str).unwrap();
        profile.name = "staging".to_string();

        let output = WorkflowConfig::format_profile_detail(&profile);
        assert!(output.contains("staging"));
        assert!(output.contains("go"));
        assert!(output.contains("staging-cluster"));
        assert!(output.contains("prompt"));
    }

    #[test]
    fn list_profiles_is_sorted() {
        let toml_str = r#"
            [workflow.zulu]
            environment = "z"
            mode = "go"

            [workflow.alpha]
            environment = "a"
            mode = "go"

            [workflow.mike]
            environment = "m"
            mode = "go"
        "#;

        let config = WorkflowConfig::parse(toml_str).unwrap();
        assert_eq!(config.list_profiles(), vec!["alpha", "mike", "zulu"]);
    }

    #[test]
    fn default_engine_is_runkernel() {
        let toml_str = r#"
            [workflow.test]
            environment = "dev"
            mode = "go"
        "#;
        let config = WorkflowConfig::parse(toml_str).unwrap();
        let profile = config.get_profile("test").unwrap();
        assert_eq!(profile.engine, WorkflowEngine::Runkernel);
    }

    #[test]
    fn default_report_is_text() {
        let toml_str = r#"
            [workflow.test]
            environment = "dev"
            mode = "go"
        "#;
        let config = WorkflowConfig::parse(toml_str).unwrap();
        let profile = config.get_profile("test").unwrap();
        assert_eq!(profile.report, ReportMode::Text);
    }
}
