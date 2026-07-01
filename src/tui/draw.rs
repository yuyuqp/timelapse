use std::time::Duration;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Table, Tabs};

use crate::doctor::CheckStatus;
use crate::config::Theme;
use crate::manage::SessionSummary;
use crate::session::DisplayTarget;
use crate::render::parse_exclusions;
use super::state::{ActiveTab, CaptureMode, CaptureState, RenderState, TuiState};

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

    let is_modern = state.config.theme == Theme::Modern;

    // Tab Headers
    let titles = if is_modern && state.supports_unicode {
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
    
    let library_emoji = if is_modern && state.supports_unicode { " ⚡ " } else { " | " };
    let session_emoji = if is_modern && state.supports_unicode { " 🎯 " } else { " | " };
    let header_title = format!(
        " Timelapse TUI{}Lib: {}{}Session: {} ",
        library_emoji, lib_name, session_emoji, selected_session_name
    );

    let border_type = if is_modern { BorderType::Rounded } else { BorderType::Plain };
    let header_border_color = if is_modern { Color::Magenta } else { Color::Gray };
    let highlight_color = if is_modern { Color::LightMagenta } else { Color::Cyan };

    let tab_block = Block::default()
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(Style::default().fg(header_border_color))
        .title(Span::styled(header_title, Style::default().fg(if is_modern { Color::Cyan } else { Color::White }).add_modifier(Modifier::BOLD)));

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
}

fn draw_capture_tab(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let is_modern = state.config.theme == Theme::Modern;
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

    let lib_label = if is_modern && use_unicode { "📂 Library Root:   " } else { "  Library Root:   " };
    let int_label = if is_modern && use_unicode { "⏱ Interval:       " } else { "  Interval:       " };
    let tgt_label = if is_modern && use_unicode { "🖥 Capture Target: " } else { "  Capture Target: " };
    let mod_label = if is_modern && use_unicode { "⚙ Capture Mode:   " } else { "  Capture Mode:   " };

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
            Span::raw("                  (Use [D] to toggle display mode)"),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::raw(mod_label),
            Span::styled(mode_str, Style::default().add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::raw("                  (Use [A] to toggle capture mode)"),
        ]),
    ];

    // Check for interval mismatch warning
    if state.capture_mode == CaptureMode::Append && !state.sessions.is_empty() {
        let session = &state.sessions[state.active_session_index];
        if let Some(ref metadata) = session.metadata {
            if metadata.interval_seconds != state.capture_interval.as_secs() {
                settings_lines.push(Line::raw(""));
                let warning_symbol = if is_modern && use_unicode { "⚠️" } else { "⚠" };
                settings_lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {} WARNING: Interval mismatch! Session uses {}s.", warning_symbol, metadata.interval_seconds),
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    ),
                ]));
            }
        }
    }

    let settings_border_color = if is_modern { Color::Magenta } else { Color::Gray };
    let settings_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if is_modern { BorderType::Rounded } else { BorderType::Plain })
        .border_style(Style::default().fg(settings_border_color))
        .title(Span::styled(" Settings ", Style::default().fg(if is_modern { Color::Cyan } else { Color::White }).add_modifier(Modifier::BOLD)));

    let settings_panel = Paragraph::new(settings_lines).block(settings_block);
    f.render_widget(settings_panel, chunks[0]);

    // Right Panel: Capture Status
    let (status_title, status_style, status_desc) = match &state.capture_state {
        CaptureState::Idle => (
            if is_modern && use_unicode { "⏸ IDLE" } else { "● IDLE" },
            Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD),
            "\n\n  Press [Space] to start capturing screenshots.".to_string(),
        ),
        CaptureState::Starting => (
            if is_modern && use_unicode { "🔄 STARTING..." } else { "● STARTING..." },
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            "\n\n  Initializing screen capture backend...".to_string(),
        ),
        CaptureState::Capturing { frames_collected, .. } => (
            if is_modern && use_unicode { "🔴 RECORDING" } else { "● RECORDING" },
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            format!(
                "\n\n  Screenshots are being collected.\n\n  Frames collected: {}\n\n  Press [Space] to stop capturing.",
                frames_collected
            ),
        ),
        CaptureState::Error(err) => (
            if is_modern && use_unicode { "❌ ERROR" } else { "● ERROR" },
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
        _ => if is_modern { Color::Cyan } else { Color::Gray },
    };

    let control_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if is_modern { BorderType::Rounded } else { BorderType::Plain })
        .border_style(Style::default().fg(control_border_color))
        .title(Span::styled(" Capture Control ", Style::default().fg(if is_modern { Color::Cyan } else { Color::White }).add_modifier(Modifier::BOLD)));

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

    let is_modern = state.config.theme == Theme::Modern;
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

    let lbl_target = if is_modern && use_unicode { "🎯 Render Target:  " } else { "  Render Target:  " };
    let lbl_fps = if is_modern && use_unicode { "⚡ Render FPS:     " } else { "  Render FPS:     " };
    let lbl_total = if is_modern && use_unicode { "🎞 Total Frames:   " } else { "  Total Frames:   " };
    let lbl_excl = if is_modern && use_unicode { "🚫 Exclusions:     " } else { "  Exclusions:     " };
    let lbl_act = if is_modern && use_unicode { "🎬 Render Frames:  " } else { "  Render Frames:  " };
    let lbl_dur = if is_modern && use_unicode { "⏱ Est. Duration:  " } else { "  Est. Duration:  " };
    let lbl_int = if is_modern && use_unicode { "⏱ Cap. Interval:  " } else { "  Cap. Interval:  " };
    let lbl_spd = if is_modern && use_unicode { "🚀 Playback Speed: " } else { "  Playback Speed: " };

    let mut settings_text = format!(
        "\n{}{}\n                 (Selected from Sessions list tab)\n\n{}{} fps  (Use [Up/Down] to adjust)\n\n{}{}",
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

    let settings_border_color = if is_modern { Color::Magenta } else { Color::Gray };
    let settings_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if is_modern { BorderType::Rounded } else { BorderType::Plain })
        .border_style(Style::default().fg(settings_border_color))
        .title(Span::styled(" Render Settings ", Style::default().fg(if is_modern { Color::Cyan } else { Color::White }).add_modifier(Modifier::BOLD)));

    let settings_panel = Paragraph::new(settings_text).block(settings_block);
    f.render_widget(settings_panel, chunks[0]);

    // Right Panel: Rendering Status
    let (status_title, status_style, status_desc) = match &state.render_state {
        RenderState::Idle => (
            if is_modern && use_unicode { "⏸ READY" } else { "● READY" },
            Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD),
            "\n\n  Press [Enter] or [R] to start rendering target to MP4.".to_string(),
        ),
        RenderState::Rendering(msg) => (
            if is_modern && use_unicode { "⚙ RENDERING" } else { "● RENDERING" },
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            format!("\n\n  FFmpeg rendering in progress...\n\n  {}", msg),
        ),
        RenderState::Success(msg) => (
            if is_modern && use_unicode { "✅ COMPLETED" } else { "● RENDER COMPLETED" },
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            format!("\n\n  {}", msg),
        ),
        RenderState::Error(err) => (
            if is_modern && use_unicode { "❌ ERROR" } else { "● ERROR" },
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
        _ => if is_modern { Color::Cyan } else { Color::Gray },
    };

    let control_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if is_modern { BorderType::Rounded } else { BorderType::Plain })
        .border_style(Style::default().fg(control_border_color))
        .title(Span::styled(" Rendering Control ", Style::default().fg(if is_modern { Color::Cyan } else { Color::White }).add_modifier(Modifier::BOLD)));

    let status_panel = Paragraph::new(status_text)
        .style(status_style)
        .block(control_block);
    f.render_widget(status_panel, chunks[1]);
}

fn draw_sessions_tab(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    let is_modern = state.config.theme == Theme::Modern;
    let use_unicode = state.supports_unicode;

    if state.sessions.is_empty() {
        let panel = Paragraph::new("\n  No sessions found in the library.\n\n  Run Capture to create a new session.")
            .block(Block::default()
                .borders(Borders::ALL)
                .border_type(if is_modern { BorderType::Rounded } else { BorderType::Plain })
                .border_style(Style::default().fg(if is_modern { Color::Magenta } else { Color::Gray }))
                .title(Span::styled(" Sessions List ", Style::default().fg(if is_modern { Color::Cyan } else { Color::White }).add_modifier(Modifier::BOLD))));
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
                let bg_color = if is_modern { Color::Magenta } else { Color::DarkGray };
                row.style(Style::default().bg(bg_color).fg(Color::White))
            } else {
                row
            }
        })
        .collect();

    let table_title = if is_modern && use_unicode {
        " Sessions List (Press [Space/Enter] to Select) "
    } else {
        " Sessions List (Press [Space/Enter] to Select for Capture/Render) "
    };

    let table_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if is_modern { BorderType::Rounded } else { BorderType::Plain })
        .border_style(Style::default().fg(if is_modern { Color::Magenta } else { Color::Gray }))
        .title(Span::styled(table_title, Style::default().fg(if is_modern { Color::Cyan } else { Color::White }).add_modifier(Modifier::BOLD)));

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

    let lbl_start = if is_modern && use_unicode { "📅 Started At:       " } else { "  Started At:       " };
    let lbl_int = if is_modern && use_unicode { "⏱ Capture Interval: " } else { "  Capture Interval: " };
    let lbl_disp = if is_modern && use_unicode { "🖥 Display Mode:     " } else { "  Display Mode:     " };
    let lbl_back = if is_modern && use_unicode { "🔧 Capture Backend:  " } else { "  Capture Backend:  " };
    let lbl_idx = if is_modern && use_unicode { "🔢 Frame Index Start: " } else { "  Frame Index Start: " };
    let lbl_excl = if is_modern && use_unicode { "🚫 Exclusions:        " } else { "  Exclusions:       " };

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
            "\n{}{}{} frame(s) active",
            lbl_excl,
            if is_modern && use_unicode { "" } else { "      " },
            exclusions.len()
        ));
    }

    let metadata_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if is_modern { BorderType::Rounded } else { BorderType::Plain })
        .border_style(Style::default().fg(if is_modern { Color::Cyan } else { Color::Gray }))
        .title(Span::styled(" Selected Session Metadata ", Style::default().fg(if is_modern { Color::Cyan } else { Color::White }).add_modifier(Modifier::BOLD)));

    let metadata_panel = Paragraph::new(metadata_text).block(metadata_block);
    f.render_widget(metadata_panel, chunks[1]);
}

fn draw_diagnostics_tab(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    let is_modern = state.config.theme == Theme::Modern;
    let use_unicode = state.supports_unicode;

    let checks = match &state.diagnostics {
        Some(c) => c,
        None => {
            let panel = Paragraph::new("\n  Running diagnostics checks...")
                .block(Block::default()
                    .borders(Borders::ALL)
                    .border_type(if is_modern { BorderType::Rounded } else { BorderType::Plain })
                    .border_style(Style::default().fg(if is_modern { Color::Magenta } else { Color::Gray }))
                    .title(Span::styled(" System Diagnostics ", Style::default().fg(if is_modern { Color::Cyan } else { Color::White }).add_modifier(Modifier::BOLD))));
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
                    if is_modern && use_unicode { "  ✅ OK   " } else { "  [ok] " },
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                ),
                CheckStatus::Warn => (
                    if is_modern && use_unicode { "  ⚠️ WARN " } else { "  [warn]" },
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ),
                CheckStatus::Error => (
                    if is_modern && use_unicode { "  ❌ ERR  " } else { "  [error]" },
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

    let table_title = if is_modern && use_unicode {
        " System Diagnostics (Press [D] to rerun) "
    } else {
        " System Diagnostics (Press [D] to rerun) "
    };

    let table_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if is_modern { BorderType::Rounded } else { BorderType::Plain })
        .border_style(Style::default().fg(if is_modern { Color::Magenta } else { Color::Gray }))
        .title(Span::styled(table_title, Style::default().fg(if is_modern { Color::Cyan } else { Color::White }).add_modifier(Modifier::BOLD)));

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
        Theme::Modern => draw_modern_welcome(f, area, state),
        Theme::Classic => draw_classic_welcome(f, area, state),
    }
}

fn draw_classic_welcome(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
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

fn draw_modern_welcome(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
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
            Span::raw(" Toggle modern/classic theme"),
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
