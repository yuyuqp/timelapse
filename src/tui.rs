use std::io::{self, Stdout};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime};

use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::crossterm::ExecutableCommand;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Tabs};
use ratatui::Terminal;

use crate::capture::{CaptureLoop, XcapBackend};
use crate::doctor::{run_diagnostics, DoctorCheck, CheckStatus};
use crate::manage::{
    create_clean_plan, execute_clean_plan, list_sessions, open_session, CleanOptions,
    SessionSummary, SessionTarget,
};
use crate::render::{render, RenderOptions, RenderTarget, parse_exclusions};
use crate::session::{DisplayTarget, Library, Session, SessionOpenOptions};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveTab {
    Capture = 0,
    Render = 1,
    Sessions = 2,
    Diagnostics = 3,
}

impl ActiveTab {
    fn next(self) -> Self {
        match self {
            ActiveTab::Capture => ActiveTab::Render,
            ActiveTab::Render => ActiveTab::Sessions,
            ActiveTab::Sessions => ActiveTab::Diagnostics,
            ActiveTab::Diagnostics => ActiveTab::Capture,
        }
    }

    fn prev(self) -> Self {
        match self {
            ActiveTab::Capture => ActiveTab::Diagnostics,
            ActiveTab::Render => ActiveTab::Capture,
            ActiveTab::Sessions => ActiveTab::Render,
            ActiveTab::Diagnostics => ActiveTab::Sessions,
        }
    }
}

enum TuiMessage {
    CaptureStarted(Arc<AtomicBool>),
    FrameCaptured(u64),
    CaptureFinished(u64),
    CaptureError(String),
    RenderProgress(String),
    RenderFinished(String),
    RenderError(String),
}

enum CaptureState {
    Idle,
    Starting,
    Capturing {
        frames_collected: u64,
        stop_handle: Arc<AtomicBool>,
    },
    Error(String),
}

enum RenderState {
    Idle,
    Rendering(String),
    Success(String),
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureMode {
    NewSession,
    Append,
}

struct TuiState {
    active_tab: ActiveTab,
    library_path: Option<PathBuf>,
    resolved_library_path: PathBuf,
    capture_interval: Duration,
    capture_display: DisplayTarget,
    capture_state: CaptureState,
    capture_mode: CaptureMode,
    render_fps: u32,
    render_state: RenderState,
    sessions: Vec<SessionSummary>,
    cursor_session_index: usize,
    active_session_index: usize,
    diagnostics: Option<Vec<DoctorCheck>>,
    confirm_clean_index: Option<usize>,
    change_library_input: Option<String>,
    status_message: Option<(String, SystemTime)>,
    show_welcome: bool,
    supports_unicode: bool,
}

fn check_unicode_support() -> bool {
    // Check standard environment variables
    for var in &["LANG", "LC_ALL", "LC_CTYPE"] {
        if let Ok(val) = std::env::var(var) {
            let val_upper = val.to_uppercase();
            if val_upper.contains("UTF-8") || val_upper.contains("UTF8") {
                return true;
            }
        }
    }

    // Windows specific checks
    if cfg!(target_os = "windows") {
        // Windows Terminal supports UTF-8 natively
        if std::env::var("WT_SESSION").is_ok() {
            return true;
        }
        // VS Code terminal, Git Bash, etc.
        if let Ok(term) = std::env::var("TERM") {
            if term == "xterm-256color" || term == "cygwin" {
                return true;
            }
        }
    } else {
        // macOS and Linux default terminals usually support UTF-8 unless explicitly configured otherwise
        if std::env::var("TERM").is_ok() {
            return true;
        }
    }

    false
}

impl TuiState {
    fn new(library_path: Option<PathBuf>) -> std::result::Result<Self, String> {
        let resolved = library_path
            .clone()
            .or_else(|| Library::default_path().ok())
            .ok_or_else(|| "Failed to resolve default library path".to_string())?;

        let supports_unicode = check_unicode_support();

        Ok(Self {
            active_tab: ActiveTab::Capture,
            library_path,
            resolved_library_path: resolved,
            capture_interval: Duration::from_secs(6),
            capture_display: DisplayTarget::All,
            capture_state: CaptureState::Idle,
            capture_mode: CaptureMode::NewSession,
            render_fps: 15,
            render_state: RenderState::Idle,
            sessions: Vec::new(),
            cursor_session_index: 0,
            active_session_index: 0,
            diagnostics: None,
            confirm_clean_index: None,
            change_library_input: None,
            status_message: None,
            show_welcome: true,
            supports_unicode,
        })
    }

    fn refresh_sessions(&mut self) {
        if let Ok(list) = list_sessions(self.library_path.clone()) {
            self.sessions = list;
            if self.cursor_session_index >= self.sessions.len() && !self.sessions.is_empty() {
                self.cursor_session_index = self.sessions.len() - 1;
            }
            if self.active_session_index >= self.sessions.len() && !self.sessions.is_empty() {
                self.active_session_index = self.sessions.len() - 1;
            }
        }
    }

    fn refresh_diagnostics(&mut self) {
        if let Ok(checks) = run_diagnostics(self.library_path.clone()) {
            self.diagnostics = Some(checks);
        }
    }

    fn get_selected_session_target(&self) -> SessionTarget {
        if self.sessions.is_empty() || self.active_session_index >= self.sessions.len() {
            SessionTarget::Latest {
                library: self.library_path.clone(),
            }
        } else {
            SessionTarget::Path(self.sessions[self.active_session_index].path.clone())
        }
    }

    fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some((msg.into(), SystemTime::now()));
    }
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
            draw_ui(f, &state);
        })?;

        // Check for terminal keyboard input events
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if state.show_welcome {
                        state.show_welcome = false;
                        continue;
                    }
                    if let Some(ref mut input_str) = state.change_library_input {
                        match key.code {
                            KeyCode::Enter => {
                                let new_path_str = input_str.trim().to_string();
                                if !new_path_str.is_empty() {
                                    let new_path = PathBuf::from(new_path_str);
                                    state.library_path = Some(new_path.clone());
                                    state.resolved_library_path = new_path;
                                    state.refresh_sessions();
                                    state.refresh_diagnostics();
                                    state.set_status("Library path updated successfully");
                                }
                                state.change_library_input = None;
                            }
                            KeyCode::Esc => {
                                state.change_library_input = None;
                            }
                            KeyCode::Backspace => {
                                input_str.pop();
                            }
                            KeyCode::Char(c) => {
                                if input_str.len() < 120 {
                                    input_str.push(c);
                                }
                            }
                            _ => {}
                        }
                    } else {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Char('Q') => {
                                // If capturing, stop it first
                                if let CaptureState::Capturing { ref stop_handle, .. } = state.capture_state {
                                    stop_handle.store(true, Ordering::SeqCst);
                                }
                                break;
                            }
                            KeyCode::Char('l') | KeyCode::Char('L') => {
                                if let CaptureState::Idle | CaptureState::Error(_) = state.capture_state {
                                    state.change_library_input = Some(state.resolved_library_path.to_string_lossy().into_owned());
                                    state.confirm_clean_index = None;
                                } else {
                                    state.set_status("Cannot change library while capturing");
                                }
                            }
                            KeyCode::Tab => {
                                state.active_tab = state.active_tab.next();
                                if state.active_tab == ActiveTab::Sessions {
                                    state.refresh_sessions();
                                } else if state.active_tab == ActiveTab::Diagnostics {
                                    state.refresh_diagnostics();
                                }
                                state.confirm_clean_index = None;
                            }
                            KeyCode::Left => {
                                state.active_tab = state.active_tab.prev();
                                if state.active_tab == ActiveTab::Sessions {
                                    state.refresh_sessions();
                                } else if state.active_tab == ActiveTab::Diagnostics {
                                    state.refresh_diagnostics();
                                }
                                state.confirm_clean_index = None;
                            }
                            KeyCode::Right => {
                                state.active_tab = state.active_tab.next();
                                if state.active_tab == ActiveTab::Sessions {
                                    state.refresh_sessions();
                                } else if state.active_tab == ActiveTab::Diagnostics {
                                    state.refresh_diagnostics();
                                }
                                state.confirm_clean_index = None;
                            }
                            KeyCode::Char('1') => {
                                state.active_tab = ActiveTab::Capture;
                                state.confirm_clean_index = None;
                            }
                            KeyCode::Char('2') => {
                                state.active_tab = ActiveTab::Render;
                                state.confirm_clean_index = None;
                            }
                            KeyCode::Char('3') => {
                                state.active_tab = ActiveTab::Sessions;
                                state.refresh_sessions();
                                state.confirm_clean_index = None;
                            }
                            KeyCode::Char('4') => {
                                state.active_tab = ActiveTab::Diagnostics;
                                state.refresh_diagnostics();
                                state.confirm_clean_index = None;
                            }
                            _ => handle_tab_input(&mut state, &key.code, &tx),
                        }
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

fn handle_tab_input(state: &mut TuiState, key: &KeyCode, tx: &Sender<TuiMessage>) {
    match state.active_tab {
        ActiveTab::Capture => match key {
            KeyCode::Char(' ') => match state.capture_state {
                CaptureState::Idle | CaptureState::Error(_) => {
                    let force = state.capture_mode == CaptureMode::Append;
                    start_capture_thread(state, tx.clone(), force);
                }
                CaptureState::Capturing { ref stop_handle, .. } => {
                    stop_handle.store(true, Ordering::SeqCst);
                }
                CaptureState::Starting => {}
            },
            KeyCode::Char('d') | KeyCode::Char('D') => {
                if let CaptureState::Idle = state.capture_state {
                    state.capture_display = match state.capture_display {
                        DisplayTarget::All => DisplayTarget::Primary,
                        DisplayTarget::Primary => DisplayTarget::All,
                    };
                }
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                if let CaptureState::Idle = state.capture_state {
                    state.capture_mode = match state.capture_mode {
                        CaptureMode::NewSession => {
                            if state.sessions.is_empty() {
                                state.set_status("No existing sessions found to append to!");
                                CaptureMode::NewSession
                            } else {
                                CaptureMode::Append
                            }
                        }
                        CaptureMode::Append => CaptureMode::NewSession,
                    };
                }
            }
            KeyCode::Up => {
                if let CaptureState::Idle = state.capture_state {
                    let secs = state.capture_interval.as_secs();
                    state.capture_interval = Duration::from_secs(secs.saturating_add(1));
                }
            }
            KeyCode::Down => {
                if let CaptureState::Idle = state.capture_state {
                    let secs = state.capture_interval.as_secs();
                    if secs > 1 {
                        state.capture_interval = Duration::from_secs(secs.saturating_sub(1));
                    }
                }
            }
            _ => {}
        },
        ActiveTab::Render => match key {
            KeyCode::Char('r') | KeyCode::Char('R') | KeyCode::Enter => {
                if let RenderState::Idle | RenderState::Success(_) | RenderState::Error(_) =
                    state.render_state
                {
                    start_render_thread(state, tx.clone());
                }
            }
            KeyCode::Up => {
                state.render_fps = state.render_fps.saturating_add(1);
            }
            KeyCode::Down => {
                if state.render_fps > 1 {
                    state.render_fps = state.render_fps.saturating_sub(1);
                }
            }
            _ => {}
        },
        ActiveTab::Sessions => {
            if let Some(index) = state.confirm_clean_index {
                let session = &state.sessions[index];
                match key {
                    KeyCode::Char('f') | KeyCode::Char('F') => {
                        let clean_options = CleanOptions {
                            target: SessionTarget::Path(session.path.clone()),
                            frames: true,
                            videos: false,
                        };
                        execute_tui_clean(state, clean_options, false);
                        state.confirm_clean_index = None;
                        state.refresh_sessions();
                    }
                    KeyCode::Char('v') | KeyCode::Char('V') => {
                        let clean_options = CleanOptions {
                            target: SessionTarget::Path(session.path.clone()),
                            frames: false,
                            videos: true,
                        };
                        execute_tui_clean(state, clean_options, false);
                        state.confirm_clean_index = None;
                        state.refresh_sessions();
                    }
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        let clean_options = CleanOptions {
                            target: SessionTarget::Path(session.path.clone()),
                            frames: true,
                            videos: true,
                        };
                        execute_tui_clean(state, clean_options, false);
                        state.confirm_clean_index = None;
                        state.refresh_sessions();
                    }
                    KeyCode::Char('d') | KeyCode::Char('D') => {
                        let clean_options = CleanOptions {
                            target: SessionTarget::Path(session.path.clone()),
                            frames: true,
                            videos: true,
                        };
                        execute_tui_clean(state, clean_options, true);
                        state.confirm_clean_index = None;
                        state.refresh_sessions();
                    }
                    _ => {
                        state.confirm_clean_index = None;
                    }
                }
            } else {
                match key {
                    KeyCode::Up => {
                        if !state.sessions.is_empty() && state.cursor_session_index > 0 {
                            state.cursor_session_index -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if !state.sessions.is_empty()
                            && state.cursor_session_index + 1 < state.sessions.len()
                        {
                            state.cursor_session_index += 1;
                        }
                    }
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        if !state.sessions.is_empty() && state.cursor_session_index < state.sessions.len() {
                            state.active_session_index = state.cursor_session_index;
                            let session_name = state.sessions[state.active_session_index].name.clone();
                            state.set_status(format!("Selected session: {}", session_name));
                        }
                    }
                    KeyCode::Char('o') | KeyCode::Char('O') => {
                        if !state.sessions.is_empty() {
                            let target = SessionTarget::Path(state.sessions[state.cursor_session_index].path.clone());
                            match open_session(target) {
                                Ok(_) => state.set_status("Opened session in file manager"),
                                Err(e) => state.set_status(format!("Failed to open: {}", e)),
                            }
                        }
                    }
                    KeyCode::Char('c') | KeyCode::Char('C') => {
                        if !state.sessions.is_empty() {
                            state.confirm_clean_index = Some(state.cursor_session_index);
                        }
                    }
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        if !state.sessions.is_empty() {
                            state.active_session_index = state.cursor_session_index;
                            state.capture_mode = CaptureMode::Append;
                            state.active_tab = ActiveTab::Capture;
                            let session_name = state.sessions[state.active_session_index].name.clone();
                            state.set_status(format!("Switched to Capture (Append mode for session: {})", session_name));
                        }
                    }
                    KeyCode::Char('u') | KeyCode::Char('U') => {
                        state.refresh_sessions();
                        state.set_status("Refreshed sessions list");
                    }
                    _ => {}
                }
            }
        }
        ActiveTab::Diagnostics => match key {
            KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Char('u') | KeyCode::Char('U') => {
                state.refresh_diagnostics();
                state.set_status("Diagnostics rerun completed");
            }
            _ => {}
        },
    }
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

fn start_render_thread(state: &mut TuiState, tx: Sender<TuiMessage>) {
    tracing::info!("TUI: Starting render thread background worker...");
    state.render_state = RenderState::Rendering("Initializing render plan...".to_string());
    let target = state.get_selected_session_target();
    let render_target = match target {
        SessionTarget::Latest { library } => RenderTarget::Latest { library },
        SessionTarget::Path(path) => RenderTarget::Path(path),
    };
    let fps = state.render_fps;

    thread::spawn(move || {
        let options = RenderOptions {
            target: render_target,
            fps,
            output: None,
            overwrite: true,
            verbose: false,
            exclude: None,
        };

        let _ = tx.send(TuiMessage::RenderProgress("Running ffmpeg...".to_string()));
        match render(options) {
            Ok(result) => {
                let _ = tx.send(TuiMessage::RenderFinished(format!(
                    "Rendered {} frames successfully to {}",
                    result.frame_count,
                    result.output_path.display()
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

fn draw_ui(f: &mut ratatui::Frame, state: &TuiState) {
    let size = f.area();

    // Screen size warning guard (Option 1)
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

    // Tab Headers
    let titles = vec![
        "[1] Capture".to_string(),
        "[2] Render".to_string(),
        "[3] Sessions".to_string(),
        "[4] Diagnostics".to_string(),
    ];
    let selected_session_name = if state.sessions.is_empty() {
        "None".to_string()
    } else {
        state.sessions[state.active_session_index].name.clone()
    };
    let lib_name = state.resolved_library_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Timelapse");
    let header_title = format!(" Timelapse TUI | Lib: {} | Session: {} ", lib_name, selected_session_name);

    let tabs = Tabs::new(titles)
        .select(state.active_tab as usize)
        .block(Block::default().borders(Borders::ALL).title(header_title))
        .style(Style::default().fg(Color::Gray))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
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
    let mut settings_lines = vec![
        Line::raw(""),
        Line::from(vec![
            Span::raw("  Library Root:   "),
            Span::raw(state.resolved_library_path.to_string_lossy().into_owned()),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::raw("  Interval:       "),
            Span::styled(format!("{}s", state.capture_interval.as_secs()), Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("  (Use [Up/Down] to adjust)"),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::raw("  Capture Target: "),
            Span::styled(display_str, Style::default().add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::raw("                  (Use [D] to toggle display mode)"),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::raw("  Capture Mode:   "),
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
                settings_lines.push(Line::from(vec![
                    Span::styled(
                        format!("  ⚠ WARNING: Interval mismatch! Session uses {}s.", metadata.interval_seconds),
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    ),
                ]));
            }
        }
    }

    let settings_panel = Paragraph::new(settings_lines)
        .block(Block::default().borders(Borders::ALL).title(" Settings "));
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
    let status_panel = Paragraph::new(status_text)
        .style(status_style)
        .block(Block::default().borders(Borders::ALL).title(" Capture Control "));
    f.render_widget(status_panel, chunks[1]);
}

fn draw_render_tab(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

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

    let mut settings_text = format!(
        "\n  Render Target:  {}\n                 (Selected from Sessions list tab)\n\n  Render FPS:     {} fps  (Use [Up/Down] to adjust)\n\n  Total Frames:   {}",
        target_name,
        state.render_fps,
        frame_count
    );

    if exclude_count > 0 {
        settings_text.push_str(&format!(
            "\n  Exclusions:     {} frame(s)\n  Render Frames:  {}",
            exclude_count,
            actual_frame_count
        ));
    }

    settings_text.push_str(&format!(
        "\n  Est. Duration:  {}\n  Cap. Interval:  {}\n  Playback Speed: {}",
        duration_desc,
        interval_desc,
        speed_desc
    ));

    let settings_panel = Paragraph::new(settings_text)
        .block(Block::default().borders(Borders::ALL).title(" Render Settings "));
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
            "● RENDER COMPLETED",
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
    let status_panel = Paragraph::new(status_text)
        .style(status_style)
        .block(Block::default().borders(Borders::ALL).title(" Rendering Control "));
    f.render_widget(status_panel, chunks[1]);
}

fn draw_sessions_tab(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    if state.sessions.is_empty() {
        let panel = Paragraph::new("\n  No sessions found in the library.\n\n  Run Capture to create a new session.")
            .block(Block::default().borders(Borders::ALL).title(" Sessions List "));
        f.render_widget(panel, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(8)])
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
                row.style(Style::default().bg(Color::DarkGray).fg(Color::White))
            } else {
                row
            }
        })
        .collect();

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
    .block(Block::default().borders(Borders::ALL).title(" Sessions List (Press [Space/Enter] to Select for Capture/Render) "));

    f.render_widget(table, chunks[0]);

    let selected = &state.sessions[state.cursor_session_index];
    let mut metadata_text = match &selected.metadata {
        Some(m) => {
            format!(
                "  Started At:       {}\n\
                   Capture Interval: {}s\n\
                   Display Mode:     {:?}\n\
                   Capture Backend:  {}\n\
                   Frame Index Start: {}  (Padding: {})",
                m.started_at.format("%Y-%m-%d %H:%M:%S"),
                m.interval_seconds,
                m.display,
                m.capture_backend,
                m.frame_start,
                m.frame_padding
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
            "\n  Exclusions:       {} frame(s) active",
            exclusions.len()
        ));
    }

    let metadata_panel = Paragraph::new(metadata_text)
        .block(Block::default().borders(Borders::ALL).title(" Selected Session Metadata "));
    f.render_widget(metadata_panel, chunks[1]);
}

fn draw_diagnostics_tab(f: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    let checks = match &state.diagnostics {
        Some(c) => c,
        None => {
            let panel = Paragraph::new("\n  Running diagnostics checks...")
                .block(Block::default().borders(Borders::ALL).title(" System Diagnostics "));
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
                CheckStatus::Ok => ("  [ok] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                CheckStatus::Warn => ("  [warn]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                CheckStatus::Error => ("  [error]", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            };

            Row::new(vec![
                Cell::from(status_str).style(status_style),
                Cell::from(check.name.clone()).style(Style::default().add_modifier(Modifier::BOLD)),
                Cell::from(check.message.clone()),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(25),
            Constraint::Min(50),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" System Diagnostics (Press [D] to rerun) "),
    );

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
        Span::styled("    Version 0.1.0", Style::default().fg(Color::Green))
    ]));
    logo_lines.push(Line::raw(""));
    logo_lines.push(Line::from(vec![
        Span::styled(format!("    Active Library: {}", state.resolved_library_path.display()), Style::default().fg(Color::DarkGray))
    ]));
    logo_lines.push(Line::raw(""));
    logo_lines.push(Line::raw(""));
    logo_lines.push(Line::from(vec![
        Span::styled("    [ Press any key to start... ]", Style::default().fg(Color::Yellow).add_modifier(Modifier::DIM))
    ]));

    let paragraph = Paragraph::new(logo_lines)
        .block(block)
        .style(Style::default().fg(Color::White));

    let center_area = centered_rect(90, 80, area);
    f.render_widget(paragraph, center_area);
}
