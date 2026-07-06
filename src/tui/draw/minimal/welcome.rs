use ratatui::layout::Rect;
use ratatui::style::{Style, Color, Modifier};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::tui::state::TuiState;
use crate::tui::draw::common::centered_rect;

pub fn draw_minimal_welcome(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let logo = r#"
  ████████ ██ ███    ███ ███████ ██       █████  ██████  ███████ ███████ 
     ██    ██ ████  ████ ██      ██      ██   ██ ██   ██ ██      ██      
     ██    ██ ██ ████ ██ █████   ██      ███████ ██████  ███████ █████   
     ██    ██ ██  ██  ██ ██      ██      ██   ██ ██           ██ ██      
     ██    ██ ██      ██ ███████ ███████ ██   ██ ██      ███████ ███████ 
"#;

    let mut logo_lines = vec![Line::raw("")];
    for line in logo.lines() {
        if !line.trim().is_empty() {
            logo_lines.push(Line::from(vec![
                Span::styled(line, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            ]));
        }
    }

    logo_lines.push(Line::raw(""));
    logo_lines.push(Line::from(vec![
        Span::styled("    Session-based Screenshot Collector & Rendering Engine", Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
    ]));
    logo_lines.push(Line::from(vec![
        Span::styled(format!("    Version {}", env!("CARGO_PKG_VERSION")), Style::default().fg(Color::Green))
    ]));
    logo_lines.push(Line::raw(""));
    logo_lines.push(Line::from(vec![
        Span::styled(format!("    Active Library: {}", state.resolved_library_path.display()), Style::default().fg(Color::DarkGray))
    ]));
    logo_lines.push(Line::raw(""));
    logo_lines.push(Line::raw(""));
    logo_lines.push(Line::from(vec![
        Span::styled(
            "    [ Press any key to start... ]  (Press [T] to toggle theme)",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::DIM)
        )
    ]));

    let paragraph = Paragraph::new(logo_lines)
        .block(block)
        .style(Style::default().fg(Color::White));

    let center_area = centered_rect(90, 80, area);
    f.render_widget(paragraph, center_area);
}
