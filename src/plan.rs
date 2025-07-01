use anyhow::Result;
use kube::api::DynamicObject;
use scribe_rust::{log, Color};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::deployment::k8sm8::get_cluster_resources;
use crate::environment::Environment;
use crate::LOGGER;

#[derive(Debug, Clone)]
pub struct ResourceChange {
    pub action: ChangeAction,
    pub resource_type: String,
    pub name: String,
    pub namespace: Option<String>,
    pub details: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChangeAction {
    Create,
    Update,
    Delete,
    NoChange,
}

impl ChangeAction {
    pub fn symbol(&self) -> &str {
        match self {
            ChangeAction::Create => "+",
            ChangeAction::Update => "~",
            ChangeAction::Delete => "-",
            ChangeAction::NoChange => "=",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            ChangeAction::Create => Color::Green,
            ChangeAction::Update => Color::Yellow,
            ChangeAction::Delete => Color::Red,
            ChangeAction::NoChange => Color::Blue,
        }
    }
}

pub struct DeploymentPlan {
    pub changes: Vec<ResourceChange>,
    pub summary: PlanSummary,
}

#[derive(Debug)]
pub struct PlanSummary {
    pub to_create: usize,
    pub to_update: usize,
    pub to_delete: usize,
    pub no_change: usize,
}

impl DeploymentPlan {
    pub fn new() -> Self {
        Self {
            changes: Vec::new(),
            summary: PlanSummary {
                to_create: 0,
                to_update: 0,
                to_delete: 0,
                no_change: 0,
            },
        }
    }

    pub fn add_change(&mut self, change: ResourceChange) {
        match change.action {
            ChangeAction::Create => self.summary.to_create += 1,
            ChangeAction::Update => self.summary.to_update += 1,
            ChangeAction::Delete => self.summary.to_delete += 1,
            ChangeAction::NoChange => self.summary.no_change += 1,
        }
        self.changes.push(change);
    }

    pub fn display(&self) {
        LOGGER.info("📋 Deployment Plan:");
        println!();

        // Display summary
        LOGGER.info(&format!(
            "Plan: {} to create, {} to update, {} to delete",
            self.summary.to_create, self.summary.to_update, self.summary.to_delete
        ));
        println!();

        // Group changes by action
        let mut creates = Vec::new();
        let mut updates = Vec::new();
        let mut deletes = Vec::new();

        for change in &self.changes {
            match change.action {
                ChangeAction::Create => creates.push(change),
                ChangeAction::Update => updates.push(change),
                ChangeAction::Delete => deletes.push(change),
                ChangeAction::NoChange => {} // Skip no-change items in display
            }
        }

        // Display creates
        for change in creates {
            let namespace_str = change
                .namespace
                .as_ref()
                .map(|ns| format!(" (namespace: {})", ns))
                .unwrap_or_default();

            log(
                change.action.color(),
                change.action.symbol(),
                &format!("{}/{}{}", change.resource_type, change.name, namespace_str),
            );

            for detail in &change.details {
                println!("{}", &format!("    {}", detail));
            }
        }

        // Display updates
        for change in updates {
            let namespace_str = change
                .namespace
                .as_ref()
                .map(|ns| format!(" (namespace: {})", ns))
                .unwrap_or_default();

            log(
                change.action.color(),
                change.action.symbol(),
                &format!("{}/{}{}", change.resource_type, change.name, namespace_str),
            );

            for detail in &change.details {
                println!("{}", &format!("    {}", detail));
            }
        }

        // Display deletes
        for change in deletes {
            let namespace_str = change
                .namespace
                .as_ref()
                .map(|ns| format!(" (namespace: {})", ns))
                .unwrap_or_default();

            log(
                change.action.color(),
                change.action.symbol(),
                &format!("{}/{}{}", change.resource_type, change.name, namespace_str),
            );
        }

        println!();
        if self.summary.to_create > 0 || self.summary.to_update > 0 || self.summary.to_delete > 0 {
            LOGGER.info("Run without --plan to apply these changes.");
        } else {
            LOGGER.info("No changes detected. Infrastructure is up to date.");
        }
    }
}

pub async fn generate_deployment_plan(env_name: &str, context: &str) -> Result<DeploymentPlan> {
    let mut plan = DeploymentPlan::new();

    LOGGER.info(&format!(
        "🔍 Analyzing deployment plan for environment '{}'...",
        env_name
    ));

    // Load environment configuration
    let env = Environment::load_from_file(&env_name.to_string())
        .map_err(|e| anyhow::anyhow!("Failed to load environment: {}", e))?;

    // Get current cluster state from actual Kubernetes cluster
    let current_resources = get_current_cluster_resources(context).await?;

    // Generate manifests and compare with cluster state
    for service in &env.service_whitelist {
        let service_path = format!("k8s/generated/{}/{}", env_name, service.get_path());

        if !Path::new(&service_path).exists() {
            LOGGER.warn(&format!(
                "Service template directory not found: {}",
                service_path
            ));
            continue;
        }

        LOGGER.debug(&format!(
            "Analyzing service '{}' at path: {}",
            service.name, service_path
        ));

        // Analyze manifests in the service directory
        analyze_service_manifests(&mut plan, &service_path, &service.name, &current_resources)?;
    }

    Ok(plan)
}

async fn get_current_cluster_resources(context: &str) -> Result<HashMap<String, Value>> {
    let mut resources = HashMap::new();

    LOGGER.info(&format!(
        "📡 Querying cluster state (context: {})...",
        context
    ));

    // Get all deployments
    if let Ok(deployments) = get_cluster_resources(
        context,
        &DynamicObject::new(
            "deployment",
            &kube::api::ApiResource {
                group: "".to_string(),
                version: "v1".to_string(),
                kind: "Deployment".to_string(),
                api_version: "apps/v1".to_string(),
                plural: "deployments".to_string(),
            },
        ),
    )
    .await
    {
        for deployment in deployments {
            if let Some(metadata) = deployment.get("metadata") {
                if let Some(name) = metadata.get("name").and_then(|n| n.as_str()) {
                    let key = format!("deployment/{}", name);
                    resources.insert(key, deployment);
                }
            }
        }
    }

    // Get all services
    if let Ok(services) = get_cluster_resources(
        context,
        &DynamicObject::new(
            "service",
            &kube::api::ApiResource {
                group: "".to_string(),
                version: "v1".to_string(),
                kind: "Service".to_string(),
                api_version: "v1".to_string(),
                plural: "services".to_string(),
            },
        ),
    )
    .await
    {
        for service in services {
            if let Some(metadata) = service.get("metadata") {
                if let Some(name) = metadata.get("name").and_then(|n| n.as_str()) {
                    let key = format!("service/{}", name);
                    resources.insert(key, service);
                }
            }
        }
    }

    // Get all configmaps
    if let Ok(configmaps) = get_cluster_resources(
        context,
        &DynamicObject::new(
            "configmap",
            &kube::api::ApiResource {
                group: "".to_string(),
                version: "v1".to_string(),
                kind: "ConfigMap".to_string(),
                api_version: "v1".to_string(),
                plural: "configmaps".to_string(),
            },
        ),
    )
    .await
    {
        for configmap in configmaps {
            if let Some(metadata) = configmap.get("metadata") {
                if let Some(name) = metadata.get("name").and_then(|n| n.as_str()) {
                    let key = format!("configmap/{}", name);
                    resources.insert(key, configmap);
                }
            }
        }
    }

    // Get all secrets
    if let Ok(secrets) = get_cluster_resources(
        context,
        &DynamicObject::new(
            "secret",
            &kube::api::ApiResource {
                group: "".to_string(),
                version: "v1".to_string(),
                kind: "Secret".to_string(),
                api_version: "v1".to_string(),
                plural: "secrets".to_string(),
            },
        ),
    )
    .await
    {
        for secret in secrets {
            if let Some(metadata) = secret.get("metadata") {
                if let Some(name) = metadata.get("name").and_then(|n| n.as_str()) {
                    let key = format!("secret/{}", name);
                    resources.insert(key, secret);
                }
            }
        }
    }

    // Get all ingresses
    if let Ok(ingresses) = get_cluster_resources(
        context,
        &DynamicObject::new(
            "ingress",
            &kube::api::ApiResource {
                group: "networking.k8s.io".to_string(),
                version: "v1".to_string(),
                kind: "Ingress".to_string(),
                api_version: "networking.k8s.io/v1".to_string(),
                plural: "ingresses".to_string(),
            },
        ),
    )
    .await
    {
        for ingress in ingresses {
            if let Some(metadata) = ingress.get("metadata") {
                if let Some(name) = metadata.get("name").and_then(|n| n.as_str()) {
                    let key = format!("ingress/{}", name);
                    resources.insert(key, ingress);
                }
            }
        }
    }

    // Get all HPAs
    if let Ok(hpas) = get_cluster_resources(
        context,
        &DynamicObject::new(
            "horizontalpodautoscaler",
            &kube::api::ApiResource {
                group: "autoscaling".to_string(),
                version: "v2".to_string(),
                kind: "HorizontalPodAutoscaler".to_string(),
                api_version: "autoscaling/v2".to_string(),
                plural: "horizontalpodautoscalers".to_string(),
            },
        ),
    )
    .await
    {
        for hpa in hpas {
            if let Some(metadata) = hpa.get("metadata") {
                if let Some(name) = metadata.get("name").and_then(|n| n.as_str()) {
                    let key = format!("horizontalpodautoscaler/{}", name);
                    resources.insert(key, hpa);
                }
            }
        }
    }

    LOGGER.info(&format!(
        "Found {} existing resources in cluster",
        resources.len()
    ));
    Ok(resources)
}

fn analyze_service_manifests(
    plan: &mut DeploymentPlan,
    service_path: &str,
    service_name: &str,
    current_resources: &HashMap<String, Value>,
) -> Result<()> {
    let manifest_files = [
        "deployment.yaml",
        "service.yaml",
        "configmap.yaml",
        "secret.yaml",
        "ingress.yaml",
        "hpa.yaml",
    ];

    for manifest_file in &manifest_files {
        let manifest_path = Path::new(service_path).join(manifest_file);

        if manifest_path.exists() {
            analyze_manifest_file(plan, &manifest_path, service_name, current_resources)?;
        }
    }

    Ok(())
}

fn analyze_manifest_file(
    plan: &mut DeploymentPlan,
    manifest_path: &Path,
    _service_name: &str,
    current_resources: &HashMap<String, Value>,
) -> Result<()> {
    let content = fs::read_to_string(manifest_path)?;

    // Parse YAML content
    let docs: Vec<Value> = serde_yaml::Deserializer::from_str(&content)
        .map(|doc| Value::deserialize(doc))
        .collect::<Result<Vec<_>, _>>()?;

    for doc in docs {
        if let Some(obj) = doc.as_object() {
            let kind = obj
                .get("kind")
                .and_then(|k| k.as_str())
                .unwrap_or("Unknown");

            let metadata = obj.get("metadata").and_then(|m| m.as_object());

            let name = metadata
                .and_then(|m| m.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("unknown");

            let namespace = metadata
                .and_then(|m| m.get("namespace"))
                .and_then(|n| n.as_str())
                .map(|s| s.to_string());

            // Determine if this is a new resource or an update
            let resource_key = format!("{}/{}", kind.to_lowercase(), name);

            let mut details: Vec<String> = Vec::new();

            let action = if current_resources.contains_key(&resource_key) {
                // Resource exists, check if it needs updates
                if needs_update(&doc, current_resources.get(&resource_key).unwrap()) {
                    details = generate_resource_details(
                        &doc,
                        current_resources.get(&resource_key),
                        &ChangeAction::Update,
                    );
                    ChangeAction::Update
                } else {
                    ChangeAction::NoChange
                }
            } else {
                details = generate_resource_details(&doc, None, &ChangeAction::Create);
                ChangeAction::Create
            };

            let change = ResourceChange {
                action,
                resource_type: kind.to_string(),
                name: name.to_string(),
                namespace,
                details,
            };

            plan.add_change(change);
        }
    }

    Ok(())
}

fn needs_update(desired: &Value, current: &Value) -> bool {
    // Compare key fields that matter for deployment decisions
    // Ignore fields like resourceVersion, status, creationTimestamp, etc.

    // Compare spec sections
    if let (Some(desired_spec), Some(current_spec)) = (desired.get("spec"), current.get("spec")) {
        // will do a nested comparison for targeted fields
        if desired_spec != current_spec {
            if desired_spec
                .as_object()
                .and_then(|d| d.get("replicas"))
                .and_then(|d| d.as_u64())
                != current_spec
                    .as_object()
                    .and_then(|c| c.get("replicas"))
                    .and_then(|c| c.as_u64())
            {
                return true;
            }

            if desired_spec
                .as_object()
                .and_then(|d| d.get("template"))
                .and_then(|d| d.get("spec"))
                .and_then(|d| d.get("containers"))
                .and_then(|c| c.as_array())
                .and_then(|containers| {
                    containers
                        .first()
                        .and_then(|c| c.as_object())
                        .and_then(|c| c.get("image"))
                })
                != current_spec
                    .as_object()
                    .and_then(|c| c.get("template"))
                    .and_then(|t| t.get("spec"))
                    .and_then(|s| s.get("containers"))
                    .and_then(|c| c.as_array())
                    .and_then(|containers| {
                        containers
                            .first()
                            .and_then(|c| c.as_object())
                            .and_then(|c| c.get("image"))
                    })
            {
                return true;
            }

            // Compare env vars
            if let (Some(desired_containers), Some(current_containers)) = (
                desired_spec
                    .as_object()
                    .and_then(|d| d.get("template"))
                    .and_then(|t| t.get("spec"))
                    .and_then(|s| s.get("containers"))
                    .and_then(|c| c.as_array()),
                current_spec
                    .as_object()
                    .and_then(|c| c.get("template"))
                    .and_then(|t| t.get("spec"))
                    .and_then(|s| s.get("containers"))
                    .and_then(|c| c.as_array()),
            ) {
                for (desired_container, current_container) in
                    desired_containers.iter().zip(current_containers.iter())
                {
                    if let (Some(desired_env), Some(current_env)) =
                        (desired_container.get("env"), current_container.get("env"))
                    {
                        if desired_env != current_env {
                            return true;
                        }
                    }
                }
            }

            // Compare volumes
            if let (Some(desired_volumes), Some(current_volumes)) = (
                desired_spec
                    .as_object()
                    .and_then(|d| d.get("template"))
                    .and_then(|t| t.get("spec"))
                    .and_then(|s| s.get("volumes"))
                    .and_then(|v| v.as_array()),
                current_spec
                    .as_object()
                    .and_then(|c| c.get("template"))
                    .and_then(|t| t.get("spec"))
                    .and_then(|s| s.get("volumes"))
                    .and_then(|v| v.as_array()),
            ) {
                if desired_volumes != current_volumes {
                    return true;
                }
            }

            // Compare resource limits/requests
            if let (Some(desired_resources), Some(current_resources)) = (
                desired_spec
                    .as_object()
                    .and_then(|d| d.get("template"))
                    .and_then(|t| t.get("spec"))
                    .and_then(|s| s.get("containers"))
                    .and_then(|c| c.as_array())
                    .and_then(|containers| containers.first())
                    .and_then(|c| c.as_object())
                    .and_then(|m| m.get("resources")),
                current_spec
                    .as_object()
                    .and_then(|c| c.get("template"))
                    .and_then(|t| t.get("spec"))
                    .and_then(|s| s.get("containers"))
                    .and_then(|c| c.as_array())
                    .and_then(|containers| containers.first())
                    .and_then(|c| c.as_object())
                    .and_then(|m| m.get("resources")),
            ) {
                if desired_resources != current_resources {
                    return true;
                }
            }
        }
    }

    // Compare data section for ConfigMaps and Secrets
    if let (Some(desired_data), Some(current_data)) = (desired.get("data"), current.get("data")) {
        if desired_data != current_data {
            return true;
        }
    }

    false
}

fn generate_resource_details(
    desired_resource: &Value,
    current_resource: Option<&Value>,
    action: &ChangeAction,
) -> Vec<String> {
    let mut details = Vec::new();

    match action {
        ChangeAction::Create => {
            if let Some(spec) = desired_resource.get("spec").and_then(|s| s.as_object()) {
                if let Some(replicas) = spec.get("replicas").and_then(|r| r.as_u64()) {
                    details.push(format!("replicas: {}", replicas));
                }

                if let Some(template) = spec
                    .get("template")
                    .and_then(|t| t.get("spec"))
                    .and_then(|s| s.get("containers"))
                    .and_then(|c| c.as_array())
                    .and_then(|containers| containers.first())
                    .and_then(|container| container.as_object())
                {
                    if let Some(image) = template.get("image").and_then(|i| i.as_str()) {
                        details.push(format!("image: {}", image));
                    }
                }
            }
        }
        ChangeAction::Update => {
            if let Some(current) = current_resource {
                if let Some(spec) = desired_resource.get("spec").and_then(|s| s.as_object()) {
                    if let Some(current_spec) = current.get("spec").and_then(|s| s.as_object()) {
                        if let Some(replicas) = spec.get("replicas").and_then(|r| r.as_u64()) {
                            if let Some(current_replicas) =
                                current_spec.get("replicas").and_then(|r| r.as_u64())
                            {
                                if replicas != current_replicas {
                                    details.push(format!(
                                        "replicas changed from {} to {}",
                                        current_replicas, replicas
                                    ));
                                }
                            } else {
                                details.push(format!("replicas set to {}", replicas));
                            }
                        }

                        if let Some(template) = spec
                            .get("template")
                            .and_then(|t| t.get("spec"))
                            .and_then(|s| s.get("containers"))
                            .and_then(|c| c.as_array())
                            .and_then(|containers| containers.first())
                            .and_then(|container| container.as_object())
                        {
                            if let Some(image) = template.get("image").and_then(|i| i.as_str()) {
                                if let Some(current_image) = current_spec
                                    .get("template")
                                    .and_then(|t| t.get("spec"))
                                    .and_then(|s| s.get("containers"))
                                    .and_then(|c| c.as_array())
                                    .and_then(|containers| containers.first())
                                    .and_then(|container| container.as_object())
                                    .and_then(|m| m.get("image"))
                                    .and_then(|i| i.as_str())
                                {
                                    if image != current_image {
                                        details.push(format!(
                                            "image changed from {} to {}",
                                            current_image, image
                                        ));
                                    }
                                } else {
                                    details.push(format!("image set to {}", image));
                                }
                            }

                            // compare resources like volumes, env vars, etc.
                            if let Some(env_vars) = template.get("env").and_then(|e| e.as_array()) {
                                let current_env_vars = current_spec
                                    .get("template")
                                    .and_then(|t| t.get("spec"))
                                    .and_then(|s| s.get("containers"))
                                    .and_then(|c| c.as_array())
                                    .and_then(|containers| containers.first())
                                    .and_then(|container| container.as_object())
                                    .and_then(|m| m.get("env"))
                                    .and_then(|e| e.as_array());

                                if let Some(current_env_vars) = current_env_vars {
                                    for env_var in env_vars {
                                        if let Some(name) = env_var
                                            .as_object()
                                            .and_then(|m| m.get("name"))
                                            .and_then(|n| n.as_str())
                                        {
                                            if !current_env_vars.iter().any(|c| {
                                                c.as_object()
                                                    .and_then(|m| m.get("name"))
                                                    .and_then(|n| n.as_str())
                                                    == Some(name)
                                            }) {
                                                details.push(format!("new env var: {}", name));
                                            }
                                        }
                                    }
                                }
                            }

                            // Compare volumes
                            if let Some(volumes) =
                                template.get("volumes").and_then(|v| v.as_array())
                            {
                                let current_volumes = current_spec
                                    .get("template")
                                    .and_then(|t| t.get("spec"))
                                    .and_then(|s| s.get("volumes"))
                                    .and_then(|v| v.as_array());

                                if let Some(current_volumes) = current_volumes {
                                    for volume in volumes {
                                        if let Some(name) = volume
                                            .as_object()
                                            .and_then(|m| m.get("name"))
                                            .and_then(|n| n.as_str())
                                        {
                                            if !current_volumes.iter().any(|c| {
                                                c.as_object()
                                                    .and_then(|m| m.get("name"))
                                                    .and_then(|n| n.as_str())
                                                    == Some(name)
                                            }) {
                                                details.push(format!("new volume: {}", name));
                                            }
                                        }
                                    }
                                }
                            }

                            // compare limits and requests
                            if let Some(resources) =
                                template.get("resources").and_then(|r| r.as_object())
                            {
                                if let Some(current_resources) = current_spec
                                    .get("template")
                                    .and_then(|t| t.get("spec"))
                                    .and_then(|s| s.get("containers"))
                                    .and_then(|c| c.as_array())
                                    .and_then(|containers| containers.first())
                                    .and_then(|container| container.as_object())
                                    .and_then(|m| m.get("resources"))
                                    .and_then(|r| r.as_object())
                                {
                                    if resources != current_resources {
                                        details
                                            .push("resource limits/requests changed".to_string());
                                    }
                                } else {
                                    details.push("resource limits/requests set".to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }

    details
}

pub fn validate_plan_safety(plan: &DeploymentPlan) -> Result<()> {
    // Check for potentially dangerous operations
    let mut warnings = Vec::new();

    for change in &plan.changes {
        match change.action {
            ChangeAction::Delete => {
                warnings.push(format!(
                    "⚠️  Deleting {}/{} - this action cannot be undone",
                    change.resource_type, change.name
                ));
            }
            ChangeAction::Update => {
                if change.resource_type == "Deployment" {
                    warnings.push(format!(
                        "⚠️  Updating deployment/{} - may cause pod restarts",
                        change.name
                    ));
                }
            }
            _ => {}
        }
    }

    if !warnings.is_empty() {
        LOGGER.info("⚠️  Safety warnings:");
        for warning in warnings {
            LOGGER.warn(&warning);
        }
        LOGGER.info("");
    }

    Ok(())
}
