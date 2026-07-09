import re

# 1. planner.rs
planner_path = "src/workflow/planner.rs"
with open(planner_path, "r") as f:
    planner = f.read()

planner = planner.replace("let mut push_plan_opt = None;", "let mut image_push_plan_opt = None;")

planner_old_push_plan_block = """                let mut items = Vec::new();
                if let Some(ref bp) = build_plan_opt {
                    for s in &bp.services {
                        if s.dirty {
                            let registry = if self.env.registry.is_empty() {
                                "docker.io".to_string()
                            } else {
                                self.env.registry.clone()
                            };
                            let tag = crate::workflow::image::derive_image_tag(Some(&s.fingerprint.full_hash));
                            items.push(crate::workflow::image::ImagePushPlanItem {
                                service: s.service.name.clone(),
                                registry,
                                repository: format!("Adriftdev/sailr/{}", s.service.name),
                                tag,
                                source_sha: Some(s.fingerprint.full_hash.clone()),
                            });
                        }
                    }
                }
                push_plan_opt = Some(crate::workflow::image::ImagePushPlan::new(items));"""

planner_new_push_plan_block = """                let mut items = Vec::new();
                if let Some(ref bp) = build_plan_opt {
                    for s in &bp.services {
                        if s.dirty {
                            let registry = if self.env.registry.is_empty() {
                                "docker.io".to_string()
                            } else {
                                self.env.registry.clone()
                            };
                            let repository = format!("Adriftdev/sailr/{}", s.service.name);
                            let tag = crate::workflow::image::derive_image_tag(Some(&s.fingerprint.full_hash));
                            let image_ref = format!("{}/{}:{}", registry, repository, tag);
                            items.push(crate::workflow::image::ImagePushPlanItem {
                                service: s.service.name.clone(),
                                registry,
                                repository,
                                tag,
                                image_ref,
                                source_sha: Some(s.fingerprint.full_hash.clone()),
                                action: crate::workflow::image::ImagePushPlanAction::WouldPush,
                            });
                        }
                    }
                    image_push_plan_opt = Some(crate::workflow::image::ImagePushPlanReport {
                        environment: self.profile.environment.clone(),
                        mutates_registry: false,
                        items,
                    });
                } else {
                    return Err("push=plan requires build=plan or build=run".to_string());
                }"""

planner = planner.replace(planner_old_push_plan_block, planner_new_push_plan_block)
planner = planner.replace("push_plan: push_plan_opt,", "image_push_plan: image_push_plan_opt,")

planner_old_task_plan = """            crate::workflow::profile::WorkflowStepMode::Plan => {
                let push_plan = plan.push_plan.clone().unwrap();
                let mut task = Task::new("workflow:push-plan");
                
                let deps_refs: Vec<&str> = last_tasks.iter().map(|s| s.as_str()).collect();
                if !deps_refs.is_empty() {
                    task = task.depends_on(&deps_refs);
                }

                task = task.exec_fn(move |_ctx| {
                    let push_plan = push_plan.clone();
                    async move {
                        if push_plan.items.is_empty() {
                            crate::LOGGER.info("No images to push.");
                            return Ok(());
                        }
                        
                        crate::LOGGER.info("Image Push Plan:");
                        for item in &push_plan.items {
                            crate::LOGGER.info(&format!(
                                "  - {} -> {}/{}:{}",
                                item.service, item.registry, item.repository, item.tag
                            ));
                        }
                        Ok(())
                    }
                });"""

planner_new_task_plan = """            crate::workflow::profile::WorkflowStepMode::Plan => {
                let push_plan = plan.image_push_plan.clone().unwrap();
                let mut task = Task::new("workflow:push-plan");
                
                let deps_refs: Vec<&str> = last_tasks.iter().map(|s| s.as_str()).collect();
                if !deps_refs.is_empty() {
                    task = task.depends_on(&deps_refs);
                }

                task = task.exec_fn(move |_ctx| {
                    let push_plan = push_plan.clone();
                    async move {
                        crate::LOGGER.info(&crate::workflow::render::render_image_push_plan_text(&push_plan));
                        Ok(())
                    }
                });"""

planner = planner.replace(planner_old_task_plan, planner_new_task_plan)

# 2. runner.rs
runner_path = "src/workflow/runner.rs"
with open(runner_path, "r") as f:
    runner = f.read()

runner_old_push_plan = """    let mut image_push_plan: Option<crate::workflow::image::ImagePushPlanReport> = None;

    if let Some(ref pp) = plan.push_plan {
        image_push_plan = Some(crate::workflow::image::ImagePushPlanReport {
            plan: pp.clone(),
        });
        
        for item in &pp.items {
            images.push(item.to_artifact_placeholder(&profile.environment));
        }
    }"""

runner_new_push_plan = """    let image_push_plan: Option<crate::workflow::image::ImagePushPlanReport> = plan.image_push_plan.clone();

    if let Some(ref pp) = image_push_plan {
        for item in &pp.items {
            images.push(crate::workflow::image::ImageArtifact::from_push_plan_item(&profile.environment, item));
        }
    }"""

runner = runner.replace(runner_old_push_plan, runner_new_push_plan)

# Write back
with open(planner_path, "w") as f:
    f.write(planner)
with open(runner_path, "w") as f:
    f.write(runner)

