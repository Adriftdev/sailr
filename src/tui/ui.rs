use crate::tui::app::{App, AppState};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.size());

    let title_block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan));
    let title = Paragraph::new(Span::styled(
        " Sailr Interactive Mode ",
        Style::default().add_modifier(Modifier::BOLD),
    ))
    .alignment(Alignment::Center)
    .block(title_block);
    f.render_widget(title, chunks[0]);

    match &app.state {
        AppState::MainMenu => {
            let items: Vec<ListItem> = app
                .menu_items
                .iter()
                .map(|i| {
                    let lines = vec![Line::from(i.as_str())];
                    ListItem::new(lines).style(Style::default().fg(Color::White))
                })
                .collect();

            let menu_list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(" Main Menu "))
                .highlight_style(
                    Style::default()
                        .bg(Color::Cyan)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol(">> ");

            f.render_stateful_widget(menu_list, chunks[1], &mut app.main_menu_state);

            let footer =
                Paragraph::new("Use \u{2191}/\u{2193} to move, Enter to select, q/Esc to quit.")
                    .alignment(Alignment::Center)
                    .block(Block::default().borders(Borders::ALL));
            f.render_widget(footer, chunks[2]);
        }
        AppState::Fetching { action, message }
        | AppState::Processing {
            action, message, ..
        } => {
            let p = Paragraph::new(format!("\n{}\n", message))
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(action.as_str()),
                );
            f.render_widget(p, chunks[1]);
        }
        AppState::DeploySelection { services } => {
            let items: Vec<ListItem> = services
                .iter()
                .enumerate()
                .map(|(i, srv)| {
                    let check = if app.selected_indices.contains(&i) {
                        "[x]"
                    } else {
                        "[ ]"
                    };
                    let name = format!("{} {} (v{})", check, srv.name, srv.version);
                    let lines = vec![Line::from(name)];
                    ListItem::new(lines).style(Style::default().fg(Color::White))
                })
                .collect();

            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Select Services to Deploy (Space to toggle) ");

            let list = List::new(items)
                .block(block)
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
                .highlight_symbol(">> ");

            f.render_stateful_widget(list, chunks[1], &mut app.selection_state);

            let instruction = "Space: toggle, Enter: deploy selected, Esc: cancel";
            let footer = Paragraph::new(instruction)
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(footer, chunks[2]);
        }
        AppState::Selection {
            action,
            items,
            multi,
        } => {
            let list_items: Vec<ListItem> = items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let prefix = if *multi {
                        if app.selected_indices.contains(&i) {
                            "[x] "
                        } else {
                            "[ ] "
                        }
                    } else {
                        "  "
                    };
                    ListItem::new(format!("{}{}", prefix, item))
                })
                .collect();

            let selection_list = List::new(list_items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!(" {} ", action.as_str())),
                )
                .highlight_style(
                    Style::default()
                        .bg(Color::Yellow)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("> ");

            f.render_stateful_widget(selection_list, chunks[1], &mut app.selection_state);

            let instruction = if *multi {
                "\u{2191}/\u{2193}: move, Space: toggle, Enter: confirm, Esc: back"
            } else {
                "\u{2191}/\u{2193}: move, Enter: select, Esc: back"
            };
            let footer = Paragraph::new(instruction)
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(footer, chunks[2]);
        }
        AppState::TextInput {
            action,
            prompt,
            input,
            ..
        } => {
            let block = Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} ", action.as_str()));

            let p = Paragraph::new(vec![
                Line::from(prompt.clone()),
                Line::from(format!("> {}\u{2588}", input)), // cursor block
            ])
            .block(block);

            f.render_widget(p, chunks[1]);

            let footer = Paragraph::new("Type to input, Enter to submit, Esc to back")
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(footer, chunks[2]);
        }
        AppState::Message {
            title,
            content,
            is_error,
        } => {
            let color = if *is_error { Color::Red } else { Color::Green };
            let p = Paragraph::new(content.clone())
                .wrap(Wrap { trim: true })
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(title.clone())
                        .style(Style::default().fg(color)),
                );
            f.render_widget(p, chunks[1]);

            let footer = Paragraph::new("Press Enter or Esc to continue")
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(footer, chunks[2]);
        }
    }
}
