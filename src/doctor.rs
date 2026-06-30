use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;
use crate::error::Result;
use crate::session::Library;
use xcap::Monitor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Ok,
    Warn,
    Error,
}

impl std::fmt::Display for CheckStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckStatus::Ok => write!(f, "[ok]"),
            CheckStatus::Warn => write!(f, "[warn]"),
            CheckStatus::Error => write!(f, "[error]"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DoctorCheck {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
    pub is_required: bool,
}

fn find_existing_ancestor(path: &Path) -> Option<PathBuf> {
    let mut current = path.to_path_buf();
    while !current.exists() {
        if let Some(parent) = current.parent() {
            current = parent.to_path_buf();
        } else {
            return None;
        }
    }
    Some(current)
}

pub fn run_diagnostics(library: Option<PathBuf>) -> Result<Vec<DoctorCheck>> {
    let mut checks = Vec::new();

    // 1. Resolve library path
    let library_root = match library {
        Some(path) => path,
        None => match Library::default_path() {
            Ok(path) => path,
            Err(err) => {
                checks.push(DoctorCheck {
                    name: "library".to_string(),
                    status: CheckStatus::Error,
                    message: format!("Failed to resolve default library path: {}", err),
                    is_required: true,
                });
                return Ok(checks);
            }
        }
    };

    let library_exists = library_root.exists();
    let library_is_dir = library_root.is_dir();

    // Check 1: Library path check
    if library_exists {
        if library_is_dir {
            checks.push(DoctorCheck {
                name: "library".to_string(),
                status: CheckStatus::Ok,
                message: format!("{}", library_root.display()),
                is_required: true,
            });
        } else {
            checks.push(DoctorCheck {
                name: "library".to_string(),
                status: CheckStatus::Error,
                message: format!("{} exists but is not a directory", library_root.display()),
                is_required: true,
            });
        }
    } else {
        checks.push(DoctorCheck {
            name: "library".to_string(),
            status: CheckStatus::Ok,
            message: format!(
                "{} (does not exist yet; will be created by collect)",
                library_root.display()
            ),
            is_required: true,
        });
    }

    // Check 2: Sessions directory check
    let sessions_dir = library_root.join("sessions");
    if sessions_dir.exists() {
        if sessions_dir.is_dir() {
            checks.push(DoctorCheck {
                name: "sessions dir".to_string(),
                status: CheckStatus::Ok,
                message: format!("{}", sessions_dir.display()),
                is_required: false,
            });
        } else {
            checks.push(DoctorCheck {
                name: "sessions dir".to_string(),
                status: CheckStatus::Error,
                message: format!("{} exists but is not a directory", sessions_dir.display()),
                is_required: false,
            });
        }
    } else {
        checks.push(DoctorCheck {
            name: "sessions dir".to_string(),
            status: CheckStatus::Warn,
            message: format!(
                "{} (does not exist; will be created by collect)",
                sessions_dir.display()
            ),
            is_required: false,
        });
    }

    // Check 3: Screenshot backend (xcap)
    match Monitor::all() {
        Ok(monitors) => {
            if monitors.is_empty() {
                checks.push(DoctorCheck {
                    name: "screenshot backend".to_string(),
                    status: CheckStatus::Error,
                    message: "xcap (no displays detected)".to_string(),
                    is_required: true,
                });
            } else {
                let display_count = monitors.len();
                let mut details = Vec::new();
                for (i, m) in monitors.iter().enumerate() {
                    let name = m.name().unwrap_or_else(|_| format!("Display {}", i));
                    let width = m.width().unwrap_or(0);
                    let height = m.height().unwrap_or(0);
                    let primary = if m.is_primary().unwrap_or(false) { " (primary)" } else { "" };
                    details.push(format!("\"{}\" {}x{}{}", name, width, height, primary));
                }
                checks.push(DoctorCheck {
                    name: "screenshot backend".to_string(),
                    status: CheckStatus::Ok,
                    message: format!(
                        "xcap (displays detected: {}, details: {})",
                        display_count,
                        details.join(", ")
                    ),
                    is_required: true,
                });
            }
        }
        Err(err) => {
            checks.push(DoctorCheck {
                name: "screenshot backend".to_string(),
                status: CheckStatus::Error,
                message: format!("xcap (failed to query displays: {})", err),
                is_required: true,
            });
        }
    }

    // Check 4: ffmpeg
    match Command::new("ffmpeg").arg("-version").output() {
        Ok(output) => {
            if output.status.success() {
                let stdout_str = String::from_utf8_lossy(&output.stdout);
                let first_line = stdout_str.lines().next().unwrap_or("").trim().to_string();
                checks.push(DoctorCheck {
                    name: "ffmpeg".to_string(),
                    status: CheckStatus::Ok,
                    message: first_line,
                    is_required: true,
                });
            } else {
                checks.push(DoctorCheck {
                    name: "ffmpeg".to_string(),
                    status: CheckStatus::Error,
                    message: format!("ffmpeg returned non-zero exit status: {}", output.status),
                    is_required: true,
                });
            }
        }
        Err(_) => {
            checks.push(DoctorCheck {
                name: "ffmpeg".to_string(),
                status: CheckStatus::Error,
                message: "ffmpeg not found in PATH; render will not work.".to_string(),
                is_required: true,
            });
        }
    }

    // Check 5: Basic Write Test
    let test_dir = if library_exists && library_is_dir {
        Some(library_root.clone())
    } else {
        find_existing_ancestor(&library_root)
    };

    if let Some(dir) = test_dir {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let temp_filename = format!("timelapse_doctor_write_test_{}.tmp", timestamp);
        let temp_path = dir.join(temp_filename);

        match fs::write(&temp_path, b"timelapse doctor write test") {
            Ok(_) => {
                let _ = fs::remove_file(&temp_path);
                if library_exists && library_is_dir {
                    checks.push(DoctorCheck {
                        name: "write test".to_string(),
                        status: CheckStatus::Ok,
                        message: "library directory is writable".to_string(),
                        is_required: true,
                    });
                } else {
                    checks.push(DoctorCheck {
                        name: "write test".to_string(),
                        status: CheckStatus::Ok,
                        message: format!(
                            "nearest existing parent directory {} is writable",
                            dir.display()
                        ),
                        is_required: true,
                    });
                }
            }
            Err(err) => {
                if library_exists && library_is_dir {
                    checks.push(DoctorCheck {
                        name: "write test".to_string(),
                        status: CheckStatus::Error,
                        message: format!("library directory is not writable: {}", err),
                        is_required: true,
                    });
                } else {
                    checks.push(DoctorCheck {
                        name: "write test".to_string(),
                        status: CheckStatus::Error,
                        message: format!(
                            "nearest existing parent directory {} is not writable: {}",
                            dir.display(),
                            err
                        ),
                        is_required: true,
                    });
                }
            }
        }
    } else {
        checks.push(DoctorCheck {
            name: "write test".to_string(),
            status: CheckStatus::Error,
            message: "no existing parent directory found to test writability".to_string(),
            is_required: true,
        });
    }

    Ok(checks)
}
