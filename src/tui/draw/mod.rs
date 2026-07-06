use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::style::{Style, Color, Modifier};

use crate::config::Theme;
use super::state::TuiState;

mod common;
mod minimal;
mod extra;

pub fn draw_ui(f: &mut ratatui::Frame, state: &TuiState) {
    let size = f.area();

    // Screen size warning guard
    if size.width < 80 || size.height < 20 {
        let msg = format!(
            "\n\n  Terminal window size is too small!\n\n  \
             Current:  {}x{}\n  \
             Required: 80x20\n\n  \
             Please resize your window, decrease font size, or zoom out.",
            size.width, size.height
        );
        let warning = Paragraph::new(msg)
            .block(Block::default().borders(Borders::ALL).title(" Warning "))
            .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
        f.render_widget(warning, size);
        return;
    }

    if state.show_welcome {
        draw_welcome_screen(f, size, state);
        return;
    }

    if state.config.theme == Theme::Extra {
        extra::draw_extra_main(f, size, state);
        return;
    }

    minimal::draw_minimal_main(f, size, state);
}

fn draw_welcome_screen(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    match state.config.theme {
        Theme::Extra => extra::draw_extra_welcome_neon(f, area, state),
        Theme::Minimal => minimal::draw_minimal_welcome(f, area, state),
    }
}
