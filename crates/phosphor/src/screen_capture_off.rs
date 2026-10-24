//! Screen capture in the build for older Macs (no ScreenCaptureKit, which needs macOS 12.3): the same shapes as
//! `screen_capture.rs`, and every capture says why it can't start.

use crate::media::video::surface::PixelBuffer;

const NEEDS: &str = "Screen and window capture need macOS 13 or later and the Apple Silicon / standard build of Phosphor";

/// Something to capture: `url` = "display:<id>" or "window:<id>".
#[derive(Clone, Debug)]
pub struct Target {
    pub label: String,
    pub url: String,
}

pub fn targets() -> (Vec<Target>, Option<String>) {
    (Vec::new(), Some(NEEDS.into()))
}

pub struct ScreenCapture;

impl ScreenCapture {
    pub fn start(_url: &str) -> Self {
        ScreenCapture
    }

    pub fn take(&self) -> Option<PixelBuffer> {
        None
    }

    pub fn error(&self) -> Option<String> {
        Some(NEEDS.into())
    }
}

pub struct SystemSound;

pub const SYSTEM_SOUND_RATE: u32 = 48_000;

impl SystemSound {
    pub fn start(_push: impl Fn(&[[f32; 2]]) + Send + Sync + 'static) -> Self {
        SystemSound
    }

    pub fn error(&self) -> Option<String> {
        Some("Hearing this Mac's sound needs macOS 13 or later and the standard build of Phosphor".into())
    }
}
