use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use timelapse::{
    CaptureBackend, CaptureLoop, DEFAULT_RENDER_FPS, DisplayTarget, RenderOptions, RenderTarget,
    SessionOpenOptions, XcapBackend,
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
