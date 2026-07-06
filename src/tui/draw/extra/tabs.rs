use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table};

use crate::doctor::CheckStatus;
use crate::session::DisplayTarget;
use crate::render::parse_exclusions;
use crate::tui::{CaptureMode, CaptureState, RenderState, TuiState};
use super::{rainbow, rainbow_off, neon_spinner, pulse_on, sparkline_str, C_DIM, C_CAPTURE, C_RENDER, C_SESSION, C_DIAG};

pub fn draw_extra_capture(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
        .split(area);

    let border_anim = rainbow_off(state.tick / 3, 0);

    // Left Panel
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
                format!("Append → {}", state.sessions[state.active_session_index].name)
            }
        }
    };

    let mut settings_lines = vec![
        Line::raw(""),
        Line::from(vec![
            Span::styled("  📂 Library   ", Style::default().fg(C_DIM)),
            Span::styled(
                state.resolved_library_path.to_string_lossy().into_owned(),
                Style::default().fg(Color::Rgb(0, 255, 180)).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  ⏱  Interval  ", Style::default().fg(C_DIM)),
            Span::styled(
                format!("{}s", state.capture_interval.as_secs()),
                Style::default().fg(Color::Rgb(0, 220, 255)).add_modifier(Modifier::BOLD),
            ),
            Span::styled("   [↑↓] adjust", Style::default().fg(C_DIM)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  🖥  Target    ", Style::default().fg(C_DIM)),
            Span::styled(
                display_str,
                Style::default().fg(C_CAPTURE).add_modifier(Modifier::BOLD),
            ),
            Span::styled("   [D] toggle", Style::default().fg(C_DIM)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  ⚙  Mode      ", Style::default().fg(C_DIM)),
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
                        "  ⚠️  Interval mismatch! Session uses {}s",
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
            " ❖ CAPTURE SETTINGS ❖ ",
            Style::default().fg(C_CAPTURE).add_modifier(Modifier::BOLD),
        ));
    f.render_widget(Paragraph::new(settings_lines).block(settings_block), cols[0]);

    // Right Panel
    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(5)])
        .split(cols[1]);

    let (status_label, status_border, status_lines): (&str, Color, Vec<Line>) =
        match &state.capture_state {
            CaptureState::Idle => {
                let col = Color::Rgb(55, 55, 85);
                (
                    " ❖ IDLE ",
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
                    " ❖ STARTING ",
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
                let dot = if pulse_on(state.tick) { "●" } else { "○" };
                let rec_col = if pulse_on(state.tick) {
                    Color::Rgb(255, 40, 40)
                } else {
                    Color::Rgb(180, 10, 10)
                };
                (
                    " ● RECORDING ",
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
                    " ❖ ERROR ",
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

    // Sparkline
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
            "  — no frames captured yet —",
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

pub fn draw_extra_render(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
        .split(area);

    let border_anim = rainbow_off(state.tick / 3, 4);

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
    } else { "—".into() };

    let (interval_str, speed_str) = state.sessions
        .get(state.active_session_index)
        .and_then(|s| s.metadata.as_ref())
        .map(|m| (
            format!("{}s", m.interval_seconds),
            format!("{}×", state.render_fps as u64 * m.interval_seconds),
        ))
        .unwrap_or_else(|| ("—".into(), "—".into()));

    let excl_span = if exclude_count > 0 {
        Span::styled(
            format!("  (−{} excluded)", exclude_count),
            Style::default().fg(Color::Rgb(200, 120, 0)),
        )
    } else {
        Span::raw("")
    };

    let settings_lines = vec![
        Line::raw(""),
        Line::from(vec![
            Span::styled("  🎯 Target    ", Style::default().fg(C_DIM)),
            Span::styled(target_name, Style::default().fg(C_RENDER).add_modifier(Modifier::BOLD)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  ⚡ FPS       ", Style::default().fg(C_DIM)),
            Span::styled(
                format!("{} fps", state.render_fps),
                Style::default().fg(Color::Rgb(255, 210, 0)).add_modifier(Modifier::BOLD),
            ),
            Span::styled("   [↑↓] adjust", Style::default().fg(C_DIM)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  🎞  Frames   ", Style::default().fg(C_DIM)),
            Span::styled(frame_count.to_string(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            excl_span,
        ]),
        Line::from(vec![
            Span::styled("  ⏱  Duration  ", Style::default().fg(C_DIM)),
            Span::styled(duration_desc,
                Style::default().fg(Color::Rgb(0, 255, 180)).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  🔀 Interval  ", Style::default().fg(C_DIM)),
            Span::styled(interval_str, Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("  🚀 Speed     ", Style::default().fg(C_DIM)),
            Span::styled(speed_str,
                Style::default().fg(Color::Rgb(180, 255, 0)).add_modifier(Modifier::BOLD)),
        ]),
    ];

    let settings_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_anim))
        .title(Span::styled(
            " ❖ RENDER SETTINGS ❖ ",
            Style::default().fg(C_RENDER).add_modifier(Modifier::BOLD),
        ));
    f.render_widget(Paragraph::new(settings_lines).block(settings_block), cols[0]);

    // Right Panel
    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(5)])
        .split(cols[1]);

    let (status_label, status_border, status_lines): (&str, Color, Vec<Line>) =
        match &state.render_state {
            RenderState::Idle => {
                let col = Color::Rgb(55, 55, 85);
                (
                    " ❖ READY ",
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
                    " ⬡ RENDERING ",
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
                    " ✔ COMPLETE ",
                    col,
                    vec![
                        Line::raw(""),
                        Line::from(Span::styled(
                            "  Render complete! ✔",
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
                    " ✘ FAILED ",
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

    // Progress Bar
    let bar_inner_w = right_rows[1].width.saturating_sub(10) as usize;
    let (pct, bar_col, prog_label) = match &state.render_state {
        RenderState::Idle => (0usize, C_DIM, "Awaiting".to_string()),
        RenderState::Rendering(_) => {
            let t = state.tick % 100;
            let p = if t < 50 { t * 2 } else { (100 - t) * 2 };
            (
                p as usize,
                rainbow(state.tick / 2),
                format!("{} Processing…", neon_spinner(state.tick)),
            )
        }
        RenderState::Success(_) => (100, Color::Rgb(57, 255, 20), "Complete ✔".into()),
        RenderState::Error(_)   => (0,   Color::Rgb(255, 50, 50), "Failed ✘".into()),
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
            Span::styled("█".repeat(filled), Style::default().fg(bar_col)),
            Span::styled("░".repeat(empty),  Style::default().fg(C_DIM)),
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

pub fn draw_extra_sessions(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    let border_anim = rainbow_off(state.tick / 3, 6);

    if state.sessions.is_empty() {
        f.render_widget(
            Paragraph::new(
                "\n\n  📂  No sessions found in the library.\n\n  \
                 Switch to Capture and press [Space] to start recording.",
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(border_anim))
                    .title(Span::styled(
                        " ❖ SESSIONS ❖ ",
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

    let header = Row::new(vec!["", "Session Name", "Frames", "Videos", "Path"])
        .style(Style::default().fg(C_SESSION).add_modifier(Modifier::BOLD))
        .height(1);

    let table_rows: Vec<Row> = state.sessions.iter().enumerate().map(|(i, s)| {
        let is_cursor = i == state.cursor_session_index;
        let is_active = i == state.active_session_index;
        let frames_str = s.frames.as_ref().map_or("0".to_string(), |f| f.frame_count.to_string());
        let videos_str = s.videos.len().to_string();

        let indicator = match (is_cursor, is_active) {
            (true,  true)  => if pulse_on(state.tick) { "▶ ◉" } else { "▷ ○" },
            (true,  false) => "▶   ",
            (false, true)  => "  ◉ ",
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
            " ❖ SESSIONS  [↑↓] navigate  ·  [Enter] select  ·  [C] clean  ·  [O] open ❖ ",
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

    let selected = &state.sessions[state.cursor_session_index];

    let mut meta_lines = vec![Line::raw("")];
    match &selected.metadata {
        Some(m) => {
            meta_lines.push(Line::from(vec![
                Span::styled("  📅 Started    ", Style::default().fg(C_DIM)),
                Span::styled(
                    m.started_at.format("%Y-%m-%d %H:%M:%S").to_string(),
                    Style::default().fg(Color::White),
                ),
            ]));
            meta_lines.push(Line::from(vec![
                Span::styled("  ⏱  Interval   ", Style::default().fg(C_DIM)),
                Span::styled(
                    format!("{}s", m.interval_seconds),
                    Style::default().fg(C_RENDER).add_modifier(Modifier::BOLD),
                ),
                Span::styled("   🖥  Display  ", Style::default().fg(C_DIM)),
                Span::styled(
                    format!("{:?}", m.display),
                    Style::default().fg(Color::Rgb(255, 140, 0)),
                ),
                Span::styled("   🔧 Backend  ", Style::default().fg(C_DIM)),
                Span::styled(m.capture_backend.clone(), Style::default().fg(Color::Gray)),
            ]));
        }
        None => {
            let err = selected.metadata_error.as_deref().unwrap_or("No session.toml metadata found.");
            meta_lines.push(Line::from(Span::styled(
                format!("  ⚠️  {}", err),
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
                        Span::styled("  🚫 Exclusions  ", Style::default().fg(C_DIM)),
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
            format!(" ❖ METADATA: {} ❖ ", selected.name),
            Style::default().fg(C_SESSION).add_modifier(Modifier::BOLD),
        ));

    f.render_widget(Paragraph::new(meta_lines).block(meta_block), rows_layout[1]);
}

pub fn draw_extra_diagnostics(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    let border_anim = rainbow_off(state.tick / 3, 8);

    let checks = match &state.diagnostics {
        Some(c) => c,
        None => {
            f.render_widget(
                Paragraph::new(format!(
                    "\n\n  {} Running diagnostics checks…",
                    neon_spinner(state.tick)
                ))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(border_anim))
                        .title(Span::styled(
                            " ❖ DIAGNOSTICS ❖ ",
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

    let ok_cnt   = checks.iter().filter(|c| matches!(c.status, CheckStatus::Ok)).count();
    let warn_cnt = checks.iter().filter(|c| matches!(c.status, CheckStatus::Warn)).count();
    let err_cnt  = checks.iter().filter(|c| matches!(c.status, CheckStatus::Error)).count();

    let health_col = if err_cnt > 0       { Color::Rgb(255, 50,  50) }
                     else if warn_cnt > 0  { Color::Rgb(255, 200,  0) }
                     else                  { Color::Rgb(57,  255, 20) };
    let health_label = if err_cnt > 0     { "✘ DEGRADED" }
                       else if warn_cnt > 0 { "⚠️ WARNING" }
                       else                { "✔ HEALTHY" };

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
            format!("   ✅ {} ok", ok_cnt),
            Style::default().fg(Color::Rgb(57, 255, 20)),
        ),
        Span::styled(
            format!("   ⚠️  {} warn", warn_cnt),
            Style::default().fg(Color::Rgb(255, 200, 0)),
        ),
        Span::styled(
            format!("   ✘  {} err", err_cnt),
            Style::default().fg(Color::Rgb(255, 50, 50)),
        ),
        Span::styled("   [D/U] re-run", Style::default().fg(C_DIM)),
    ]);

    f.render_widget(Paragraph::new(summary_line).block(summary_block), rows_layout[0]);

    let header = Row::new(vec!["  Status", "Check", "Detail"])
        .style(Style::default().fg(C_DIAG).add_modifier(Modifier::BOLD))
        .height(1);

    let check_rows: Vec<Row> = checks.iter().map(|c| {
        let (icon, label, col) = match c.status {
            CheckStatus::Ok    => ("✅", "  OK  ", Color::Rgb(57, 255, 20)),
            CheckStatus::Warn  => ("⚠️ ", "  WARN", Color::Rgb(255, 200, 0)),
            CheckStatus::Error => ("✘ ", "  ERR ", Color::Rgb(255, 50, 50)),
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
            " ❖ DIAGNOSTIC CHECKS ❖ ",
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
