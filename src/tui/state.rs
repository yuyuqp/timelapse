use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use ratatui::crossterm::event::KeyCode;

use crate::config::{AppConfig, Theme};
use crate::doctor::{run_diagnostics, DoctorCheck};
use crate::manage::{list_sessions, open_session, CleanOptions, SessionSummary, SessionTarget};
use crate::session::{DisplayTarget, Library};
use super::TuiMessage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Capture = 0,
    Render = 1,
    Sessions = 2,
    Diagnostics = 3,
}

impl ActiveTab {
    pub fn next(self) -> Self {
        match self {
            ActiveTab::Capture => ActiveTab::Render,
            ActiveTab::Render => ActiveTab::Sessions,
            ActiveTab::Sessions => ActiveTab::Diagnostics,
            ActiveTab::Diagnostics => ActiveTab::Capture,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            ActiveTab::Capture => ActiveTab::Diagnostics,
            ActiveTab::Render => ActiveTab::Capture,
            ActiveTab::Sessions => ActiveTab::Render,
            ActiveTab::Diagnostics => ActiveTab::Sessions,
        }
    }
}

pub enum CaptureState {
    Idle,
    Starting,
    Capturing {
        frames_collected: u64,
        stop_handle: Arc<AtomicBool>,
    },
    Error(String),
}

pub enum RenderState {
    Idle,
    Rendering(String),
    Success(String),
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureMode {
    NewSession,
    Append,
}

pub struct TuiState {
    pub active_tab: ActiveTab,
    pub library_path: Option<PathBuf>,
    pub resolved_library_path: PathBuf,
    pub capture_interval: Duration,
    pub capture_display: DisplayTarget,
    pub capture_state: CaptureState,
    pub capture_mode: CaptureMode,
    pub render_fps: u32,
    pub render_state: RenderState,
    pub sessions: Vec<SessionSummary>,
    pub cursor_session_index: usize,
    pub active_session_index: usize,
    pub diagnostics: Option<Vec<DoctorCheck>>,
    pub confirm_clean_index: Option<usize>,
    pub change_library_input: Option<String>,
    pub status_message: Option<(String, SystemTime)>,
    pub show_welcome: bool,
    pub supports_unicode: bool,
    pub config: AppConfig,
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
    pub fn new(library_path: Option<PathBuf>) -> std::result::Result<Self, String> {
        let config = AppConfig::load();
        let resolved = library_path
            .clone()
            .or_else(|| config.default_library.clone())
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
            config,
        })
    }

    pub fn refresh_sessions(&mut self) {
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

    pub fn refresh_diagnostics(&mut self) {
        if let Ok(checks) = run_diagnostics(self.library_path.clone()) {
            self.diagnostics = Some(checks);
        }
    }

    pub fn get_selected_session_target(&self) -> SessionTarget {
        if self.sessions.is_empty() || self.active_session_index >= self.sessions.len() {
            SessionTarget::Latest {
                library: self.library_path.clone(),
            }
        } else {
            SessionTarget::Path(self.sessions[self.active_session_index].path.clone())
        }
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some((msg.into(), SystemTime::now()));
    }

    pub fn handle_key(&mut self, key: KeyCode, tx: &Sender<TuiMessage>) -> bool {
        // Return true if the app should exit
        if self.show_welcome {
            match key {
                KeyCode::Char('q') | KeyCode::Char('Q') => return true,
                KeyCode::Char('t') | KeyCode::Char('T') => {
                    let next_theme = match self.config.theme {
                        Theme::Modern => Theme::Classic,
                        Theme::Classic => Theme::Modern,
                    };
                    self.config.theme = next_theme;
                    if let Err(e) = self.config.save() {
                        self.set_status(format!("Failed to save config: {}", e));
                    } else {
                        self.set_status(format!("Theme toggled to {:?}", next_theme));
                    }
                }
                _ => {
                    self.show_welcome = false;
                }
            }
            return false;
        }

        if let Some(ref mut input_str) = self.change_library_input {
            match key {
                KeyCode::Enter => {
                    let new_path_str = input_str.trim().to_string();
                    if !new_path_str.is_empty() {
                        let new_path = PathBuf::from(new_path_str);
                        self.library_path = Some(new_path.clone());
                        self.resolved_library_path = new_path;
                        self.refresh_sessions();
                        self.refresh_diagnostics();
                        self.set_status("Library path updated successfully");
                    }
                    self.change_library_input = None;
                }
                KeyCode::Esc => {
                    self.change_library_input = None;
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
            return false;
        }

        match key {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                if let CaptureState::Capturing { ref stop_handle, .. } = self.capture_state {
                    stop_handle.store(true, Ordering::SeqCst);
                }
                return true;
            }
            KeyCode::Char('l') | KeyCode::Char('L') => {
                if let CaptureState::Idle | CaptureState::Error(_) = self.capture_state {
                    self.change_library_input = Some(self.resolved_library_path.to_string_lossy().into_owned());
                    self.confirm_clean_index = None;
                } else {
                    self.set_status("Cannot change library while capturing");
                }
            }
            KeyCode::Tab => {
                self.active_tab = self.active_tab.next();
                if self.active_tab == ActiveTab::Sessions {
                    self.refresh_sessions();
                } else if self.active_tab == ActiveTab::Diagnostics {
                    self.refresh_diagnostics();
                }
                self.confirm_clean_index = None;
            }
            KeyCode::Left => {
                self.active_tab = self.active_tab.prev();
                if self.active_tab == ActiveTab::Sessions {
                    self.refresh_sessions();
                } else if self.active_tab == ActiveTab::Diagnostics {
                    self.refresh_diagnostics();
                }
                self.confirm_clean_index = None;
            }
            KeyCode::Right => {
                self.active_tab = self.active_tab.next();
                if self.active_tab == ActiveTab::Sessions {
                    self.refresh_sessions();
                } else if self.active_tab == ActiveTab::Diagnostics {
                    self.refresh_diagnostics();
                }
                self.confirm_clean_index = None;
            }
            KeyCode::Char('1') => {
                self.active_tab = ActiveTab::Capture;
                self.confirm_clean_index = None;
            }
            KeyCode::Char('2') => {
                self.active_tab = ActiveTab::Render;
                self.confirm_clean_index = None;
            }
            KeyCode::Char('3') => {
                self.active_tab = ActiveTab::Sessions;
                self.refresh_sessions();
                self.confirm_clean_index = None;
            }
            KeyCode::Char('4') => {
                self.active_tab = ActiveTab::Diagnostics;
                self.refresh_diagnostics();
                self.confirm_clean_index = None;
            }
            _ => self.handle_tab_input(key, tx),
        }
        false
    }

    fn handle_tab_input(&mut self, key: KeyCode, tx: &Sender<TuiMessage>) {
        match self.active_tab {
            ActiveTab::Capture => match key {
                KeyCode::Char(' ') => match self.capture_state {
                    CaptureState::Idle | CaptureState::Error(_) => {
                        let force = self.capture_mode == CaptureMode::Append;
                        super::start_capture_thread(self, tx.clone(), force);
                    }
                    CaptureState::Capturing { ref stop_handle, .. } => {
                        stop_handle.store(true, Ordering::SeqCst);
                    }
                    CaptureState::Starting => {}
                },
                KeyCode::Char('d') | KeyCode::Char('D') => {
                    if let CaptureState::Idle = self.capture_state {
                        self.capture_display = match self.capture_display {
                            DisplayTarget::All => DisplayTarget::Primary,
                            DisplayTarget::Primary => DisplayTarget::All,
                        };
                    }
                }
                KeyCode::Char('a') | KeyCode::Char('A') => {
                    if let CaptureState::Idle = self.capture_state {
                        self.capture_mode = match self.capture_mode {
                            CaptureMode::NewSession => {
                                if self.sessions.is_empty() {
                                    self.set_status("No existing sessions found to append to!");
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
                    if let CaptureState::Idle = self.capture_state {
                        let secs = self.capture_interval.as_secs();
                        self.capture_interval = Duration::from_secs(secs.saturating_add(1));
                    }
                }
                KeyCode::Down => {
                    if let CaptureState::Idle = self.capture_state {
                        let secs = self.capture_interval.as_secs();
                        if secs > 1 {
                            self.capture_interval = Duration::from_secs(secs.saturating_sub(1));
                        }
                    }
                }
                _ => {}
            },
            ActiveTab::Render => match key {
                KeyCode::Char('r') | KeyCode::Char('R') | KeyCode::Enter => {
                    if let RenderState::Idle | RenderState::Success(_) | RenderState::Error(_) =
                        self.render_state
                    {
                        super::start_render_thread(self, tx.clone());
                    }
                }
                KeyCode::Up => {
                    self.render_fps = self.render_fps.saturating_add(1);
                }
                KeyCode::Down => {
                    if self.render_fps > 1 {
                        self.render_fps = self.render_fps.saturating_sub(1);
                    }
                }
                _ => {}
            },
            ActiveTab::Sessions => {
                if let Some(index) = self.confirm_clean_index {
                    let session = &self.sessions[index];
                    match key {
                        KeyCode::Char('f') | KeyCode::Char('F') => {
                            let clean_options = CleanOptions {
                                target: SessionTarget::Path(session.path.clone()),
                                frames: true,
                                videos: false,
                            };
                            super::execute_tui_clean(self, clean_options, false);
                            self.confirm_clean_index = None;
                            self.refresh_sessions();
                        }
                        KeyCode::Char('v') | KeyCode::Char('V') => {
                            let clean_options = CleanOptions {
                                target: SessionTarget::Path(session.path.clone()),
                                frames: false,
                                videos: true,
                            };
                            super::execute_tui_clean(self, clean_options, false);
                            self.confirm_clean_index = None;
                            self.refresh_sessions();
                        }
                        KeyCode::Char('a') | KeyCode::Char('A') => {
                            let clean_options = CleanOptions {
                                target: SessionTarget::Path(session.path.clone()),
                                frames: true,
                                videos: true,
                            };
                            super::execute_tui_clean(self, clean_options, false);
                            self.confirm_clean_index = None;
                            self.refresh_sessions();
                        }
                        KeyCode::Char('d') | KeyCode::Char('D') => {
                            let clean_options = CleanOptions {
                                target: SessionTarget::Path(session.path.clone()),
                                frames: true,
                                videos: true,
                            };
                            super::execute_tui_clean(self, clean_options, true);
                            self.confirm_clean_index = None;
                            self.refresh_sessions();
                        }
                        _ => {
                            self.confirm_clean_index = None;
                        }
                    }
                } else {
                    match key {
                        KeyCode::Up => {
                            if !self.sessions.is_empty() && self.cursor_session_index > 0 {
                                self.cursor_session_index -= 1;
                            }
                        }
                        KeyCode::Down => {
                            if !self.sessions.is_empty()
                                && self.cursor_session_index + 1 < self.sessions.len()
                            {
                                self.cursor_session_index += 1;
                            }
                        }
                        KeyCode::Enter | KeyCode::Char(' ') => {
                            if !self.sessions.is_empty() && self.cursor_session_index < self.sessions.len() {
                                self.active_session_index = self.cursor_session_index;
                                let session_name = self.sessions[self.active_session_index].name.clone();
                                self.set_status(format!("Selected session: {}", session_name));
                            }
                        }
                        KeyCode::Char('o') | KeyCode::Char('O') => {
                            if !self.sessions.is_empty() {
                                let target = SessionTarget::Path(self.sessions[self.cursor_session_index].path.clone());
                                match open_session(target) {
                                    Ok(_) => self.set_status("Opened session in file manager"),
                                    Err(e) => self.set_status(format!("Failed to open: {}", e)),
                                }
                            }
                        }
                        KeyCode::Char('c') | KeyCode::Char('C') => {
                            if !self.sessions.is_empty() {
                                self.confirm_clean_index = Some(self.cursor_session_index);
                            }
                        }
                        KeyCode::Char('a') | KeyCode::Char('A') => {
                            if !self.sessions.is_empty() {
                                self.active_session_index = self.cursor_session_index;
                                self.capture_mode = CaptureMode::Append;
                                self.active_tab = ActiveTab::Capture;
                                let session_name = self.sessions[self.active_session_index].name.clone();
                                self.set_status(format!("Switched to Capture (Append mode for session: {})", session_name));
                            }
                        }
                        KeyCode::Char('u') | KeyCode::Char('U') => {
                            self.refresh_sessions();
                            self.set_status("Refreshed sessions list");
                        }
                        _ => {}
                    }
                }
            }
            ActiveTab::Diagnostics => match key {
                KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Char('u') | KeyCode::Char('U') => {
                    self.refresh_diagnostics();
                    self.set_status("Diagnostics rerun completed");
                }
                _ => {}
            },
        }
    }
}
