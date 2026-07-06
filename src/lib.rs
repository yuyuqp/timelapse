pub mod capture;
pub mod cli;
pub mod config;
pub mod doctor;
pub mod engine;
pub mod error;
pub mod logging;
pub mod tui;

pub use capture::{CaptureBackend, CaptureLoop, CapturedImage, XcapBackend};
pub use config::{
    AppConfig, Theme, DEFAULT_DISPLAY, DEFAULT_FRAME_PADDING, DEFAULT_FRAME_START, DEFAULT_INTERVAL,
};
pub use error::{Result, TimelapseError};
pub use engine::frame_store::FrameStore;
pub use engine::manage::{
    CleanOptions, CleanPlan, CleanResult, SessionSummary, SessionTarget, clean_session,
    create_clean_plan, execute_clean_plan, list_sessions, open_session, resolve_session_target,
    open_file_manager_select,
};
pub use engine::render::{
    DEFAULT_RENDER_FPS, parse_exclusions, resolve_frames_dir, resolve_target_path, FrameSequence,
    RenderOptions, RenderPlan, RenderResult, RenderSourceKind, RenderTarget,
};
pub use engine::session::{
    DisplayTarget, Library, Session, SessionMetadata, SessionOpenOptions, SessionPaths,
};
