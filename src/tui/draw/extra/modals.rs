use ratatui::layout::Rect;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::manage::SessionSummary;
use crate::tui::draw::common::centered_rect;
use super::{rainbow, rainbow_off, pulse_on, C_DIM};

pub fn draw_extra_clean_modal(
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
            " ⚠️  CLEAN SESSION ⚠️ ",
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
                "  ⚠️  Already clean — no files to delete.",
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
                Span::styled("Dry run — simulate clean (nothing deleted)", vs),
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

pub fn draw_extra_library_modal(
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
            " ❖ CHANGE LIBRARY PATH ❖ ",
            Style::default().fg(border_col).add_modifier(Modifier::BOLD),
        ));

    let cursor = if pulse_on(tick) { "█" } else { "▕" };

    let lines = vec![
        Line::raw(""),
        Line::from(Span::styled(
            "  Enter new Timelapse library root path:",
            Style::default().fg(Color::Gray),
        )),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  ❯ ", Style::default().fg(border_col).add_modifier(Modifier::BOLD)),
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

pub fn draw_extra_render_confirm_modal(
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
            " 🎬  CONFIRM RENDER COMMAND  🎬 ",
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
