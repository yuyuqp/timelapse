use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use image::RgbaImage;

use crate::error::Result;
use crate::engine::session::{DisplayTarget, Session};

mod xcap_backend;

pub use xcap_backend::XcapBackend;

#[derive(Debug)]
pub struct CapturedImage {
    image: RgbaImage,
}

impl CapturedImage {
    pub fn new(image: RgbaImage) -> Self {
        Self { image }
    }

    pub fn save(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        self.image.save(path)?;
        Ok(())
    }
}

pub trait CaptureBackend {
    fn name(&self) -> &'static str;
    fn capture(&self, display: DisplayTarget) -> Result<CapturedImage>;
}

#[derive(Debug, Clone)]
pub struct CaptureLoop {
    interval: Duration,
    display: DisplayTarget,
    should_stop: Arc<AtomicBool>,
}

impl CaptureLoop {
    pub fn new(interval: Duration, display: DisplayTarget) -> Self {
        Self {
            interval,
            display,
            should_stop: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn stop_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.should_stop)
    }

    pub fn run<B: CaptureBackend>(&self, session: &mut Session, backend: &B) -> Result<u64> {
        self.run_with_progress(session, backend, |_| Ok(()))
    }

    pub fn run_with_progress<B, F>(
        &self,
        session: &mut Session,
        backend: &B,
        mut on_frame: F,
    ) -> Result<u64>
    where
        B: CaptureBackend,
        F: FnMut(u64) -> Result<()>,
    {
        let mut captured = 0;
        tracing::info!(
            "Starting capture loop using backend '{}' (interval: {}s, display: {:?})",
            backend.name(),
            self.interval.as_secs(),
            self.display
        );

        while !self.should_stop.load(Ordering::SeqCst) {
            let frame_index = session.frame_store().next_index();
            let path = session.frame_store().next_frame_path();
            tracing::debug!("Capturing frame index {} to {}", frame_index, path.display());
            let image = backend.capture(self.display);
            match image {
                Ok(img) => {
                    if let Err(e) = img.save(&path) {
                        tracing::error!("Failed to save captured frame {}: {}", frame_index, e);
                        return Err(e);
                    }
                    session.frame_store_mut().advance();
                    captured += 1;
                    on_frame(frame_index)?;
                }
                Err(e) => {
                    tracing::error!("Failed to capture display: {}", e);
                    return Err(e);
                }
            }

            sleep_interruptibly(self.interval, &self.should_stop);
        }

        tracing::info!("Capture loop stopped. Total frames collected: {}", captured);
        Ok(captured)
    }
}

fn sleep_interruptibly(interval: Duration, should_stop: &AtomicBool) {
    let step = Duration::from_millis(100);
    let mut slept = Duration::ZERO;

    while slept < interval && !should_stop.load(Ordering::SeqCst) {
        let remaining = interval.saturating_sub(slept);
        let nap = remaining.min(step);
        thread::sleep(nap);
        slept += nap;
    }
}
