use std::path::Path;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, Registry, EnvFilter};

/// Initializes the global tracing subscriber.
/// 
/// If `is_tui` is true, logs are directed to a file named `timelapse.log` in the library root.
/// Otherwise, logs are printed to `stderr` with ANSI styling enabled.
pub fn init_logging(is_tui: bool, library_root: &Path) -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    if is_tui {
        // In TUI mode: Log ONLY to timelapse.log in the library root to avoid breaking the UI screen
        std::fs::create_dir_all(library_root)?;
        let log_file = std::fs::File::create(library_root.join("timelapse.log"))?;
        let file_layer = fmt::layer()
            .with_writer(log_file)
            .with_ansi(false);
        
        let _ = Registry::default()
            .with(filter)
            .with(file_layer)
            .try_init();
    } else {
        // In CLI mode: Log to stderr
        let stderr_layer = fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(true);
        
        let _ = Registry::default()
            .with(filter)
            .with(stderr_layer)
            .try_init();
    }

    Ok(())
}
