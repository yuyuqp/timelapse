use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use timelapse::{
    CaptureBackend, CaptureLoop, CleanOptions, DEFAULT_RENDER_FPS, DisplayTarget, RenderOptions,
    RenderTarget, SessionOpenOptions, SessionTarget, XcapBackend,
};

#[derive(Debug, Parser)]
#[command(name = "timelapse")]
#[command(about = "Session-based screenshot collection")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Collect screenshots into a session folder.
    Collect(CollectArgs),
    /// Render a session or frame directory to MP4.
    Render(RenderArgs),
    /// List sessions in the Timelapse library.
    List(ListArgs),
    /// Open a session in the system file manager.
    Open(OpenArgs),
    /// Permanently delete generated files from a session.
    Clean(CleanArgs),
}

#[derive(Debug, Parser)]
struct CollectArgs {
    /// Capture interval, such as 6s or 1m. Append uses session metadata when omitted.
    #[arg(long, value_parser = parse_duration)]
    interval: Option<Duration>,

    /// Display target to capture.
    #[arg(long, value_enum, default_value_t = DisplayArg::All)]
    display: DisplayArg,

    /// Timelapse library root. New sessions are created under PATH/sessions/.
    #[arg(long)]
    library: Option<PathBuf>,

    /// Exact session directory for this capture run.
    #[arg(long)]
    session: Option<PathBuf>,

    /// Append to an existing explicit session directory.
    #[arg(long)]
    append: bool,

    /// Allow append to continue when existing session metadata is missing or mismatched.
    #[arg(long)]
    force: bool,
}

#[derive(Debug, Parser)]
struct RenderArgs {
    /// Render target: latest, a session directory, or a numbered PNG frames directory.
    target: String,

    /// Timelapse library root used with the latest target.
    #[arg(long)]
    library: Option<PathBuf>,

    /// Output MP4 path.
    #[arg(long)]
    output: Option<PathBuf>,

    /// Render frames per second.
    #[arg(long, default_value_t = DEFAULT_RENDER_FPS)]
    fps: u32,

    /// Overwrite the output file if it already exists.
    #[arg(long)]
    overwrite: bool,

    /// Show ffmpeg output while rendering.
    #[arg(long)]
    verbose: bool,
}

#[derive(Debug, Parser)]
struct ListArgs {
    /// Timelapse library root.
    #[arg(long)]
    library: Option<PathBuf>,
}

#[derive(Debug, Parser)]
struct OpenArgs {
    /// Session target: latest or a session directory.
    target: String,

    /// Timelapse library root used with the latest target.
    #[arg(long)]
    library: Option<PathBuf>,
}

#[derive(Debug, Parser)]
struct CleanArgs {
    /// Session target: latest or a session directory.
    target: String,

    /// Timelapse library root used with the latest target.
    #[arg(long)]
    library: Option<PathBuf>,

    /// Delete numbered PNG frames from the session frames/ directory.
    #[arg(long)]
    frames: bool,

    /// Delete MP4 videos from the session directory.
    #[arg(long)]
    videos: bool,

    /// Skip the confirmation prompt.
    #[arg(short = 'y', long)]
    yes: bool,

    /// Show what would be deleted without deleting files.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DisplayArg {
    All,
    Primary,
}

impl From<DisplayArg> for DisplayTarget {
    fn from(value: DisplayArg) -> Self {
        match value {
            DisplayArg::All => DisplayTarget::All,
            DisplayArg::Primary => DisplayTarget::Primary,
        }
    }
}

pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Collect(args) => run_collect(args),
        Commands::Render(args) => run_render(args),
        Commands::List(args) => run_list(args),
        Commands::Open(args) => run_open(args),
        Commands::Clean(args) => run_clean(args),
    }
}

fn run_collect(args: CollectArgs) -> anyhow::Result<()> {
    if args.library.is_some() && args.session.is_some() {
        anyhow::bail!("--library cannot be used with --session");
    }

    let display = DisplayTarget::from(args.display);
    let interval = args.interval;
    let mut session = timelapse::Session::open(SessionOpenOptions {
        library: args.library,
        session: args.session,
        append: args.append,
        interval,
        display,
        force: args.force,
    })?;

    for warning in session.warnings() {
        eprintln!("{warning}");
    }

    let capture_loop = CaptureLoop::new(session.capture_interval(), display);
    install_ctrlc_handler(&capture_loop)?;

    let backend = XcapBackend;
    eprintln!("session: {}", session.paths().session_dir.display());
    eprintln!("frames: {}", session.paths().frames_dir.display());
    eprintln!("capture backend: {}", backend.name());
    eprintln!("press Ctrl+C to stop");

    let mut stdout = io::stdout().lock();
    let captured = capture_loop
        .run_with_progress(&mut session, &backend, |frame_index| {
            write!(stdout, "\rFrame {frame_index}")?;
            stdout.flush()?;
            Ok(())
        })
        .context("capture loop failed")?;

    writeln!(stdout)?;
    stdout.flush()?;

    eprintln!("stopped after {captured} frame(s)");
    Ok(())
}

fn run_render(args: RenderArgs) -> anyhow::Result<()> {
    if args.library.is_some() && args.target != "latest" {
        anyhow::bail!("--library can only be used with `timelapse render latest`");
    }

    let target = if args.target == "latest" {
        RenderTarget::Latest {
            library: args.library,
        }
    } else {
        RenderTarget::Path(PathBuf::from(args.target))
    };

    let options = RenderOptions {
        target,
        fps: args.fps,
        output: args.output,
        overwrite: args.overwrite,
        verbose: args.verbose,
    };

    let plan = timelapse::render::create_render_plan(options.clone())?;
    eprintln!("frames: {}", plan.sequence.frames_dir.display());
    eprintln!("frames found: {}", plan.sequence.frame_count);
    eprintln!("input pattern: {}", plan.sequence.input_pattern());
    eprintln!("output: {}", plan.output_path.display());
    eprintln!("fps: {}", plan.fps);

    let result = timelapse::render::render(options).context("render failed")?;
    eprintln!(
        "rendered {} frame(s) to {}",
        result.frame_count,
        result.output_path.display()
    );
    Ok(())
}

fn run_list(args: ListArgs) -> anyhow::Result<()> {
    let sessions = timelapse::list_sessions(args.library)?;
    if sessions.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }

    for session in sessions {
        let frame_text = session.frames.as_ref().map_or_else(
            || "0 frames".to_string(),
            |frames| {
                format!(
                    "{} frames ({}-{})",
                    frames.frame_count, frames.start_number, frames.end_number
                )
            },
        );
        let video_text = match session.videos.len() {
            0 => "0 videos".to_string(),
            1 => "1 video".to_string(),
            count => format!("{count} videos"),
        };

        println!("{}", session.name);
        println!("  path: {}", session.path.display());
        println!("  {frame_text}, {video_text}");

        if let Some(metadata) = session.metadata {
            println!(
                "  started: {}, interval: {}s, display: {}",
                metadata.started_at, metadata.interval_seconds, metadata.display
            );
        }
        if let Some(error) = session.metadata_error {
            println!("  metadata: {error}");
        }
        if let Some(error) = session.frame_error {
            println!("  frames: {error}");
        }
    }

    Ok(())
}

fn run_open(args: OpenArgs) -> anyhow::Result<()> {
    if args.library.is_some() && args.target != "latest" {
        anyhow::bail!("--library can only be used with `timelapse open latest`");
    }

    let target = if args.target == "latest" {
        SessionTarget::Latest {
            library: args.library,
        }
    } else {
        SessionTarget::Path(PathBuf::from(args.target))
    };

    let path = timelapse::open_session(target)?;
    eprintln!("opened {}", path.display());
    Ok(())
}

fn run_clean(args: CleanArgs) -> anyhow::Result<()> {
    if args.library.is_some() && args.target != "latest" {
        anyhow::bail!("--library can only be used with `timelapse clean latest`");
    }

    let target = session_target(args.target, args.library);
    let options = CleanOptions {
        target,
        frames: args.frames,
        videos: args.videos,
    };
    let plan = timelapse::create_clean_plan(options)?;

    if plan.frames.is_empty() && plan.videos.is_empty() {
        println!("Nothing to delete in {}.", plan.session_path.display());
        return Ok(());
    }

    if args.dry_run {
        println!("Dry run: would delete files from:");
    } else {
        println!("About to permanently delete files from:");
    }
    println!("  {}", plan.session_path.display());
    println!("  frames: {}", plan.frames.len());
    println!("  videos: {}", plan.videos.len());

    if args.dry_run {
        println!("No files deleted.");
        return Ok(());
    }

    if !args.yes && !confirm_clean()? {
        println!("Clean cancelled.");
        return Ok(());
    }

    let result = timelapse::execute_clean_plan(&plan)?;
    println!(
        "Deleted {} frame(s) and {} video(s).",
        result.frames_deleted, result.videos_deleted
    );
    Ok(())
}

fn session_target(target: String, library: Option<PathBuf>) -> SessionTarget {
    if target == "latest" {
        SessionTarget::Latest { library }
    } else {
        SessionTarget::Path(PathBuf::from(target))
    }
}

fn confirm_clean() -> anyhow::Result<bool> {
    print!("Proceed? [y/N]: ");
    io::stdout().flush()?;

    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn parse_duration(raw: &str) -> Result<Duration, String> {
    let duration = humantime::parse_duration(raw).map_err(|err| err.to_string())?;
    if duration.is_zero() {
        return Err("interval must be greater than zero".to_string());
    }
    if duration.subsec_nanos() != 0 {
        return Err("interval must be a whole number of seconds".to_string());
    }
    Ok(duration)
}

fn install_ctrlc_handler(capture_loop: &CaptureLoop) -> anyhow::Result<()> {
    let should_stop = capture_loop.stop_handle();
    ctrlc::set_handler(move || {
        should_stop.store(true, Ordering::SeqCst);
    })
    .context("failed to install Ctrl+C handler")
}
