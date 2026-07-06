use std::io::{self, Stdout};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::crossterm::ExecutableCommand;
use ratatui::Terminal;

use crate::capture::{CaptureLoop, XcapBackend};
use crate::engine::manage::{create_clean_plan, execute_clean_plan, CleanOptions};
use crate::engine::session::{Session, SessionOpenOptions};

pub mod draw;
pub mod state;

pub use state::{ActiveTab, CaptureMode, CaptureState, RenderState, TuiState};

pub enum TuiMessage {
    CaptureStarted(Arc<AtomicBool>),
    FrameCaptured(u64),
    CaptureFinished(u64),
    CaptureError(String),
    RenderProgress(String),
    RenderFinished(String),
    RenderError(String),
}

pub fn run_tui(library: Option<PathBuf>) -> anyhow::Result<()> {
    tracing::info!("Launching interactive TUI...");
    let mut state = TuiState::new(library).map_err(|e| anyhow::anyhow!(e))?;
    state.refresh_sessions();
    state.refresh_diagnostics();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = tui_loop(&mut terminal, state);

    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    tracing::info!("TUI loop exited. Restored terminal raw modes.");
    res
}

fn tui_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    mut state: TuiState,
) -> anyhow::Result<()> {
    let (tx, rx) = mpsc::channel::<TuiMessage>();

    loop {
        // Render TUI
        terminal.draw(|f| {
            draw::draw_ui(f, &state);
        })?;
        state.tick = state.tick.wrapping_add(1);

        // Check for terminal keyboard input events
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if state.handle_key(key.code, &tx) {
                        break;
                    }
                }
            }
        }

        // Process any messages from background worker threads
        while let Ok(msg) = rx.try_recv() {
            match msg {
                TuiMessage::CaptureStarted(stop_handle) => {
                    tracing::info!("TUI background capture started successfully.");
                    state.capture_state = CaptureState::Capturing {
                        frames_collected: 0,
                        stop_handle,
                    };
                    state.set_status("Capture session started");
                }
                TuiMessage::FrameCaptured(index) => {
                    tracing::debug!("TUI received frame captured event (index: {}).", index);
                    if let CaptureState::Capturing { ref mut frames_collected, .. } = state.capture_state {
                        *frames_collected = index + 1;
                        let count = *frames_collected;
                        state.frame_sparkline.push(count);
                        if state.frame_sparkline.len() > 40 {
                            state.frame_sparkline.remove(0);
                        }
                    }
                }
                TuiMessage::CaptureFinished(count) => {
                    tracing::info!("TUI background capture stopped. Saved {} frames.", count);
                    state.capture_state = CaptureState::Idle;
                    state.set_status(format!("Capture stopped. Saved {} frames.", count));
                    state.refresh_sessions();
                }
                TuiMessage::CaptureError(err) => {
                    tracing::error!("TUI background capture thread error: {}", err);
                    state.capture_state = CaptureState::Error(err.clone());
                    state.set_status(format!("Capture error: {}", err));
                }
                TuiMessage::RenderProgress(msg) => {
                    tracing::debug!("TUI render progress: {}", msg);
                    state.render_state = RenderState::Rendering(msg);
                }
                TuiMessage::RenderFinished(msg) => {
                    tracing::info!("TUI render thread finished: {}", msg);
                    state.render_state = RenderState::Success(msg.clone());
                    state.set_status("Render completed successfully");
                    state.refresh_sessions();
                }
                TuiMessage::RenderError(err) => {
                    tracing::error!("TUI render thread error: {}", err);
                    state.render_state = RenderState::Error(err.clone());
                    state.set_status(format!("Render failed: {}", err));
                }
            }
        }
    }

    Ok(())
}

fn start_capture_thread(state: &mut TuiState, tx: Sender<TuiMessage>, force: bool) {
    tracing::info!(
        "TUI: Starting capture thread background worker (mode: {:?}, interval: {}s, display: {:?}, force: {})",
        state.capture_mode,
        state.capture_interval.as_secs(),
        state.capture_display,
        force
    );
    state.capture_state = CaptureState::Starting;
    let library_path = state.library_path.clone();
    let interval = state.capture_interval;
    let display = state.capture_display;

    // Resolve append mode target session path
    let session_path = if state.capture_mode == CaptureMode::Append && !state.sessions.is_empty() {
        Some(state.sessions[state.active_session_index].path.clone())
    } else {
        None
    };
    let append = state.capture_mode == CaptureMode::Append && session_path.is_some();

    thread::spawn(move || {
        let open_options = SessionOpenOptions {
            library: library_path,
            session: session_path,
            append,
            interval: Some(interval),
            display,
            force, // Use the force parameter passed
        };

        let mut session = match Session::open(open_options) {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.send(TuiMessage::CaptureError(e.to_string()));
                return;
            }
        };

        let capture_loop = CaptureLoop::new(session.capture_interval(), display);
        let stop_handle = capture_loop.stop_handle();

        if tx.send(TuiMessage::CaptureStarted(stop_handle)).is_err() {
            return;
        }

        let backend = XcapBackend;
        match capture_loop.run_with_progress(&mut session, &backend, |frame_index| {
            let _ = tx.send(TuiMessage::FrameCaptured(frame_index));
            Ok(())
        }) {
            Ok(count) => {
                let _ = tx.send(TuiMessage::CaptureFinished(count));
            }
            Err(e) => {
                let _ = tx.send(TuiMessage::CaptureError(e.to_string()));
            }
        }
    });
}

fn start_render_thread(plan: crate::engine::render::RenderPlan, state: &mut TuiState, tx: Sender<TuiMessage>) {
    tracing::info!("TUI: Starting render thread background worker...");
    state.render_state = RenderState::Rendering("Initializing render plan...".to_string());

    thread::spawn(move || {
        let _ = tx.send(TuiMessage::RenderProgress("Running ffmpeg...".to_string()));
        let actual_frame_count = plan.sequence.frame_count - plan.exclude.iter().filter(|&&x| x >= plan.sequence.start_number && x <= plan.sequence.end_number).count();
        match crate::engine::render::run_ffmpeg(&plan) {
            Ok(_) => {
                let _ = tx.send(TuiMessage::RenderFinished(format!(
                    "Rendered {} frames successfully to {}",
                    actual_frame_count,
                    plan.output_path.display()
                )));
            }
            Err(e) => {
                let _ = tx.send(TuiMessage::RenderError(e.to_string()));
            }
        }
    });
}

fn execute_tui_clean(state: &mut TuiState, clean_options: CleanOptions, dry_run: bool) {
    match create_clean_plan(clean_options) {
        Ok(plan) => {
            if dry_run {
                state.set_status(format!(
                    "Dry run: would delete {} frame(s) and {} video(s)",
                    plan.frames.len(),
                    plan.videos.len()
                ));
            } else {
                match execute_clean_plan(&plan) {
                    Ok(result) => {
                        state.set_status(format!(
                            "Cleaned {} frame(s) and {} video(s)",
                            result.frames_deleted, result.videos_deleted
                        ));
                    }
                    Err(e) => {
                        state.set_status(format!("Clean execution failed: {}", e));
                    }
                }
            }
        }
        Err(e) => {
            state.set_status(format!("Clean plan failed: {}", e));
        }
    }
}
