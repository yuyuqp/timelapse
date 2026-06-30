use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use timelapse::{CaptureBackend, CaptureLoop, DisplayTarget, SessionOpenOptions, XcapBackend};

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
}

#[derive(Debug, Parser)]
struct CollectArgs {
    /// Capture interval, such as 6s or 1m.
    #[arg(long, value_parser = parse_duration, default_value = "6s")]
    interval: Duration,

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
    })?;

    let capture_loop = CaptureLoop::new(interval, display);
    install_ctrlc_handler(&capture_loop)?;

    let backend = XcapBackend;
    eprintln!("session: {}", session.paths().session_dir.display());
    eprintln!("frames: {}", session.paths().frames_dir.display());
    eprintln!("capture backend: {}", backend.name());
    eprintln!("press Ctrl+C to stop");

    let captured = capture_loop
        .run(&mut session, &backend)
        .context("capture loop failed")?;

    eprintln!("stopped after {captured} frame(s)");
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
