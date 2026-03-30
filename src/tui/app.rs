use crate::cli::InteractiveArgs;
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

pub struct App {
    pub state: AppState,
    pub menu_items: Vec<Action>,
    pub main_menu_state: ListState,
    pub selection_state: ListState,
    pub selected_indices: HashSet<usize>,
    pub args: InteractiveArgs,
    pub should_quit: bool,

    // For storing actions that require suspending the TUI (like shell)
    pub pending_external_action: Option<(Action, Vec<String>, String)>,
}

impl App {
    pub fn new(args: InteractiveArgs) -> Self {
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
        ];

        let mut main_menu_state = ListState::default();
        main_menu_state.select(Some(0));

        Self {
            state: AppState::MainMenu,
            menu_items,
            main_menu_state,
            selection_state: ListState::default(),
            selected_indices: HashSet::new(),
            args,
            should_quit: false,
            pending_external_action: None,
        }
    }

    pub fn next(&mut self) {
        match &self.state {
            AppState::MainMenu => {
                let i = match self.main_menu_state.selected() {
                    Some(i) => {
                        if i >= self.menu_items.len() - 1 {
                            0
                        } else {
                            i + 1
                        }
                    }
                    None => 0,
                };
                self.main_menu_state.select(Some(i));
            }
            AppState::Selection { items, .. } => {
                let i = match self.selection_state.selected() {
                    Some(i) => {
                        if i >= items.len() - 1 {
                            0
                        } else {
                            i + 1
                        }
                    }
                    None => 0,
                };
                self.selection_state.select(Some(i));
            }
            _ => {}
        }
    }

    pub fn previous(&mut self) {
        match &self.state {
            AppState::MainMenu => {
                let i = match self.main_menu_state.selected() {
                    Some(i) => {
                        if i == 0 {
                            self.menu_items.len() - 1
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                };
                self.main_menu_state.select(Some(i));
            }
            AppState::Selection { items, .. } => {
                let i = match self.selection_state.selected() {
                    Some(i) => {
                        if i == 0 {
                            items.len() - 1
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                };
                self.selection_state.select(Some(i));
            }
            _ => {}
        }
    }

    pub fn toggle_selection(&mut self) {
        if let AppState::Selection { multi, .. } = self.state {
            if multi {
                if let Some(i) = self.selection_state.selected() {
                    if self.selected_indices.contains(&i) {
                        self.selected_indices.remove(&i);
                    } else {
                        self.selected_indices.insert(i);
                    }
                }
            }
        }
    }
}
