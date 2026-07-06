use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::doctor::CheckStatus;
use crate::tui::state::TuiState;
use crate::tui::draw::common::centered_rect;
use super::{rainbow, rainbow_off, pulse_on, C_DIM, C_RENDER, C_SESSION};

pub fn draw_extra_welcome_neon(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    f.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(4, 4, 14))),
        area,
    );

    let border_col = rainbow(state.tick / 4);
    let title_col  = rainbow_off(state.tick / 4, 4);

    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_col))
        .title(Span::styled(
            format!(" ⚡ TIMELAPSE v{} ⚡ ", env!("CARGO_PKG_VERSION")),
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
            Constraint::Length(5), // ASCII logo
            Constraint::Length(1), // tagline
            Constraint::Length(1), // separator rule
            Constraint::Min(4),    // stats panel + controls panel
            Constraint::Length(1), // pulsing prompt
        ])
        .split(inner);

    const LOGO: [&str; 5] = [
        "  ████████ █ █   ███ ███████ █       █████  ██████  ███████ ███████ ",
        "     ██    █ ████  ████ █      █      ██   ██ █   ██ █      █      ",
        "     ██    █ █ ████ █ █████  █      ███████ ██████  ███████ █████   ",
        "     ██    █ █  ██  ██ █      █      ██   ██ █           ██ █      ",
        "     ██    █ █      ██ ███████ ███████ █   █ █     ███████ ███████ ",
    ];

    let logo_text: Vec<Line> = LOGO.iter().enumerate().map(|(i, &line)| {
        let col = rainbow_off(state.tick / 4, i * 2);
        Line::from(Span::styled(line, Style::default().fg(col).add_modifier(Modifier::BOLD)))
    }).collect();

    f.render_widget(
        Paragraph::new(logo_text).alignment(Alignment::Center),
        rows[0],
    );

    let tag_col = rainbow_off(state.tick / 4, 7);
    f.render_widget(
        Paragraph::new(Span::styled(
            "⚡  SESSION-BASED SCREENSHOT TIMELAPSE ENGINE  ⚡",
            Style::default().fg(tag_col),
        )).alignment(Alignment::Center),
        rows[1],
    );

    let sep_width = inner.width as usize;
    f.render_widget(
        Paragraph::new(Span::styled(
            "─".repeat(sep_width),
            Style::default().fg(Color::Rgb(35, 35, 55)),
        )),
        rows[2],
    );

    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(rows[3]);

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
            Span::styled("  📂 Sessions  ", Style::default().fg(C_DIM)),
            Span::styled(total_sessions.to_string(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  📷 Frames    ", Style::default().fg(C_DIM)),
            Span::styled(total_frames.to_string(),
                Style::default().fg(C_RENDER).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  🎬 Videos    ", Style::default().fg(C_DIM)),
            Span::styled(total_videos.to_string(),
                Style::default().fg(C_SESSION).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  🔧 ffmpeg    ", Style::default().fg(C_DIM)),
            Span::styled(
                if ffmpeg_ok { "✅ Ready" } else { "❌  Missing" },
                Style::default()
                    .fg(if ffmpeg_ok { C_SESSION } else { Color::Rgb(255, 50, 50) })
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    f.render_widget(Paragraph::new(stats_lines).block(stats_block), panels[0]);

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
            Span::styled("  [1–4]", ks), Span::styled("  switch tabs          ", vs),
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
            Span::styled("  [↑↓]  ", ks), Span::styled("  interval / fps       ", vs),
            Span::styled("[L]    ", ks), Span::styled("  change library", vs),
        ]),
    ];

    f.render_widget(Paragraph::new(ctrl_lines).block(ctrl_block), panels[1]);

    let prompt_col = if pulse_on(state.tick) { rainbow(state.tick / 3) } else { C_DIM };
    f.render_widget(
        Paragraph::new(Span::styled(
            "  ▶▶  PRESS ANY KEY TO ENTER THE DASHBOARD  ◀◀  ",
            Style::default().fg(prompt_col).add_modifier(Modifier::BOLD),
        )).alignment(Alignment::Center),
        rows[4],
    );
}

#[allow(dead_code)]
pub fn draw_extra_welcome(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
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
