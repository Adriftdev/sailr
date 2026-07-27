use crate::builder::SailrBuildPlan;
use runkernel::{Pipeline, Task};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

const TRANSLATOR_CACHE_VERSION: &str = "sailr-translator-v1";

#[derive(Debug, Clone)]
pub struct TranslatedTask {
    pub id: String,
    pub label: String,
    pub dependencies: Vec<String>,
    pub service: Option<String>,
    pub phase: String,
    pub command: Option<String>,
    pub cwd: PathBuf,
    pub inputs: Vec<PathBuf>,
    pub cache_key: String,
    pub finally_commands: Vec<String>,
}

pub fn translate_build_plan(plan: &SailrBuildPlan, execute_push: bool) -> Vec<TranslatedTask> {
    let dirty = plan
        .services
        .iter()
        .filter(|service| service.dirty)
        .map(|service| service.service.name.clone())
        .collect::<BTreeSet<_>>();
    let mut tasks = Vec::new();
    let mut terminal_by_service = BTreeMap::<String, String>::new();

    let before_all_terminal = if plan.before_all.is_empty() {
        None
    } else {
        let id = crate::workflow::task_id::BUILD_BEFORE_ALL.to_string();
        let command = plan.before_all.join(" && ");
        tasks.push(TranslatedTask {
            id: id.clone(),
            label: "Before All Build Hooks".to_string(),
            dependencies: Vec::new(),
            service: None,
            phase: "before_all".to_string(),
            command: Some(command.clone()),
            cwd: PathBuf::from("."),
            inputs: Vec::new(),
            cache_key: format!("{TRANSLATOR_CACHE_VERSION}:before-all:{command}"),
            finally_commands: Vec::new(),
        });
        Some(id)
    };

    for service in plan.services.iter().filter(|service| service.dirty) {
        let name = service.service.name.clone();
        let mut dependencies = service
            .dependencies
            .iter()
            .filter(|dependency| dirty.contains(*dependency))
            .filter_map(|dependency| terminal_by_service.get(dependency).cloned())
            .collect::<Vec<_>>();
        if let Some(before_all) = &before_all_terminal {
            dependencies.push(before_all.clone());
        }
        dependencies.sort();
        dependencies.dedup();
        let service_root_dependencies = dependencies.clone();

        let mut index_by_phase = HashMap::<String, usize>::new();
        let finally_commands = service.phases.finally.clone();
        append_sequential_tasks(
            &mut tasks,
            &mut index_by_phase,
            service,
            "before_synchronous",
            &service.phases.before_synchronously,
            &mut dependencies,
        );
        append_sequential_tasks(
            &mut tasks,
            &mut index_by_phase,
            service,
            "before",
            &service.phases.before,
            &mut dependencies,
        );

        if !service.phases.run_parallel.is_empty() {
            let fan_in = dependencies.clone();
            let mut parallel = Vec::new();
            for (index, command) in service.phases.run_parallel.iter().enumerate() {
                let id = crate::workflow::task_id::service_phase(&name, "run_parallel", index);
                tasks.push(TranslatedTask {
                    id: id.clone(),
                    label: format!("{} run parallel {}", name, index + 1),
                    dependencies: fan_in.clone(),
                    service: Some(name.clone()),
                    phase: "run_parallel".to_string(),
                    command: Some(command.clone()),
                    cwd: service.cwd.clone(),
                    inputs: service.matched_input_files.clone(),
                    cache_key: format!(
                        "{TRANSLATOR_CACHE_VERSION}:{}:run_parallel:{index}:{}:{}",
                        service.fingerprint.full_hash, name, command
                    ),
                    finally_commands: finally_commands.clone(),
                });
                parallel.push(id);
            }
            dependencies = parallel;
        }

        append_sequential_tasks(
            &mut tasks,
            &mut index_by_phase,
            service,
            "run_synchronous",
            &service.phases.run_synchronously,
            &mut dependencies,
        );
        append_sequential_tasks(
            &mut tasks,
            &mut index_by_phase,
            service,
            "build",
            &service.phases.build,
            &mut dependencies,
        );
        if execute_push {
            append_sequential_tasks(
                &mut tasks,
                &mut index_by_phase,
                service,
                "push",
                &service.phases.push,
                &mut dependencies,
            );
        }
        append_sequential_tasks(
            &mut tasks,
            &mut index_by_phase,
            service,
            "after",
            &service.phases.after,
            &mut dependencies,
        );

        if !finally_commands.is_empty() {
            let id = crate::workflow::task_id::service_phase(&name, "finally", 0);
            tasks.push(TranslatedTask {
                id: id.clone(),
                label: format!("{name} finally"),
                dependencies: dependencies.clone(),
                service: Some(name.clone()),
                phase: "finally".to_string(),
                command: None,
                cwd: service.cwd.clone(),
                inputs: service.matched_input_files.clone(),
                cache_key: format!(
                    "{TRANSLATOR_CACHE_VERSION}:{}:finally:{}",
                    service.fingerprint.full_hash,
                    finally_commands.join("\n")
                ),
                finally_commands: finally_commands.clone(),
            });
            dependencies = vec![id];
        }

        dependencies.extend(service_root_dependencies);
        dependencies.sort();
        dependencies.dedup();
        let aggregate_id = crate::workflow::task_id::service_build(&name);
        tasks.push(TranslatedTask {
            id: aggregate_id.clone(),
            label: format!("Build {name}"),
            dependencies,
            service: Some(name.clone()),
            phase: "service_complete".to_string(),
            command: None,
            cwd: service.cwd.clone(),
            inputs: service.matched_input_files.clone(),
            cache_key: format!(
                "{TRANSLATOR_CACHE_VERSION}:{}:service-complete:{name}",
                service.fingerprint.full_hash
            ),
            finally_commands: Vec::new(),
        });
        terminal_by_service.insert(name, aggregate_id);
    }

    if !dirty.is_empty() && !plan.after_all.is_empty() {
        let mut dependencies = terminal_by_service.values().cloned().collect::<Vec<_>>();
        dependencies.sort();
        let command = plan.after_all.join(" && ");
        tasks.push(TranslatedTask {
            id: crate::workflow::task_id::BUILD_AFTER_ALL.to_string(),
            label: "After All Build Hooks".to_string(),
            dependencies,
            service: None,
            phase: "after_all".to_string(),
            command: Some(command.clone()),
            cwd: PathBuf::from("."),
            inputs: Vec::new(),
            cache_key: format!("{TRANSLATOR_CACHE_VERSION}:after-all:{command}"),
            finally_commands: Vec::new(),
        });
    }

    tasks
}

fn append_sequential_tasks(
    tasks: &mut Vec<TranslatedTask>,
    index_by_phase: &mut HashMap<String, usize>,
    service: &crate::builder::ServiceBuildPlan,
    phase: &str,
    commands: &[String],
    dependencies: &mut Vec<String>,
) {
    let name = &service.service.name;
    for command in commands {
        let index = *index_by_phase.entry(phase.to_string()).or_default();
        *index_by_phase.get_mut(phase).expect("phase index exists") += 1;
        let id = crate::workflow::task_id::service_phase(name, phase, index);
        tasks.push(TranslatedTask {
            id: id.clone(),
            label: format!("{} {} {}", name, phase.replace('_', " "), index + 1),
            dependencies: dependencies.clone(),
            service: Some(name.to_string()),
            phase: phase.to_string(),
            command: Some(command.clone()),
            cwd: service.cwd.clone(),
            inputs: service.matched_input_files.clone(),
            cache_key: format!(
                "{TRANSLATOR_CACHE_VERSION}:{}:{phase}:{index}:{}:{}",
                service.fingerprint.full_hash, name, command
            ),
            finally_commands: service.phases.finally.clone(),
        });
        *dependencies = vec![id];
    }
}

pub fn terminal_task_for_service(
    plan: &SailrBuildPlan,
    service_name: &str,
    execute_push: bool,
) -> Option<String> {
    translate_build_plan(plan, execute_push)
        .into_iter()
        .filter(|task| task.service.as_deref() == Some(service_name))
        .map(|task| task.id)
        .next_back()
}

pub fn add_translated_tasks(
    pipeline: &mut Pipeline,
    plan: &SailrBuildPlan,
    execute_push: bool,
    planned_tasks: Option<&BTreeMap<String, Vec<String>>>,
    max_parallelism: Option<usize>,
) {
    let semaphore = max_parallelism
        .filter(|limit| *limit > 0)
        .map(|limit| Arc::new(tokio::sync::Semaphore::new(limit)));
    let mut finalizer_guards = HashMap::<String, Arc<AtomicBool>>::new();

    for spec in translate_build_plan(plan, execute_push) {
        if planned_tasks.is_some_and(|planned| !planned.contains_key(&spec.id)) {
            continue;
        }
        let dependencies = planned_tasks
            .and_then(|planned| planned.get(&spec.id))
            .cloned()
            .unwrap_or_else(|| spec.dependencies.clone());
        let dependency_refs = dependencies.iter().map(String::as_str).collect::<Vec<_>>();
        let input_strings = spec
            .inputs
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        let input_refs = input_strings.iter().map(String::as_str).collect::<Vec<_>>();
        let guard = spec
            .service
            .as_ref()
            .map(|service| {
                finalizer_guards
                    .entry(service.clone())
                    .or_insert_with(|| Arc::new(AtomicBool::new(false)))
                    .clone()
            })
            .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
        let command = spec.command.clone();
        let cwd = spec.cwd.to_string_lossy().to_string();
        let name = spec.service.clone().unwrap_or_else(|| spec.id.clone());
        let finally_commands = spec.finally_commands.clone();
        let is_finally = spec.phase == "finally";
        let semaphore = semaphore.clone();

        let task = Task::new(spec.id)
            .description(spec.label)
            .depends_on(&dependency_refs)
            .inputs(&input_refs)
            .cache_key(spec.cache_key)
            .exec_fn(move |_ctx| {
                let command = command.clone();
                let cwd = cwd.clone();
                let name = name.clone();
                let finally_commands = finally_commands.clone();
                let guard = guard.clone();
                let semaphore = semaphore.clone();
                async move {
                    let _permit = match semaphore {
                        Some(semaphore) => Some(
                            semaphore
                                .acquire_owned()
                                .await
                                .map_err(|_| anyhow::anyhow!("Build concurrency limiter closed"))?,
                        ),
                        None => None,
                    };

                    if is_finally {
                        run_finally_once(&cwd, &name, &finally_commands, &guard).await;
                        return Ok(());
                    }

                    if let Some(command) = command {
                        if let Err(error) = crate::builder::exec_cmd(&cwd, &command, &name).await {
                            run_finally_once(&cwd, &name, &finally_commands, &guard).await;
                            anyhow::bail!("{error}");
                        }
                    }
                    Ok(())
                }
            });
        pipeline.add(task);
    }
}

async fn run_finally_once(cwd: &str, name: &str, commands: &[String], guard: &AtomicBool) {
    if commands.is_empty() || guard.swap(true, Ordering::SeqCst) {
        return;
    }
    for command in commands {
        if let Err(error) = crate::builder::exec_cmd(cwd, command, name).await {
            crate::LOGGER.warn(&format!(
                "finalizer command failed for service {name}: {error}"
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::{ServiceBuildPlan, ServiceFingerprint, ServicePhases};
    use crate::environment::{Service, ServiceBuildConfig};

    fn service_plan(name: &str, dependency: Option<&str>) -> ServiceBuildPlan {
        ServiceBuildPlan {
            service: Service::new(name, None, "latest"),
            build: ServiceBuildConfig {
                path: ".".to_string(),
                include: None,
                ignore_cache: None,
                relies_on: dependency.map(|value| vec![value.to_string()]),
                before_synchronous: None,
                before: None,
                run_parallel: None,
                run_synchronous: None,
                after: None,
                finally: None,
                dockerfile: None,
                build_command: None,
                push_command: None,
            },
            cwd: PathBuf::from("."),
            dependencies: dependency
                .map(|value| vec![value.to_string()])
                .unwrap_or_default(),
            dependency_paths: Vec::new(),
            input_patterns: Vec::new(),
            matched_input_files: Vec::new(),
            dirty: true,
            dirty_reasons: Vec::new(),
            fingerprint: ServiceFingerprint {
                source_hash: "source".to_string(),
                dependency_hash: "dependency".to_string(),
                command_hash: "command".to_string(),
                config_hash: "config".to_string(),
                full_hash: format!("{name}-hash"),
            },
            phases: ServicePhases {
                run_parallel: vec!["one".to_string(), "two".to_string()],
                run_synchronously: vec!["three".to_string(), "four".to_string()],
                build: vec!["build".to_string()],
                finally: vec!["cleanup".to_string()],
                ..Default::default()
            },
        }
    }

    #[test]
    fn translates_parallel_fan_out_and_synchronous_chain() {
        let plan = SailrBuildPlan {
            services: vec![service_plan("api", None)],
            before_all: Vec::new(),
            after_all: Vec::new(),
            force: false,
            max_parallelism: None,
            cache_dir: PathBuf::from("."),
        };
        let tasks = translate_build_plan(&plan, false);
        let first_sync = tasks
            .iter()
            .find(|task| task.id == "service:api:run_synchronous:0")
            .unwrap();
        assert_eq!(
            first_sync.dependencies,
            vec!["service:api:run_parallel:0", "service:api:run_parallel:1"]
        );
        let second_sync = tasks
            .iter()
            .find(|task| task.id == "service:api:run_synchronous:1")
            .unwrap();
        assert_eq!(
            second_sync.dependencies,
            vec!["service:api:run_synchronous:0"]
        );
    }

    #[test]
    fn service_dependency_points_to_terminal_task() {
        let plan = SailrBuildPlan {
            services: vec![
                service_plan("base", None),
                service_plan("api", Some("base")),
            ],
            before_all: Vec::new(),
            after_all: Vec::new(),
            force: false,
            max_parallelism: None,
            cache_dir: PathBuf::from("."),
        };
        let tasks = translate_build_plan(&plan, false);
        let api_root = tasks
            .iter()
            .find(|task| task.id == "service:api:run_parallel:0")
            .unwrap();
        assert_eq!(api_root.dependencies, vec!["service:base:build"]);
    }
}
