use crate::tui::app::{Action, App, AppState};
use crate::{cli::InteractiveArgs, deployment::k8sm8, errors::CliError};
use crossterm::event::{self, Event, KeyCode};
use scribe_rust::log;
use std::time::Duration;

pub async fn main_menu(args: InteractiveArgs) -> Result<(), CliError> {
    let mut app = App::new(args.clone());

    loop {
        let mut terminal = crate::tui::init()
            .map_err(|e| CliError::Other(format!("Failed to init TUI: {}", e)))?;
        let res = run_app(&mut terminal, &mut app).await;
        crate::tui::restore()
            .map_err(|e| CliError::Other(format!("Failed to restore TUI: {}", e)))?;

        if let Err(e) = res {
            println!("Error: {}", e);
            break;
        }

        if app.should_quit {
            println!("Exiting...");
            break;
        }

        if let Some((action, items, input)) = app.pending_external_action.take() {
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

            // reset state to main menu
            app.state = AppState::MainMenu;
            app.selected_indices.clear();
        }
    }

    Ok(())
}

async fn run_app(terminal: &mut crate::tui::Tui, app: &mut App) -> Result<(), CliError> {
    loop {
        terminal
            .draw(|f| crate::tui::ui::draw(f, app))
            .map_err(|e| CliError::Other(e.to_string()))?;

        if app.should_quit || app.pending_external_action.is_some() {
            return Ok(());
        }

        // Handle auto-transitions
        let fetch_action = match &app.state {
            AppState::Fetching { action, .. } => Some(action.clone()),
            _ => None,
        };

        if let Some(action) = fetch_action {
            match fetch_items(&app.args, &action).await {
                Ok(items) => {
                    if items.is_empty() {
                        app.state = AppState::Message {
                            title: "Not Found".to_string(),
                            content: format!(
                                "No items found for this action in namespace '{}'.",
                                app.args.namespace
                            ),
                            is_error: true,
                        };
                    } else {
                        app.state = AppState::Selection {
                            action: action.clone(),
                            items,
                            multi: action.is_multi_select(),
                        };
                    }
                }
                Err(e) => {
                    app.state = AppState::Message {
                        title: "Error".to_string(),
                        content: format!("Failed to fetch: {}", e),
                        is_error: true,
                    };
                }
            }
            continue;
        }

        let process_action = match &app.state {
            AppState::Processing {
                action,
                selected_items,
                input,
                ..
            } => Some((action.clone(), selected_items.clone(), input.clone())),
            _ => None,
        };

        if let Some((action, selected_items, input)) = process_action {
            // Processing means scheduling it for external action and returning
            let input_val = input.unwrap_or_default();
            app.pending_external_action = Some((action, selected_items, input_val));
            return Ok(());
        }

        if event::poll(Duration::from_millis(50)).map_err(|e| CliError::Other(e.to_string()))? {
            if let Event::Key(key) = event::read().map_err(|e| CliError::Other(e.to_string()))? {
                match &mut app.state {
                    AppState::MainMenu => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
                        KeyCode::Up => app.previous(),
                        KeyCode::Down => app.next(),
                        KeyCode::Enter => {
                            let selected_idx = app.main_menu_state.selected().unwrap_or(0);
                            let selected_action = app.menu_items[selected_idx].clone();

                            if selected_action.skips_selection() {
                                app.state = AppState::Processing {
                                    action: selected_action,
                                    message: "Starting...".into(),
                                    selected_items: vec![],
                                    input: None,
                                };
                            } else {
                                app.state = AppState::Fetching {
                                    action: selected_action.clone(),
                                    message: format!(
                                        "Fetching data for {}...",
                                        selected_action.as_str()
                                    ),
                                };
                            }
                        }
                        _ => {}
                    },
                    AppState::DeploySelection { services } => match key.code {
                        KeyCode::Esc => {
                            app.state = AppState::MainMenu;
                            app.selected_indices.clear();
                        }

                        KeyCode::Up => app.previous(),
                        KeyCode::Down => app.next(),
                        KeyCode::Char(' ') => app.toggle_selection(),
                        KeyCode::Enter => {
                            let mut selected = Vec::new();
                            for &idx in &app.selected_indices {
                                if let Some(srv) = services.get(idx) {
                                    selected.push(srv.name.clone());
                                }
                            }
                            if !selected.is_empty() {
                                app.state = AppState::Processing {
                                    action: Action::InteractiveDeploy,
                                    message: "Starting deployment...".into(),
                                    selected_items: selected,
                                    input: None,
                                };
                            }
                        }
                        _ => {}
                    },

                    AppState::Selection {
                        action,
                        items,
                        multi,
                    } => {
                        match key.code {
                            KeyCode::Esc => {
                                app.state = AppState::MainMenu;
                                app.selected_indices.clear();
                            }
                            KeyCode::Up => app.previous(),
                            KeyCode::Down => app.next(),
                            KeyCode::Char(' ') if *multi => app.toggle_selection(),
                            KeyCode::Enter => {
                                let mut selected = Vec::new();
                                if *multi {
                                    for &idx in &app.selected_indices {
                                        if let Some(item) = items.get(idx) {
                                            selected.push(item.clone());
                                        }
                                    }
                                } else if let Some(idx) = app.selection_state.selected() {
                                    if let Some(item) = items.get(idx) {
                                        selected.push(item.clone());
                                    }
                                }

                                if selected.is_empty() && *multi {
                                    // Must select at least one
                                } else {
                                    let action_clone = action.clone();
                                    if let Some(prompt) = action_clone.requires_input() {
                                        app.state = AppState::TextInput {
                                            action: action_clone,
                                            prompt: prompt.to_string(),
                                            input: String::new(),
                                            selected_items: selected,
                                        };
                                    } else {
                                        app.state = AppState::Processing {
                                            action: action_clone,
                                            message: "Processing...".to_string(),
                                            selected_items: selected,
                                            input: None,
                                        };
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    AppState::TextInput {
                        action,
                        prompt: _,
                        input,
                        selected_items,
                    } => match key.code {
                        KeyCode::Esc => {
                            app.state = AppState::MainMenu;
                            app.selected_indices.clear();
                        }
                        KeyCode::Enter => {
                            app.state = AppState::Processing {
                                action: action.clone(),
                                message: "Processing...".to_string(),
                                selected_items: selected_items.clone(),
                                input: Some(input.clone()),
                            };
                        }
                        KeyCode::Char(c) => {
                            input.push(c);
                        }
                        KeyCode::Backspace => {
                            input.pop();
                        }
                        _ => {}
                    },
                    AppState::Message { .. } => match key.code {
                        KeyCode::Enter | KeyCode::Esc => {
                            app.state = AppState::MainMenu;
                        }
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
            let env_name = &args.namespace; // Quick proxy, assume interactive takes an env
            match crate::environment::Environment::load_from_file(env_name) {
                Ok(env) => {
                    let mut srvs = Vec::new();
                    for s in env.services {
                        srvs.push(format!("{} (v{})", s.name, s.version));
                    }
                    Ok(srvs)
                }
                Err(_) => Err(CliError::Other(
                    "Failed to load environment for deploy".to_string(),
                )),
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
