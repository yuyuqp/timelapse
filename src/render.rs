use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::{Result, TimelapseError};
use crate::session::Library;

pub const DEFAULT_RENDER_FPS: u32 = 15;

#[derive(Debug, Clone)]
pub enum RenderTarget {
    Latest { library: Option<PathBuf> },
    Path(PathBuf),
}

#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub target: RenderTarget,
    pub fps: u32,
    pub output: Option<PathBuf>,
    pub overwrite: bool,
    pub verbose: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameSequence {
    pub frames_dir: PathBuf,
    pub padding: usize,
    pub start_number: u64,
    pub end_number: u64,
    pub frame_count: usize,
}

impl FrameSequence {
    pub fn scan(frames_dir: impl Into<PathBuf>) -> Result<Self> {
        let frames_dir = frames_dir.into();
        if !frames_dir.exists() {
            return Err(TimelapseError::InvalidFrameSequence {
                path: frames_dir,
                message: "frames directory does not exist".to_string(),
            });
        }
        if !frames_dir.is_dir() {
            return Err(TimelapseError::InvalidFrameSequence {
                path: frames_dir,
                message: "frames path is not a directory".to_string(),
            });
        }

        let mut frames = Vec::new();
        for entry in fs::read_dir(&frames_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("png") {
                continue;
            }

            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if stem.is_empty() || !stem.bytes().all(|byte| byte.is_ascii_digit()) {
                continue;
            }

            let number = stem
                .parse::<u64>()
                .map_err(|_| TimelapseError::InvalidFrameSequence {
                    path: path.clone(),
                    message: "frame number is too large".to_string(),
                })?;
            frames.push((number, stem.len(), path));
        }

        if frames.is_empty() {
            return Err(TimelapseError::InvalidFrameSequence {
                path: frames_dir,
                message: "no numbered .png frames were found".to_string(),
            });
        }

        frames.sort_by_key(|(number, _, _)| *number);
        let padding = frames[0].1;
        if let Some((_, _, path)) = frames.iter().find(|(_, width, _)| *width != padding) {
            return Err(TimelapseError::InvalidFrameSequence {
                path: path.clone(),
                message: "numbered PNG frames must use consistent zero-padding".to_string(),
            });
        }

        let start_number = frames[0].0;
        let mut expected = start_number;
        for (number, _, _) in &frames {
            if *number != expected {
                return Err(TimelapseError::InvalidFrameSequence {
                    path: frames_dir,
                    message: format!("missing frame {expected}"),
                });
            }
            expected += 1;
        }

        let end_number = expected - 1;
        let frame_count = frames.len();
        Ok(Self {
            frames_dir,
            padding,
            start_number,
            end_number,
            frame_count,
        })
    }

    pub fn input_pattern(&self) -> String {
        format!("%0{}d.png", self.padding)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderSourceKind {
    Session,
    FramesDirectory,
}

#[derive(Debug, Clone)]
pub struct RenderPlan {
    pub source_kind: RenderSourceKind,
    pub target_path: PathBuf,
    pub sequence: FrameSequence,
    pub output_path: PathBuf,
    pub fps: u32,
    pub overwrite: bool,
    pub verbose: bool,
}

#[derive(Debug, Clone)]
pub struct RenderResult {
    pub output_path: PathBuf,
    pub frame_count: usize,
}

pub fn create_render_plan(options: RenderOptions) -> Result<RenderPlan> {
    if options.fps == 0 {
        return Err(TimelapseError::InvalidArgument(
            "--fps must be greater than zero".to_string(),
        ));
    }

    let resolved = resolve_target(options.target)?;
    let sequence = FrameSequence::scan(resolved.frames_dir.clone())?;
    let output_path = match options.output {
        Some(path) => absolute_from_current(path)?,
        None => resolved.default_output_path,
    };

    if output_path.exists() && !options.overwrite {
        return Err(TimelapseError::OutputExists(output_path));
    }
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(TimelapseError::InvalidRenderTarget {
                path: output_path,
                message: "output parent directory does not exist".to_string(),
            });
        }
    }

    Ok(RenderPlan {
        source_kind: resolved.source_kind,
        target_path: resolved.target_path,
        sequence,
        output_path,
        fps: options.fps,
        overwrite: options.overwrite,
        verbose: options.verbose,
    })
}

pub fn render(options: RenderOptions) -> Result<RenderResult> {
    let plan = create_render_plan(options)?;
    run_ffmpeg(&plan)?;
    Ok(RenderResult {
        output_path: plan.output_path,
        frame_count: plan.sequence.frame_count,
    })
}

pub fn run_ffmpeg(plan: &RenderPlan) -> Result<()> {
    let mut command = Command::new("ffmpeg");
    command
        .current_dir(&plan.sequence.frames_dir)
        .stdin(Stdio::null());

    if plan.verbose {
        command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    } else {
        command
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error");
    }

    command
        .arg(if plan.overwrite { "-y" } else { "-n" })
        .arg("-framerate")
        .arg(plan.fps.to_string())
        .arg("-start_number")
        .arg(plan.sequence.start_number.to_string())
        .arg("-i")
        .arg(plan.sequence.input_pattern())
        .arg("-c:v")
        .arg("libx264")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-movflags")
        .arg("+faststart")
        .arg(&plan.output_path);

    let output = command.output().map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            TimelapseError::FfmpegNotFound
        } else {
            TimelapseError::Io(err)
        }
    })?;

    if !output.status.success() {
        let stderr = if plan.verbose {
            "see ffmpeg output above".to_string()
        } else {
            String::from_utf8_lossy(&output.stderr).trim().to_string()
        };
        return Err(TimelapseError::FfmpegFailed {
            status: output.status.to_string(),
            stderr: if stderr.is_empty() {
                "no ffmpeg error output".to_string()
            } else {
                stderr
            },
        });
    }

    Ok(())
}

impl RenderPlan {
    pub fn ffmpeg_args(&self) -> Vec<OsString> {
        let mut args = Vec::new();
        if !self.verbose {
            args.extend([
                OsString::from("-hide_banner"),
                OsString::from("-loglevel"),
                OsString::from("error"),
            ]);
        }

        args.extend([
            OsString::from(if self.overwrite { "-y" } else { "-n" }),
            OsString::from("-framerate"),
            OsString::from(self.fps.to_string()),
            OsString::from("-start_number"),
            OsString::from(self.sequence.start_number.to_string()),
            OsString::from("-i"),
            OsString::from(self.sequence.input_pattern()),
            OsString::from("-c:v"),
            OsString::from("libx264"),
            OsString::from("-pix_fmt"),
            OsString::from("yuv420p"),
            OsString::from("-movflags"),
            OsString::from("+faststart"),
        ]);
        args.push(self.output_path.clone().into_os_string());
        args
    }
}

#[derive(Debug)]
struct ResolvedRenderTarget {
    source_kind: RenderSourceKind,
    target_path: PathBuf,
    frames_dir: PathBuf,
    default_output_path: PathBuf,
}

fn resolve_target(target: RenderTarget) -> Result<ResolvedRenderTarget> {
    match target {
        RenderTarget::Latest { library } => resolve_latest(library),
        RenderTarget::Path(path) => resolve_path_target(path),
    }
}

fn resolve_latest(library: Option<PathBuf>) -> Result<ResolvedRenderTarget> {
    let library_root = match library {
        Some(path) => path,
        None => Library::default_path()?,
    };
    let sessions_dir = Library::new(library_root).sessions_dir();
    if !sessions_dir.is_dir() {
        return Err(TimelapseError::InvalidRenderTarget {
            path: sessions_dir,
            message: "sessions directory does not exist".to_string(),
        });
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

    let latest = sessions
        .pop()
        .ok_or_else(|| TimelapseError::InvalidRenderTarget {
            path: sessions_dir,
            message: "no sessions were found".to_string(),
        })?;

    resolve_path_target(latest)
}

fn resolve_path_target(path: PathBuf) -> Result<ResolvedRenderTarget> {
    if !path.exists() {
        return Err(TimelapseError::InvalidRenderTarget {
            path,
            message: "target path does not exist".to_string(),
        });
    }
    if !path.is_dir() {
        return Err(TimelapseError::InvalidRenderTarget {
            path,
            message: "target path is not a directory".to_string(),
        });
    }

    let target_path = absolute_from_current(path)?;
    let session_frames_dir = target_path.join("frames");
    if session_frames_dir.is_dir() {
        return Ok(ResolvedRenderTarget {
            source_kind: RenderSourceKind::Session,
            default_output_path: target_path.join(default_output_file_name(&target_path)?),
            target_path,
            frames_dir: session_frames_dir,
        });
    }

    if contains_numbered_png(&target_path)? {
        return Ok(ResolvedRenderTarget {
            source_kind: RenderSourceKind::FramesDirectory,
            default_output_path: target_path.join(default_output_file_name(&target_path)?),
            frames_dir: target_path.clone(),
            target_path,
        });
    }

    Err(TimelapseError::InvalidRenderTarget {
        path: target_path,
        message: "expected a session directory with frames/ or a frames directory with numbered PNG files".to_string(),
    })
}

fn contains_numbered_png(path: &Path) -> Result<bool> {
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
            return Ok(true);
        }
    }
    Ok(false)
}

fn default_output_file_name(path: &Path) -> Result<PathBuf> {
    let Some(name) = path.file_name() else {
        return Err(TimelapseError::InvalidRenderTarget {
            path: path.to_path_buf(),
            message: "target directory has no name".to_string(),
        });
    };

    let mut file_name = name.to_os_string();
    file_name.push(".mp4");
    Ok(PathBuf::from(file_name))
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
    fn frame_sequence_scans_padding_start_and_count() {
        let temp = TempDir::new().unwrap();
        touch(&temp.path().join("000000003.png"));
        touch(&temp.path().join("000000001.png"));
        touch(&temp.path().join("000000002.png"));
        touch(&temp.path().join("notes.txt"));

        let sequence = FrameSequence::scan(temp.path()).unwrap();

        assert_eq!(sequence.padding, 9);
        assert_eq!(sequence.start_number, 1);
        assert_eq!(sequence.end_number, 3);
        assert_eq!(sequence.frame_count, 3);
        assert_eq!(sequence.input_pattern(), "%09d.png");
    }

    #[test]
    fn frame_sequence_fails_on_gap() {
        let temp = TempDir::new().unwrap();
        touch(&temp.path().join("000000001.png"));
        touch(&temp.path().join("000000003.png"));

        let err = FrameSequence::scan(temp.path()).unwrap_err();

        assert!(matches!(err, TimelapseError::InvalidFrameSequence { .. }));
        assert!(err.to_string().contains("missing frame 2"));
    }

    #[test]
    fn frame_sequence_fails_on_mixed_padding() {
        let temp = TempDir::new().unwrap();
        touch(&temp.path().join("001.png"));
        touch(&temp.path().join("0002.png"));

        let err = FrameSequence::scan(temp.path()).unwrap_err();

        assert!(matches!(err, TimelapseError::InvalidFrameSequence { .. }));
        assert!(err.to_string().contains("consistent zero-padding"));
    }

    #[test]
    fn render_plan_resolves_session_default_output() {
        let temp = TempDir::new().unwrap();
        let session = temp.path().join("2026-06-29_220503");
        let frames = session.join("frames");
        fs::create_dir_all(&frames).unwrap();
        touch(&frames.join("000000001.png"));

        let plan = create_render_plan(RenderOptions {
            target: RenderTarget::Path(session.clone()),
            fps: 15,
            output: None,
            overwrite: false,
            verbose: false,
        })
        .unwrap();

        assert_eq!(plan.source_kind, RenderSourceKind::Session);
        assert_eq!(plan.sequence.frames_dir, frames);
        assert_eq!(plan.output_path, session.join("2026-06-29_220503.mp4"));
    }

    #[test]
    fn render_plan_resolves_plain_frames_default_output() {
        let temp = TempDir::new().unwrap();
        let frames = temp.path().join("frames");
        fs::create_dir_all(&frames).unwrap();
        touch(&frames.join("0001.png"));

        let plan = create_render_plan(RenderOptions {
            target: RenderTarget::Path(frames.clone()),
            fps: 24,
            output: None,
            overwrite: false,
            verbose: false,
        })
        .unwrap();

        assert_eq!(plan.source_kind, RenderSourceKind::FramesDirectory);
        assert_eq!(plan.sequence.input_pattern(), "%04d.png");
        assert_eq!(plan.output_path, frames.join("frames.mp4"));
        assert_eq!(plan.fps, 24);
    }

    #[test]
    fn render_plan_resolves_latest_session_by_name() {
        let temp = TempDir::new().unwrap();
        let library = temp.path().join("library");
        let old_frames = library
            .join("sessions")
            .join("2026-01-01_010101")
            .join("frames");
        let new_frames = library
            .join("sessions")
            .join("2026-02-01_010101")
            .join("frames");
        fs::create_dir_all(&old_frames).unwrap();
        fs::create_dir_all(&new_frames).unwrap();
        touch(&old_frames.join("000000001.png"));
        touch(&new_frames.join("000000001.png"));

        let plan = create_render_plan(RenderOptions {
            target: RenderTarget::Latest {
                library: Some(library),
            },
            fps: 15,
            output: None,
            overwrite: false,
            verbose: false,
        })
        .unwrap();

        assert!(
            plan.target_path
                .ends_with(Path::new("sessions").join("2026-02-01_010101"))
        );
    }

    #[test]
    fn render_plan_refuses_existing_output_without_overwrite() {
        let temp = TempDir::new().unwrap();
        let frames = temp.path().join("frames");
        fs::create_dir_all(&frames).unwrap();
        touch(&frames.join("0001.png"));
        let output = temp.path().join("out.mp4");
        touch(&output);

        let err = create_render_plan(RenderOptions {
            target: RenderTarget::Path(frames),
            fps: 15,
            output: Some(output),
            overwrite: false,
            verbose: false,
        })
        .unwrap_err();

        assert!(matches!(err, TimelapseError::OutputExists(_)));
    }
}
