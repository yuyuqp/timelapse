use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::style::{Color, Modifier, Style};

use crate::manage::SessionSummary;
use crate::tui::draw::common::centered_rect;

pub fn draw_confirm_modal(f: &mut ratatui::Frame, screen_area: Rect, session: &SessionSummary) {
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

pub fn draw_render_confirm_modal(f: &mut ratatui::Frame, screen_area: Rect, plan: &crate::render::RenderPlan) {
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

pub fn draw_library_modal(f: &mut ratatui::Frame, screen_area: Rect, input: &str) {
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
