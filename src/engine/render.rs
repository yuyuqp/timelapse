use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::{Result, TimelapseError};
use crate::engine::session::Library;

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
    pub exclude: Option<Vec<u64>>,
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
    pub exclude: Vec<u64>,
}

#[derive(Debug, Clone)]
pub struct RenderResult {
    pub output_path: PathBuf,
    pub frame_count: usize,
}

fn clean_spaces_around_hyphen(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '-' {
            while result.ends_with(' ') {
                result.pop();
            }
            result.push('-');
            i += 1;
            while i < chars.len() && chars[i] == ' ' {
                i += 1;
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

fn tokenize_exclusions(raw: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut quote_char = '"';

    for c in raw.chars() {
        if in_quotes {
            if c == quote_char {
                in_quotes = false;
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            } else {
                current.push(c);
            }
        } else if c == '"' || c == '\'' {
            in_quotes = true;
            quote_char = c;
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
        } else if c == ' ' || c == ',' || c == ';' {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

pub fn parse_exclusions(raw: &str) -> std::result::Result<Vec<u64>, String> {
    let mut excluded = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cleaned = clean_spaces_around_hyphen(line);
        let tokens = tokenize_exclusions(&cleaned);

        for token in tokens {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }

            // Try to see if it's a file path/filename first by parsing as Path
            let path = Path::new(token);
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if !stem.is_empty() && stem.bytes().all(|b| b.is_ascii_digit()) {
                    if let Ok(val) = stem.parse::<u64>() {
                        excluded.push(val);
                        continue;
                    }
                }
            }

            if token.contains('-') {
                let subparts: Vec<&str> = token.split('-').collect();
                if subparts.len() != 2 {
                    return Err(format!("Invalid range format: '{}'", token));
                }
                let start = subparts[0].trim().parse::<u64>().map_err(|e| format!("invalid number in range: {}", e))?;
                let end = subparts[1].trim().parse::<u64>().map_err(|e| format!("invalid number in range: {}", e))?;
                if start > end {
                    return Err(format!("Start of range cannot be greater than end: '{}'", token));
                }
                for i in start..=end {
                    excluded.push(i);
                }
            } else {
                let val = token.parse::<u64>().map_err(|e| format!("invalid token '{}': {}", token, e))?;
                excluded.push(val);
            }
        }
    }
    excluded.sort_unstable();
    excluded.dedup();
    Ok(excluded)
}

fn read_exclude_file(dir: &Path) -> Option<Vec<u64>> {
    let path = dir.join("exclude.txt");
    if !path.is_file() {
        return None;
    }
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to read exclude.txt at {}: {}", path.display(), e);
            return None;
        }
    };
    let mut excluded = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match parse_exclusions(line) {
            Ok(mut parsed) => excluded.append(&mut parsed),
            Err(e) => {
                tracing::warn!("Failed to parse line '{}' in exclude.txt: {}", line, e);
            }
        }
    }
    excluded.sort_unstable();
    excluded.dedup();
    Some(excluded)
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

    let mut exclude = options.exclude.unwrap_or_default();
    if exclude.is_empty() {
        if let Some(txt_exclude) = read_exclude_file(&resolved.target_path) {
            exclude = txt_exclude;
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
        exclude,
    })
}

pub fn render(options: RenderOptions) -> Result<RenderResult> {
    tracing::info!("Creating render plan for options: {:?}", options);
    let plan = create_render_plan(options)?;
    let actual_frame_count = plan.sequence.frame_count - plan.exclude.iter().filter(|&&x| x >= plan.sequence.start_number && x <= plan.sequence.end_number).count();
    tracing::info!(
        "Executing ffmpeg render plan (frames count: {}, output: {}, fps: {})",
        actual_frame_count,
        plan.output_path.display(),
        plan.fps
    );
    match run_ffmpeg(&plan) {
        Ok(_) => {
            tracing::info!("Render completed successfully to {}", plan.output_path.display());
            Ok(RenderResult {
                output_path: plan.output_path,
                frame_count: actual_frame_count,
            })
        }
        Err(e) => {
            tracing::error!("Ffmpeg rendering failed: {}", e);
            Err(e)
        }
    }
}

pub fn run_ffmpeg(plan: &RenderPlan) -> Result<()> {
    if plan.exclude.is_empty() {
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
                tracing::error!("ffmpeg not found in PATH");
                TimelapseError::FfmpegNotFound
            } else {
                tracing::error!("Failed to execute ffmpeg command: {}", err);
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
    } else {
        let duration = 1.0 / plan.fps as f64;
        let mut concat_content = String::new();
        let mut last_frame_path = None;
        let mut included_count = 0;

        for i in plan.sequence.start_number..=plan.sequence.end_number {
            if plan.exclude.contains(&i) {
                continue;
            }
            let frame_name = format!("{:0width$}.png", i, width = plan.sequence.padding);
            let frame_path = plan.sequence.frames_dir.join(&frame_name);
            let path_str = frame_path.to_string_lossy().replace('\\', "/");
            concat_content.push_str(&format!("file '{}'\nduration {}\n", path_str, duration));
            last_frame_path = Some(path_str);
            included_count += 1;
        }

        if included_count == 0 {
            return Err(TimelapseError::InvalidArgument(
                "All frames in the sequence are excluded; nothing to render.".to_string(),
            ));
        }

        if let Some(ref path_str) = last_frame_path {
            concat_content.push_str(&format!("file '{}'\n", path_str));
        }

        let concat_file_path = plan.sequence.frames_dir.join(format!(".concat_{}.txt", plan.sequence.start_number));
        fs::write(&concat_file_path, concat_content)?;

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
            .arg("-f")
            .arg("concat")
            .arg("-safe")
            .arg("0")
            .arg("-i")
            .arg(&concat_file_path)
            .arg("-r")
            .arg(plan.fps.to_string())
            .arg("-c:v")
            .arg("libx264")
            .arg("-pix_fmt")
            .arg("yuv420p")
            .arg("-movflags")
            .arg("+faststart")
            .arg(&plan.output_path);

        let output = command.output();
        let _ = fs::remove_file(&concat_file_path);

        let output = output.map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                tracing::error!("ffmpeg not found in PATH");
                TimelapseError::FfmpegNotFound
            } else {
                tracing::error!("Failed to execute ffmpeg command: {}", err);
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

        if self.exclude.is_empty() {
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
        } else {
            let concat_file_name = format!(".concat_{}.txt", self.sequence.start_number);
            args.extend([
                OsString::from(if self.overwrite { "-y" } else { "-n" }),
                OsString::from("-f"),
                OsString::from("concat"),
                OsString::from("-safe"),
                OsString::from("0"),
                OsString::from("-i"),
                OsString::from(concat_file_name),
                OsString::from("-r"),
                OsString::from(self.fps.to_string()),
                OsString::from("-c:v"),
                OsString::from("libx264"),
                OsString::from("-pix_fmt"),
                OsString::from("yuv420p"),
                OsString::from("-movflags"),
                OsString::from("+faststart"),
            ]);
        }
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

pub fn resolve_frames_dir(target: RenderTarget) -> Result<PathBuf> {
    let resolved = resolve_target(target)?;
    Ok(resolved.frames_dir)
}

pub fn resolve_target_path(target: RenderTarget) -> Result<PathBuf> {
    let resolved = resolve_target(target)?;
    Ok(resolved.target_path)
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
            exclude: None,
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
            exclude: None,
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
            exclude: None,
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
            exclude: None,
        })
        .unwrap_err();

        assert!(matches!(err, TimelapseError::OutputExists(_)));
    }

    #[test]
    fn test_parse_exclusions() {
        assert_eq!(parse_exclusions("1,2,5-7").unwrap(), vec![1, 2, 5, 6, 7]);
        assert_eq!(parse_exclusions(" 10 - 12 , 3 ").unwrap(), vec![3, 10, 11, 12]);
        assert_eq!(
            parse_exclusions("\"C:\\path\\000000005.png\" C:\\path\\000000006.png, 000000007.png; 10-12 3").unwrap(),
            vec![3, 5, 6, 7, 10, 11, 12]
        );
        assert_eq!(
            parse_exclusions("# This is a comment\n1\n2\n# Another comment\n3-5\n").unwrap(),
            vec![1, 2, 3, 4, 5]
        );
        assert!(parse_exclusions("abc").is_err());
        assert!(parse_exclusions("5-3").is_err());
    }

    #[test]
    fn render_plan_reads_exclude_txt() {
        let temp = TempDir::new().unwrap();
        let frames = temp.path().join("frames");
        fs::create_dir_all(&frames).unwrap();
        touch(&frames.join("0001.png"));
        touch(&frames.join("0002.png"));
        touch(&frames.join("0003.png"));

        fs::write(frames.join("exclude.txt"), "2\n3").unwrap();

        let plan = create_render_plan(RenderOptions {
            target: RenderTarget::Path(frames.clone()),
            fps: 15,
            output: None,
            overwrite: false,
            verbose: false,
            exclude: None,
        })
        .unwrap();

        assert_eq!(plan.exclude, vec![2, 3]);
    }

    #[test]
    fn test_real_render_with_exclusions() {
        let temp = TempDir::new().unwrap();
        let frames_dir = temp.path().join("frames");
        fs::create_dir_all(&frames_dir).unwrap();

        // Create 4 dummy valid PNG images: 0001.png, 0002.png, 0003.png, 0004.png
        for i in 1..=4 {
            let path = frames_dir.join(format!("{:04}.png", i));
            let img: image::ImageBuffer<image::Rgb<u8>, Vec<u8>> =
                image::ImageBuffer::from_pixel(2, 2, image::Rgb([255, 0, 0]));
            img.save(&path).unwrap();
        }

        let output_path = temp.path().join("output.mp4");

        let result = render(RenderOptions {
            target: RenderTarget::Path(frames_dir),
            fps: 2,
            output: Some(output_path.clone()),
            overwrite: true,
            verbose: false,
            exclude: Some(vec![2, 3]),
        })
        .unwrap();

        assert_eq!(result.frame_count, 2);
        assert!(result.output_path.exists());
        assert_eq!(result.output_path, output_path);
    }
}
