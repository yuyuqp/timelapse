pub mod capture;
pub mod config;
pub mod error;
pub mod frame_store;
pub mod session;

pub use capture::{CaptureBackend, CaptureLoop, CapturedImage, XcapBackend};
pub use config::{DEFAULT_DISPLAY, DEFAULT_FRAME_PADDING, DEFAULT_FRAME_START, DEFAULT_INTERVAL};
pub use error::{Result, TimelapseError};
pub use frame_store::FrameStore;
pub use session::{
    DisplayTarget, Library, Session, SessionMetadata, SessionOpenOptions, SessionPaths,
};
