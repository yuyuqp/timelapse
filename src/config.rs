use std::time::Duration;

use crate::session::DisplayTarget;

pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(6);
pub const DEFAULT_DISPLAY: DisplayTarget = DisplayTarget::All;
pub const DEFAULT_FRAME_PADDING: usize = 9;
pub const DEFAULT_FRAME_START: u64 = 1;
pub const CAPTURE_BACKEND_NAME: &str = "xcap";
