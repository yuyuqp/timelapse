use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Local};
use directories::{ProjectDirs, UserDirs};
use serde::{Deserialize, Serialize};

use crate::config::{CAPTURE_BACKEND_NAME, DEFAULT_FRAME_PADDING, DEFAULT_FRAME_START};
use crate::error::{Result, TimelapseError};
use crate::frame_store::FrameStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DisplayTarget {
    All,
    Primary,
}

impl std::fmt::Display for DisplayTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DisplayTarget::All => f.write_str("all"),
            DisplayTarget::Primary => f.write_str("primary"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Library {
    root: PathBuf,
}

impl Library {
    pub fn default_path() -> Result<PathBuf> {
        if let Some(video_dir) =
            UserDirs::new().and_then(|dirs| dirs.video_dir().map(Path::to_path_buf))
        {
            return Ok(video_dir.join("Timelapse"));
        }

        if let Some(project_dirs) = ProjectDirs::from("com", "yuyue", "timelapse") {
            return Ok(project_dirs.data_dir().to_path_buf());
        }

        Ok(std::env::current_dir()?.join("Timelapse"))
    }

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.root.join("sessions")
    }

    pub fn timestamped_session_path(&self, started_at: DateTime<Local>) -> PathBuf {
        self.sessions_dir()
            .join(started_at.format("%Y-%m-%d_%H%M%S").to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub schema_version: u32,
    pub started_at: DateTime<Local>,
    pub interval_seconds: u64,
    pub display: DisplayTarget,
    pub capture_backend: String,
    pub frame_padding: usize,
    pub frame_start: u64,
}

impl SessionMetadata {
    pub fn new(started_at: DateTime<Local>, interval: Duration, display: DisplayTarget) -> Self {
        Self {
            schema_version: 1,
            started_at,
            interval_seconds: interval.as_secs(),
            display,
            capture_backend: CAPTURE_BACKEND_NAME.to_string(),
            frame_padding: DEFAULT_FRAME_PADDING,
            frame_start: DEFAULT_FRAME_START,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionPaths {
    pub session_dir: PathBuf,
    pub frames_dir: PathBuf,
    pub metadata_file: PathBuf,
}

impl SessionPaths {
    pub fn new(session_dir: PathBuf) -> Self {
        Self {
            frames_dir: session_dir.join("frames"),
            metadata_file: session_dir.join("session.toml"),
            session_dir,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionOpenOptions {
    pub library: Option<PathBuf>,
    pub session: Option<PathBuf>,
    pub append: bool,
    pub interval: Duration,
    pub display: DisplayTarget,
}

#[derive(Debug)]
pub struct Session {
    paths: SessionPaths,
    metadata: SessionMetadata,
    frame_store: FrameStore,
}

impl Session {
    pub fn open(options: SessionOpenOptions) -> Result<Self> {
        if options.append && options.session.is_none() {
            return Err(TimelapseError::InvalidArgument(
                "--append requires --session".to_string(),
            ));
        }

        let started_at = Local::now();
        let session_dir = match options.session {
            Some(path) => path,
            None => {
                let library_root = match options.library {
                    Some(path) => path,
                    None => Library::default_path()?,
                };
                Library::new(library_root).timestamped_session_path(started_at)
            }
        };

        let paths = SessionPaths::new(session_dir);
        let metadata = SessionMetadata::new(started_at, options.interval, options.display);

        if options.append {
            Self::open_append(paths, metadata)
        } else {
            Self::create_new(paths, metadata)
        }
    }

    pub fn paths(&self) -> &SessionPaths {
        &self.paths
    }

    pub fn metadata(&self) -> &SessionMetadata {
        &self.metadata
    }

    pub fn frame_store(&self) -> &FrameStore {
        &self.frame_store
    }

    pub fn frame_store_mut(&mut self) -> &mut FrameStore {
        &mut self.frame_store
    }

    fn create_new(paths: SessionPaths, metadata: SessionMetadata) -> Result<Self> {
        if paths.session_dir.exists() && directory_has_entries(&paths.session_dir)? {
            return Err(TimelapseError::InvalidSession {
                path: paths.session_dir,
                message: "session directory already exists and is not empty; use --append with --session to continue it".to_string(),
            });
        }

        fs::create_dir_all(&paths.session_dir)?;
        let frame_store = FrameStore::create_new(
            paths.frames_dir.clone(),
            metadata.frame_padding,
            metadata.frame_start,
        )?;
        write_metadata(&paths.metadata_file, &metadata)?;

        Ok(Self {
            paths,
            metadata,
            frame_store,
        })
    }

    fn open_append(paths: SessionPaths, metadata: SessionMetadata) -> Result<Self> {
        if !paths.session_dir.exists() {
            return Err(TimelapseError::InvalidSession {
                path: paths.session_dir,
                message: "--append requires an existing session directory".to_string(),
            });
        }
        if !paths.session_dir.is_dir() {
            return Err(TimelapseError::InvalidSession {
                path: paths.session_dir,
                message: "session path is not a directory".to_string(),
            });
        }
        if !paths.frames_dir.is_dir() && !paths.metadata_file.is_file() {
            return Err(TimelapseError::InvalidSession {
                path: paths.session_dir,
                message: "expected a session folder with frames/ and/or session.toml".to_string(),
            });
        }

        if !paths.frames_dir.exists() {
            fs::create_dir_all(&paths.frames_dir)?;
        }
        if !paths.metadata_file.exists() {
            write_metadata(&paths.metadata_file, &metadata)?;
        }

        let frame_store = FrameStore::open_append(
            paths.frames_dir.clone(),
            metadata.frame_padding,
            metadata.frame_start,
        )?;

        Ok(Self {
            paths,
            metadata,
            frame_store,
        })
    }
}

fn write_metadata(path: &Path, metadata: &SessionMetadata) -> Result<()> {
    let text = toml::to_string_pretty(metadata)?;
    fs::write(path, text)?;
    Ok(())
}

fn directory_has_entries(path: &Path) -> Result<bool> {
    Ok(path.is_dir() && fs::read_dir(path)?.next().transpose()?.is_some())
}
