use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, TimelapseError>;

#[derive(Debug, Error)]
pub enum TimelapseError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("failed to write session metadata: {0}")]
    MetadataWrite(#[from] toml::ser::Error),

    #[error("failed to read session metadata at {path}: {message}")]
    MetadataRead { path: PathBuf, message: String },

    #[error("failed to save frame: {0}")]
    Image(#[from] image::ImageError),

    #[error("capture failed: {0}")]
    Capture(String),

    #[error("no displays were found")]
    NoDisplays,

    #[error("invalid session at {path}: {message}")]
    InvalidSession { path: PathBuf, message: String },

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("append metadata mismatch at {path}: {message}")]
    AppendMetadataMismatch { path: PathBuf, message: String },

    #[error("invalid render target at {path}: {message}")]
    InvalidRenderTarget { path: PathBuf, message: String },

    #[error("invalid frame sequence at {path}: {message}")]
    InvalidFrameSequence { path: PathBuf, message: String },

    #[error("render output already exists: {0}")]
    OutputExists(PathBuf),

    #[error("ffmpeg was not found in PATH")]
    FfmpegNotFound,

    #[error("ffmpeg failed with status {status}: {stderr}")]
    FfmpegFailed { status: String, stderr: String },

    #[error("no sessions were found in {0}")]
    NoSessions(PathBuf),

    #[error("failed to open file manager for {path}: {message}")]
    OpenFileManager { path: PathBuf, message: String },
}
