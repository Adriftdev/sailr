use crate::orchestrator::RoomError;
use crate::tui::app::{Action, App, AppAction, AppState, PendingExternalAction};
use crate::{cli::InteractiveArgs, deployment::k8sm8, errors::CliError};
use crossterm::event::{self, Event, KeyCode};
use scribe_rust::log;
use std::fs;
use std::path::Path;
use std::time::Duration;

pub async fn main_menu(args: InteractiveArgs) -> Result<(), CliError> {
    let app = App::new().map_err(room_error)?;

    loop {
        let mut terminal = crate::tui::init()
            .map_err(|e| CliError::Other(format!("Failed to init TUI: {}", e)))?;
        let res = run_app(&mut terminal, &app, &args).await;
        crate::tui::restore()
            .map_err(|e| CliError::Other(format!("Failed to restore TUI: {}", e)))?;

        if let Err(e) = res {
            println!("Error: {}", e);
            break;
        }

        if app.should_quit() {
            println!("Exiting...");
            break;
        }

        if let Some(PendingExternalAction {
            action,
            items,
            input,
        }) = app.take_pending_external_action().map_err(room_error)?
        {
            println!("\nExecuting {}...", action.as_str());
            if let Err(e) = run_external_action(args.clone(), action.clone(), items, input).await {
                log(scribe_rust::Color::Red, "Error", &format!("Failed: {}", e));
            } else {
                log(
                    scribe_rust::Color::Green,
                    "Success",
                    &format!("Command '{}' executed successfully", action.as_str()),
                );
            }
            println!("\nPress Enter to return to menu...");
            let mut buf = String::new();
            let _ = std::io::stdin().read_line(&mut buf);
            app.dispatch(AppAction::ExternalActionCompleted)
                .map_err(room_error)?;
        }
    }

    Ok(())
}

async fn run_app(
    terminal: &mut crate::tui::Tui,
    app: &App,
    args: &InteractiveArgs,
) -> Result<(), CliError> {
    loop {
        terminal
            .draw(|f| crate::tui::ui::draw(f, app))
            .map_err(|e| CliError::Other(e.to_string()))?;

        if app.should_quit() || app.has_pending_external_action() {
            return Ok(());
        }

        if let Some(action) = app.fetch_action() {
            match fetch_items(args, &action).await {
                Ok(items) => {
                    if items.is_empty() {
                        app.dispatch(AppAction::ShowMessage {
                            title: "Not Found".to_string(),
                            content: format!(
                                "No items found for this action in namespace '{}'.",
                                args.namespace
                            ),
                            is_error: true,
                        })
                        .map_err(room_error)?;
                    } else {
                        app.dispatch(AppAction::LoadSelection {
                            action: action.clone(),
                            items,
                        })
                        .map_err(room_error)?;
                    }
                }
                Err(e) => {
                    app.dispatch(AppAction::ShowMessage {
                        title: "Error".to_string(),
                        content: format!("Failed to fetch: {}", e),
                        is_error: true,
                    })
                    .map_err(room_error)?;
                }
            }
            continue;
        }

        if app.processing_action().is_some() {
            app.dispatch(AppAction::QueuePendingExternalAction)
                .map_err(room_error)?;
            return Ok(());
        }

        if event::poll(Duration::from_millis(50)).map_err(|e| CliError::Other(e.to_string()))? {
            if let Event::Key(key) = event::read().map_err(|e| CliError::Other(e.to_string()))? {
                let snapshot = app.state();
                match &snapshot.state {
                    AppState::MainMenu => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            app.dispatch(AppAction::Quit).map_err(room_error)?
                        }
                        KeyCode::Up => app
                            .dispatch(AppAction::MovePrevious {
                                menu_len: app.menu_items().len(),
                            })
                            .map_err(room_error)?,
                        KeyCode::Down => app
                            .dispatch(AppAction::MoveNext {
                                menu_len: app.menu_items().len(),
                            })
                            .map_err(room_error)?,
                        KeyCode::Enter => {
                            if let Some(selected_action) = app.current_main_menu_action() {
                                app.dispatch(AppAction::ActivateMenu(selected_action))
                                    .map_err(room_error)?;
                            }
                        }
                        _ => {}
                    },
                    AppState::Selection { multi, .. } => match key.code {
                        KeyCode::Esc => app
                            .dispatch(AppAction::ReturnToMainMenu)
                            .map_err(room_error)?,
                        KeyCode::Up => app
                            .dispatch(AppAction::MovePrevious {
                                menu_len: app.menu_items().len(),
                            })
                            .map_err(room_error)?,
                        KeyCode::Down => app
                            .dispatch(AppAction::MoveNext {
                                menu_len: app.menu_items().len(),
                            })
                            .map_err(room_error)?,
                        KeyCode::Char(' ') if *multi => app
                            .dispatch(AppAction::ToggleSelection)
                            .map_err(room_error)?,
                        KeyCode::Enter => app
                            .dispatch(AppAction::ConfirmSelection)
                            .map_err(room_error)?,
                        _ => {}
                    },
                    AppState::TextInput { .. } => match key.code {
                        KeyCode::Esc => app
                            .dispatch(AppAction::ReturnToMainMenu)
                            .map_err(room_error)?,
                        KeyCode::Enter => app
                            .dispatch(AppAction::SubmitTextInput)
                            .map_err(room_error)?,
                        KeyCode::Char(c) => {
                            app.dispatch(AppAction::InputChar(c)).map_err(room_error)?
                        }
                        KeyCode::Backspace => app
                            .dispatch(AppAction::BackspaceInput)
                            .map_err(room_error)?,
                        _ => {}
                    },
                    AppState::Message { .. } => match key.code {
                        KeyCode::Enter | KeyCode::Esc => app
                            .dispatch(AppAction::ReturnToMainMenu)
                            .map_err(room_error)?,
                        _ => {}
                    },
                    _ => {}
                }
            }
        }
    }
}

async fn fetch_items(args: &InteractiveArgs, action: &Action) -> Result<Vec<String>, CliError> {
    let client = k8sm8::create_client(args.context.to_string())
        .await
        .map_err(|e| CliError::Other(format!("Failed to create Kubernetes client: {}", e)))?;

    match action {
        Action::LogMerger
        | Action::LogStreamer
        | Action::DescribePod
        | Action::ShellIntoPod
        | Action::PortForward
        | Action::DeletePods => {
            let pods = k8sm8::pods::get_all_pods(client, &args.namespace)
                .await
                .map_err(|e| CliError::Other(format!("Failed to get all pods: {}", e)))?;
            Ok(pods.into_iter().filter_map(|p| p.metadata.name).collect())
        }
        Action::DisplayConfigMaps | Action::DeleteConfigMaps => {
            let cm = k8sm8::configmaps::get_all_configmaps(client, &args.namespace)
                .await
                .map_err(|e| CliError::Other(format!("Failed to get all configmaps: {}", e)))?;
            Ok(cm.into_iter().filter_map(|c| c.metadata.name).collect())
        }
        Action::RestartDeployments | Action::DeleteDeployments => {
            let d = k8sm8::deployments::get_all_deployments(client, &args.namespace)
                .await
                .map_err(|e| CliError::Other(format!("Failed to get all deployments: {}", e)))?;
            Ok(d.into_iter().filter_map(|x| x.metadata.name).collect())
        }
        Action::RestartDaemonSets | Action::DeleteDaemonSets => {
            let d = k8sm8::daemonsets::get_all_daemonsets(client, &args.namespace)
                .await
                .map_err(|e| CliError::Other(format!("Failed to get all daemonsets: {}", e)))?;
            Ok(d.into_iter().filter_map(|x| x.metadata.name).collect())
        }
        Action::DisplaySecrets | Action::DeleteSecrets => {
            let s = k8sm8::secrets::get_all_secrets(client, &args.namespace)
                .await
                .map_err(|e| CliError::Other(format!("Failed to get all secrets: {}", e)))?;
            Ok(s.into_iter().filter_map(|x| x.metadata.name).collect())
        }
        Action::DeleteServices => {
            let s = k8sm8::services::get_all_services(client, &args.namespace)
                .await
                .map_err(|e| CliError::Other(format!("Failed to get all services: {}", e)))?;
            Ok(s.into_iter().filter_map(|x| x.metadata.name).collect())
        }
        Action::InteractiveDeploy => {
            let env_name = resolve_interactive_environment(args)?;
            match crate::environment::Environment::load_from_file(&env_name) {
                Ok(env) => {
                    let mut srvs = Vec::new();
                    for s in env.services {
                        srvs.push(format!("{} (v{})", s.name, s.version));
                    }
                    Ok(srvs)
                }
                Err(e) => Err(CliError::Other(format!(
                    "Failed to load environment '{}' for deploy: {}",
                    env_name, e
                ))),
            }
        }
        Action::DisplayEvents => Ok(vec![]),
    }
}

async fn run_external_action(
    args: InteractiveArgs,
    action: Action,
    items: Vec<String>,
    input: String,
) -> Result<(), CliError> {
    let client = k8sm8::create_client(args.context.to_string())
        .await
        .map_err(|e| CliError::Other(format!("Failed to create Kubernetes client: {}", e)))?;

    match action {
        Action::LogMerger => k8sm8::logs::log_merger(client, &args.namespace, items)
            .await
            .map_err(|e| CliError::Other(e.to_string()))?,
        Action::LogStreamer => k8sm8::logs::log_streamer(client, &args.namespace, items)
            .await
            .map_err(|e| CliError::Other(e.to_string()))?,
        Action::DisplayConfigMaps => {
            for cm_name in items {
                let cm =
                    k8sm8::configmaps::get_configmap(client.clone(), &args.namespace, &cm_name)
                        .await
                        .map_err(|e| CliError::Other(e.to_string()))?;
                println!(
                    "ConfigMap: {}\nData: {:?}\n",
                    cm.metadata.name.unwrap_or_default(),
                    cm.data
                );
            }
        }
        Action::InteractiveDeploy => {
            let parsed_services = items
                .iter()
                .map(|s| s.split_whitespace().next().unwrap_or("").to_string())
                .collect::<Vec<String>>();
            println!(
                "Ready to deploy services: {:?} to env: {}",
                parsed_services, args.namespace
            );
            // Hook up to standard deployment builder flow here
            // Call into builder.rs / plan.rs logic
        }
        Action::DisplayEvents => {
            let events = k8sm8::events::get_all_events(client, &args.namespace)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
            for event in events {
                log(
                    scribe_rust::Color::Green,
                    &event.metadata.name.unwrap_or_default(),
                    &format!(
                        "\nReason: {}\nMessage: {}\n",
                        event.reason.unwrap_or_default(),
                        event.message.unwrap_or_default()
                    ),
                );
            }
        }
        Action::DisplaySecrets => {
            for secret_name in items {
                let secret =
                    k8sm8::secrets::get_secret(client.clone(), &args.namespace, &secret_name)
                        .await
                        .map_err(|e| CliError::Other(e.to_string()))?;
                secret.data.unwrap().iter().for_each(|(k, v)| {
                    println!("{}: {}", k, String::from_utf8_lossy(v.0.as_ref()));
                });
            }
        }
        Action::DescribePod => {
            for pod in items {
                println!("Describing pod {}...", pod);
                let _ = std::process::Command::new("kubectl")
                    .arg("describe")
                    .arg(format!("pod/{}", pod))
                    .arg("-n")
                    .arg(&args.namespace)
                    .status();
            }
        }
        Action::ShellIntoPod => {
            if let Some(pod) = items.first() {
                let shell = if input.is_empty() {
                    "/bin/bash"
                } else {
                    &input
                };
                println!("Starting shell session in {}...", pod);
                let _ = std::process::Command::new("kubectl")
                    .arg("exec")
                    .arg("-it")
                    .arg(pod)
                    .arg("-n")
                    .arg(&args.namespace)
                    .arg("--")
                    .arg(shell)
                    .status();
            }
        }
        Action::RestartDeployments => {
            for dep in items {
                k8sm8::deployments::restart_deployment(client.clone(), &args.namespace, &dep)
                    .await
                    .map_err(|e| CliError::Other(e.to_string()))?;
            }
        }
        Action::RestartDaemonSets => {
            for ds in items {
                k8sm8::daemonsets::restart_daemonset(client.clone(), &args.namespace, &ds)
                    .await
                    .map_err(|e| CliError::Other(e.to_string()))?;
            }
        }
        Action::PortForward => {
            if let Some(pod) = items.first() {
                println!("Starting port-forward for {} on {}...", pod, input);
                let _ = std::process::Command::new("kubectl")
                    .arg("port-forward")
                    .arg(format!("pod/{}", pod))
                    .arg("-n")
                    .arg(&args.namespace)
                    .arg(&input)
                    .status();
            }
        }
        Action::DeleteConfigMaps => {
            for cm in items {
                k8sm8::configmaps::delete_configmap(client.clone(), &args.namespace, &cm)
                    .await
                    .map_err(|e| CliError::Other(e.to_string()))?;
            }
        }
        Action::DeleteDeployments => {
            for dep in items {
                k8sm8::deployments::delete_deployment(client.clone(), &args.namespace, &dep)
                    .await
                    .map_err(|e| CliError::Other(e.to_string()))?;
            }
        }
        Action::DeleteDaemonSets => {
            for ds in items {
                k8sm8::daemonsets::delete_daemonset(client.clone(), &args.namespace, &ds)
                    .await
                    .map_err(|e| CliError::Other(e.to_string()))?;
            }
        }
        Action::DeletePods => {
            for pod in items {
                k8sm8::pods::delete_pod(client.clone(), &args.namespace, &pod)
                    .await
                    .map_err(|e| CliError::Other(e.to_string()))?;
            }
        }
        Action::DeleteServices => {
            for svc in items {
                k8sm8::services::delete_service(client.clone(), &args.namespace, &svc)
                    .await
                    .map_err(|e| CliError::Other(e.to_string()))?;
            }
        }
        Action::DeleteSecrets => {
            for sec in items {
                k8sm8::secrets::delete_secret(client.clone(), &args.namespace, &sec)
                    .await
                    .map_err(|e| CliError::Other(e.to_string()))?;
            }
        }
    }
    Ok(())
}

fn room_error(error: RoomError) -> CliError {
    CliError::Other(format!(
        "Failed to update interactive room state: {}",
        error
    ))
}

fn resolve_interactive_environment(args: &InteractiveArgs) -> Result<String, CliError> {
    if let Some(environment) = &args.environment {
        return Ok(environment.clone());
    }

    if environment_exists(&args.namespace) {
        return Ok(args.namespace.clone());
    }

    let environments = discover_environments()?;
    match environments.as_slice() {
        [only_environment] => Ok(only_environment.clone()),
        [] => Err(CliError::Other(
            "No Sailr environments found in ./k8s/environments. Pass --environment to interactive mode.".to_string(),
        )),
        _ => Err(CliError::Other(format!(
            "Interactive deploy needs a Sailr environment. Pass --environment. Available environments: {}",
            environments.join(", ")
        ))),
    }
}

fn discover_environments() -> Result<Vec<String>, CliError> {
    let environments_dir = Path::new("./k8s/environments");
    if !environments_dir.exists() {
        return Ok(Vec::new());
    }

    let mut environments = fs::read_dir(environments_dir)
        .map_err(|e| CliError::Other(format!("Failed to read environments directory: {}", e)))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let config_path = path.join("config.toml");
            if path.is_dir() && config_path.exists() {
                entry.file_name().into_string().ok()
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    environments.sort();
    Ok(environments)
}

fn environment_exists(environment: &str) -> bool {
    Path::new("./k8s/environments")
        .join(environment)
        .join("config.toml")
        .exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    fn cwd_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn resolve_interactive_environment_prefers_explicit_argument() {
        let _guard = cwd_lock().lock().expect("cwd lock should be available");
        let temp_dir = tempdir().expect("temp dir should be created");
        let previous_dir = env::current_dir().expect("cwd should be readable");
        env::set_current_dir(temp_dir.path()).expect("cwd should switch to temp dir");

        fs::create_dir_all("k8s/environments").expect("environments dir should exist");
        fs::create_dir_all("k8s/environments/dev").expect("env dir should exist");
        fs::write("k8s/environments/dev/config.toml", "").expect("config should be written");

        let args = InteractiveArgs {
            context: "ctx".to_string(),
            environment: Some("staging".to_string()),
            namespace: "default".to_string(),
        };

        let resolved = resolve_interactive_environment(&args).expect("environment should resolve");
        assert_eq!(resolved, "staging");

        env::set_current_dir(previous_dir).expect("cwd should be restored");
    }

    #[test]
    fn resolve_interactive_environment_uses_single_local_environment() {
        let _guard = cwd_lock().lock().expect("cwd lock should be available");
        let temp_dir = tempdir().expect("temp dir should be created");
        let previous_dir = env::current_dir().expect("cwd should be readable");
        env::set_current_dir(temp_dir.path()).expect("cwd should switch to temp dir");

        fs::create_dir_all("k8s/environments").expect("environments dir should exist");
        fs::create_dir_all("k8s/environments/dev").expect("env dir should exist");
        fs::write("k8s/environments/dev/config.toml", "").expect("config should be written");

        let args = InteractiveArgs {
            context: "ctx".to_string(),
            environment: None,
            namespace: "default".to_string(),
        };

        let resolved = resolve_interactive_environment(&args).expect("environment should resolve");
        assert_eq!(resolved, "dev");

        env::set_current_dir(previous_dir).expect("cwd should be restored");
    }

    #[test]
    fn resolve_interactive_environment_errors_when_ambiguous() {
        let _guard = cwd_lock().lock().expect("cwd lock should be available");
        let temp_dir = tempdir().expect("temp dir should be created");
        let previous_dir = env::current_dir().expect("cwd should be readable");
        env::set_current_dir(temp_dir.path()).expect("cwd should switch to temp dir");

        fs::create_dir_all("k8s/environments").expect("environments dir should exist");
        fs::create_dir_all("k8s/environments/dev").expect("dev dir should exist");
        fs::create_dir_all("k8s/environments/staging").expect("staging dir should exist");
        fs::write("k8s/environments/dev/config.toml", "").expect("dev config should be written");
        fs::write("k8s/environments/staging/config.toml", "")
            .expect("staging config should be written");

        let args = InteractiveArgs {
            context: "ctx".to_string(),
            environment: None,
            namespace: "default".to_string(),
        };

        let error =
            resolve_interactive_environment(&args).expect_err("resolution should be ambiguous");
        assert!(error
            .to_string()
            .contains("Interactive deploy needs a Sailr environment"));

        env::set_current_dir(previous_dir).expect("cwd should be restored");
    }
}
