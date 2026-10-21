//! Video tracking settings (VDMX's Vision tracking): which picture to watch and what to look for. The app runs
//! the detection (Apple Vision on macOS) and writes what it finds into Hand modulators named `Track …`, so any
//! control can follow a face, a hand or a body (`Composition::set_hand`).

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tracking {
    pub enabled: bool,
    /// What to watch: a layer or group id, or `MASTER_ID` for the mix.
    pub source: u64,
    pub faces: bool,
    pub hands: bool,
    pub body: bool,
    /// 0 = values jump to each new find, towards 1 = they glide (less jitter, more lag).
    pub smoothing: f32,
}

impl Default for Tracking {
    fn default() -> Self {
        Self { enabled: false, source: crate::MASTER_ID, faces: true, hands: false, body: false, smoothing: 0.5 }
    }
}

impl Tracking {
    /// The modulators tracking fills in, for what's switched on.
    pub fn outputs(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.faces {
            out.extend(FACE);
        }
        if self.hands {
            out.extend(HAND);
        }
        if self.body {
            out.extend(BODY);
        }
        out
    }
}

/// Face: the biggest face's centre and size (share of the picture height); Found = 1 while one is seen.
pub const FACE: [&str; 4] = ["Track Face X", "Track Face Y", "Track Face Size", "Track Face Found"];
/// Hand: the index fingertip of the first hand, and how far the thumb and index tips are apart (a pinch = 0).
pub const HAND: [&str; 4] = ["Track Hand X", "Track Hand Y", "Track Pinch", "Track Hand Found"];
/// Body: the hips' centre and both wrists' height.
pub const BODY: [&str; 5] = ["Track Body X", "Track Body Y", "Track Left Wrist Y", "Track Right Wrist Y", "Track Body Found"];

/// Glides `current` towards `found` by `smoothing` (0 = jump).
pub fn smooth_toward(current: f32, found: f32, smoothing: f32) -> f32 {
    let k = 1.0 - smoothing.clamp(0.0, 0.95);
    current + (found - current) * k
}
