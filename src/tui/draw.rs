use std::time::Duration;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Table, Tabs};

use crate::doctor::CheckStatus;
use crate::config::Theme;
use crate::manage::SessionSummary;
use crate::session::DisplayTarget;
use crate::render::parse_exclusions;
use super::state::{ActiveTab, CaptureMode, CaptureState, RenderState, TuiState};

// ═══════════════════════════════════════════════════════════════════════
// EXTRA THEME — Neon cyberpunk palette and animation helpers
// ═══════════════════════════════════════════════════════════════════════

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

/// Cycle through the rainbow palette using the animation tick.
fn rainbow(tick: u64) -> Color {
    PALETTE[tick as usize % PALETTE.len()]
}

/// Cycle through the palette with a fixed hue offset.
fn rainbow_off(tick: u64, offset: usize) -> Color {
    PALETTE[(tick as usize + offset) % PALETTE.len()]
}

// Fixed per-tab accent colours (consistent branding per section)
const C_CAPTURE: Color = Color::Rgb(255, 80,  140); // hot pink
const C_RENDER:  Color = Color::Rgb(0,   220, 255); // electric cyan
const C_SESSION: Color = Color::Rgb(80,  255,  80); // neon green
const C_DIAG:    Color = Color::Rgb(255, 200,   0); // gold
const C_DIM:     Color = Color::Rgb(70,   70,  90); // dim purple-grey

fn tab_accent(tab: ActiveTab) -> Color {
    match tab {
        ActiveTab::Capture     => C_CAPTURE,
        ActiveTab::Render      => C_RENDER,
        ActiveTab::Sessions    => C_SESSION,
        ActiveTab::Diagnostics => C_DIAG,
    }
}

const SPINNER_FRAMES: [&str; 10] = ["\u{280b}", "\u{2819}", "\u{2839}", "\u{2838}", "\u{283c}", "\u{2834}", "\u{2826}", "\u{2827}", "\u{2807}", "\u{280f}"];

fn neon_spinner(tick: u64) -> &'static str {
    SPINNER_FRAMES[tick as usize % SPINNER_FRAMES.len()]
}

/// Returns true for half the ~700 ms pulse cycle (14 ticks × 50 ms).
fn pulse_on(tick: u64) -> bool {
    tick % 14 < 7
}

/// Render `data` as a row of Unicode block characters (\u2581\u2582\u2583\u2584\u2585\u2586\u2587\u2588),
/// scaled to the rolling maximum, fitting within `max_chars` columns.
fn sparkline_str(data: &[u64], max_chars: usize) -> String {
    const BARS: [char; 9] = [' ', '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}', '\u{2588}'];
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
        draw_extra_main(f, size, state);
        return;
    }

    // Main layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tab headers
            Constraint::Min(3),    // Content
            Constraint::Length(1), // Status bar
            Constraint::Length(1), // Footer keys
        ])
        .split(size);

    let is_extra = state.config.theme == Theme::Extra;

    // Tab Headers
    let titles = if is_extra && state.supports_unicode {
        vec![
            "📸 Capture".to_string(),
            "🎬 Render".to_string(),
            "📂 Sessions".to_string(),
            "🛠 Diagnostics".to_string(),
        ]
    } else {
        vec![
            "[1] Capture".to_string(),
            "[2] Render".to_string(),
            "[3] Sessions".to_string(),
            "[4] Diagnostics".to_string(),
        ]
    };
    let selected_session_name = if state.sessions.is_empty() {
        "None".to_string()
    } else {
        state.sessions[state.active_session_index].name.clone()
    };
    let lib_name = state.resolved_library_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Timelapse");
    
    let library_emoji = if is_extra && state.supports_unicode { " ⚡ " } else { " | " };
    let session_emoji = if is_extra && state.supports_unicode { " 🎯 " } else { " | " };
    let header_title = format!(
        " Timelapse TUI{}Lib: {}{}Session: {} ",
        library_emoji, lib_name, session_emoji, selected_session_name
    );

    let border_type = if is_extra { BorderType::Rounded } else { BorderType::Plain };
    let header_border_color = if is_extra { Color::Magenta } else { Color::Gray };
    let highlight_color = if is_extra { Color::LightMagenta } else { Color::Cyan };

    let tab_block = Block::default()
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(Style::default().fg(header_border_color))
        .title(Span::styled(header_title, Style::default().fg(if is_extra { Color::Cyan } else { Color::White }).add_modifier(Modifier::BOLD)));

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
        ActiveTab::Capture => draw_capture_tab(f, chunks[1], state),
        ActiveTab::Render => draw_render_tab(f, chunks[1], state),
        ActiveTab::Sessions => draw_sessions_tab(f, chunks[1], state),
        ActiveTab::Diagnostics => draw_diagnostics_tab(f, chunks[1], state),
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
        draw_confirm_modal(f, size, &state.sessions[index]);
    }

    // Change Library Modal Overlay
    if let Some(ref input_str) = state.change_library_input {
        draw_library_modal(f, size, input_str);
    }

    // Confirm Render Modal Overlay
    if let Some(ref plan) = state.confirm_render_plan {
        draw_render_confirm_modal(f, size, plan);
    }
}

fn draw_capture_tab(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let is_extra = state.config.theme == Theme::Extra;
    let use_unicode = state.supports_unicode;

    // Left Panel: Settings
    let display_str = match state.capture_display {
        DisplayTarget::All => "All Connected Displays",
        DisplayTarget::Primary => "Primary Display Only",
    };
    let mode_str = match state.capture_mode {
        CaptureMode::NewSession => "Create New Timestamped Session".to_string(),
        CaptureMode::Append => {
            if state.sessions.is_empty() {
                "Append (Disabled: No sessions found)".to_string()
            } else {
                format!("Append to Session: {}", state.sessions[state.active_session_index].name)
            }
        }
    };

    let lib_label = if is_extra && use_unicode { "📂 Library Root:      " } else { "  Library Root:       " };
    let int_label = if is_extra && use_unicode { "⏱  Interval:         " } else { "  Interval:           " };
    let tgt_label = if is_extra && use_unicode { "🖥  Capture Target:   " } else { "  Capture Target:     " };
    let mod_label = if is_extra && use_unicode { "⚙  Capture Mode:     " } else { "  Capture Mode:       " };

    let mut settings_lines = vec![
        Line::raw(""),
        Line::from(vec![
            Span::raw(lib_label),
            Span::raw(state.resolved_library_path.to_string_lossy().into_owned()),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::raw(int_label),
            Span::styled(format!("{}s", state.capture_interval.as_secs()), Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("  (Use [Up/Down] to adjust)"),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::raw(tgt_label),
            Span::styled(display_str, Style::default().add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::raw("                      (Use [D] to toggle display mode)"),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::raw(mod_label),
            Span::styled(mode_str, Style::default().add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::raw("                      (Use [A] to toggle capture mode)"),
        ]),
    ];

    // Check for interval mismatch warning
    if state.capture_mode == CaptureMode::Append && !state.sessions.is_empty() {
        let session = &state.sessions[state.active_session_index];
        if let Some(ref metadata) = session.metadata {
            if metadata.interval_seconds != state.capture_interval.as_secs() {
                settings_lines.push(Line::raw(""));
                let warning_symbol = if is_extra && use_unicode { "⚠️" } else { "⚠" };
                settings_lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {} WARNING: Interval mismatch! Session uses {}s.", warning_symbol, metadata.interval_seconds),
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    ),
                ]));
            }
        }
    }

    let settings_border_color = if is_extra { Color::Magenta } else { Color::Gray };
    let settings_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if is_extra { BorderType::Rounded } else { BorderType::Plain })
        .border_style(Style::default().fg(settings_border_color))
        .title(Span::styled(" Settings ", Style::default().fg(if is_extra { Color::Cyan } else { Color::White }).add_modifier(Modifier::BOLD)));

    let settings_panel = Paragraph::new(settings_lines).block(settings_block);
    f.render_widget(settings_panel, chunks[0]);

    // Right Panel: Capture Status
    let (status_title, status_style, status_desc) = match &state.capture_state {
        CaptureState::Idle => (
            "● IDLE",
            Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD),
            "\n\n  Press [Space] to start capturing screenshots.".to_string(),
        ),
        CaptureState::Starting => (
            "● STARTING...",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            "\n\n  Initializing screen capture backend...".to_string(),
        ),
        CaptureState::Capturing { frames_collected, .. } => (
            "● RECORDING",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            format!(
                "\n\n  Screenshots are being collected.\n\n  Frames collected: {}\n\n  Press [Space] to stop capturing.",
                frames_collected
            ),
        ),
        CaptureState::Error(err) => (
            "● ERROR",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            format!("\n\n  Capture failed:\n\n  {}", err),
        ),
    };

    let status_text = format!(
        "\n  Status: {}\n{}",
        status_title, status_desc
    );

    let control_border_color = match &state.capture_state {
        CaptureState::Capturing { .. } => Color::Red,
        CaptureState::Starting => Color::Yellow,
        _ => if is_extra { Color::Cyan } else { Color::Gray },
    };

    let control_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if is_extra { BorderType::Rounded } else { BorderType::Plain })
        .border_style(Style::default().fg(control_border_color))
        .title(Span::styled(" Capture Control ", Style::default().fg(if is_extra { Color::Cyan } else { Color::White }).add_modifier(Modifier::BOLD)));

    let status_panel = Paragraph::new(status_text)
        .style(status_style)
        .block(control_block);
    f.render_widget(status_panel, chunks[1]);
}

fn draw_render_tab(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let is_extra = state.config.theme == Theme::Extra;
    let use_unicode = state.supports_unicode;

    // Left Panel: Settings
    let target_name = if state.sessions.is_empty() {
        "latest (no sessions found)".to_string()
    } else if state.active_session_index < state.sessions.len() {
        state.sessions[state.active_session_index].name.clone()
    } else {
        "latest".to_string()
    };

    let frame_count = if state.sessions.is_empty() {
        0
    } else {
        state.sessions[state.active_session_index]
            .frames
            .as_ref()
            .map_or(0, |f| f.frame_count)
    };

    let mut exclude_count = 0;
    if !state.sessions.is_empty() && state.active_session_index < state.sessions.len() {
        let session = &state.sessions[state.active_session_index];
        let mut exclusions = Vec::new();
        let exclude_file_path = session.path.join("exclude.txt");
        if exclude_file_path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&exclude_file_path) {
                if let Ok(parsed) = parse_exclusions(&content) {
                    exclusions = parsed;
                }
            }
        }
        if let Some(ref sequence) = session.frames {
            exclude_count = exclusions.iter()
                .filter(|&&x| x >= sequence.start_number && x <= sequence.end_number)
                .count();
        }
    }

    let actual_frame_count = frame_count.saturating_sub(exclude_count);

    let duration_desc = if actual_frame_count > 0 && state.render_fps > 0 {
        let secs = actual_frame_count as f64 / state.render_fps as f64;
        if secs < 60.0 {
            format!("{:.1}s", secs)
        } else {
            let mins = (secs / 60.0).floor() as u32;
            let remaining_secs = secs % 60.0;
            format!("{}m {:.1}s", mins, remaining_secs)
        }
    } else {
        "0.0s".to_string()
    };

    let (interval_desc, speed_desc) = if state.sessions.is_empty() {
        ("N/A".to_string(), "N/A".to_string())
    } else {
        let session = &state.sessions[state.active_session_index];
        if let Some(ref m) = session.metadata {
            let interval = m.interval_seconds;
            let speed = state.render_fps as u64 * interval;
            (format!("{}s", interval), format!("{}x", speed))
        } else {
            ("Unknown".to_string(), "Unknown".to_string())
        }
    };

    let lbl_target = if is_extra && use_unicode { "🎯 Render Target:     " } else { "  Render Target:      " };
    let lbl_fps = if is_extra && use_unicode { "⚡  Render FPS:        " } else { "  Render FPS:         " };
    let lbl_total = if is_extra && use_unicode { "🎞  Total Frames:      " } else { "  Total Frames:       " };
    let lbl_excl = if is_extra && use_unicode { "🚫 Exclusions:        " } else { "  Exclusions:         " };
    let lbl_act = if is_extra && use_unicode { "🎬  Render Frames:     " } else { "  Render Frames:      " };
    let lbl_dur = if is_extra && use_unicode { "⏱  Est. Duration:     " } else { "  Est. Duration:      " };
    let lbl_int = if is_extra && use_unicode { "⏱  Cap. Interval:     " } else { "  Cap. Interval:      " };
    let lbl_spd = if is_extra && use_unicode { "🚀  Playback Speed:    " } else { "  Playback Speed:     " };

    let mut settings_text = format!(
        "\n{}{}\n                      (Selected from Sessions list tab)\n\n{}{} fps  (Use [Up/Down] to adjust)\n\n{}{}",
        lbl_target, target_name,
        lbl_fps, state.render_fps,
        lbl_total, frame_count
    );

    if exclude_count > 0 {
        settings_text.push_str(&format!(
            "\n{}{}\n{}{}",
            lbl_excl, exclude_count,
            lbl_act, actual_frame_count
        ));
    }

    settings_text.push_str(&format!(
        "\n{}{}\n{}{}\n{}{}",
        lbl_dur, duration_desc,
        lbl_int, interval_desc,
        lbl_spd, speed_desc
    ));

    let settings_border_color = if is_extra { Color::Magenta } else { Color::Gray };
    let settings_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if is_extra { BorderType::Rounded } else { BorderType::Plain })
        .border_style(Style::default().fg(settings_border_color))
        .title(Span::styled(" Render Settings ", Style::default().fg(if is_extra { Color::Cyan } else { Color::White }).add_modifier(Modifier::BOLD)));

    let settings_panel = Paragraph::new(settings_text).block(settings_block);
    f.render_widget(settings_panel, chunks[0]);

    // Right Panel: Rendering Status
    let (status_title, status_style, status_desc) = match &state.render_state {
        RenderState::Idle => (
            "● READY",
            Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD),
            "\n\n  Press [Enter] or [R] to start rendering target to MP4.".to_string(),
        ),
        RenderState::Rendering(msg) => (
            "● RENDERING",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            format!("\n\n  FFmpeg rendering in progress...\n\n  {}", msg),
        ),
        RenderState::Success(msg) => (
            "● COMPLETED",
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            format!("\n\n  {}", msg),
        ),
        RenderState::Error(err) => (
            "● ERROR",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            format!("\n\n  Render failed:\n\n  {}", err),
        ),
    };

    let status_text = format!(
        "\n  Status: {}\n{}",
        status_title, status_desc
    );

    let control_border_color = match &state.render_state {
        RenderState::Rendering(_) => Color::Yellow,
        RenderState::Success(_) => Color::Green,
        RenderState::Error(_) => Color::Red,
        _ => if is_extra { Color::Cyan } else { Color::Gray },
    };

    let control_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if is_extra { BorderType::Rounded } else { BorderType::Plain })
        .border_style(Style::default().fg(control_border_color))
        .title(Span::styled(" Rendering Control ", Style::default().fg(if is_extra { Color::Cyan } else { Color::White }).add_modifier(Modifier::BOLD)));

    let status_panel = Paragraph::new(status_text)
        .style(status_style)
        .block(control_block);
    f.render_widget(status_panel, chunks[1]);
}

fn draw_sessions_tab(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    let is_extra = state.config.theme == Theme::Extra;
    let use_unicode = state.supports_unicode;

    if state.sessions.is_empty() {
        let panel = Paragraph::new("\n  No sessions found in the library.\n\n  Run Capture to create a new session.")
            .block(Block::default()
                .borders(Borders::ALL)
                .border_type(if is_extra { BorderType::Rounded } else { BorderType::Plain })
                .border_style(Style::default().fg(if is_extra { Color::Magenta } else { Color::Gray }))
                .title(Span::styled(" Sessions List ", Style::default().fg(if is_extra { Color::Cyan } else { Color::White }).add_modifier(Modifier::BOLD))));
        f.render_widget(panel, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(9)])
        .split(area);

    let header_cells = vec!["Session Name", "Frames", "Videos", "Directory Path"];
    let header = Row::new(header_cells)
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .height(1);

    let rows: Vec<Row> = state
        .sessions
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let frames_count = s.frames.as_ref().map_or("0".to_string(), |f| f.frame_count.to_string());
            let videos_count = s.videos.len().to_string();
            let is_cursor = i == state.cursor_session_index;
            let is_active = i == state.active_session_index;

            let marker = if state.supports_unicode {
                if is_cursor && is_active {
                    "▶ ● "
                } else if is_cursor {
                    "▶   "
                } else if is_active {
                    "  ● "
                } else {
                    "    "
                }
            } else {
                if is_cursor && is_active {
                    "> * "
                } else if is_cursor {
                    ">   "
                } else if is_active {
                    "  * "
                } else {
                    "    "
                }
            };
            let name = format!("{}{}", marker, s.name);

            let row = Row::new(vec![
                name,
                frames_count,
                videos_count,
                s.path.display().to_string(),
            ]);

            if is_cursor {
                let bg_color = if is_extra { Color::Magenta } else { Color::DarkGray };
                row.style(Style::default().bg(bg_color).fg(Color::White))
            } else {
                row
            }
        })
        .collect();

    let table_title = if is_extra && use_unicode {
        " Sessions List (Press [Space/Enter] to Select) "
    } else {
        " Sessions List (Press [Space/Enter] to Select for Capture/Render) "
    };

    let table_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if is_extra { BorderType::Rounded } else { BorderType::Plain })
        .border_style(Style::default().fg(if is_extra { Color::Magenta } else { Color::Gray }))
        .title(Span::styled(table_title, Style::default().fg(if is_extra { Color::Cyan } else { Color::White }).add_modifier(Modifier::BOLD)));

    let table = Table::new(
        rows,
        [
            Constraint::Length(30),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Min(40),
        ],
    )
    .header(header)
    .block(table_block);

    f.render_widget(table, chunks[0]);

    let selected = &state.sessions[state.cursor_session_index];

    let lbl_start = if is_extra && use_unicode { "📅 Started At:          " } else { "  Started At:            " };
    let lbl_int = if is_extra && use_unicode { "⏱  Capture Interval:   " } else { "  Capture Interval:      " };
    let lbl_disp = if is_extra && use_unicode { "🖥  Display Mode:       " } else { "  Display Mode:          " };
    let lbl_back = if is_extra && use_unicode { "🔧  Capture Backend:    " } else { "  Capture Backend:       " };
    let lbl_idx = if is_extra && use_unicode { "🔢  Frame Index Start:  " } else { "  Frame Index Start:     " };
    let lbl_excl = if is_extra && use_unicode { "🚫 Exclusions:          " } else { "  Exclusions:            " };

    let mut metadata_text = match &selected.metadata {
        Some(m) => {
            format!(
                "{}{}\n\
                 {}{}s\n\
                 {}{:?}\n\
                 {}{}\n\
                 {}{}  (Padding: {})",
                lbl_start, m.started_at.format("%Y-%m-%d %H:%M:%S"),
                lbl_int, m.interval_seconds,
                lbl_disp, m.display,
                lbl_back, m.capture_backend,
                lbl_idx, m.frame_start, m.frame_padding
            )
        }
        None => {
            if let Some(ref err) = selected.metadata_error {
                format!("  Error loading session.toml: {}", err)
            } else {
                "  No session.toml metadata file found.".to_string()
            }
        }
    };

    let mut exclusions = Vec::new();
    let exclude_file_path = selected.path.join("exclude.txt");
    if exclude_file_path.is_file() {
        if let Ok(content) = std::fs::read_to_string(&exclude_file_path) {
            if let Ok(parsed) = parse_exclusions(&content) {
                exclusions = parsed;
            }
        }
    }

    if !exclusions.is_empty() {
        metadata_text.push_str(&format!(
            "\n{}{}",
            lbl_excl,
            exclusions.len()
        ));
        metadata_text.push_str(" frame(s) active");
    }

    let metadata_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if is_extra { BorderType::Rounded } else { BorderType::Plain })
        .border_style(Style::default().fg(if is_extra { Color::Cyan } else { Color::Gray }))
        .title(Span::styled(" Selected Session Metadata ", Style::default().fg(if is_extra { Color::Cyan } else { Color::White }).add_modifier(Modifier::BOLD)));

    let metadata_panel = Paragraph::new(metadata_text).block(metadata_block);
    f.render_widget(metadata_panel, chunks[1]);
}

fn draw_diagnostics_tab(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    let is_extra = state.config.theme == Theme::Extra;
    let use_unicode = state.supports_unicode;

    let checks = match &state.diagnostics {
        Some(c) => c,
        None => {
            let panel = Paragraph::new("\n  Running diagnostics checks...")
                .block(Block::default()
                    .borders(Borders::ALL)
                    .border_type(if is_extra { BorderType::Rounded } else { BorderType::Plain })
                    .border_style(Style::default().fg(if is_extra { Color::Magenta } else { Color::Gray }))
                    .title(Span::styled(" System Diagnostics ", Style::default().fg(if is_extra { Color::Cyan } else { Color::White }).add_modifier(Modifier::BOLD))));
            f.render_widget(panel, area);
            return;
        }
    };

    let header_cells = vec!["Status", "Diagnostic Check", "Result Detail"];
    let header = Row::new(header_cells)
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .height(1);

    let rows: Vec<Row> = checks
        .iter()
        .map(|check| {
            let (status_str, status_style) = match check.status {
                CheckStatus::Ok => (
                    if is_extra && use_unicode { "  ✅ OK   " } else { "  [ok] " },
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                ),
                CheckStatus::Warn => (
                    if is_extra && use_unicode { "  ⚠️ WARN " } else { "  [warn]" },
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ),
                CheckStatus::Error => (
                    if is_extra && use_unicode { "  ❌ ERR  " } else { "  [error]" },
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
            };

            Row::new(vec![
                Cell::from(status_str).style(status_style),
                Cell::from(check.name.clone()).style(Style::default().add_modifier(Modifier::BOLD)),
                Cell::from(check.message.clone()),
            ])
        })
        .collect();

    let table_title = if is_extra && use_unicode {
        " System Diagnostics (Press [D] to rerun) "
    } else {
        " System Diagnostics (Press [D] to rerun) "
    };

    let table_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if is_extra { BorderType::Rounded } else { BorderType::Plain })
        .border_style(Style::default().fg(if is_extra { Color::Magenta } else { Color::Gray }))
        .title(Span::styled(table_title, Style::default().fg(if is_extra { Color::Cyan } else { Color::White }).add_modifier(Modifier::BOLD)));

    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length(25),
            Constraint::Min(50),
        ],
    )
    .header(header)
    .block(table_block);

    f.render_widget(table, area);
}

fn draw_confirm_modal(f: &mut ratatui::Frame, screen_area: Rect, session: &SessionSummary) {
    let modal_area = centered_rect(65, 32, screen_area);
    f.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Clean Confirmation ")
        .border_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));

    // Dynamic warning: If terminal size/modal area size is too small to render options
    if modal_area.height < 12 || modal_area.width < 50 {
        let warning_text = "\n  ⚠ Warning:\n  Terminal window is too small\n  to display the clean options.\n\n  Please enlarge your window\n  or decrease your font size.";
        let paragraph = Paragraph::new(warning_text)
            .block(block)
            .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
        f.render_widget(paragraph, modal_area);
        return;
    }

    let frames_count = session.frames.as_ref().map_or(0, |f| f.frame_count);
    let videos_count = session.videos.len();

    // Dynamic warning: If the session has no files to clean
    let confirm_text = if frames_count == 0 && videos_count == 0 {
        format!(
            "\n  Clean session files from: {}\n\n  ⚠ Warning: This session is already clean.\n  No frames or videos were found to delete.\n\n  Press [Any key] to return.",
            session.name
        )
    } else {
        format!(
            "\n  Clean session files from: {}\n\n  Select an action:\n\n    [D] - Dry run (simulates cleaning both)\n    [F] - Delete all screenshots/frames ({} files)\n    [V] - Delete rendered MP4 videos ({} files)\n    [A] - Delete BOTH frames and videos\n\n  Press [Any other key] to cancel.",
            session.name, frames_count, videos_count
        )
    };

    let paragraph = Paragraph::new(confirm_text)
        .block(block)
        .style(Style::default().fg(Color::White));

    f.render_widget(paragraph, modal_area);
}

fn draw_render_confirm_modal(f: &mut ratatui::Frame, screen_area: Rect, plan: &crate::render::RenderPlan) {
    let modal_area = centered_rect(75, 45, screen_area);
    f.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Confirm Render Command ")
        .border_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

    if modal_area.height < 10 || modal_area.width < 50 {
        let warning_text = "\n  Terminal window is too small\n  to display confirmation.";
        let paragraph = Paragraph::new(warning_text)
            .block(block)
            .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
        f.render_widget(paragraph, modal_area);
        return;
    }

    let mut cmd = "ffmpeg".to_string();
    for arg in plan.ffmpeg_args() {
        let arg_str = arg.to_string_lossy();
        if arg_str.contains(' ') || arg_str.is_empty() {
            cmd.push_str(&format!(" \"{}\"", arg_str));
        } else {
            cmd.push_str(&format!(" {}", arg_str));
        }
    }

    let confirm_text = format!(
        "\n  About to execute the following ffmpeg command:\n\n  {}\n\n  Press [Y] or [Enter] to confirm and render,\n  or [Esc]/[N]/[Any other key] to cancel.",
        cmd
    );

    let paragraph = Paragraph::new(confirm_text)
        .block(block)
        .style(Style::default().fg(Color::White))
        .wrap(ratatui::widgets::Wrap { trim: false });

    f.render_widget(paragraph, modal_area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn draw_library_modal(f: &mut ratatui::Frame, screen_area: Rect, input: &str) {
    let modal_area = centered_rect(70, 20, screen_area);
    f.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Change Library Path ")
        .border_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

    let prompt_text = format!(
        "\n  Enter new Timelapse library root path:\n\n  > {}█\n\n  Press [Enter] to confirm, [Esc] to cancel.",
        input
    );
    let paragraph = Paragraph::new(prompt_text)
        .block(block)
        .style(Style::default().fg(Color::White));

    f.render_widget(paragraph, modal_area);
}

fn draw_welcome_screen(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    match state.config.theme {
        Theme::Extra => draw_extra_welcome_neon(f, area, state),
        Theme::Minimal => draw_minimal_welcome(f, area, state),
    }
}

fn draw_minimal_welcome(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
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

#[allow(dead_code)]
fn draw_extra_welcome(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    let background_block = Block::default()
        .style(Style::default().bg(Color::Black));
    f.render_widget(background_block, area);

    let outer_area = centered_rect(95, 85, area);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(outer_area);

    let logo_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(2),
            Constraint::Length(8),
            Constraint::Min(2),
        ])
        .split(chunks[0]);

    let logo = r#"
  ████████ ██ ███    ███ ███████ ██       █████  ██████  ███████ ███████ 
     ██    ██ ████  ████ ██      ██      ██   ██ ██   ██ ██      ██      
     ██    ██ ██ ████ ██ █████   ██      ███████ ██████  ███████ █████   
     ██    ██ ██  ██  ██ ██      ██      ██   ██ ██           ██ ██      
     ██    ██ ██      ██ ███████ ███████ ██   ██ ██      ███████ ███████ 
"#;

    let gradient_colors = vec![
        Color::LightRed,
        Color::Magenta,
        Color::LightMagenta,
        Color::Cyan,
        Color::LightBlue,
    ];

    let mut logo_lines = Vec::new();
    let mut color_idx = 0;
    for line in logo.lines() {
        if !line.trim().is_empty() {
            let color = gradient_colors[color_idx % gradient_colors.len()];
            color_idx += 1;
            logo_lines.push(Line::from(vec![
                Span::styled(line, Style::default().fg(color).add_modifier(Modifier::BOLD))
            ]));
        }
    }
    logo_lines.push(Line::raw(""));
    logo_lines.push(Line::from(vec![
        Span::styled("   ⚡ HIGH PERFORMANCE SCREENSHOT TIMELAPSE ENGINE", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD))
    ]));

    let logo_paragraph = Paragraph::new(logo_lines);
    f.render_widget(logo_paragraph, logo_layout[1]);

    let total_sessions = state.sessions.len();
    let total_frames: usize = state.sessions.iter()
        .map(|s| s.frames.as_ref().map_or(0, |f| f.frame_count))
        .sum();
    let total_videos: usize = state.sessions.iter()
        .map(|s| s.videos.len())
        .sum();
    
    let ffmpeg_status = if let Some(ref checks) = state.diagnostics {
        checks.iter()
            .find(|c| c.name.to_lowercase().contains("ffmpeg"))
            .map(|c| {
                if c.status.to_string().to_lowercase().contains("ok") {
                    "Ready"
                } else {
                    "Error"
                }
            })
            .unwrap_or("Ready")
    } else {
        "Ready"
    };

    let dashboard_lines = vec![
        Line::from(vec![
            Span::styled("TIMELAPSE TUI OVERVIEW", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        ]),
        Line::from(vec![
            Span::styled("======================", Style::default().fg(Color::DarkGray))
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  Library Path:   ", Style::default().fg(Color::Magenta)),
            Span::raw(state.resolved_library_path.to_string_lossy().into_owned()),
        ]),
        Line::from(vec![
            Span::styled("  Total Sessions: ", Style::default().fg(Color::Magenta)),
            Span::styled(total_sessions.to_string(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  Total Frames:   ", Style::default().fg(Color::Magenta)),
            Span::styled(total_frames.to_string(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::raw(" captured PNGs"),
        ]),
        Line::from(vec![
            Span::styled("  Total Videos:   ", Style::default().fg(Color::Magenta)),
            Span::styled(total_videos.to_string(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::raw(" rendered MP4s"),
        ]),
        Line::from(vec![
            Span::styled("  ffmpeg Status:  ", Style::default().fg(Color::Magenta)),
            Span::styled(ffmpeg_status, Style::default().fg(if ffmpeg_status == "Ready" { Color::Green } else { Color::Red }).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  Version:        ", Style::default().fg(Color::Magenta)),
            Span::styled(env!("CARGO_PKG_VERSION"), Style::default().fg(Color::Green)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("SYSTEM CONTROLS QUICK REFERENCE", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        ]),
        Line::from(vec![
            Span::styled("-------------------------------", Style::default().fg(Color::DarkGray))
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  [1] - [4] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(" Switch between tabs directly"),
        ]),
        Line::from(vec![
            Span::styled("  [Tab]     ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(" Cycle tabs left-to-right"),
        ]),
        Line::from(vec![
            Span::styled("  [T]       ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(" Toggle extra/minimal theme"),
        ]),
        Line::from(vec![
            Span::styled("  [Q]       ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(" Gracefully exit application"),
        ]),
        Line::raw(""),
        Line::raw(""),
        Line::from(vec![
            Span::styled(
                if state.supports_unicode {
                    "    ▶ PRESS ANY KEY TO ENTER DASHBOARD ◀"
                } else {
                    "    > PRESS ANY KEY TO ENTER DASHBOARD <"
                },
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            )
        ]),
    ];

    let dashboard_paragraph = Paragraph::new(dashboard_lines)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Magenta))
            .title(Span::styled(" SYSTEM DASHBOARD ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))))
        .style(Style::default().fg(Color::White));

    f.render_widget(dashboard_paragraph, chunks[1]);
}

// ═══════════════════════════════════════════════════════════════════════════════
// EXTRA THEME — Spectacular neon welcome screen
// ═══════════════════════════════════════════════════════════════════════════════

fn draw_extra_welcome_neon(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    // Deep dark fill
    f.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(4, 4, 14))),
        area,
    );

    let border_col = rainbow(state.tick / 4);
    let title_col  = rainbow_off(state.tick / 4, 4);

    // Outer animated rainbow border
    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_col))
        .title(Span::styled(
            format!(" \u{26a1} TIMELAPSE v{} \u{26a1} ", env!("CARGO_PKG_VERSION")),
            Style::default().fg(title_col).add_modifier(Modifier::BOLD),
        ));
    let inner = outer_block.inner(area);
    f.render_widget(outer_block, area);

    if inner.height < 12 {
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // ASCII logo (gradient colours)
            Constraint::Length(1), // tagline
            Constraint::Length(1), // separator rule
            Constraint::Min(4),    // stats panel + controls panel
            Constraint::Length(1), // pulsing "press any key" prompt
        ])
        .split(inner);

    // ── Gradient ASCII logo ───────────────────────────────────────────────────
    const LOGO: [&str; 5] = [
        "  \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} \u{2588}\u{2588} \u{2588}\u{2588}\u{2588}    \u{2588}\u{2588}\u{2588} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} \u{2588}\u{2588}       \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}  \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}  \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
        "     \u{2588}\u{2588}    \u{2588}\u{2588} \u{2588}\u{2588}\u{2588}\u{2588}  \u{2588}\u{2588}\u{2588}\u{2588} \u{2588}\u{2588}      \u{2588}\u{2588}      \u{2588}\u{2588}   \u{2588}\u{2588} \u{2588}\u{2588}   \u{2588}\u{2588} \u{2588}\u{2588}      \u{2588}\u{2588}      ",
        "     \u{2588}\u{2588}    \u{2588}\u{2588} \u{2588}\u{2588} \u{2588}\u{2588}\u{2588}\u{2588} \u{2588}\u{2588} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}  \u{2588}\u{2588}      \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}  \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}   ",
        "     \u{2588}\u{2588}    \u{2588}\u{2588} \u{2588}\u{2588}  \u{2588}\u{2588}  \u{2588}\u{2588} \u{2588}\u{2588}      \u{2588}\u{2588}      \u{2588}\u{2588}   \u{2588}\u{2588} \u{2588}\u{2588}           \u{2588}\u{2588} \u{2588}\u{2588}      ",
        "     \u{2588}\u{2588}    \u{2588}\u{2588} \u{2588}\u{2588}      \u{2588}\u{2588} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} \u{2588}\u{2588}   \u{2588}\u{2588} \u{2588}\u{2588}     \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
    ];

    // Each logo row uses a different colour that slowly rotates with `tick`
    let logo_text: Vec<Line> = LOGO.iter().enumerate().map(|(i, &line)| {
        let col = rainbow_off(state.tick / 4, i * 2);
        Line::from(Span::styled(line, Style::default().fg(col).add_modifier(Modifier::BOLD)))
    }).collect();

    f.render_widget(
        Paragraph::new(logo_text).alignment(Alignment::Center),
        rows[0],
    );

    // ── Tagline ───────────────────────────────────────────────────────────────
    let tag_col = rainbow_off(state.tick / 4, 7);
    f.render_widget(
        Paragraph::new(Span::styled(
            "\u{26a1}  SESSION-BASED SCREENSHOT TIMELAPSE ENGINE  \u{26a1}",
            Style::default().fg(tag_col),
        )).alignment(Alignment::Center),
        rows[1],
    );

    // ── Separator ─────────────────────────────────────────────────────────────
    let sep_width = inner.width as usize;
    f.render_widget(
        Paragraph::new(Span::styled(
            "\u{2500}".repeat(sep_width),
            Style::default().fg(Color::Rgb(35, 35, 55)),
        )),
        rows[2],
    );

    // ── Stats + Controls side by side ────────────────────────────────────────
    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(rows[3]);

    // Library stats
    let total_sessions = state.sessions.len();
    let total_frames: usize = state.sessions.iter()
        .map(|s| s.frames.as_ref().map_or(0, |f| f.frame_count))
        .sum();
    let total_videos: usize = state.sessions.iter().map(|s| s.videos.len()).sum();
    let ffmpeg_ok = state.diagnostics.as_ref().map_or(true, |checks| {
        checks.iter()
            .find(|c| c.name.to_lowercase().contains("ffmpeg"))
            .map_or(true, |c| matches!(c.status, CheckStatus::Ok))
    });

    let stat_border = rainbow_off(state.tick / 4, 2);
    let stats_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(stat_border))
        .title(Span::styled(
            " Library ",
            Style::default().fg(stat_border).add_modifier(Modifier::BOLD),
        ));

    let stats_lines = vec![
        Line::from(vec![
            Span::styled("  \u{1f4c1} Sessions  ", Style::default().fg(C_DIM)),
            Span::styled(total_sessions.to_string(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  \u{1f4f8} Frames    ", Style::default().fg(C_DIM)),
            Span::styled(total_frames.to_string(),
                Style::default().fg(C_RENDER).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  \u{1f3ac} Videos    ", Style::default().fg(C_DIM)),
            Span::styled(total_videos.to_string(),
                Style::default().fg(C_SESSION).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  \u{1f527} ffmpeg    ", Style::default().fg(C_DIM)),
            Span::styled(
                if ffmpeg_ok { "\u{2705} Ready" } else { "\u{2716}  Missing" },
                Style::default()
                    .fg(if ffmpeg_ok { C_SESSION } else { Color::Rgb(255, 50, 50) })
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    f.render_widget(Paragraph::new(stats_lines).block(stats_block), panels[0]);

    // Quick controls
    let ctrl_border = rainbow_off(state.tick / 4, 8);
    let ctrl_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ctrl_border))
        .title(Span::styled(
            " Controls ",
            Style::default().fg(ctrl_border).add_modifier(Modifier::BOLD),
        ));

    let ks = Style::default().fg(Color::Rgb(255, 210, 0)).add_modifier(Modifier::BOLD);
    let vs = Style::default().fg(Color::Rgb(155, 155, 175));

    let ctrl_lines = vec![
        Line::from(vec![
            Span::styled("  [1\u{2013}4]", ks), Span::styled("  switch tabs          ", vs),
            Span::styled("[Space]", ks), Span::styled("  start/stop capture", vs),
        ]),
        Line::from(vec![
            Span::styled("  [Tab]", ks), Span::styled("  cycle tabs           ", vs),
            Span::styled("[Enter]", ks), Span::styled("  start render", vs),
        ]),
        Line::from(vec![
            Span::styled("  [T]  ", ks), Span::styled("  toggle Extra/Minimal ", vs),
            Span::styled("[Q]    ", ks), Span::styled("  quit", vs),
        ]),
        Line::from(vec![
            Span::styled("  [\u{2191}\u{2193}]  ", ks), Span::styled("  interval / fps       ", vs),
            Span::styled("[L]    ", ks), Span::styled("  change library", vs),
        ]),
    ];

    f.render_widget(Paragraph::new(ctrl_lines).block(ctrl_block), panels[1]);

    // ── Pulsing "press any key" prompt ────────────────────────────────────────
    let prompt_col = if pulse_on(state.tick) { rainbow(state.tick / 3) } else { C_DIM };
    f.render_widget(
        Paragraph::new(Span::styled(
            "  \u{25b6}\u{25b6}  PRESS ANY KEY TO ENTER THE DASHBOARD  \u{25c4}\u{25c4}  ",
            Style::default().fg(prompt_col).add_modifier(Modifier::BOLD),
        )).alignment(Alignment::Center),
        rows[4],
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// EXTRA THEME — Main UI: animated tab bar + content dispatch + overlay modals
// ═══════════════════════════════════════════════════════════════════════════════

fn draw_extra_main(f: &mut ratatui::Frame, size: Rect, state: &TuiState) {
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
        " \u{26a1} TIMELAPSE  \u{00b7}  lib: {}  \u{00b7}  session: {} ",
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
        "  \u{1f4f8} CAPTURE  ".to_string(),
        "  \u{1f3ac} RENDER   ".to_string(),
        "  \u{1f4c2} SESSIONS ".to_string(),
        "  \u{1f6e0} DIAG     ".to_string(),
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
        ActiveTab::Capture     => draw_extra_capture(f, chunks[1], state),
        ActiveTab::Render      => draw_extra_render(f, chunks[1], state),
        ActiveTab::Sessions    => draw_extra_sessions(f, chunks[1], state),
        ActiveTab::Diagnostics => draw_extra_diagnostics(f, chunks[1], state),
    }

    // ── Pulsing status message ────────────────────────────────────────────────
    let status_text = if let Some((ref msg, ts)) = state.status_message {
        if ts.elapsed().unwrap_or(Duration::ZERO) < Duration::from_secs(4) {
            format!(" \u{25c8}  {}  \u{25c8} ", msg)
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
            " [Tab] tabs  \u{00b7}  [Space] capture  \u{00b7}  [\u{2191}\u{2193}] interval  \u{00b7}  [D] display  \u{00b7}  [A] mode  \u{00b7}  [L] library  \u{00b7}  [Q] quit",
        ActiveTab::Render =>
            " [Tab] tabs  \u{00b7}  [Enter/R] render  \u{00b7}  [\u{2191}\u{2193}] fps  \u{00b7}  [L] library  \u{00b7}  [Q] quit",
        ActiveTab::Sessions =>
            " [Tab] tabs  \u{00b7}  [\u{2191}\u{2193}] select  \u{00b7}  [Enter] activate  \u{00b7}  [O] open  \u{00b7}  [C] clean  \u{00b7}  [A] append  \u{00b7}  [U] refresh  \u{00b7}  [Q] quit",
        ActiveTab::Diagnostics =>
            " [Tab] tabs  \u{00b7}  [D/U] refresh  \u{00b7}  [L] library  \u{00b7}  [Q] quit",
    };
    let footer_col = rainbow_off(state.tick / 5, 7);
    f.render_widget(
        Paragraph::new(footer_text).style(Style::default().fg(footer_col)),
        chunks[3],
    );

    // ── Modal overlays ────────────────────────────────────────────────────────
    if let Some(idx) = state.confirm_clean_index {
        draw_extra_clean_modal(f, size, &state.sessions[idx], state.tick);
    }
    if let Some(ref input) = state.change_library_input {
        draw_extra_library_modal(f, size, input, state.tick);
    }
    if let Some(ref plan) = state.confirm_render_plan {
        draw_extra_render_confirm_modal(f, size, plan, state.tick);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// EXTRA THEME — Capture tab
// ═══════════════════════════════════════════════════════════════════════════════

fn draw_extra_capture(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
        .split(area);

    let border_anim = rainbow_off(state.tick / 3, 0);

    // ── Left: Capture settings ────────────────────────────────────────────────
    let display_str = match state.capture_display {
        DisplayTarget::All     => "All Displays",
        DisplayTarget::Primary => "Primary Only",
    };
    let mode_str = match &state.capture_mode {
        CaptureMode::NewSession => "New Timestamped Session".to_string(),
        CaptureMode::Append => {
            if state.sessions.is_empty() {
                "Append  (no sessions found)".to_string()
            } else {
                format!("Append \u{2192} {}", state.sessions[state.active_session_index].name)
            }
        }
    };

    let mut settings_lines = vec![
        Line::raw(""),
        Line::from(vec![
            Span::styled("  \u{1f4c2} Library   ", Style::default().fg(C_DIM)),
            Span::styled(
                state.resolved_library_path.to_string_lossy().into_owned(),
                Style::default().fg(Color::Rgb(0, 255, 180)).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  \u{23f1}  Interval  ", Style::default().fg(C_DIM)),
            Span::styled(
                format!("{}s", state.capture_interval.as_secs()),
                Style::default().fg(Color::Rgb(0, 220, 255)).add_modifier(Modifier::BOLD),
            ),
            Span::styled("   [\u{2191}\u{2193}] adjust", Style::default().fg(C_DIM)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  \u{1f5a5}  Target    ", Style::default().fg(C_DIM)),
            Span::styled(
                display_str,
                Style::default().fg(C_CAPTURE).add_modifier(Modifier::BOLD),
            ),
            Span::styled("   [D] toggle", Style::default().fg(C_DIM)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  \u{2699}  Mode      ", Style::default().fg(C_DIM)),
            Span::styled(
                mode_str,
                Style::default().fg(Color::Rgb(180, 255, 0)).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("               ", Style::default().fg(C_DIM)),
            Span::styled("[A] toggle mode", Style::default().fg(C_DIM)),
        ]),
    ];

    // Animated interval-mismatch warning
    if state.capture_mode == CaptureMode::Append && !state.sessions.is_empty() {
        if let Some(ref m) = state.sessions[state.active_session_index].metadata {
            if m.interval_seconds != state.capture_interval.as_secs() {
                let warn_col = if pulse_on(state.tick) {
                    Color::Rgb(255, 220, 0)
                } else {
                    Color::Rgb(180, 130, 0)
                };
                settings_lines.push(Line::raw(""));
                settings_lines.push(Line::from(Span::styled(
                    format!(
                        "  \u{26a0}  Interval mismatch! Session uses {}s",
                        m.interval_seconds
                    ),
                    Style::default().fg(warn_col).add_modifier(Modifier::BOLD),
                )));
            }
        }
    }

    let settings_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_anim))
        .title(Span::styled(
            " \u{25c8} CAPTURE SETTINGS \u{25c8} ",
            Style::default().fg(C_CAPTURE).add_modifier(Modifier::BOLD),
        ));
    f.render_widget(Paragraph::new(settings_lines).block(settings_block), cols[0]);

    // ── Right: Status (top) + Sparkline (bottom) ──────────────────────────────
    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(5)])
        .split(cols[1]);

    let (status_label, status_border, status_lines): (&str, Color, Vec<Line>) =
        match &state.capture_state {
            CaptureState::Idle => {
                let col = Color::Rgb(55, 55, 85);
                (
                    " \u{25ce} IDLE ",
                    col,
                    vec![
                        Line::raw(""),
                        Line::from(Span::styled(
                            "  Ready to capture screenshots.",
                            Style::default().fg(C_DIM),
                        )),
                        Line::raw(""),
                        Line::from(Span::styled(
                            "  Press [Space] to begin.",
                            Style::default().fg(Color::Gray),
                        )),
                    ],
                )
            }
            CaptureState::Starting => {
                let col = Color::Rgb(255, 210, 0);
                (
                    " \u{25d1} STARTING ",
                    col,
                    vec![
                        Line::raw(""),
                        Line::from(Span::styled(
                            format!("  {} Initializing screen capture backend...", neon_spinner(state.tick)),
                            Style::default().fg(col).add_modifier(Modifier::BOLD),
                        )),
                    ],
                )
            }
            CaptureState::Capturing { frames_collected, .. } => {
                let dot = if pulse_on(state.tick) { "\u{2b24}" } else { "\u{25cb}" };
                let rec_col = if pulse_on(state.tick) {
                    Color::Rgb(255, 40, 40)
                } else {
                    Color::Rgb(180, 10, 10)
                };
                (
                    " \u{25cf} RECORDING ",
                    rec_col,
                    vec![
                        Line::raw(""),
                        Line::from(vec![
                            Span::styled(
                                format!("  {} ", dot),
                                Style::default().fg(rec_col).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                format!("{:>8}  frames captured", frames_collected),
                                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                            ),
                        ]),
                        Line::raw(""),
                        Line::from(Span::styled(
                            format!(
                                "  {} capturing at {}s intervals",
                                neon_spinner(state.tick),
                                state.capture_interval.as_secs()
                            ),
                            Style::default().fg(Color::Rgb(200, 200, 200)),
                        )),
                        Line::raw(""),
                        Line::from(Span::styled(
                            "  Press [Space] to stop.",
                            Style::default().fg(C_DIM),
                        )),
                    ],
                )
            }
            CaptureState::Error(err) => {
                let col = Color::Rgb(255, 50, 50);
                (
                    " \u{2716} ERROR ",
                    col,
                    vec![
                        Line::raw(""),
                        Line::from(Span::styled(
                            "  Capture failed:",
                            Style::default().fg(col).add_modifier(Modifier::BOLD),
                        )),
                        Line::raw(""),
                        Line::from(Span::styled(
                            format!("  {}", err),
                            Style::default().fg(Color::White),
                        )),
                    ],
                )
            }
        };

    let status_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(status_border))
        .title(Span::styled(
            status_label,
            Style::default().fg(status_border).add_modifier(Modifier::BOLD),
        ));
    f.render_widget(Paragraph::new(status_lines).block(status_block), right_rows[0]);

    // Sparkline panel
    let spark_col = if matches!(state.capture_state, CaptureState::Capturing { .. }) {
        rainbow(state.tick / 4)
    } else {
        C_DIM
    };

    let spark_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_DIM))
        .title(Span::styled(" Frame History ", Style::default().fg(spark_col)));

    let spark_w = right_rows[1].width.saturating_sub(4) as usize;
    let spark_para = if state.frame_sparkline.is_empty() {
        Paragraph::new(Span::styled(
            "  \u{2014} no frames captured yet \u{2014}",
            Style::default().fg(C_DIM),
        ))
        .block(spark_block)
    } else {
        let bars = sparkline_str(&state.frame_sparkline, spark_w);
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(bars, Style::default().fg(spark_col)),
        ]))
        .block(spark_block)
    };
    f.render_widget(spark_para, right_rows[1]);
}

// ═══════════════════════════════════════════════════════════════════════════════
// EXTRA THEME — Render tab
// ═══════════════════════════════════════════════════════════════════════════════

fn draw_extra_render(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
        .split(area);

    let border_anim = rainbow_off(state.tick / 3, 4);

    // Compute render stats
    let target_name = state.sessions
        .get(state.active_session_index)
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "latest".into());

    let frame_count = state.sessions
        .get(state.active_session_index)
        .and_then(|s| s.frames.as_ref())
        .map_or(0, |f| f.frame_count);

    let mut exclude_count = 0usize;
    if let Some(session) = state.sessions.get(state.active_session_index) {
        let exc_path = session.path.join("exclude.txt");
        if exc_path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&exc_path) {
                if let Ok(parsed) = parse_exclusions(&content) {
                    if let Some(ref seq) = session.frames {
                        exclude_count = parsed.iter()
                            .filter(|&&x| x >= seq.start_number && x <= seq.end_number)
                            .count();
                    }
                }
            }
        }
    }

    let actual_frames = frame_count.saturating_sub(exclude_count);
    let duration_desc = if actual_frames > 0 && state.render_fps > 0 {
        let secs = actual_frames as f64 / state.render_fps as f64;
        if secs < 60.0 { format!("{:.1}s", secs) }
        else { format!("{}m {:.1}s", (secs / 60.0) as u32, secs % 60.0) }
    } else { "\u{2014}".into() };

    let (interval_str, speed_str) = state.sessions
        .get(state.active_session_index)
        .and_then(|s| s.metadata.as_ref())
        .map(|m| (
            format!("{}s", m.interval_seconds),
            format!("{}\u{00d7}", state.render_fps as u64 * m.interval_seconds),
        ))
        .unwrap_or_else(|| ("\u{2014}".into(), "\u{2014}".into()));

    // Left: settings panel
    let excl_span = if exclude_count > 0 {
        Span::styled(
            format!("  (\u{2212}{} excluded)", exclude_count),
            Style::default().fg(Color::Rgb(200, 120, 0)),
        )
    } else {
        Span::raw("")
    };

    let settings_lines = vec![
        Line::raw(""),
        Line::from(vec![
            Span::styled("  \u{1f3af} Target    ", Style::default().fg(C_DIM)),
            Span::styled(target_name, Style::default().fg(C_RENDER).add_modifier(Modifier::BOLD)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  \u{26a1} FPS       ", Style::default().fg(C_DIM)),
            Span::styled(
                format!("{} fps", state.render_fps),
                Style::default().fg(Color::Rgb(255, 210, 0)).add_modifier(Modifier::BOLD),
            ),
            Span::styled("   [\u{2191}\u{2193}] adjust", Style::default().fg(C_DIM)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  \u{1f39e}  Frames   ", Style::default().fg(C_DIM)),
            Span::styled(frame_count.to_string(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            excl_span,
        ]),
        Line::from(vec![
            Span::styled("  \u{23f1}  Duration  ", Style::default().fg(C_DIM)),
            Span::styled(duration_desc,
                Style::default().fg(Color::Rgb(0, 255, 180)).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  \u{1f500} Interval  ", Style::default().fg(C_DIM)),
            Span::styled(interval_str, Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("  \u{1f680} Speed     ", Style::default().fg(C_DIM)),
            Span::styled(speed_str,
                Style::default().fg(Color::Rgb(180, 255, 0)).add_modifier(Modifier::BOLD)),
        ]),
    ];

    let settings_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_anim))
        .title(Span::styled(
            " \u{25c8} RENDER SETTINGS \u{25c8} ",
            Style::default().fg(C_RENDER).add_modifier(Modifier::BOLD),
        ));
    f.render_widget(Paragraph::new(settings_lines).block(settings_block), cols[0]);

    // Right: status (top) + animated progress bar (bottom)
    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(5)])
        .split(cols[1]);

    let (status_label, status_border, status_lines): (&str, Color, Vec<Line>) =
        match &state.render_state {
            RenderState::Idle => {
                let col = Color::Rgb(55, 55, 85);
                (
                    " \u{25ce} READY ",
                    col,
                    vec![
                        Line::raw(""),
                        Line::from(Span::styled(
                            "  Press [Enter] or [R] to render the selected session to MP4.",
                            Style::default().fg(Color::Gray),
                        )),
                    ],
                )
            }
            RenderState::Rendering(msg) => {
                let col = rainbow(state.tick / 2);
                (
                    " \u{2b21} RENDERING ",
                    col,
                    vec![
                        Line::raw(""),
                        Line::from(vec![
                            Span::styled(
                                format!("  {} ", neon_spinner(state.tick)),
                                Style::default().fg(col).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(msg.as_str(), Style::default().fg(Color::White)),
                        ]),
                    ],
                )
            }
            RenderState::Success(msg) => {
                let col = Color::Rgb(57, 255, 20);
                (
                    " \u{2714} COMPLETE ",
                    col,
                    vec![
                        Line::raw(""),
                        Line::from(Span::styled(
                            "  Render complete! \u{2714}",
                            Style::default().fg(col).add_modifier(Modifier::BOLD),
                        )),
                        Line::raw(""),
                        Line::from(Span::styled(
                            format!("  {}", msg),
                            Style::default().fg(Color::White),
                        )),
                    ],
                )
            }
            RenderState::Error(err) => {
                let col = Color::Rgb(255, 50, 50);
                (
                    " \u{2716} FAILED ",
                    col,
                    vec![
                        Line::raw(""),
                        Line::from(Span::styled(
                            "  Render failed:",
                            Style::default().fg(col).add_modifier(Modifier::BOLD),
                        )),
                        Line::raw(""),
                        Line::from(Span::styled(
                            format!("  {}", err),
                            Style::default().fg(Color::White),
                        )),
                    ],
                )
            }
        };

    let status_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(status_border))
        .title(Span::styled(
            status_label,
            Style::default().fg(status_border).add_modifier(Modifier::BOLD),
        ));
    f.render_widget(Paragraph::new(status_lines).block(status_block), right_rows[0]);

    // Animated indeterminate progress bar
    let bar_inner_w = right_rows[1].width.saturating_sub(10) as usize;
    let (pct, bar_col, prog_label) = match &state.render_state {
        RenderState::Idle => (0usize, C_DIM, "Awaiting".to_string()),
        RenderState::Rendering(_) => {
            // Bounce 0 \u{2192} 100 \u{2192} 0 over 100 ticks (\u{2248}5 s) for an indeterminate animation
            let t = state.tick % 100;
            let p = if t < 50 { t * 2 } else { (100 - t) * 2 };
            (
                p as usize,
                rainbow(state.tick / 2),
                format!("{} Processing\u{2026}", neon_spinner(state.tick)),
            )
        }
        RenderState::Success(_) => (100, Color::Rgb(57, 255, 20), "Complete \u{2714}".into()),
        RenderState::Error(_)   => (0,   Color::Rgb(255, 50, 50), "Failed \u{2716}".into()),
    };

    let filled  = (bar_inner_w * pct / 100).min(bar_inner_w);
    let empty   = bar_inner_w - filled;

    let progress_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(bar_col))
        .title(Span::styled(" Render Progress ", Style::default().fg(bar_col)));

    let progress_lines = vec![
        Line::from(vec![
            Span::styled("  [", Style::default().fg(C_DIM)),
            Span::styled("\u{2588}".repeat(filled), Style::default().fg(bar_col)),
            Span::styled("\u{2591}".repeat(empty),  Style::default().fg(C_DIM)),
            Span::styled(
                format!("] {:3}%", pct),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(Span::styled(
            format!("  {}", prog_label),
            Style::default().fg(bar_col),
        )),
    ];

    f.render_widget(Paragraph::new(progress_lines).block(progress_block), right_rows[1]);
}

// ═══════════════════════════════════════════════════════════════════════════════
// EXTRA THEME — Sessions tab
// ═══════════════════════════════════════════════════════════════════════════════

fn draw_extra_sessions(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    let border_anim = rainbow_off(state.tick / 3, 6);

    if state.sessions.is_empty() {
        f.render_widget(
            Paragraph::new(
                "\n\n  \u{1f4c2}  No sessions found in the library.\n\n  \
                 Switch to Capture and press [Space] to start recording.",
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(border_anim))
                    .title(Span::styled(
                        " \u{25c8} SESSIONS \u{25c8} ",
                        Style::default().fg(C_SESSION).add_modifier(Modifier::BOLD),
                    )),
            ),
            area,
        );
        return;
    }

    let rows_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(10)])
        .split(area);

    // Session table
    let header = Row::new(vec!["", "Session Name", "Frames", "Videos", "Path"])
        .style(Style::default().fg(C_SESSION).add_modifier(Modifier::BOLD))
        .height(1);

    let table_rows: Vec<Row> = state.sessions.iter().enumerate().map(|(i, s)| {
        let is_cursor = i == state.cursor_session_index;
        let is_active = i == state.active_session_index;
        let frames_str = s.frames.as_ref().map_or("0".to_string(), |f| f.frame_count.to_string());
        let videos_str = s.videos.len().to_string();

        let indicator = match (is_cursor, is_active) {
            (true,  true)  => if pulse_on(state.tick) { "\u{25b6} \u{25c9}" } else { "\u{25b7} \u{25cb}" },
            (true,  false) => "\u{25b6}   ",
            (false, true)  => "  \u{25c9} ",
            (false, false) => "    ",
        };

        let name_style = if is_cursor {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };

        let row = Row::new(vec![
            Cell::from(indicator)
                .style(Style::default().fg(if is_cursor { C_SESSION } else { C_DIM })),
            Cell::from(s.name.clone()).style(name_style),
            Cell::from(frames_str).style(Style::default().fg(C_RENDER)),
            Cell::from(videos_str).style(Style::default().fg(Color::Rgb(180, 255, 0))),
            Cell::from(s.path.display().to_string()).style(Style::default().fg(C_DIM)),
        ]);

        if is_cursor { row.style(Style::default().bg(Color::Rgb(22, 8, 32))) } else { row }
    }).collect();

    let table_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_anim))
        .title(Span::styled(
            " \u{25c8} SESSIONS  [\u{2191}\u{2193}] navigate  \u{00b7}  [Enter] select  \u{00b7}  [C] clean  \u{00b7}  [O] open \u{25c8} ",
            Style::default().fg(C_SESSION).add_modifier(Modifier::BOLD),
        ));

    let table = Table::new(table_rows, [
        Constraint::Length(5),
        Constraint::Length(28),
        Constraint::Length(10),
        Constraint::Length(8),
        Constraint::Min(28),
    ])
    .header(header)
    .block(table_block);

    f.render_widget(table, rows_layout[0]);

    // Metadata panel for cursor-highlighted session
    let selected = &state.sessions[state.cursor_session_index];

    let mut meta_lines = vec![Line::raw("")];
    match &selected.metadata {
        Some(m) => {
            meta_lines.push(Line::from(vec![
                Span::styled("  \u{1f4c5} Started    ", Style::default().fg(C_DIM)),
                Span::styled(
                    m.started_at.format("%Y-%m-%d %H:%M:%S").to_string(),
                    Style::default().fg(Color::White),
                ),
            ]));
            meta_lines.push(Line::from(vec![
                Span::styled("  \u{23f1}  Interval   ", Style::default().fg(C_DIM)),
                Span::styled(
                    format!("{}s", m.interval_seconds),
                    Style::default().fg(C_RENDER).add_modifier(Modifier::BOLD),
                ),
                Span::styled("   \u{1f5a5}  Display  ", Style::default().fg(C_DIM)),
                Span::styled(
                    format!("{:?}", m.display),
                    Style::default().fg(Color::Rgb(255, 140, 0)),
                ),
                Span::styled("   \u{1f527} Backend  ", Style::default().fg(C_DIM)),
                Span::styled(m.capture_backend.clone(), Style::default().fg(Color::Gray)),
            ]));
        }
        None => {
            let err = selected.metadata_error.as_deref().unwrap_or("No session.toml metadata found.");
            meta_lines.push(Line::from(Span::styled(
                format!("  \u{26a0}  {}", err),
                Style::default().fg(Color::Yellow),
            )));
        }
    }

    let exc_path = selected.path.join("exclude.txt");
    if exc_path.is_file() {
        if let Ok(content) = std::fs::read_to_string(&exc_path) {
            if let Ok(parsed) = parse_exclusions(&content) {
                if !parsed.is_empty() {
                    meta_lines.push(Line::from(vec![
                        Span::styled("  \u{1f6ab} Exclusions  ", Style::default().fg(C_DIM)),
                        Span::styled(
                            format!("{} frames filtered out", parsed.len()),
                            Style::default().fg(Color::Yellow),
                        ),
                    ]));
                }
            }
        }
    }

    let meta_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_SESSION))
        .title(Span::styled(
            format!(" \u{25c8} METADATA: {} \u{25c8} ", selected.name),
            Style::default().fg(C_SESSION).add_modifier(Modifier::BOLD),
        ));

    f.render_widget(Paragraph::new(meta_lines).block(meta_block), rows_layout[1]);
}

// ═══════════════════════════════════════════════════════════════════════════════
// EXTRA THEME — Diagnostics tab
// ═══════════════════════════════════════════════════════════════════════════════

fn draw_extra_diagnostics(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    let border_anim = rainbow_off(state.tick / 3, 8);

    let checks = match &state.diagnostics {
        Some(c) => c,
        None => {
            f.render_widget(
                Paragraph::new(format!(
                    "\n\n  {} Running diagnostics checks\u{2026}",
                    neon_spinner(state.tick)
                ))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(border_anim))
                        .title(Span::styled(
                            " \u{25c8} DIAGNOSTICS \u{25c8} ",
                            Style::default().fg(C_DIAG).add_modifier(Modifier::BOLD),
                        )),
                )
                .style(Style::default().fg(C_DIAG)),
                area,
            );
            return;
        }
    };

    let rows_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5)])
        .split(area);

    // Overall health summary banner
    let ok_cnt   = checks.iter().filter(|c| matches!(c.status, CheckStatus::Ok)).count();
    let warn_cnt = checks.iter().filter(|c| matches!(c.status, CheckStatus::Warn)).count();
    let err_cnt  = checks.iter().filter(|c| matches!(c.status, CheckStatus::Error)).count();

    let health_col = if err_cnt > 0       { Color::Rgb(255, 50,  50) }
                     else if warn_cnt > 0  { Color::Rgb(255, 200,  0) }
                     else                  { Color::Rgb(57,  255, 20) };
    let health_label = if err_cnt > 0     { "\u{2716} DEGRADED" }
                       else if warn_cnt > 0 { "\u{26a0} WARNING" }
                       else                { "\u{2714} HEALTHY" };

    let summary_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(health_col))
        .title(Span::styled(
            " System Health ",
            Style::default().fg(health_col).add_modifier(Modifier::BOLD),
        ));

    let summary_line = Line::from(vec![
        Span::styled(
            format!("  {} ", health_label),
            Style::default().fg(health_col).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("   \u{2705} {} ok", ok_cnt),
            Style::default().fg(Color::Rgb(57, 255, 20)),
        ),
        Span::styled(
            format!("   \u{26a0}  {} warn", warn_cnt),
            Style::default().fg(Color::Rgb(255, 200, 0)),
        ),
        Span::styled(
            format!("   \u{2716}  {} err", err_cnt),
            Style::default().fg(Color::Rgb(255, 50, 50)),
        ),
        Span::styled("   [D/U] re-run", Style::default().fg(C_DIM)),
    ]);

    f.render_widget(Paragraph::new(summary_line).block(summary_block), rows_layout[0]);

    // Individual check rows
    let header = Row::new(vec!["  Status", "Check", "Detail"])
        .style(Style::default().fg(C_DIAG).add_modifier(Modifier::BOLD))
        .height(1);

    let check_rows: Vec<Row> = checks.iter().map(|c| {
        let (icon, label, col) = match c.status {
            CheckStatus::Ok    => ("\u{2705}", "  OK  ", Color::Rgb(57, 255, 20)),
            CheckStatus::Warn  => ("\u{26a0} ", "  WARN", Color::Rgb(255, 200, 0)),
            CheckStatus::Error => ("\u{2716} ", "  ERR ", Color::Rgb(255, 50, 50)),
        };
        Row::new(vec![
            Cell::from(format!("  {} {}", icon, label))
                .style(Style::default().fg(col).add_modifier(Modifier::BOLD)),
            Cell::from(c.name.clone())
                .style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Cell::from(c.message.clone())
                .style(Style::default().fg(Color::Gray)),
        ])
    }).collect();

    let table_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_anim))
        .title(Span::styled(
            " \u{25c8} DIAGNOSTIC CHECKS \u{25c8} ",
            Style::default().fg(C_DIAG).add_modifier(Modifier::BOLD),
        ));

    f.render_widget(
        Table::new(check_rows, [
            Constraint::Length(14),
            Constraint::Length(28),
            Constraint::Min(40),
        ])
        .header(header)
        .block(table_block),
        rows_layout[1],
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// EXTRA THEME — Modal overlays
// ═══════════════════════════════════════════════════════════════════════════════

fn draw_extra_clean_modal(
    f: &mut ratatui::Frame,
    screen: Rect,
    session: &SessionSummary,
    tick: u64,
) {
    let modal = centered_rect(65, 45, screen);
    f.render_widget(Clear, modal);

    let blink_col = if pulse_on(tick) {
        Color::Rgb(255, 50, 50)
    } else {
        Color::Rgb(160, 10, 10)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(blink_col))
        .title(Span::styled(
            " \u{26a0}  CLEAN SESSION \u{26a0} ",
            Style::default().fg(blink_col).add_modifier(Modifier::BOLD),
        ));

    if modal.height < 12 || modal.width < 50 {
        f.render_widget(
            Paragraph::new("\n  Terminal too small.\n  Please enlarge your window.")
                .block(block)
                .style(Style::default().fg(Color::Yellow)),
            modal,
        );
        return;
    }

    let frames_count = session.frames.as_ref().map_or(0, |f| f.frame_count);
    let videos_count = session.videos.len();

    let lines = if frames_count == 0 && videos_count == 0 {
        vec![
            Line::raw(""),
            Line::from(Span::styled(
                format!("  Session: {}", session.name),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "  \u{26a0}  Already clean \u{2014} no files to delete.",
                Style::default().fg(Color::Yellow),
            )),
            Line::raw(""),
            Line::from(Span::styled("  Press any key to dismiss.", Style::default().fg(C_DIM))),
        ]
    } else {
        let ks = Style::default().fg(Color::Rgb(255, 210, 0)).add_modifier(Modifier::BOLD);
        let vs = Style::default().fg(Color::White);
        vec![
            Line::raw(""),
            Line::from(Span::styled(
                format!("  Session: {}", session.name),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
            Line::from(Span::styled("  Select action:", Style::default().fg(Color::Gray))),
            Line::raw(""),
            Line::from(vec![
                Span::styled("  [D] ", ks),
                Span::styled("Dry run \u{2014} simulate clean (nothing deleted)", vs),
            ]),
            Line::from(vec![
                Span::styled("  [F] ", ks),
                Span::styled(format!("Delete {} screenshot file(s)", frames_count), vs),
            ]),
            Line::from(vec![
                Span::styled("  [V] ", ks),
                Span::styled(format!("Delete {} video file(s)", videos_count), vs),
            ]),
            Line::from(vec![
                Span::styled("  [A] ", ks),
                Span::styled("Delete BOTH frames and videos", vs),
            ]),
            Line::raw(""),
            Line::from(Span::styled("  Any other key cancels.", Style::default().fg(C_DIM))),
        ]
    };

    f.render_widget(Paragraph::new(lines).block(block), modal);
}

fn draw_extra_library_modal(
    f: &mut ratatui::Frame,
    screen: Rect,
    input: &str,
    tick: u64,
) {
    let modal = centered_rect(70, 30, screen);
    f.render_widget(Clear, modal);

    let border_col = rainbow(tick / 2);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_col))
        .title(Span::styled(
            " \u{25c8} CHANGE LIBRARY PATH \u{25c8} ",
            Style::default().fg(border_col).add_modifier(Modifier::BOLD),
        ));

    let cursor = if pulse_on(tick) { "\u{2588}" } else { "\u{258f}" };

    let lines = vec![
        Line::raw(""),
        Line::from(Span::styled(
            "  Enter new Timelapse library root path:",
            Style::default().fg(Color::Gray),
        )),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  \u{276f} ", Style::default().fg(border_col).add_modifier(Modifier::BOLD)),
            Span::styled(input, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(cursor, Style::default().fg(border_col)),
        ]),
        Line::raw(""),
        Line::from(Span::styled(
            "  [Enter] confirm   [Esc] cancel",
            Style::default().fg(C_DIM),
        )),
    ];

    f.render_widget(Paragraph::new(lines).block(block), modal);
}

fn draw_extra_render_confirm_modal(
    f: &mut ratatui::Frame,
    screen: Rect,
    plan: &crate::render::RenderPlan,
    tick: u64,
) {
    let modal = centered_rect(75, 45, screen);
    f.render_widget(Clear, modal);

    let border_col = rainbow_off(tick / 3, 4);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_col))
        .title(Span::styled(
            " \u{1f3ac}  CONFIRM RENDER COMMAND  \u{1f3ac} ",
            Style::default().fg(border_col).add_modifier(Modifier::BOLD),
        ));

    if modal.height < 10 || modal.width < 50 {
        f.render_widget(
            Paragraph::new("\n  Terminal too small.\n  Please enlarge your window.")
                .block(block)
                .style(Style::default().fg(Color::Yellow)),
            modal,
        );
        return;
    }

    let mut cmd = "ffmpeg".to_string();
    for arg in plan.ffmpeg_args() {
        let arg_str = arg.to_string_lossy();
        if arg_str.contains(' ') || arg_str.is_empty() {
            cmd.push_str(&format!(" \"{}\"", arg_str));
        } else {
            cmd.push_str(&format!(" {}", arg_str));
        }
    }

    let confirm_text = format!(
        "\n  About to execute the following ffmpeg command:\n\n  {}\n\n  Press [Y] or [Enter] to confirm and render,\n  or [Esc]/[N]/[Any other key] to cancel.",
        cmd
    );

    f.render_widget(
        Paragraph::new(confirm_text)
            .block(block)
            .style(Style::default().fg(Color::White))
            .wrap(ratatui::widgets::Wrap { trim: false }),
        modal,
    );
}
