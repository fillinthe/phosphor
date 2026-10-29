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
    /// Read QR codes (and barcodes) in the picture: the newest one's contents (`Composition::seen_code`), and
    /// "Track Code Found".
    pub codes: bool,
    /// Read text in the picture (`Composition::seen_text`).
    pub read_text: bool,
}

impl Default for Tracking {
    fn default() -> Self {
        Self { enabled: false, source: crate::MASTER_ID, faces: true, hands: false, body: false, smoothing: 0.5, codes: false, read_text: false }
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
    /// Puts `layer` on the face: its X, Y and size follow face tracking (a mask, a logo, a picture over the face).
    pub fn follow_face(&mut self, layer: u64) -> bool {
        self.tracking.enabled = true;
        self.tracking.faces = true;
        let mut links = Vec::new();
        for name in &FACE[..3] {
            if self.hand(name).is_none() {
                self.set_hand(name, 0.0);
            }
            let Some(m) = self.modulators.iter().find(|m| m.name == *name) else { return false };
            links.push(m.id);
        }
        let Some(l) = self.layer_mut(layer) else { return false };
        // Range mode over each control's range: X / Y = face − 0.5 (offsets from the centre), scale ≈ 1.3 × the
        // face's height (scale runs 0.01 to 8).
        let follow = |link: u64, min: f32, max: f32| Some(super::Modulation { link, range: true, min, max, amount: 1.0, ..Default::default() });
        l.transform.x_mod = follow(links[0], 0.25, 0.75);
        l.transform.y_mod = follow(links[1], 0.25, 0.75);
        l.transform.scale_mod = follow(links[2], 0.0, 1.3 / 7.99);
        l.transform.corners = None;
        true
    }

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
        let fx = super::EffectState { id, effect: desc.id.to_string(), enabled: true, values: desc.defaults(), mix: 1.0, mods, blend: Default::default(), other: 0, light: 0 };
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
    fn a_layer_follows_the_face() {
        let mut c = crate::Composition::default();
        c.layers.push(crate::LayerState { id: 3, ..Default::default() });
        assert!(c.follow_face(3));
        let t = &c.layers[0].transform;
        let inputs = |x: f32, y: f32, size: f32| {
            let mut i = crate::control::ModInputs::default();
            let ids: Vec<u64> = c.modulators.iter().map(|m| m.id).collect();
            i.shared[0] = (ids[0], x);
            i.shared[1] = (ids[1], y);
            i.shared[2] = (ids[2], size);
            i
        };
        let at = inputs(0.75, 0.25, 0.4);
        let m = |v, lo, hi, md: &Option<crate::Modulation>| crate::control::modulate(v, lo, hi, md.as_ref(), &at);
        assert!((m(t.x, -1.0, 1.0, &t.x_mod) - 0.25).abs() < 1e-4, "face right of centre");
        assert!((m(t.y, -1.0, 1.0, &t.y_mod) + 0.25).abs() < 1e-4, "face below centre");
        assert!((m(t.scale, 0.01, 8.0, &t.scale_mod) - (0.01 + 0.4 * 1.3)).abs() < 1e-3, "about the face's size");
        assert!(!c.follow_face(99));
    }

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
