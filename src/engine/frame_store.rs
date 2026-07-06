use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Result, TimelapseError};

#[derive(Debug, Clone)]
pub struct FrameStore {
    frames_dir: PathBuf,
    padding: usize,
    next_index: u64,
}

impl FrameStore {
    pub fn create_new(frames_dir: PathBuf, padding: usize, first_index: u64) -> Result<Self> {
        fs::create_dir_all(&frames_dir)?;
        Ok(Self {
            frames_dir,
            padding,
            next_index: first_index,
        })
    }

    pub fn open_append(frames_dir: PathBuf, padding: usize, first_index: u64) -> Result<Self> {
        if !frames_dir.exists() {
            return Err(TimelapseError::InvalidSession {
                path: frames_dir,
                message: "frames directory does not exist".to_string(),
            });
        }
        if !frames_dir.is_dir() {
            return Err(TimelapseError::InvalidSession {
                path: frames_dir,
                message: "frames path is not a directory".to_string(),
            });
        }

        let next_index = Self::next_index_from_existing(&frames_dir)?.unwrap_or(first_index);
        Ok(Self {
            frames_dir,
            padding,
            next_index,
        })
    }

    pub fn frames_dir(&self) -> &Path {
        &self.frames_dir
    }

    pub fn next_index(&self) -> u64 {
        self.next_index
    }

    pub fn next_frame_path(&self) -> PathBuf {
        self.frames_dir.join(format!(
            "{:0width$}.png",
            self.next_index,
            width = self.padding
        ))
    }

    pub fn advance(&mut self) {
        self.next_index += 1;
    }

    fn next_index_from_existing(frames_dir: &Path) -> Result<Option<u64>> {
        let mut max_index = None;

        for entry in fs::read_dir(frames_dir)? {
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

            let index = stem
                .parse::<u64>()
                .map_err(|_| TimelapseError::InvalidSession {
                    path: path.clone(),
                    message: "frame number is too large".to_string(),
                })?;
            max_index = Some(max_index.map_or(index, |current: u64| current.max(index)));
        }

        Ok(max_index.map(|index| index + 1))
    }
}
