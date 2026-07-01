use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Local};
use directories::{ProjectDirs, UserDirs};
use serde::{Deserialize, Serialize};

use crate::config::{
    CAPTURE_BACKEND_NAME, DEFAULT_FRAME_PADDING, DEFAULT_FRAME_START, DEFAULT_INTERVAL,
};
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

        if let Some(project_dirs) = ProjectDirs::from("com", "yuyuqp", "timelapse") {
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
    pub interval: Option<Duration>,
    pub display: DisplayTarget,
    pub force: bool,
}

#[derive(Debug)]
pub struct Session {
    paths: SessionPaths,
    metadata: SessionMetadata,
    frame_store: FrameStore,
    capture_interval: Duration,
    warnings: Vec<String>,
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
        let requested_interval = options.interval.unwrap_or(DEFAULT_INTERVAL);
        let metadata = SessionMetadata::new(started_at, requested_interval, options.display);

        if options.append {
            Self::open_append(paths, metadata, options.interval, options.force)
        } else {
            if options.force {
                return Err(TimelapseError::InvalidArgument(
                    "--force is only valid with --append".to_string(),
                ));
            }
            Self::create_new(paths, metadata)
        }
    }

    pub fn paths(&self) -> &SessionPaths {
        &self.paths
    }

    pub fn metadata(&self) -> &SessionMetadata {
        &self.metadata
    }

    pub fn capture_interval(&self) -> Duration {
        self.capture_interval
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
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
            capture_interval: Duration::from_secs(metadata.interval_seconds),
            metadata,
            frame_store,
            warnings: Vec::new(),
        })
    }

    fn open_append(
        paths: SessionPaths,
        requested_metadata: SessionMetadata,
        requested_interval: Option<Duration>,
        force: bool,
    ) -> Result<Self> {
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

        let mut warnings = Vec::new();
        let metadata = match read_existing_metadata(&paths.metadata_file) {
            Ok(metadata) => {
                if let Some(interval) = requested_interval {
                    let requested_seconds = interval.as_secs();
                    if requested_seconds != metadata.interval_seconds {
                        let message = format!(
                            "requested interval is {requested_seconds}s, but session metadata uses {}s",
                            metadata.interval_seconds
                        );
                        if !force {
                            return Err(TimelapseError::AppendMetadataMismatch {
                                path: paths.metadata_file.clone(),
                                message,
                            });
                        }
                        warnings.push(format!(
                            "WARNING: appending despite interval mismatch: {message}; session.toml was not rewritten"
                        ));
                    }
                }
                metadata
            }
            Err(message) => {
                if !force {
                    return Err(TimelapseError::MetadataRead {
                        path: paths.metadata_file.clone(),
                        message,
                    });
                }
                warnings.push(format!(
                    "WARNING: appending without usable session metadata: {message}; detecting next frame from files"
                ));
                requested_metadata
            }
        };

        let capture_interval = requested_interval
            .unwrap_or_else(|| Duration::from_secs(metadata.interval_seconds.max(1)));

        let frame_store = FrameStore::open_append(
            paths.frames_dir.clone(),
            metadata.frame_padding,
            metadata.frame_start,
        )?;

        Ok(Self {
            paths,
            capture_interval,
            metadata,
            frame_store,
            warnings,
        })
    }
}

fn write_metadata(path: &Path, metadata: &SessionMetadata) -> Result<()> {
    let text = toml::to_string_pretty(metadata)?;
    fs::write(path, text)?;
    Ok(())
}

pub fn read_existing_metadata(path: &Path) -> std::result::Result<SessionMetadata, String> {
    let text = fs::read_to_string(path)
        .map_err(|err| format!("could not read {}: {err}", path.display()))?;
    toml::from_str(&text).map_err(|err| format!("could not parse {}: {err}", path.display()))
}

fn directory_has_entries(path: &Path) -> Result<bool> {
    Ok(path.is_dir() && fs::read_dir(path)?.next().transpose()?.is_some())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tempfile::TempDir;

    use super::*;

    fn new_session_options(session_dir: PathBuf, interval_secs: u64) -> SessionOpenOptions {
        SessionOpenOptions {
            library: None,
            session: Some(session_dir),
            append: false,
            interval: Some(Duration::from_secs(interval_secs)),
            display: DisplayTarget::All,
            force: false,
        }
    }

    fn append_options(
        session_dir: PathBuf,
        interval: Option<Duration>,
        force: bool,
    ) -> SessionOpenOptions {
        SessionOpenOptions {
            library: None,
            session: Some(session_dir),
            append: true,
            interval,
            display: DisplayTarget::All,
            force,
        }
    }

    #[test]
    fn append_with_matching_interval_succeeds() {
        let temp = TempDir::new().unwrap();
        let session_dir = temp.path().join("session");
        Session::open(new_session_options(session_dir.clone(), 6)).unwrap();

        let session = Session::open(append_options(
            session_dir,
            Some(Duration::from_secs(6)),
            false,
        ))
        .unwrap();

        assert_eq!(session.capture_interval(), Duration::from_secs(6));
        assert!(session.warnings().is_empty());
    }

    #[test]
    fn append_without_interval_inherits_session_metadata() {
        let temp = TempDir::new().unwrap();
        let session_dir = temp.path().join("session");
        Session::open(new_session_options(session_dir.clone(), 12)).unwrap();

        let session = Session::open(append_options(session_dir, None, false)).unwrap();

        assert_eq!(session.capture_interval(), Duration::from_secs(12));
        assert!(session.warnings().is_empty());
    }

    #[test]
    fn append_with_mismatched_interval_fails_without_force() {
        let temp = TempDir::new().unwrap();
        let session_dir = temp.path().join("session");
        Session::open(new_session_options(session_dir.clone(), 6)).unwrap();

        let err = Session::open(append_options(
            session_dir,
            Some(Duration::from_secs(10)),
            false,
        ))
        .unwrap_err();

        assert!(matches!(err, TimelapseError::AppendMetadataMismatch { .. }));
    }

    #[test]
    fn append_with_mismatched_interval_warns_with_force_and_does_not_rewrite_metadata() {
        let temp = TempDir::new().unwrap();
        let session_dir = temp.path().join("session");
        let session = Session::open(new_session_options(session_dir.clone(), 6)).unwrap();
        let before = fs::read_to_string(&session.paths().metadata_file).unwrap();

        let session = Session::open(append_options(
            session_dir,
            Some(Duration::from_secs(10)),
            true,
        ))
        .unwrap();
        let after = fs::read_to_string(&session.paths().metadata_file).unwrap();

        assert_eq!(session.capture_interval(), Duration::from_secs(10));
        assert_eq!(before, after);
        assert_eq!(session.metadata().interval_seconds, 6);
        assert_eq!(session.warnings().len(), 1);
    }

    #[test]
    fn append_with_missing_metadata_fails_without_force() {
        let temp = TempDir::new().unwrap();
        let session_dir = temp.path().join("session");
        fs::create_dir_all(session_dir.join("frames")).unwrap();

        let err = Session::open(append_options(session_dir, None, false)).unwrap_err();

        assert!(matches!(err, TimelapseError::MetadataRead { .. }));
    }

    #[test]
    fn append_with_missing_metadata_warns_with_force() {
        let temp = TempDir::new().unwrap();
        let session_dir = temp.path().join("session");
        fs::create_dir_all(session_dir.join("frames")).unwrap();

        let session = Session::open(append_options(session_dir.clone(), None, true)).unwrap();

        assert_eq!(session.capture_interval(), DEFAULT_INTERVAL);
        assert_eq!(session.warnings().len(), 1);
        assert!(!session_dir.join("session.toml").exists());
    }
}
