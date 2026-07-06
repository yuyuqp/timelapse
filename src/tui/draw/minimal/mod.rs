use std::time::Duration;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Tabs};
use ratatui::style::{Style, Color, Modifier};
use ratatui::text::Span;

use crate::tui::state::{ActiveTab, TuiState};

pub mod welcome;
pub mod tabs;
pub mod modals;

pub use welcome::draw_minimal_welcome;

pub fn draw_minimal_main(f: &mut ratatui::Frame, size: Rect, state: &TuiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tab headers
            Constraint::Min(3),    // Content
            Constraint::Length(1), // Status bar
            Constraint::Length(1), // Footer keys
        ])
        .split(size);

    // Tab Headers
    let titles = vec![
        "[1] Capture".to_string(),
        "[2] Render".to_string(),
        "[3] Sessions".to_string(),
        "[4] Diagnostics".to_string(),
    ];
    let selected_session_name = if state.sessions.is_empty() {
        "None".to_string()
    } else {
        state.sessions[state.active_session_index].name.clone()
    };
    let lib_name = state.resolved_library_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Timelapse");
    
    let library_emoji = " | ";
    let session_emoji = " | ";
    let header_title = format!(
        " Timelapse TUI{}Lib: {}{}Session: {} ",
        library_emoji, lib_name, session_emoji, selected_session_name
    );

    let border_type = BorderType::Plain;
    let header_border_color = Color::Gray;
    let highlight_color = Color::Cyan;

    let tab_block = Block::default()
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(Style::default().fg(header_border_color))
        .title(Span::styled(header_title, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)));

    let tabs = Tabs::new(titles)
        .select(state.active_tab as usize)
        .block(tab_block)
        .style(Style::default().fg(Color::Gray))
        .highlight_style(
            Style::default()
                .fg(highlight_color)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs, chunks[0]);

    // Content area
    match state.active_tab {
        ActiveTab::Capture => tabs::draw_capture_tab(f, chunks[1], state),
        ActiveTab::Render => tabs::draw_render_tab(f, chunks[1], state),
        ActiveTab::Sessions => tabs::draw_sessions_tab(f, chunks[1], state),
        ActiveTab::Diagnostics => tabs::draw_diagnostics_tab(f, chunks[1], state),
    }

    // Status Message Bar
    let status_text = if let Some((ref msg, timestamp)) = state.status_message {
        if timestamp.elapsed().unwrap_or(Duration::ZERO) < Duration::from_secs(4) {
            msg.clone()
        } else {
            "".to_string()
        }
    } else {
        "".to_string()
    };
    let status_bar = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::ITALIC));
    f.render_widget(status_bar, chunks[2]);

    // Footer Help keys
    let footer_text = match state.active_tab {
        ActiveTab::Capture => {
            "[Tab] Switch Tabs | [Space] Start/Stop Capture | [Up/Down] Adjust Interval | [D] Toggle Display | [A] Toggle Mode | [L] Change Library | [Q] Quit"
        }
        ActiveTab::Render => {
            "[Tab] Switch Tabs | [Enter/R] Start Render | [Up/Down] Adjust FPS | [L] Change Library | [Q] Quit"
        }
        ActiveTab::Sessions => {
            "[Tab] Switch Tabs | [Up/Down] Select Session | [O] Open Explorer | [C] Clean | [A] Append Mode | [U] Refresh | [L] Change Library | [Q] Quit"
        }
        ActiveTab::Diagnostics => {
            "[Tab] Switch Tabs | [D/U] Refresh Checks | [L] Change Library | [Q] Quit"
        }
    };
    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(footer, chunks[3]);

    // Confirm Clean Modal Overlay
    if let Some(index) = state.confirm_clean_index {
        modals::draw_confirm_modal(f, size, &state.sessions[index]);
    }

    // Change Library Modal Overlay
    if let Some(ref input_str) = state.change_library_input {
        modals::draw_library_modal(f, size, input_str);
    }

    // Confirm Render Modal Overlay
    if let Some(ref plan) = state.confirm_render_plan {
        modals::draw_render_confirm_modal(f, size, plan);
    }
}
