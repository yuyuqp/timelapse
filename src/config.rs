use std::path::PathBuf;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use directories::ProjectDirs;

use crate::engine::session::DisplayTarget;

pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(6);
pub const DEFAULT_DISPLAY: DisplayTarget = DisplayTarget::All;
pub const DEFAULT_FRAME_PADDING: usize = 9;
pub const DEFAULT_FRAME_START: u64 = 1;
pub const CAPTURE_BACKEND_NAME: &str = "xcap";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub theme: Theme,
    pub default_library: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Extra,
    Minimal,
}

impl Default for Theme {
    fn default() -> Self {
        Self::Extra
    }
}

impl AppConfig {
    pub fn file_path() -> Option<PathBuf> {
        ProjectDirs::from("com", "yuyuqp", "timelapse")
            .map(|proj| proj.config_dir().join("config.toml"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::file_path() else {
            return Self::default();
        };

        if !path.is_file() {
            return Self::default();
        }

        let Ok(content) = std::fs::read_to_string(&path) else {
            return Self::default();
        };

        toml::from_str(&content).unwrap_or_default()
    }

    pub fn save(&self) -> std::result::Result<(), String> {
        let Some(path) = Self::file_path() else {
            return Err("Failed to resolve config directory".to_string());
        };

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let content = toml::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;

        std::fs::write(&path, content)
            .map_err(|e| format!("Failed to write config file: {}", e))?;

        Ok(())
    }
}
