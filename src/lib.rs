pub mod capture;
pub mod config;
pub mod error;
pub mod frame_store;
pub mod manage;
pub mod render;
pub mod session;

pub use capture::{CaptureBackend, CaptureLoop, CapturedImage, XcapBackend};
pub use config::{DEFAULT_DISPLAY, DEFAULT_FRAME_PADDING, DEFAULT_FRAME_START, DEFAULT_INTERVAL};
pub use error::{Result, TimelapseError};
pub use frame_store::FrameStore;
pub use manage::{
    CleanOptions, CleanPlan, CleanResult, SessionSummary, SessionTarget, clean_session,
    create_clean_plan, execute_clean_plan, list_sessions, open_session, resolve_session_target,
};
pub use render::{
    DEFAULT_RENDER_FPS, FrameSequence, RenderOptions, RenderPlan, RenderResult, RenderSourceKind,
    RenderTarget,
};
pub use session::{
    DisplayTarget, Library, Session, SessionMetadata, SessionOpenOptions, SessionPaths,
};
