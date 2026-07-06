use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Result, TimelapseError};
use crate::engine::render::FrameSequence;
use crate::engine::session::{Library, SessionMetadata, SessionPaths};

#[derive(Debug, Clone)]
pub enum SessionTarget {
    Latest { library: Option<PathBuf> },
    Path(PathBuf),
}

#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub path: PathBuf,
    pub name: String,
    pub metadata: Option<SessionMetadata>,
    pub metadata_error: Option<String>,
    pub frames: Option<FrameSequence>,
    pub frame_error: Option<String>,
    pub videos: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct CleanOptions {
    pub target: SessionTarget,
    pub frames: bool,
    pub videos: bool,
}

#[derive(Debug, Clone)]
pub struct CleanPlan {
    pub session_path: PathBuf,
    pub frames: Vec<PathBuf>,
    pub videos: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CleanResult {
    pub frames_deleted: usize,
    pub videos_deleted: usize,
}

pub fn list_sessions(library: Option<PathBuf>) -> Result<Vec<SessionSummary>> {
    let sessions_dir = sessions_dir(library)?;
    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }
    if !sessions_dir.is_dir() {
        return Err(TimelapseError::InvalidSession {
            path: sessions_dir,
            message: "sessions path is not a directory".to_string(),
        });
    }

    let mut sessions = Vec::new();
    for entry in fs::read_dir(&sessions_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            sessions.push(inspect_session(path)?);
        }
    }

    sessions.sort_by(|left, right| right.name.cmp(&left.name));
    Ok(sessions)
}

pub fn resolve_session_target(target: SessionTarget) -> Result<PathBuf> {
    match target {
        SessionTarget::Latest { library } => latest_session(library),
        SessionTarget::Path(path) => {
            if !path.exists() {
                return Err(TimelapseError::InvalidSession {
                    path,
                    message: "session path does not exist".to_string(),
                });
            }
            if !path.is_dir() {
                return Err(TimelapseError::InvalidSession {
                    path,
                    message: "session path is not a directory".to_string(),
                });
            }
            Ok(absolute_from_current(path)?)
        }
    }
}

pub fn open_session(target: SessionTarget) -> Result<PathBuf> {
    tracing::info!("Opening session for target: {:?}", target);
    let path = resolve_session_target(target)?;
    tracing::info!("Opening system file manager at {:?}", path);
    open_file_manager(&path)?;
    Ok(path)
}

pub fn create_clean_plan(options: CleanOptions) -> Result<CleanPlan> {
    if !options.frames && !options.videos {
        return Err(TimelapseError::InvalidArgument(
            "select at least one clean target: --frames and/or --videos".to_string(),
        ));
    }

    let session_path = resolve_session_target(options.target)?;
    ensure_recognized_session(&session_path)?;

    let paths = SessionPaths::new(session_path.clone());
    let frames = if options.frames {
        find_numbered_pngs(&paths.frames_dir)?
    } else {
        Vec::new()
    };
    let videos = if options.videos {
        find_videos(&session_path)?
    } else {
        Vec::new()
    };

    Ok(CleanPlan {
        session_path,
        frames,
        videos,
    })
}

pub fn clean_session(options: CleanOptions) -> Result<CleanResult> {
    let plan = create_clean_plan(options)?;
    execute_clean_plan(&plan)
}

pub fn execute_clean_plan(plan: &CleanPlan) -> Result<CleanResult> {
    tracing::info!(
        "Executing clean plan for session {}: deleting {} frames, {} videos",
        plan.session_path.display(),
        plan.frames.len(),
        plan.videos.len()
    );
    for frame in &plan.frames {
        tracing::debug!("Deleting frame file: {:?}", frame);
        fs::remove_file(frame)?;
    }
    for video in &plan.videos {
        tracing::debug!("Deleting video file: {:?}", video);
        fs::remove_file(video)?;
    }

    let result = CleanResult {
        frames_deleted: plan.frames.len(),
        videos_deleted: plan.videos.len(),
    };
    tracing::info!("Clean execution complete. Result: {:?}", result);
    Ok(result)
}

fn inspect_session(path: PathBuf) -> Result<SessionSummary> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("<unnamed>")
        .to_string();
    let paths = SessionPaths::new(path.clone());
    let (metadata, metadata_error) = read_metadata(&paths.metadata_file);
    let (frames, frame_error) = read_frames(&paths.frames_dir);
    let videos = find_videos(&path)?;

    Ok(SessionSummary {
        path,
        name,
        metadata,
        metadata_error,
        frames,
        frame_error,
        videos,
    })
}

fn latest_session(library: Option<PathBuf>) -> Result<PathBuf> {
    let sessions_dir = sessions_dir(library)?;
    if !sessions_dir.is_dir() {
        return Err(TimelapseError::NoSessions(sessions_dir));
    }

    let mut sessions = Vec::new();
    for entry in fs::read_dir(&sessions_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            sessions.push(path);
        }
    }
    sessions.sort_by(|left, right| left.file_name().cmp(&right.file_name()));

    sessions
        .pop()
        .ok_or(TimelapseError::NoSessions(sessions_dir))
}

fn sessions_dir(library: Option<PathBuf>) -> Result<PathBuf> {
    let library_root = match library {
        Some(path) => path,
        None => Library::default_path()?,
    };
    Ok(Library::new(library_root).sessions_dir())
}

fn read_metadata(path: &Path) -> (Option<SessionMetadata>, Option<String>) {
    match fs::read_to_string(path) {
        Ok(text) => match toml::from_str(&text) {
            Ok(metadata) => (Some(metadata), None),
            Err(err) => (None, Some(format!("metadata parse failed: {err}"))),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => (None, None),
        Err(err) => (None, Some(format!("metadata read failed: {err}"))),
    }
}

fn read_frames(path: &Path) -> (Option<FrameSequence>, Option<String>) {
    if !path.exists() {
        return (None, None);
    }

    match FrameSequence::scan(path) {
        Ok(sequence) => (Some(sequence), None),
        Err(TimelapseError::InvalidFrameSequence { message, .. })
            if message == "no numbered .png frames were found" =>
        {
            (None, None)
        }
        Err(err) => (None, Some(err.to_string())),
    }
}

fn find_videos(path: &Path) -> Result<Vec<PathBuf>> {
    let mut videos = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("mp4") {
            videos.push(path);
        }
    }
    videos.sort();
    Ok(videos)
}

fn find_numbered_pngs(path: &Path) -> Result<Vec<PathBuf>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    if !path.is_dir() {
        return Err(TimelapseError::InvalidSession {
            path: path.to_path_buf(),
            message: "frames path is not a directory".to_string(),
        });
    }

    let mut frames = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("png") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if !stem.is_empty() && stem.bytes().all(|byte| byte.is_ascii_digit()) {
            frames.push(path);
        }
    }
    frames.sort();
    Ok(frames)
}

fn ensure_recognized_session(path: &Path) -> Result<()> {
    let paths = SessionPaths::new(path.to_path_buf());
    if paths.frames_dir.is_dir() || paths.metadata_file.is_file() {
        return Ok(());
    }

    Err(TimelapseError::InvalidSession {
        path: path.to_path_buf(),
        message: "expected a session folder with frames/ and/or session.toml".to_string(),
    })
}

pub fn open_file_manager_select(file_path: &Path) -> Result<()> {
    let mut command = if cfg!(target_os = "windows") {
        let mut command = Command::new("explorer");
        command.arg(format!("/select,{}", file_path.display()));
        command
    } else if cfg!(target_os = "macos") {
        let mut command = Command::new("open");
        command.arg("-R");
        command.arg(file_path);
        command
    } else {
        let parent = file_path.parent().unwrap_or(file_path);
        let mut command = Command::new("xdg-open");
        command.arg(parent);
        command
    };

    command
        .spawn()
        .map_err(|err| TimelapseError::OpenFileManager {
            path: file_path.to_path_buf(),
            message: err.to_string(),
        })?;
    Ok(())
}

fn open_file_manager(path: &Path) -> Result<()> {
    let mut command = if cfg!(target_os = "windows") {
        let mut command = Command::new("explorer");
        command.arg(path);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = Command::new("open");
        command.arg(path);
        command
    } else {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    };

    command
        .spawn()
        .map_err(|err| TimelapseError::OpenFileManager {
            path: path.to_path_buf(),
            message: err.to_string(),
        })?;
    Ok(())
}

fn absolute_from_current(path: PathBuf) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use tempfile::TempDir;

    use super::*;

    fn touch(path: &Path) {
        File::create(path).unwrap();
    }

    #[test]
    fn list_sessions_returns_newest_first_with_counts() {
        let temp = TempDir::new().unwrap();
        let library = temp.path().join("library");
        let old_frames = library
            .join("sessions")
            .join("2026-01-01_010101")
            .join("frames");
        let new_session = library.join("sessions").join("2026-02-01_010101");
        let new_frames = new_session.join("frames");
        fs::create_dir_all(&old_frames).unwrap();
        fs::create_dir_all(&new_frames).unwrap();
        touch(&old_frames.join("000000001.png"));
        touch(&new_frames.join("000000001.png"));
        touch(&new_frames.join("000000002.png"));
        touch(&new_session.join("2026-02-01_010101.mp4"));

        let sessions = list_sessions(Some(library)).unwrap();

        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].name, "2026-02-01_010101");
        assert_eq!(sessions[0].frames.as_ref().unwrap().frame_count, 2);
        assert_eq!(sessions[0].videos.len(), 1);
    }

    #[test]
    fn resolve_latest_session_uses_newest_name() {
        let temp = TempDir::new().unwrap();
        let library = temp.path().join("library");
        fs::create_dir_all(library.join("sessions").join("2026-01-01_010101")).unwrap();
        fs::create_dir_all(library.join("sessions").join("2026-03-01_010101")).unwrap();

        let latest = resolve_session_target(SessionTarget::Latest {
            library: Some(library),
        })
        .unwrap();

        assert!(latest.ends_with(Path::new("sessions").join("2026-03-01_010101")));
    }

    #[test]
    fn clean_plan_only_targets_numbered_frames_and_session_videos() {
        let temp = TempDir::new().unwrap();
        let session = temp.path().join("session");
        let frames = session.join("frames");
        fs::create_dir_all(&frames).unwrap();
        touch(&frames.join("000000001.png"));
        touch(&frames.join("000000002.png"));
        touch(&frames.join("not-a-frame.png"));
        touch(&session.join("session.mp4"));
        touch(&session.join("notes.txt"));

        let plan = create_clean_plan(CleanOptions {
            target: SessionTarget::Path(session),
            frames: true,
            videos: true,
        })
        .unwrap();

        assert_eq!(plan.frames.len(), 2);
        assert_eq!(plan.videos.len(), 1);
    }

    #[test]
    fn clean_session_deletes_selected_files() {
        let temp = TempDir::new().unwrap();
        let session = temp.path().join("session");
        let frames = session.join("frames");
        fs::create_dir_all(&frames).unwrap();
        let frame = frames.join("000000001.png");
        let video = session.join("session.mp4");
        touch(&frame);
        touch(&video);

        let result = clean_session(CleanOptions {
            target: SessionTarget::Path(session),
            frames: true,
            videos: false,
        })
        .unwrap();

        assert_eq!(
            result,
            CleanResult {
                frames_deleted: 1,
                videos_deleted: 0,
            }
        );
        assert!(!frame.exists());
        assert!(video.exists());
    }
}
