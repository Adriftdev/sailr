use serde::{Deserialize, Serialize};

/// A workflow profile defines how a particular workflow (local dev, PR check,
/// staging deploy, production deploy) should behave. Profiles are loaded from
/// `sailr.workflow.toml` and converted into runkernel pipelines by the planner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowProfile {
    /// Profile name, injected at load time from the TOML key.
    #[serde(skip)]
    pub name: String,

    /// Target environment name (must match a Sailr environment config).
    pub environment: String,

    /// Overall workflow mode — what this profile does.
    pub mode: WorkflowMode,

    /// Execution engine.
    #[serde(default = "default_engine")]
    pub engine: WorkflowEngine,

    /// Whether the workflow can prompt for user input.
    /// `None` means the runner decides (local = true, CI = false).
    #[serde(default)]
    pub interactive: Option<bool>,

    /// How to handle the build step.
    #[serde(default)]
    pub build: Option<WorkflowStepMode>,

    /// How to handle the generate step.
    #[serde(default)]
    pub generate: Option<WorkflowStepMode>,

    /// How to handle the deploy step.
    #[serde(default)]
    pub deploy: Option<WorkflowStepMode>,

    /// How to handle the test step.
    #[serde(default)]
    pub test: Option<WorkflowStepMode>,

    /// How to handle the verify step.
    #[serde(default)]
    pub verify: Option<WorkflowStepMode>,

    /// Kubernetes context to deploy to.
    #[serde(default)]
    pub deploy_context: Option<String>,

    /// Kubernetes namespace override.
    #[serde(default)]
    pub namespace: Option<String>,

    /// How approvals are handled before deployment.
    #[serde(default)]
    pub approval: ApprovalMode,

    /// Whether to apply changes (mutate the cluster).
    #[serde(default)]
    pub apply: Option<bool>,

    /// The Docker Buildx remote builder endpoint (e.g. ssh://my-builder)
    #[serde(default)]
    pub remote_builder: Option<String>,

    /// Report output format.
    #[serde(default)]
    pub report: ReportMode,

    /// Artifact upload and storage policy.
    #[serde(default)]
    pub artifacts: ArtifactPolicy,

    /// Promotion policy for reusing artifacts from another environment.
    #[serde(default)]
    pub promotion: Option<PromotionPolicy>,
}

impl WorkflowProfile {
    /// Returns a human-readable summary line for list display.
    pub fn summary_line(&self) -> String {
        let interactive_label = match self.interactive {
            Some(true) => "interactive",
            Some(false) => "non-interactive",
            None => "auto",
        };
        format!(
            "{:<16} env={:<14} mode={:<10} {}",
            self.name,
            self.environment,
            self.mode.as_str(),
            interactive_label,
        )
    }

    /// Normalizes the profile by applying mode-aware defaults based on whether it is running in CI.
    pub fn normalize(&self, runner_is_ci: bool) -> NormalizedWorkflowProfile {
        let mut interactive = self.interactive.unwrap_or(!runner_is_ci);
        let mut approval = self.approval;
        let mut apply = self.apply.unwrap_or(false);

        let (default_build, default_generate, default_deploy, default_test, default_verify) =
            match self.mode {
                WorkflowMode::Check => (
                    WorkflowStepMode::Plan,
                    WorkflowStepMode::Run,
                    WorkflowStepMode::Disabled,
                    WorkflowStepMode::Disabled,
                    WorkflowStepMode::Disabled,
                ),
                WorkflowMode::Build => (
                    WorkflowStepMode::Run,
                    WorkflowStepMode::Disabled,
                    WorkflowStepMode::Disabled,
                    WorkflowStepMode::Disabled,
                    WorkflowStepMode::Disabled,
                ),
                WorkflowMode::Go => (
                    WorkflowStepMode::Run,
                    WorkflowStepMode::Run,
                    if apply {
                        WorkflowStepMode::Run
                    } else {
                        WorkflowStepMode::Plan
                    },
                    WorkflowStepMode::Disabled,
                    WorkflowStepMode::Disabled,
                ),
                WorkflowMode::Deploy => (
                    WorkflowStepMode::Disabled,
                    WorkflowStepMode::Run,
                    if apply {
                        WorkflowStepMode::Run
                    } else {
                        WorkflowStepMode::Plan
                    },
                    WorkflowStepMode::Disabled,
                    WorkflowStepMode::Disabled,
                ),
                WorkflowMode::Promote | WorkflowMode::Rollback => (
                    WorkflowStepMode::Disabled,
                    WorkflowStepMode::Disabled,
                    WorkflowStepMode::Disabled,
                    WorkflowStepMode::Disabled,
                    WorkflowStepMode::Disabled,
                ),
            };

        let build = self.build.unwrap_or(default_build);
        let generate = self.generate.unwrap_or(default_generate);
        let mut deploy = self.deploy.unwrap_or(default_deploy);
        let test = self.test.unwrap_or(default_test);
        let verify = self.verify.unwrap_or(default_verify);

        match self.mode {
            WorkflowMode::Check => {
                interactive = false;
                approval = ApprovalMode::None;
                apply = false;
                deploy = WorkflowStepMode::Disabled;
            }
            WorkflowMode::Build => {
                approval = ApprovalMode::None;
                apply = false;
            }
            WorkflowMode::Go | WorkflowMode::Deploy => {
                if approval == ApprovalMode::None {
                    approval = if runner_is_ci {
                        ApprovalMode::External
                    } else {
                        ApprovalMode::Prompt
                    };
                }
            }
            WorkflowMode::Promote | WorkflowMode::Rollback => {}
        }

        NormalizedWorkflowProfile {
            name: self.name.clone(),
            environment: self.environment.clone(),
            mode: self.mode,
            engine: self.engine,
            interactive,
            build,
            generate,
            deploy,
            test,
            verify,
            deploy_context: self.deploy_context.clone(),
            namespace: self.namespace.clone(),
            approval,
            apply,
            report: self.report,
        }
    }
}

/// A normalized version of the WorkflowProfile with all implicit defaults explicitly resolved.
#[derive(Debug, Clone)]
pub struct NormalizedWorkflowProfile {
    pub name: String,
    pub environment: String,
    pub mode: WorkflowMode,
    pub engine: WorkflowEngine,
    pub interactive: bool,
    pub build: WorkflowStepMode,
    pub generate: WorkflowStepMode,
    pub deploy: WorkflowStepMode,
    pub test: WorkflowStepMode,
    pub verify: WorkflowStepMode,
    pub deploy_context: Option<String>,
    pub namespace: Option<String>,
    pub approval: ApprovalMode,
    pub apply: bool,
    pub report: ReportMode,
}

fn default_engine() -> WorkflowEngine {
    WorkflowEngine::Runkernel
}

// ---------------------------------------------------------------------------
// WorkflowMode
// ---------------------------------------------------------------------------

/// The overall intent of a workflow profile.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum WorkflowMode {
    Build,
    Check,
    Deploy,
    Go,
    Promote,
    Rollback,
}

impl WorkflowMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Check => "check",
            Self::Deploy => "deploy",
            Self::Go => "go",
            Self::Promote => "promote",
            Self::Rollback => "rollback",
        }
    }
}

impl std::fmt::Display for WorkflowMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// WorkflowEngine
// ---------------------------------------------------------------------------

/// Execution engine for the workflow.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum WorkflowEngine {
    Runkernel,
    Roomservice,
}

impl std::fmt::Display for WorkflowEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Runkernel => f.write_str("runkernel"),
            Self::Roomservice => f.write_str("roomservice"),
        }
    }
}

// ---------------------------------------------------------------------------
// WorkflowStepMode
// ---------------------------------------------------------------------------

/// Controls how a specific step (build, generate, deploy, etc.) behaves
/// within a workflow profile.
///
/// Defaults to `Disabled`. The workflow planner applies context-aware defaults
/// based on `WorkflowMode` when a step is not explicitly configured.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum WorkflowStepMode {
    #[default]
    Disabled,
    Plan,
    DryRun,
    Run,
}

impl WorkflowStepMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Plan => "plan",
            Self::DryRun => "dry-run",
            Self::Run => "run",
        }
    }

    pub fn is_disabled(&self) -> bool {
        *self == Self::Disabled
    }

    pub fn is_active(&self) -> bool {
        !self.is_disabled()
    }
}

impl std::fmt::Display for WorkflowStepMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// ApprovalMode
// ---------------------------------------------------------------------------

/// How deployment approval is handled.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalMode {
    /// No approval needed.
    #[default]
    None,
    /// Prompt in local terminal.
    Prompt,
    /// CI provider or external system handles approval.
    External,
    /// Command must include `--approve` or `--apply`.
    RequireFlag,
}

impl ApprovalMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Prompt => "prompt",
            Self::External => "external",
            Self::RequireFlag => "require-flag",
        }
    }
}

impl std::fmt::Display for ApprovalMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// ReportMode
// ---------------------------------------------------------------------------

/// Output format for workflow reports.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ReportMode {
    #[default]
    Text,
    Json,
    Both,
}

impl std::fmt::Display for ReportMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text => f.write_str("text"),
            Self::Json => f.write_str("json"),
            Self::Both => f.write_str("both"),
        }
    }
}

// ---------------------------------------------------------------------------
// ArtifactPolicy
// ---------------------------------------------------------------------------

/// Controls artifact upload and storage behaviour.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ArtifactPolicy {
    /// Whether to upload artifacts (e.g. to CI artifact storage).
    #[serde(default)]
    pub upload: bool,

    /// Directory to write reports and artifacts to.
    #[serde(default)]
    pub directory: Option<String>,
}

// ---------------------------------------------------------------------------
// PromotionPolicy
// ---------------------------------------------------------------------------

/// Policy for promoting artifacts from one environment to another
/// instead of rebuilding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionPolicy {
    /// Source environment to promote from.
    pub from: String,

    /// How to identify and transfer the artifact.
    pub strategy: PromotionStrategy,
}

/// Strategy for artifact promotion.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PromotionStrategy {
    ImageTag,
    ImageDigest,
    ArtifactManifest,
}

impl std::fmt::Display for PromotionStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ImageTag => f.write_str("image-tag"),
            Self::ImageDigest => f.write_str("image-digest"),
            Self::ArtifactManifest => f.write_str("artifact-manifest"),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_profile() {
        let toml_str = r#"
            environment = "dev"
            mode = "go"
        "#;
        let profile: WorkflowProfile = toml::from_str(toml_str).unwrap();
        assert_eq!(profile.environment, "dev");
        assert_eq!(profile.mode, WorkflowMode::Go);
        assert_eq!(profile.engine, WorkflowEngine::Runkernel);
        assert_eq!(profile.build, None);
        assert_eq!(profile.generate, None);
        assert_eq!(profile.deploy, None);
        assert_eq!(profile.approval, ApprovalMode::None);
        assert_eq!(profile.report, ReportMode::Text);
        assert!(profile.interactive.is_none());
        assert!(profile.deploy_context.is_none());
        assert!(profile.namespace.is_none());
        assert!(profile.apply.is_none());
        assert!(profile.promotion.is_none());
        assert!(!profile.artifacts.upload);
        assert!(profile.artifacts.directory.is_none());
    }

    #[test]
    fn parse_full_profile() {
        let toml_str = r#"
            environment = "production"
            mode = "go"
            engine = "runkernel"
            interactive = false
            build = "disabled"
            generate = "run"
            deploy = "plan"
            test = "run"
            verify = "run"
            deploy_context = "prod"
            namespace = "production"
            approval = "external"
            apply = false
            report = "both"

            [artifacts]
            upload = true
            directory = ".sailr/reports"

            [promotion]
            from = "staging"
            strategy = "image-digest"
        "#;
        let profile: WorkflowProfile = toml::from_str(toml_str).unwrap();
        assert_eq!(profile.environment, "production");
        assert_eq!(profile.mode, WorkflowMode::Go);
        assert_eq!(profile.engine, WorkflowEngine::Runkernel);
        assert_eq!(profile.interactive, Some(false));
        assert_eq!(profile.build, Some(WorkflowStepMode::Disabled));
        assert_eq!(profile.generate, Some(WorkflowStepMode::Run));
        assert_eq!(profile.deploy, Some(WorkflowStepMode::Plan));
        assert_eq!(profile.test, Some(WorkflowStepMode::Run));
        assert_eq!(profile.verify, Some(WorkflowStepMode::Run));
        assert_eq!(profile.deploy_context.as_deref(), Some("prod"));
        assert_eq!(profile.namespace.as_deref(), Some("production"));
        assert_eq!(profile.approval, ApprovalMode::External);
        assert_eq!(profile.apply, Some(false));
        assert_eq!(profile.report, ReportMode::Both);
        assert!(profile.artifacts.upload);
        assert_eq!(
            profile.artifacts.directory.as_deref(),
            Some(".sailr/reports")
        );
        let promotion = profile.promotion.unwrap();
        assert_eq!(promotion.from, "staging");
        assert_eq!(promotion.strategy, PromotionStrategy::ImageDigest);
    }

    #[test]
    fn parse_all_workflow_modes() {
        for (input, expected) in [
            ("build", WorkflowMode::Build),
            ("check", WorkflowMode::Check),
            ("deploy", WorkflowMode::Deploy),
            ("go", WorkflowMode::Go),
            ("promote", WorkflowMode::Promote),
            ("rollback", WorkflowMode::Rollback),
        ] {
            let toml_str = format!(
                r#"environment = "test"
                   mode = "{}""#,
                input
            );
            let profile: WorkflowProfile = toml::from_str(&toml_str).unwrap();
            assert_eq!(profile.mode, expected, "failed for input: {}", input);
        }
    }

    #[test]
    fn parse_all_step_modes() {
        for (input, expected) in [
            ("disabled", WorkflowStepMode::Disabled),
            ("plan", WorkflowStepMode::Plan),
            ("dry-run", WorkflowStepMode::DryRun),
            ("run", WorkflowStepMode::Run),
        ] {
            let toml_str = format!(
                r#"environment = "test"
                   mode = "go"
                   build = "{}""#,
                input
            );
            let profile: WorkflowProfile = toml::from_str(&toml_str).unwrap();
            assert_eq!(profile.build, Some(expected), "failed for input: {}", input);
        }
    }

    #[test]
    fn parse_all_approval_modes() {
        for (input, expected) in [
            ("none", ApprovalMode::None),
            ("prompt", ApprovalMode::Prompt),
            ("external", ApprovalMode::External),
            ("require-flag", ApprovalMode::RequireFlag),
        ] {
            let toml_str = format!(
                r#"environment = "test"
                   mode = "go"
                   approval = "{}""#,
                input
            );
            let profile: WorkflowProfile = toml::from_str(&toml_str).unwrap();
            assert_eq!(profile.approval, expected, "failed for input: {}", input);
        }
    }

    #[test]
    fn parse_all_promotion_strategies() {
        for (input, expected) in [
            ("image-tag", PromotionStrategy::ImageTag),
            ("image-digest", PromotionStrategy::ImageDigest),
            ("artifact-manifest", PromotionStrategy::ArtifactManifest),
        ] {
            let toml_str = format!(
                r#"environment = "test"
                   mode = "promote"
                   [promotion]
                   from = "staging"
                   strategy = "{}""#,
                input
            );
            let profile: WorkflowProfile = toml::from_str(&toml_str).unwrap();
            let promotion = profile.promotion.unwrap();
            assert_eq!(promotion.strategy, expected, "failed for input: {}", input);
        }
    }

    #[test]
    fn step_mode_defaults_to_disabled() {
        assert_eq!(WorkflowStepMode::default(), WorkflowStepMode::Disabled);
    }

    #[test]
    fn step_mode_is_active() {
        assert!(!WorkflowStepMode::Disabled.is_active());
        assert!(WorkflowStepMode::Plan.is_active());
        assert!(WorkflowStepMode::DryRun.is_active());
        assert!(WorkflowStepMode::Run.is_active());
    }

    #[test]
    fn workflow_mode_display() {
        assert_eq!(WorkflowMode::Go.to_string(), "go");
        assert_eq!(WorkflowMode::Check.to_string(), "check");
        assert_eq!(WorkflowMode::Promote.to_string(), "promote");
    }

    #[test]
    fn serde_roundtrip() {
        let toml_str = r#"
            environment = "staging"
            mode = "go"
            engine = "runkernel"
            interactive = false
            build = "run"
            generate = "run"
            deploy = "run"
            deploy_context = "staging"
            approval = "prompt"
            report = "json"
        "#;
        let profile: WorkflowProfile = toml::from_str(toml_str).unwrap();
        let serialized = toml::to_string(&profile).unwrap();
        let roundtripped: WorkflowProfile = toml::from_str(&serialized).unwrap();
        assert_eq!(profile.environment, roundtripped.environment);
        assert_eq!(profile.mode, roundtripped.mode);
        assert_eq!(profile.engine, roundtripped.engine);
        assert_eq!(profile.interactive, roundtripped.interactive);
        assert_eq!(profile.build, roundtripped.build);
        assert_eq!(profile.deploy, roundtripped.deploy);
        assert_eq!(profile.approval, roundtripped.approval);
        assert_eq!(profile.report, roundtripped.report);
    }

    #[test]
    fn invalid_mode_rejected() {
        let toml_str = r#"
            environment = "dev"
            mode = "yolo"
        "#;
        let result = toml::from_str::<WorkflowProfile>(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_step_mode_rejected() {
        let toml_str = r#"
            environment = "dev"
            mode = "go"
            build = "turbo"
        "#;
        let result = toml::from_str::<WorkflowProfile>(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn summary_line_contains_key_info() {
        let mut profile: WorkflowProfile = toml::from_str(
            r#"
            environment = "staging"
            mode = "go"
        "#,
        )
        .unwrap();
        profile.name = "staging".to_string();
        let line = profile.summary_line();
        assert!(line.contains("staging"));
        assert!(line.contains("go"));
        assert!(line.contains("auto"));
    }

    #[test]
    fn normalize_check_profile() {
        let toml_str = r#"
            environment = "test"
            mode = "check"
        "#;
        let profile: WorkflowProfile = toml::from_str(toml_str).unwrap();
        let normalized = profile.normalize(false);
        assert!(!normalized.interactive);
        assert_eq!(normalized.build, WorkflowStepMode::Plan);
        assert_eq!(normalized.generate, WorkflowStepMode::Run);
        assert_eq!(normalized.deploy, WorkflowStepMode::Disabled);
        assert_eq!(normalized.test, WorkflowStepMode::Disabled);
        assert_eq!(normalized.verify, WorkflowStepMode::Disabled);
        assert_eq!(normalized.approval, ApprovalMode::None);
        assert!(!normalized.apply);
    }

    #[test]
    fn check_profile_respects_explicit_build_disabled() {
        let toml_str = r#"
            name = "ci"
            environment = "local"
            mode = "check"
            build = "disabled"
            generate = "disabled"
            deploy = "disabled"
        "#;
        let profile: WorkflowProfile = toml::from_str(toml_str).unwrap();
        let normalized = profile.normalize(true);
        assert_eq!(normalized.build, WorkflowStepMode::Disabled);
        assert_eq!(normalized.generate, WorkflowStepMode::Disabled);
        assert_eq!(normalized.deploy, WorkflowStepMode::Disabled);
    }

    #[test]
    fn check_profile_defaults_omitted_build_to_plan() {
        let toml_str = r#"
            name = "default-check"
            environment = "local"
            mode = "check"
        "#;
        let profile: WorkflowProfile = toml::from_str(toml_str).unwrap();
        let normalized = profile.normalize(true);
        assert_eq!(normalized.build, WorkflowStepMode::Plan);
        assert_eq!(normalized.generate, WorkflowStepMode::Run);
        assert_eq!(normalized.deploy, WorkflowStepMode::Disabled);
    }

    #[test]
    fn normalize_build_profile() {
        let toml_str = r#"
            environment = "test"
            mode = "build"
        "#;
        let profile: WorkflowProfile = toml::from_str(toml_str).unwrap();
        let normalized = profile.normalize(false);
        assert_eq!(normalized.build, WorkflowStepMode::Run);
        assert_eq!(normalized.generate, WorkflowStepMode::Disabled);
        assert_eq!(normalized.deploy, WorkflowStepMode::Disabled);
        assert_eq!(normalized.approval, ApprovalMode::None);
        assert!(!normalized.apply);
    }

    #[test]
    fn normalize_local_deploy_profile() {
        let toml_str = r#"
            environment = "local"
            mode = "go"
            interactive = true
            deploy = "run"
            deploy_context = "minikube"
            namespace = "default"
            approval = "prompt"
            apply = true
        "#;
        let profile: WorkflowProfile = toml::from_str(toml_str).unwrap();
        let normalized = profile.normalize(false); // not CI
        assert_eq!(normalized.interactive, true);
        assert_eq!(normalized.deploy, WorkflowStepMode::Run);
        assert_eq!(normalized.approval, ApprovalMode::Prompt);
        assert_eq!(normalized.apply, true);
        assert_eq!(normalized.deploy_context.as_deref(), Some("minikube"));
        assert_eq!(normalized.namespace.as_deref(), Some("default"));
    }
}
