use crate::workflow::plan::WorkflowPlan;

pub fn render_workflow_plan_text(plan: &WorkflowPlan) -> String {
    let mut out = String::new();
    out.push_str(&format!("Sailr Workflow Plan: {}\n", plan.profile.name));
    out.push_str(&format!("Environment: {}\n", plan.profile.environment));
    out.push_str(&format!("Mode: {:?}\n", plan.profile.mode));
    out.push_str(&format!("Engine: {:?}\n", plan.profile.engine));
    out.push_str(&format!("Build: {:?}\n", plan.profile.build));
    out.push_str(&format!("Push: {:?}\n", plan.profile.push));
    out.push_str(&format!("Generate: {:?}\n", plan.profile.generate));
    out.push_str(&format!("Deploy: {:?}\n", plan.profile.deploy));
    out.push_str("\nTasks:\n");

    for task in &plan.tasks {
        out.push_str(&format!(" - [{}] {}\n", task.id, task.label));
        out.push_str(&format!("   Kind: {:?}\n", task.kind));
        if !task.dependencies.is_empty() {
            out.push_str(&format!(
                "   Depends On: {}\n",
                task.dependencies.join(", ")
            ));
        }
        out.push_str(&format!("   Description: {}\n", task.description));
        out.push('\n');
    }

    if plan.profile.approval == crate::workflow::profile::ApprovalMode::External {
        out.push_str("Approval:\n");
        out.push_str("  mode: external\n");
        out.push_str("  provider: GitHub Environment\n");
        out.push_str(&format!("  environment: {}\n\n", plan.profile.environment));
    }

    out.push_str("Overall Effects:\n");
    out.push_str(&format!(
        " - Mutates Filesystem: {}\n",
        plan.effects.mutates_filesystem
    ));
    out.push_str(&format!(
        " - Mutates Docker: {}\n",
        plan.effects.mutates_docker
    ));
    out.push_str(&format!(
        " - Mutates Registry: {}\n",
        plan.effects.mutates_registry
    ));
    out.push_str(&format!(
        " - Mutates Cluster: {}\n",
        plan.effects.mutates_cluster
    ));
    out.push_str(&format!(" - Prompts User: {}\n", plan.effects.prompts_user));

    out
}

pub fn render_workflow_graph_text(plan: &WorkflowPlan) -> String {
    let mut out = String::new();
    out.push_str(&format!("Workflow Graph: {}\n\n", plan.profile.name));

    // Simple text-based adjacency list
    for task in &plan.tasks {
        let outgoing: Vec<_> = plan
            .edges
            .iter()
            .filter(|e| e.from == task.id)
            .map(|e| e.to.clone())
            .collect();

        if outgoing.is_empty() {
            out.push_str(&format!("{} -> (end)\n", task.id));
        } else {
            out.push_str(&format!("{} -> {}\n", task.id, outgoing.join(", ")));
        }
    }

    out
}

pub fn render_workflow_graph_mermaid(plan: &WorkflowPlan) -> String {
    let mut out = String::new();
    out.push_str("graph TD\n");

    for task in &plan.tasks {
        out.push_str(&format!(
            "  {}[{}]\n",
            sanitize_id(&task.id),
            sanitize_label(&task.label)
        ));
    }

    out.push('\n');

    for edge in &plan.edges {
        out.push_str(&format!(
            "  {} --> {}\n",
            sanitize_id(&edge.from),
            sanitize_id(&edge.to)
        ));
    }

    out
}

pub fn render_workflow_explain_text(plan: &WorkflowPlan, task_id: &str) -> Result<String, String> {
    let task = plan
        .tasks
        .iter()
        .find(|t| t.id == task_id)
        .ok_or_else(|| format!("Task '{}' not found in plan.", task_id))?;

    let mut out = String::new();
    out.push_str(&format!("Task Explanation: {}\n", task.id));
    out.push_str("--------------------------------------------------\n");
    out.push_str(&format!("Label:       {}\n", task.label));
    out.push_str(&format!("Kind:        {:?}\n", task.kind));
    out.push_str(&format!("Description: {}\n", task.description));

    if task.dependencies.is_empty() {
        out.push_str("Dependencies: (none)\n");
    } else {
        out.push_str(&format!("Dependencies: {}\n", task.dependencies.join(", ")));
    }

    if task.kind == crate::workflow::plan::WorkflowTaskKind::Deploy {
        if plan.profile.approval == crate::workflow::profile::ApprovalMode::External {
            out.push_str("\nApproval:\n");
            out.push_str("  mode: external\n");
            out.push_str("  provider: GitHub Environment\n");
            out.push_str(&format!("  environment: {}\n", plan.profile.environment));
        }

        if let Some(ctx) = &plan.profile.deploy_context {
            out.push_str("\nContext:\n");
            out.push_str(&format!("  {}\n", ctx));
        }

        if let Some(ns) = &plan.profile.namespace {
            out.push_str("\nNamespace:\n");
            out.push_str(&format!("  {}\n", ns));
        }
    }

    out.push_str("\nSide Effects:\n");
    out.push_str(&format!(
        " - Mutates Filesystem: {}\n",
        task.effects.mutates_filesystem
    ));
    out.push_str(&format!(
        " - Mutates Docker: {}\n",
        task.effects.mutates_docker
    ));
    out.push_str(&format!(
        " - Mutates Registry: {}\n",
        task.effects.mutates_registry
    ));
    out.push_str(&format!(
        " - Mutates Cluster: {}\n",
        task.effects.mutates_cluster
    ));
    out.push_str(&format!(" - Prompts User: {}\n", task.effects.prompts_user));

    Ok(out)
}

fn sanitize_id(id: &str) -> String {
    id.replace("-", "_").replace(":", "_")
}

fn sanitize_label(label: &str) -> String {
    label.replace("\"", "'")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::plan::{
        WorkflowEdge, WorkflowEffects, WorkflowTaskKind, WorkflowTaskPlan,
    };
    use crate::workflow::profile::{
        ApprovalMode, NormalizedWorkflowProfile, ReportMode, WorkflowEngine, WorkflowMode,
        WorkflowStepMode,
    };
    use crate::workflow::runner::{RunnerContext, RunnerKind};

    fn dummy_plan() -> WorkflowPlan {
        WorkflowPlan {
            profile: NormalizedWorkflowProfile {
                name: "test".to_string(),
                environment: "local".to_string(),
                mode: WorkflowMode::Check,
                engine: WorkflowEngine::Runkernel,
                interactive: false,
                build: WorkflowStepMode::Run,
                push: WorkflowStepMode::Disabled,
                generate: WorkflowStepMode::Run,
                deploy: WorkflowStepMode::Disabled,
                test: WorkflowStepMode::Disabled,
                verify: WorkflowStepMode::Disabled,
                deploy_context: None,
                namespace: None,
                approval: ApprovalMode::None,
                apply: false,
                report: ReportMode::Text,
            },
            runner: RunnerContext {
                kind: RunnerKind::Local,
                ci: false,
                interactive: false,
            },
            tasks: vec![
                WorkflowTaskPlan {
                    id: "workflow:validate".to_string(),
                    label: "Validate".to_string(),
                    kind: WorkflowTaskKind::ValidateConfig,
                    dependencies: vec![],
                    effects: WorkflowEffects::default(),
                    description: "Validates config".to_string(),
                },
                WorkflowTaskPlan {
                    id: "build:api".to_string(),
                    label: "Build API".to_string(),
                    kind: WorkflowTaskKind::ServiceBuild,
                    dependencies: vec!["workflow:validate".to_string()],
                    effects: WorkflowEffects {
                        mutates_docker: true,
                        ..Default::default()
                    },
                    description: "Builds API".to_string(),
                },
            ],
            edges: vec![WorkflowEdge {
                from: "workflow:validate".to_string(),
                to: "build:api".to_string(),
            }],
            build_plan: None,
            image_push_plan: None,
            effects: WorkflowEffects {
                mutates_docker: true,
                ..Default::default()
            },
        }
    }

    #[test]
    fn test_render_plan_text() {
        let plan = dummy_plan();
        let text = render_workflow_plan_text(&plan);
        assert!(text.contains("Sailr Workflow Plan: test"));
        assert!(text.contains("Mutates Docker: true"));
        assert!(text.contains("build:api"));
    }

    #[test]
    fn test_render_graph_mermaid() {
        let plan = dummy_plan();
        let text = render_workflow_graph_mermaid(&plan);
        assert!(text.contains("graph TD"));
        assert!(text.contains("workflow_validate[Validate]"));
        assert!(text.contains("build_api[Build API]"));
        assert!(text.contains("workflow_validate --> build_api"));
    }

    #[test]
    fn test_render_explain() {
        let plan = dummy_plan();
        let text = render_workflow_explain_text(&plan, "build:api").unwrap();
        assert!(text.contains("Task Explanation: build:api"));
        assert!(text.contains("Mutates Docker: true"));
    }
}

pub fn render_image_push_plan_text(plan: &crate::workflow::image::ImagePushPlanReport) -> String {
    let mut out = String::new();

    out.push_str("Sailr image push plan:\n");
    out.push_str(&format!("  environment: {}\n", plan.environment));
    out.push_str(&format!(
        "  mutates registry: {}\n\n",
        if plan.mutates_registry { "yes" } else { "no" }
    ));

    out.push_str("Images:\n");

    if plan.items.is_empty() {
        out.push_str("  none\n");
    } else {
        for item in &plan.items {
            out.push_str(&format!(
                "  - service: {}\n    image: {}\n    action: would push\n",
                item.service, item.target_image_ref
            ));
        }
    }

    out
}

#[cfg(test)]
mod tests_addendum {
    use super::*;

    #[test]
    fn render_image_push_plan_text_includes_planned_image_ref() {
        let report = crate::workflow::image::ImagePushPlanReport {
            environment: "staging".to_string(),
            mutates_registry: false,
            items: vec![crate::workflow::image::ImagePushPlanItem {
                service: "ci-build-hello".to_string(),
                registry: "ghcr.io".to_string(),
                repository: "adriftdev/sailr/ci-build-hello".to_string(),
                tag: "61eaa8b".to_string(),
                target_image_ref: "ghcr.io/adriftdev/sailr/ci-build-hello:61eaa8b".to_string(),
                local_image_ref: "ghcr.io/adriftdev/sailr/ci-build-hello:61eaa8b".to_string(),
                source_sha: "61eaa8bb0e52f5bb1d5a621760b0a2eae601ccd3".to_string(),
                action: crate::workflow::image::ImagePushPlanAction::WouldPush,
            }],
        };

        let text = render_image_push_plan_text(&report);
        assert!(text.contains("Sailr image push plan:"));
        assert!(text.contains("mutates registry: no"));
        assert!(text.contains("ghcr.io/adriftdev/sailr/ci-build-hello:61eaa8b"));
    }
}
