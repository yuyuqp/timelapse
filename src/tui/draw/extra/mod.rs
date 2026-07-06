use std::time::Duration;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Tabs};
use ratatui::text::Span;

use crate::tui::state::{ActiveTab, TuiState};

pub mod welcome;
pub mod tabs;
pub mod modals;

pub use welcome::draw_extra_welcome_neon;

const PALETTE: [Color; 12] = [
    Color::Rgb(255, 0,   128), // 0  hot pink
    Color::Rgb(220, 0,   255), // 1  purple
    Color::Rgb(100, 0,   255), // 2  indigo
    Color::Rgb(0,   100, 255), // 3  blue
    Color::Rgb(0,   220, 255), // 4  electric cyan
    Color::Rgb(0,   255, 180), // 5  spring green
    Color::Rgb(0,   255, 80),  // 6  neon green
    Color::Rgb(180, 255, 0),   // 7  lime
    Color::Rgb(255, 210, 0),   // 8  gold
    Color::Rgb(255, 120, 0),   // 9  orange
    Color::Rgb(255, 30,  60),  // 10 red-pink
    Color::Rgb(255, 0,   200), // 11 magenta
];

pub fn rainbow(tick: u64) -> Color {
    PALETTE[tick as usize % PALETTE.len()]
}

pub fn rainbow_off(tick: u64, offset: usize) -> Color {
    PALETTE[(tick as usize + offset) % PALETTE.len()]
}

pub const C_CAPTURE: Color = Color::Rgb(255, 80,  140); // hot pink
pub const C_RENDER:  Color = Color::Rgb(0,   220, 255); // electric cyan
pub const C_SESSION: Color = Color::Rgb(80,  255,  80); // neon green
pub const C_DIAG:    Color = Color::Rgb(255, 200,   0); // gold
pub const C_DIM:     Color = Color::Rgb(70,   70,  90); // dim purple-grey

pub fn tab_accent(tab: ActiveTab) -> Color {
    match tab {
        ActiveTab::Capture     => C_CAPTURE,
        ActiveTab::Render      => C_RENDER,
        ActiveTab::Sessions    => C_SESSION,
        ActiveTab::Diagnostics => C_DIAG,
    }
}

const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn neon_spinner(tick: u64) -> &'static str {
    SPINNER_FRAMES[tick as usize % SPINNER_FRAMES.len()]
}

pub fn pulse_on(tick: u64) -> bool {
    tick % 14 < 7
}

pub fn sparkline_str(data: &[u64], max_chars: usize) -> String {
    const BARS: [char; 9] = [' ', ' ', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    if data.is_empty() || max_chars == 0 {
        return String::new();
    }
    let max_val = data.iter().copied().max().unwrap_or(1).max(1);
    let count   = max_chars.min(data.len());
    let start   = data.len().saturating_sub(count);
    data[start..].iter().map(|&v| {
        BARS[((v * 8) / max_val).min(8) as usize]
    }).collect()
}

pub fn draw_extra_main(f: &mut ratatui::Frame, size: Rect, state: &TuiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // animated tab bar
            Constraint::Min(3),    // tab content
            Constraint::Length(1), // pulsing status message
            Constraint::Length(1), // rainbow key-hints footer
        ])
        .split(size);

    let accent      = tab_accent(state.active_tab);
    let border_anim = rainbow(state.tick / 3);

    // ── Animated rainbow tab bar ──────────────────────────────────────────────
    let lib_name = state.resolved_library_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("timelapse");
    let session_name = state.sessions
        .get(state.active_session_index)
        .map(|s| s.name.as_str())
        .unwrap_or("none");
    let header_title = format!(
        " ⚡ TIMELAPSE  ·  lib: {}  ·  session: {} ",
        lib_name, session_name
    );

    let tab_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_anim))
        .title(Span::styled(
            header_title,
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ));

    let tabs = Tabs::new(vec![
        "  📷 CAPTURE  ".to_string(),
        "  🎬 RENDER   ".to_string(),
        "  📂 SESSIONS ".to_string(),
        "  🛠 DIAG     ".to_string(),
    ])
    .select(state.active_tab as usize)
    .block(tab_block)
    .style(Style::default().fg(C_DIM))
    .highlight_style(
        Style::default().fg(Color::Black).bg(accent).add_modifier(Modifier::BOLD),
    );
    f.render_widget(tabs, chunks[0]);

    // ── Content area ──────────────────────────────────────────────────────────
    match state.active_tab {
        ActiveTab::Capture     => tabs::draw_extra_capture(f, chunks[1], state),
        ActiveTab::Render      => tabs::draw_extra_render(f, chunks[1], state),
        ActiveTab::Sessions    => tabs::draw_extra_sessions(f, chunks[1], state),
        ActiveTab::Diagnostics => tabs::draw_extra_diagnostics(f, chunks[1], state),
    }

    // ── Pulsing status message ────────────────────────────────────────────────
    let status_text = if let Some((ref msg, ts)) = state.status_message {
        if ts.elapsed().unwrap_or(Duration::ZERO) < Duration::from_secs(4) {
            format!(" ❖  {}  ❖ ", msg)
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    let status_col = rainbow_off(state.tick / 3, 4);
    f.render_widget(
        Paragraph::new(status_text)
            .style(Style::default().fg(status_col).add_modifier(Modifier::BOLD)),
        chunks[2],
    );

    // ── Rainbow key-hints footer ──────────────────────────────────────────────
    let footer_text = match state.active_tab {
        ActiveTab::Capture =>
            " [Tab] tabs  ·  [Space] capture  ·  [↑↓] interval  ·  [D] display  ·  [A] mode  ·  [L] library  ·  [Q] quit",
        ActiveTab::Render =>
            " [Tab] tabs  ·  [Enter/R] render  ·  [↑↓] fps  ·  [L] library  ·  [Q] quit",
        ActiveTab::Sessions =>
            " [Tab] tabs  ·  [↑↓] select  ·  [Enter] activate  ·  [O] open  ·  [C] clean  ·  [A] append  ·  [U] refresh  ·  [Q] quit",
        ActiveTab::Diagnostics =>
            " [Tab] tabs  ·  [D/U] refresh  ·  [L] library  ·  [Q] quit",
    };
    let footer_col = rainbow_off(state.tick / 5, 7);
    f.render_widget(
        Paragraph::new(footer_text).style(Style::default().fg(footer_col)),
        chunks[3],
    );

    // ── Modal overlays ────────────────────────────────────────────────────────
    if let Some(idx) = state.confirm_clean_index {
        modals::draw_extra_clean_modal(f, size, &state.sessions[idx], state.tick);
    }
    if let Some(ref input) = state.change_library_input {
        modals::draw_extra_library_modal(f, size, input, state.tick);
    }
    if let Some(ref plan) = state.confirm_render_plan {
        modals::draw_extra_render_confirm_modal(f, size, plan, state.tick);
    }
}
