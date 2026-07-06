use image::{RgbaImage, imageops};
use xcap::Monitor;

use crate::capture::{CaptureBackend, CapturedImage};
use crate::error::{Result, TimelapseError};
use crate::engine::session::DisplayTarget;

#[derive(Debug, Default, Clone, Copy)]
pub struct XcapBackend;

impl CaptureBackend for XcapBackend {
    fn name(&self) -> &'static str {
        "xcap"
    }

    fn capture(&self, display: DisplayTarget) -> Result<CapturedImage> {
        match display {
            DisplayTarget::All => capture_all(),
            DisplayTarget::Primary => capture_primary(),
        }
    }
}

fn capture_primary() -> Result<CapturedImage> {
    let monitors = monitors()?;
    let monitor = monitors
        .iter()
        .find(|monitor| monitor.is_primary().unwrap_or(false))
        .or_else(|| monitors.first())
        .ok_or(TimelapseError::NoDisplays)?;

    let image = monitor
        .capture_image()
        .map_err(|err| TimelapseError::Capture(err.to_string()))?;
    Ok(CapturedImage::new(image))
}

fn capture_all() -> Result<CapturedImage> {
    let monitors = monitors()?;
    if monitors.is_empty() {
        return Err(TimelapseError::NoDisplays);
    }

    let mut captures = Vec::with_capacity(monitors.len());
    for monitor in monitors {
        let x = monitor
            .x()
            .map_err(|err| TimelapseError::Capture(err.to_string()))?;
        let y = monitor
            .y()
            .map_err(|err| TimelapseError::Capture(err.to_string()))?;
        let image = monitor
            .capture_image()
            .map_err(|err| TimelapseError::Capture(err.to_string()))?;
        captures.push((x, y, image));
    }

    let min_x = captures.iter().map(|(x, _, _)| *x).min().unwrap_or(0);
    let min_y = captures.iter().map(|(_, y, _)| *y).min().unwrap_or(0);
    let max_x = captures
        .iter()
        .map(|(x, _, image)| *x + image.width() as i32)
        .max()
        .unwrap_or(0);
    let max_y = captures
        .iter()
        .map(|(_, y, image)| *y + image.height() as i32)
        .max()
        .unwrap_or(0);

    let width = (max_x - min_x).max(1) as u32;
    let height = (max_y - min_y).max(1) as u32;
    let mut combined = RgbaImage::new(width, height);

    for (x, y, image) in captures {
        imageops::overlay(
            &mut combined,
            &image,
            i64::from(x - min_x),
            i64::from(y - min_y),
        );
    }

    Ok(CapturedImage::new(combined))
}

fn monitors() -> Result<Vec<Monitor>> {
    Monitor::all().map_err(|err| TimelapseError::Capture(err.to_string()))
}
