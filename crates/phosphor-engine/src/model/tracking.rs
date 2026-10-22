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

impl super::Composition {
    /// Hides faces in the chain `owner` (a layer, group or `MASTER_ID`): face tracking on, and a Hide Face effect
    /// whose X, Y, Size and Found follow the face. Returns the effect's id.
    pub fn add_face_hider(&mut self, owner: u64) -> Option<u64> {
        self.tracking.enabled = true;
        self.tracking.faces = true;
        let mut links = Vec::new();
        for name in FACE {
            if self.hand(name).is_none() {
                self.set_hand(name, 0.0);
            }
            links.push(self.modulators.iter().find(|m| m.name == name)?.id);
        }
        let desc = crate::shaders::effect("hide_face")?;
        let id = self.new_id();
        // Each follows its modulator straight (range mode over the whole 0..1). Face order: X, Y, Size, Found.
        let mods = links.iter().enumerate().map(|(index, &link)| super::ParamMod { index, m: super::Modulation { link, range: true, min: 0.0, max: 1.0, amount: 1.0, ..Default::default() } }).collect();
        let fx = super::EffectState { id, effect: desc.id.to_string(), enabled: true, values: desc.defaults(), mix: 1.0, mods, blend: Default::default() };
        self.effects_mut(owner)?.push(fx);
        Some(id)
    }
}

/// Glides `current` towards `found` by `smoothing` (0 = jump).
pub fn smooth_toward(current: f32, found: f32, smoothing: f32) -> f32 {
    let k = 1.0 - smoothing.clamp(0.0, 0.95);
    current + (found - current) * k
}

#[cfg(test)]
mod hider_tests {
    #[test]
    fn a_face_hider_follows_the_face() {
        let mut c = crate::Composition::default();
        let id = c.add_face_hider(crate::MASTER_ID).expect("added");
        assert!(c.tracking.enabled && c.tracking.faces);
        let fx = c.master_effects.iter().find(|e| e.id == id).unwrap();
        let x = c.modulators.iter().find(|m| m.name == "Track Face X").unwrap().id;
        assert_eq!((fx.mods[0].index, fx.mods[0].m.link, fx.mods.len()), (0, x, 4));
        c.add_face_hider(crate::MASTER_ID);
        assert_eq!(c.modulators.len(), 4, "the same four modulators");
    }
}
