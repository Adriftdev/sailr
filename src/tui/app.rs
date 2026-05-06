use crate::orchestrator::{LoggingMiddleware, Room, RoomError, RoomService, RoomServiceBuilder};
use ratatui::widgets::ListState;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    LogMerger,
    LogStreamer,
    DisplayConfigMaps,
    DisplayEvents,
    DisplaySecrets,
    DescribePod,
    ShellIntoPod,
    RestartDeployments,
    RestartDaemonSets,
    PortForward,
    DeleteConfigMaps,
    DeleteDeployments,
    DeleteDaemonSets,
    DeletePods,
    DeleteServices,
    DeleteSecrets,
    InteractiveDeploy,
}

impl Action {
    pub fn as_str(&self) -> &'static str {
        match self {
            Action::LogMerger => "Log Merger",
            Action::LogStreamer => "Log Streamer",
            Action::DisplayConfigMaps => "Display ConfigMaps",
            Action::DisplayEvents => "Display Events",
            Action::DisplaySecrets => "Display Secrets",
            Action::DescribePod => "Describe Pod",
            Action::ShellIntoPod => "Shell into Pod",
            Action::RestartDeployments => "Restart Deployments",
            Action::RestartDaemonSets => "Restart DaemonSets",
            Action::PortForward => "Port Forward",
            Action::DeleteConfigMaps => "Delete ConfigMaps",
            Action::DeleteDeployments => "Delete Deployments",
            Action::DeleteDaemonSets => "Delete DaemonSets",
            Action::DeletePods => "Delete Pods",
            Action::DeleteServices => "Delete Services",
            Action::DeleteSecrets => "Delete Secrets",
            Action::InteractiveDeploy => "Interactive Deploy (Select Services)",
        }
    }

    pub fn is_multi_select(&self) -> bool {
        !matches!(
            self,
            Action::DisplayEvents
                | Action::DescribePod
                | Action::ShellIntoPod
                | Action::PortForward
        )
    }

    pub fn requires_input(&self) -> Option<&'static str> {
        match self {
            Action::ShellIntoPod => Some("Enter shell (default: /bin/bash):"),
            Action::PortForward => Some("Enter ports (e.g. 8080:80):"),
            _ => None,
        }
    }

    pub fn skips_selection(&self) -> bool {
        matches!(self, Action::DisplayEvents)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppState {
    MainMenu,
    Fetching {
        action: Action,
        message: String,
    },
    Selection {
        action: Action,
        items: Vec<String>,
        multi: bool,
    },
    TextInput {
        action: Action,
        prompt: String,
        input: String,
        selected_items: Vec<String>,
    },
    Processing {
        action: Action,
        message: String,
        selected_items: Vec<String>,
        input: Option<String>,
    },
    Message {
        title: String,
        content: String,
        is_error: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingExternalAction {
    pub action: Action,
    pub items: Vec<String>,
    pub input: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppModel {
    pub state: AppState,
    pub main_menu_index: usize,
    pub selection_index: Option<usize>,
    pub selected_indices: HashSet<usize>,
    pub should_quit: bool,
    pub pending_external_action: Option<PendingExternalAction>,
}

impl Default for AppModel {
    fn default() -> Self {
        Self {
            state: AppState::MainMenu,
            main_menu_index: 0,
            selection_index: None,
            selected_indices: HashSet::new(),
            should_quit: false,
            pending_external_action: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppAction {
    MoveNext {
        menu_len: usize,
    },
    MovePrevious {
        menu_len: usize,
    },
    ToggleSelection,
    Quit,
    ActivateMenu(Action),
    LoadSelection {
        action: Action,
        items: Vec<String>,
    },
    ConfirmSelection,
    InputChar(char),
    BackspaceInput,
    SubmitTextInput,
    QueuePendingExternalAction,
    ClearPendingExternalAction,
    ReturnToMainMenu,
    ExternalActionCompleted,
    ShowMessage {
        title: String,
        content: String,
        is_error: bool,
    },
}

pub struct App {
    room: RoomService<AppModel, AppAction>,
    menu_items: Vec<Action>,
}

impl App {
    pub fn new() -> Result<Self, RoomError> {
        let menu_items = vec![
            Action::LogMerger,
            Action::LogStreamer,
            Action::DisplayConfigMaps,
            Action::DisplayEvents,
            Action::DisplaySecrets,
            Action::DescribePod,
            Action::ShellIntoPod,
            Action::RestartDeployments,
            Action::RestartDaemonSets,
            Action::PortForward,
            Action::DeleteConfigMaps,
            Action::DeleteDeployments,
            Action::DeleteDaemonSets,
            Action::DeletePods,
            Action::DeleteServices,
            Action::DeleteSecrets,
            Action::InteractiveDeploy,
        ];

        let room = RoomServiceBuilder::new()
            .initial_state(AppModel::default())
            .reducer(reduce_app_model)
            .middleware(LoggingMiddleware)
            .build()?;

        Ok(Self { room, menu_items })
    }

    pub fn state(&self) -> std::sync::Arc<AppModel> {
        self.room.get_state()
    }

    pub fn dispatch(&self, action: AppAction) -> Result<(), RoomError> {
        self.room.dispatch(action)
    }

    pub fn menu_items(&self) -> &[Action] {
        &self.menu_items
    }

    pub fn should_quit(&self) -> bool {
        self.state().should_quit
    }

    pub fn has_pending_external_action(&self) -> bool {
        self.state().pending_external_action.is_some()
    }

    pub fn fetch_action(&self) -> Option<Action> {
        match &self.state().state {
            AppState::Fetching { action, .. } => Some(action.clone()),
            _ => None,
        }
    }

    pub fn processing_action(&self) -> Option<PendingExternalAction> {
        match &self.state().state {
            AppState::Processing {
                action,
                selected_items,
                input,
                ..
            } => Some(PendingExternalAction {
                action: action.clone(),
                items: selected_items.clone(),
                input: input.clone().unwrap_or_default(),
            }),
            _ => None,
        }
    }

    pub fn take_pending_external_action(&self) -> Result<Option<PendingExternalAction>, RoomError> {
        let pending = self.state().pending_external_action.clone();
        if pending.is_some() {
            self.dispatch(AppAction::ClearPendingExternalAction)?;
        }
        Ok(pending)
    }

    pub fn current_main_menu_action(&self) -> Option<Action> {
        let state = self.state();
        self.menu_items.get(state.main_menu_index).cloned()
    }

    pub fn main_menu_state(&self) -> ListState {
        make_list_state(Some(self.state().main_menu_index))
    }

    pub fn selection_state(&self) -> ListState {
        make_list_state(self.state().selection_index)
    }
}

fn make_list_state(selected: Option<usize>) -> ListState {
    let mut state = ListState::default();
    state.select(selected);
    state
}

fn clear_selection(model: &mut AppModel) {
    model.selection_index = None;
    model.selected_indices.clear();
}

fn advance_index(current: Option<usize>, len: usize) -> Option<usize> {
    if len == 0 {
        None
    } else {
        Some(match current {
            Some(index) if index + 1 < len => index + 1,
            _ => 0,
        })
    }
}

fn rewind_index(current: Option<usize>, len: usize) -> Option<usize> {
    if len == 0 {
        None
    } else {
        Some(match current {
            Some(0) | None => len - 1,
            Some(index) => index - 1,
        })
    }
}

fn selected_items(items: &[String], selected_indices: &HashSet<usize>) -> Vec<String> {
    let mut indices = selected_indices.iter().copied().collect::<Vec<_>>();
    indices.sort_unstable();
    indices
        .into_iter()
        .filter_map(|index| items.get(index).cloned())
        .collect()
}

fn reduce_app_model(model: &mut AppModel, action: AppAction) {
    match action {
        AppAction::MoveNext { menu_len } => match &model.state {
            AppState::MainMenu => {
                if menu_len > 0 {
                    model.main_menu_index = advance_index(Some(model.main_menu_index), menu_len)
                        .expect("menu index should exist when menu_len > 0");
                }
            }
            AppState::Selection { items, .. } => {
                model.selection_index = advance_index(model.selection_index, items.len());
            }
            _ => {}
        },
        AppAction::MovePrevious { menu_len } => match &model.state {
            AppState::MainMenu => {
                if menu_len > 0 {
                    model.main_menu_index = rewind_index(Some(model.main_menu_index), menu_len)
                        .expect("menu index should exist when menu_len > 0");
                }
            }
            AppState::Selection { items, .. } => {
                model.selection_index = rewind_index(model.selection_index, items.len());
            }
            _ => {}
        },
        AppAction::ToggleSelection => {
            if let AppState::Selection { multi: true, .. } = &model.state {
                if let Some(index) = model.selection_index {
                    if !model.selected_indices.insert(index) {
                        model.selected_indices.remove(&index);
                    }
                }
            }
        }
        AppAction::Quit => {
            model.should_quit = true;
        }
        AppAction::ActivateMenu(action) => {
            clear_selection(model);
            if action.skips_selection() {
                model.state = AppState::Processing {
                    action,
                    message: "Starting...".to_string(),
                    selected_items: Vec::new(),
                    input: None,
                };
            } else {
                model.state = AppState::Fetching {
                    message: format!("Fetching data for {}...", action.as_str()),
                    action,
                };
            }
        }
        AppAction::LoadSelection { action, items } => {
            clear_selection(model);
            model.selection_index = (!items.is_empty()).then_some(0);
            model.state = AppState::Selection {
                multi: action.is_multi_select(),
                action,
                items,
            };
        }
        AppAction::ConfirmSelection => {
            if let AppState::Selection {
                action,
                items,
                multi,
            } = model.state.clone()
            {
                let selected = if multi {
                    selected_items(&items, &model.selected_indices)
                } else {
                    model
                        .selection_index
                        .and_then(|index| items.get(index).cloned())
                        .into_iter()
                        .collect()
                };

                if multi && selected.is_empty() {
                    return;
                }

                if let Some(prompt) = action.requires_input() {
                    model.state = AppState::TextInput {
                        action,
                        prompt: prompt.to_string(),
                        input: String::new(),
                        selected_items: selected,
                    };
                } else {
                    model.state = AppState::Processing {
                        action,
                        message: "Processing...".to_string(),
                        selected_items: selected,
                        input: None,
                    };
                }
            }
        }
        AppAction::InputChar(c) => {
            if let AppState::TextInput { input, .. } = &mut model.state {
                input.push(c);
            }
        }
        AppAction::BackspaceInput => {
            if let AppState::TextInput { input, .. } = &mut model.state {
                input.pop();
            }
        }
        AppAction::SubmitTextInput => {
            if let AppState::TextInput {
                action,
                input,
                selected_items,
                ..
            } = model.state.clone()
            {
                model.state = AppState::Processing {
                    action,
                    message: "Processing...".to_string(),
                    selected_items,
                    input: Some(input),
                };
            }
        }
        AppAction::QueuePendingExternalAction => {
            if let AppState::Processing {
                action,
                selected_items,
                input,
                ..
            } = model.state.clone()
            {
                model.pending_external_action = Some(PendingExternalAction {
                    action,
                    items: selected_items,
                    input: input.unwrap_or_default(),
                });
            }
        }
        AppAction::ClearPendingExternalAction => {
            model.pending_external_action = None;
        }
        AppAction::ReturnToMainMenu | AppAction::ExternalActionCompleted => {
            clear_selection(model);
            model.state = AppState::MainMenu;
        }
        AppAction::ShowMessage {
            title,
            content,
            is_error,
        } => {
            model.state = AppState::Message {
                title,
                content,
                is_error,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection_model(action: Action, items: Vec<&str>, multi: bool) -> AppModel {
        AppModel {
            state: AppState::Selection {
                action,
                items: items.into_iter().map(str::to_string).collect(),
                multi,
            },
            selection_index: Some(0),
            ..AppModel::default()
        }
    }

    #[test]
    fn menu_navigation_wraps() {
        let mut model = AppModel::default();

        reduce_app_model(&mut model, AppAction::MovePrevious { menu_len: 3 });
        assert_eq!(model.main_menu_index, 2);

        reduce_app_model(&mut model, AppAction::MoveNext { menu_len: 3 });
        assert_eq!(model.main_menu_index, 0);
    }

    #[test]
    fn selection_toggle_updates_selected_indices() {
        let mut model = selection_model(Action::DeletePods, vec!["pod-a", "pod-b"], true);

        reduce_app_model(&mut model, AppAction::ToggleSelection);
        assert!(model.selected_indices.contains(&0));

        reduce_app_model(&mut model, AppAction::ToggleSelection);
        assert!(model.selected_indices.is_empty());
    }

    #[test]
    fn text_input_flows_to_processing() {
        let mut model = selection_model(Action::PortForward, vec!["pod-a"], false);

        reduce_app_model(&mut model, AppAction::ConfirmSelection);
        assert!(matches!(model.state, AppState::TextInput { .. }));

        reduce_app_model(&mut model, AppAction::InputChar('8'));
        reduce_app_model(&mut model, AppAction::InputChar('0'));
        reduce_app_model(&mut model, AppAction::BackspaceInput);
        reduce_app_model(&mut model, AppAction::SubmitTextInput);

        assert!(matches!(
            model.state,
            AppState::Processing {
                input: Some(ref value),
                ..
            } if value == "8"
        ));
    }

    #[test]
    fn message_dismiss_returns_to_menu() {
        let mut model = AppModel {
            state: AppState::Message {
                title: "Error".to_string(),
                content: "boom".to_string(),
                is_error: true,
            },
            ..AppModel::default()
        };

        reduce_app_model(&mut model, AppAction::ReturnToMainMenu);

        assert!(matches!(model.state, AppState::MainMenu));
    }

    #[test]
    fn processing_transitions_to_pending_external_action() {
        let mut model = AppModel {
            state: AppState::Processing {
                action: Action::DisplayEvents,
                message: "Starting...".to_string(),
                selected_items: vec!["one".to_string()],
                input: Some("value".to_string()),
            },
            ..AppModel::default()
        };

        reduce_app_model(&mut model, AppAction::QueuePendingExternalAction);
        assert_eq!(
            model.pending_external_action,
            Some(PendingExternalAction {
                action: Action::DisplayEvents,
                items: vec!["one".to_string()],
                input: "value".to_string(),
            })
        );

        reduce_app_model(&mut model, AppAction::ExternalActionCompleted);
        assert!(matches!(model.state, AppState::MainMenu));
        assert!(model.selected_indices.is_empty());
    }
}
