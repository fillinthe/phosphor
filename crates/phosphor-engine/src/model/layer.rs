//! Layers, their effects, and modulation (LFOs, audio, beat…).

use serde::{Deserialize, Serialize};

use super::*;
use crate::shaders;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectState {
    pub id: u64,
    pub effect: String,
    pub enabled: bool,
    pub values: Vec<f32>,
    /// Dry/wet: 0 = input untouched, 1 = fully processed.
    #[serde(default = "full_mix")]
    pub mix: f32,
    #[serde(default)]
    pub mods: Vec<ParamMod>,
    /// How the effect's result lands on what went into it (Normal = the plain dry / wet mix).
    #[serde(default)]
    pub blend: BlendMode,
    /// Another layer whose picture the effect uses as well (a points render's colours), by id; 0 = none.
    #[serde(default)]
    pub other: u64,
}

fn default_transition_effect() -> String {
    "zoom_blur".into()
}

fn full_mix() -> f32 {
    1.0
}

/// A live signal (0..1) that can drive any parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModSource {
    AudioLevel,
    AudioLow,
    AudioMid,
    AudioHigh,
    LfoSine,
    LfoTriangle,
    LfoSaw,
    LfoSquare,
    /// Sawtooth that restarts on every beat of the BPM clock.
    Beat,
    /// A new random value every cycle (sample and hold).
    Random,
    /// An 8-step sequencer.
    Steps,
    /// A pulse on every beat the audio beat detector hears.
    AudioBeat,
    /// A value set by hand, a MIDI knob, OSC or the remote (shared modulators: their `value`).
    Hand,
    /// A wave you draw: a smooth loop through the eight `steps` points.
    Drawn,
}

impl ModSource {
    pub const ALL: [ModSource; 14] = [
        ModSource::AudioLevel,
        ModSource::AudioLow,
        ModSource::AudioMid,
        ModSource::AudioHigh,
        ModSource::AudioBeat,
        ModSource::LfoSine,
        ModSource::LfoTriangle,
        ModSource::LfoSaw,
        ModSource::LfoSquare,
        ModSource::Beat,
        ModSource::Random,
        ModSource::Steps,
        ModSource::Drawn,
        ModSource::Hand,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ModSource::AudioLevel => "Audio level",
            ModSource::AudioLow => "Audio low",
            ModSource::AudioMid => "Audio mid",
            ModSource::AudioHigh => "Audio high",
            ModSource::LfoSine => "LFO sine",
            ModSource::LfoTriangle => "LFO triangle",
            ModSource::LfoSaw => "Ramp (saw)",
            ModSource::LfoSquare => "LFO square",
            ModSource::Beat => "Beat (BPM)",
            ModSource::Random => "Random",
            ModSource::Steps => "Step sequencer",
            ModSource::AudioBeat => "Audio beat (kick)",
            ModSource::Hand => "Hand / MIDI knob",
            ModSource::Drawn => "Drawn wave",
        }
    }

    pub fn is_lfo(self) -> bool {
        matches!(
            self,
            ModSource::LfoSine | ModSource::LfoTriangle | ModSource::LfoSaw | ModSource::LfoSquare | ModSource::Random | ModSource::Steps | ModSource::Drawn
        )
    }

    pub fn is_audio(self) -> bool {
        matches!(self, ModSource::AudioLevel | ModSource::AudioLow | ModSource::AudioMid | ModSource::AudioHigh | ModSource::AudioBeat)
    }
}

/// How a signal moves a parameter: `amount` is a fraction of the parameter's range (−1..1).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Modulation {
    pub source: ModSource,
    pub amount: f32,
    /// Cycles per second, or beats per cycle when `sync` is on (ignored for audio sources).
    pub rate: f32,
    /// Lock the cycle to the BPM clock.
    pub sync: bool,
    /// Range mode: the signal sweeps the parameter between `min` and `max` (fractions of its
    /// range) and the slider is ignored. Otherwise the signal is added on top of the slider.
    pub range: bool,
    pub min: f32,
    pub max: f32,
    pub invert: bool,
    pub steps: [f32; 8],
    /// Follows a shared modulator (its id; 0 = its own signal above).
    pub link: u64,
}

impl Default for Modulation {
    fn default() -> Self {
        Self {
            source: ModSource::AudioLevel,
            amount: 0.5,
            rate: 1.0,
            sync: false,
            range: false,
            min: 0.0,
            max: 1.0,
            invert: false,
            steps: [1.0, 0.0, 0.5, 0.0, 1.0, 0.25, 0.75, 0.0],
            link: 0,
        }
    }
}

/// A modulation attached to parameter `index` of an effect or generator.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParamMod {
    pub index: usize,
    pub m: Modulation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayerState {
    pub id: u64,
    pub name: String,
    pub source: SourceSpec,
    pub enabled: bool,
    /// When any layer is soloed, only soloed layers reach the mix.
    pub solo: bool,
    /// Skip this layer's effect chain (quick A/B without touching the chain).
    pub fx_bypass: bool,
    /// Blended into the mix. Off = only mapping surfaces that pick this layer show it.
    pub in_mix: bool,
    pub opacity: f32,
    pub blend: BlendMode,
    pub fit: FitMode,
    pub generator_values: Vec<f32>,
    pub generator_mods: Vec<ParamMod>,
    pub opacity_mod: Option<Modulation>,
    /// Position, size, rotation and flips in the composition.
    pub transform: LayerTransform,
    /// Movie sound volume (1 = as recorded).
    pub volume: f32,
    /// Sound fades with the layer's opacity (and its group's).
    pub volume_follows_opacity: bool,
    /// Sound off (keeps the volume for when it's back on).
    pub muted: bool,
    pub effects: Vec<EffectState>,
    pub playing: bool,
    pub rate: f32,
    pub looping: bool,
    /// Not looping: black once the movie ends (instead of holding its last frame).
    pub black_at_end: bool,
    /// Generators draw at this share of the canvas size (1 = full; 0.5 = a quarter of the pixels, scaled up), to save
    /// the GPU on heavy shaders. Their effects run at that size too.
    pub render_scale: f32,
    /// Draw this layer (source and effects) this many times a second and hold the picture between (0 = every frame).
    pub render_fps: f32,
    /// The jump back / forward buttons' step, in beats of playback (or seconds with `jump_in_seconds`).
    pub jump: f32,
    pub jump_in_seconds: bool,
    pub direction: PlayDirection,
    /// Crossfader side (Mixer).
    pub group: CrossGroup,
    /// The layer group it belongs to (`Composition::groups`); members sit next to each other in the stack.
    pub parent: Option<u64>,
    /// The comp it's in (`Composition::comps`); None or a gone comp = the first comp.
    pub comp: Option<u64>,
    /// Blended by a mixer plugin instead of `blend` (`shaders::mixer` id, FFGL), and its settings.
    pub mixer: Option<String>,
    /// Behind a Designer box (`Comp::network`): made and removed with the box, kept out of the layer panels.
    pub designer: bool,
    pub mixer_values: Vec<f32>,
    /// Loop region as fractions of the movie (0..1), so it survives switching clips and maps like any slider.
    pub loop_in: f32,
    pub loop_out: f32,
    /// Beat loop: when > 0 the loop is this many beats long from `loop_in` (overrides `loop_out`).
    pub loop_beats: f32,
    /// BPM sync: when > 0 the movie (or its loop) is stretched to last this many beats; speed follows the tempo.
    pub sync_beats: f32,
    /// Clip slots, by column.
    pub clips: Vec<Clip>,
    pub transition_secs: f32,
    pub transition_kind: TransitionKind,
    /// Effect for the "Through an effect" fade style (effect id).
    #[serde(default = "default_transition_effect")]
    pub transition_effect: String,
    /// A shader transition's own settings (one value per parameter; empty or a different length = its defaults).
    #[serde(default)]
    pub transition_values: Vec<f32>,
    /// Which pair of the sound output's channels its sound plays on (0 = 1-2, 1 = 3-4…; a multichannel interface).
    pub sound_pair: u8,
    /// Another sound device its sound plays on (by name; None = the main sound output).
    #[serde(default)]
    pub sound_device: Option<String>,
    /// Where this row's clips play (this layer, or shared over the layers beside it).
    pub voices: VoiceMode,
    /// Whose turn it is next (`VoiceMode::NextLayer`).
    #[serde(skip)]
    pub voice_turn: usize,
    /// Plays the next / previous / a random clip by itself.
    pub autopilot: Autopilot,
    /// Autopilot moves on after this many beats (0 = when the clip ends).
    pub autopilot_beats: f32,
    /// Play this media collection instead of the clip row: the autopilot steps through it (next / previous /
    /// random), new files in its watched folders included.
    #[serde(default)]
    pub collection: Option<String>,
    /// What a flash clip goes back to when released.
    #[serde(skip)]
    pub flash_return: Option<Box<Clip>>,
    #[serde(skip)]
    pub transition: Option<Transition>,
    #[serde(skip)]
    pub active_clip: Option<u64>,
    /// The playing clip's (or file's) own effects: they run before the layer's and follow the clip / file.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub clip_effects: Vec<EffectState>,
    /// Whose effects `clip_effects` are.
    #[serde(skip_serializing_if = "crate::EffectsOwner::is_none")]
    pub clip_effects_from: crate::EffectsOwner,
}

/// Where a layer sits in the composition (applied when it's mixed; effects run before it).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayerTransform {
    /// Offset from the centre as a fraction of the composition (x right, y up).
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    /// Extra width / height stretch (1 = none).
    pub stretch_x: f32,
    pub stretch_y: f32,
    /// Degrees, clockwise.
    pub rotation: f32,
    pub flip_x: bool,
    pub flip_y: bool,
    pub scale_mod: Option<Modulation>,
    pub rotation_mod: Option<Modulation>,
    /// Movement on the position (a face to follow, an LFO sway…).
    pub x_mod: Option<Modulation>,
    pub y_mod: Option<Modulation>,
    /// Cut off the picture's edges (left, right, top, bottom; fractions of the picture), before it's placed.
    pub crop: [f32; 4],
    /// Corner pin (VDMX's quad mode): the picture stretched into these four points of the canvas (top-left,
    /// top-right, bottom-right, bottom-left; 0..1, y down). Replaces move / scale / rotate while set.
    #[serde(default)]
    pub corners: Option<[[f32; 2]; 4]>,
    /// Depth: positive moves the picture away (smaller, in perspective), negative brings it closer. Canvas heights.
    pub z: f32,
    /// Tilts in 3D (degrees): forward / back round the horizontal axis, and round the vertical one.
    pub tilt_x: f32,
    pub tilt_y: f32,
}

impl Default for LayerTransform {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0, scale: 1.0, stretch_x: 1.0, stretch_y: 1.0, rotation: 0.0, flip_x: false, flip_y: false, scale_mod: None, rotation_mod: None, x_mod: None, y_mod: None, crop: [0.0; 4], corners: None, z: 0.0, tilt_x: 0.0, tilt_y: 0.0 }
    }
}

impl LayerTransform {
    /// The settings besides position / scale / rotation (`ControlTarget::LayerTransform { part }`): scale X / Y,
    /// depth, tilts, crop.
    pub const PARTS: [crate::mapping::TransformPart; 9] = {
        use crate::mapping::TransformPart as P;
        [
            P { name: "Scale X", min: 0.05, max: 8.0, log: true, default: 1.0 },
            P { name: "Scale Y", min: 0.05, max: 8.0, log: true, default: 1.0 },
            P { name: "Z (depth)", min: -1.5, max: 8.0, log: false, default: 0.0 },
            P { name: "Tilt X °", min: -80.0, max: 80.0, log: false, default: 0.0 },
            P { name: "Tilt Y °", min: -80.0, max: 80.0, log: false, default: 0.0 },
            P { name: "Crop left", min: 0.0, max: 0.5, log: false, default: 0.0 },
            P { name: "Crop right", min: 0.0, max: 0.5, log: false, default: 0.0 },
            P { name: "Crop top", min: 0.0, max: 0.5, log: false, default: 0.0 },
            P { name: "Crop bottom", min: 0.0, max: 0.5, log: false, default: 0.0 },
        ]
    };

    /// Setting `part` of `PARTS`.
    pub fn part_mut(&mut self, part: usize) -> &mut f32 {
        match part.min(8) {
            0 => &mut self.stretch_x,
            1 => &mut self.stretch_y,
            2 => &mut self.z,
            3 => &mut self.tilt_x,
            4 => &mut self.tilt_y,
            n => &mut self.crop[n - 5],
        }
    }

    /// Setting `part` as 0..1 across its range.
    pub fn unit(&self, part: usize) -> f32 {
        let v = match part.min(8) {
            0 => self.stretch_x,
            1 => self.stretch_y,
            2 => self.z,
            3 => self.tilt_x,
            4 => self.tilt_y,
            n => self.crop[n - 5],
        };
        Self::PARTS[part.min(8)].to_unit(v)
    }

    pub fn set_unit(&mut self, part: usize, u: f32) {
        *self.part_mut(part) = Self::PARTS[part.min(8)].from_unit(u);
    }

    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }

    /// Placed in 3D (a depth or a tilt)?
    pub fn is_3d(&self) -> bool {
        self.z.abs() > 1e-4 || self.tilt_x.abs() > 1e-3 || self.tilt_y.abs() > 1e-3
    }

    /// Where the picture's corners land on the canvas (top-left, top-right, bottom-right, bottom-left; 0..1, y down)
    /// with this move / scale / rotation and then its depth and tilts, seen through a camera two canvas heights away.
    /// `scale`, `rotation` (degrees), `x`, `y` are the values now (modulation applied). The composite draws it as a
    /// corner pin (one homography).
    pub fn corners_3d(&self, aspect: f32, scale: f32, rotation: f32, x: f32, y: f32) -> [[f32; 2]; 4] {
        const CAMERA: f32 = 2.0;
        let flip = |f: bool| if f { -1.0 } else { 1.0 };
        let (sx, sy) = ((scale * self.stretch_x).max(0.001) * flip(self.flip_x), (scale * self.stretch_y).max(0.001) * flip(self.flip_y));
        let (c, s) = (rotation.to_radians().cos(), rotation.to_radians().sin());
        let (tx, ty) = (self.tilt_x.to_radians(), self.tilt_y.to_radians());
        [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]].map(|p: [f32; 2]| {
            // As the composite places it (in canvas heights, centred, y down)…
            let (qx, qy) = ((p[0] - 0.5) * sx * aspect, (p[1] - 0.5) * sy);
            let (qx, qy) = (c * qx - s * qy, s * qx + c * qy);
            // …then tilted and pushed back, and seen in perspective.
            let (y1, z1) = (qy * tx.cos(), qy * tx.sin());
            let (x2, z2) = (qx * ty.cos() + z1 * ty.sin(), -qx * ty.sin() + z1 * ty.cos());
            let k = CAMERA / (CAMERA + z2 + self.z).max(0.05);
            [x2 * k / aspect + 0.5 + x, y1 * k + 0.5 - y]
        })
    }
}

impl Default for LayerState {
    fn default() -> Self {
        Self {
            id: 0,
            name: "Layer".into(),
            source: SourceSpec::None,
            enabled: true,
            solo: false,
            fx_bypass: false,
            in_mix: true,
            opacity: 1.0,
            blend: BlendMode::Normal,
            fit: FitMode::Fill,
            generator_values: Vec::new(),
            generator_mods: Vec::new(),
            opacity_mod: None,
            transform: LayerTransform::default(),
            volume: 1.0,
            volume_follows_opacity: true,
            muted: false,
            effects: Vec::new(),
            playing: true,
            rate: 1.0,
            looping: true,
            black_at_end: false,
            render_scale: 1.0,
            render_fps: 0.0,
            jump: 1.0,
            jump_in_seconds: false,
            direction: PlayDirection::Forward,
            group: CrossGroup::None,
            parent: None,
            comp: None,
            mixer: None,
            designer: false,
            mixer_values: Vec::new(),
            loop_in: 0.0,
            loop_out: 1.0,
            loop_beats: 0.0,
            sync_beats: 0.0,
            clips: Vec::new(),
            transition_secs: 0.5,
            transition_kind: TransitionKind::Crossfade,
            transition_effect: default_transition_effect(),
            transition_values: Vec::new(),
            sound_pair: 0,
            sound_device: None,
            voices: VoiceMode::Own,
            voice_turn: 0,
            autopilot: Autopilot::Off,
            autopilot_beats: 0.0,
            collection: None,
            flash_return: None,
            transition: None,
            active_clip: None,
            clip_effects: Vec::new(),
            clip_effects_from: crate::EffectsOwner::None,
        }
    }
}

impl LayerState {
    /// Its pictures hold points (a Points shape, or a points effect in its chain): the engine keeps them 16-bit
    /// float and at their own size (see points.wgsl).
    pub fn is_data(&self) -> bool {
        matches!(&self.source, SourceSpec::Generator(id) if crate::shaders::is_points(id))
            || matches!(&self.source, SourceSpec::Live { format, .. } if format == crate::shaders::MODEL_POINTS)
            || self.effects.iter().any(|e| e.enabled && crate::shaders::is_points(&e.effect))
    }


    /// Playback speed: the speed slider, or with BPM sync the speed that makes the movie (or its loop) last
    /// `sync_beats` beats at `bpm`.
    pub fn play_rate(&self, duration: Option<f64>, bpm: f32) -> f32 {
        let (Some(d), true) = (duration, self.sync_beats > 0.0 && self.loop_beats <= 0.0) else { return self.rate };
        let (lo, hi) = self.loop_range(d, bpm).unwrap_or((0.0, d));
        let beats_secs = self.sync_beats as f64 * 60.0 / bpm.max(1.0) as f64;
        (((hi - lo) / beats_secs) as f32).clamp(0.1, 4.0)
    }

    /// How far (seconds into the movie) one jump back / forward goes: `jump` seconds, or `jump` beats of
    /// playback at the layer's speed.
    pub fn jump_secs(&self, duration: Option<f64>, bpm: f32) -> f64 {
        let step = self.jump.max(0.0) as f64;
        if self.jump_in_seconds { step } else { step * 60.0 / bpm.max(1.0) as f64 * self.play_rate(duration, bpm) as f64 }
    }

    /// The loop region in seconds for a movie of `duration` at `bpm`, or None for the whole movie.
    pub fn loop_range(&self, duration: f64, bpm: f32) -> Option<(f64, f64)> {
        let lo = (self.loop_in.clamp(0.0, 1.0) as f64) * duration;
        let hi = if self.loop_beats > 0.0 {
            lo + self.loop_beats as f64 * 60.0 / bpm.max(1.0) as f64
        } else {
            self.loop_out.clamp(0.0, 1.0) as f64 * duration
        }
        .min(duration);
        if lo <= 1e-6 && hi >= duration - 1e-6 {
            return None;
        }
        // At least a frame or two long.
        Some((lo.min(duration - 0.05).max(0.0), hi.max(lo + 0.05)))
    }

    /// Starts a `secs`-long transition from the current source (0 = cut).
    pub fn start_transition(&mut self, now: f32, secs: f32) {
        if secs > 0.01 && self.source != SourceSpec::None {
            self.transition = Some(Transition {
                from: self.source.clone(),
                from_values: self.generator_values.clone(),
                start: now,
                duration: secs,
                kind: self.transition_kind,
                effect: self.transition_effect.clone(),
                values: self.transition_values.clone(),
            });
        } else {
            self.transition = None;
        }
    }

    /// Puts clip `slot` on this layer with a `secs` transition. Returns false for an empty slot.
    pub fn trigger_clip(&mut self, slot: usize, now: f32, secs: f32) -> bool {
        let Some(clip) = self.clips.get(slot).filter(|c| !c.is_empty()).cloned() else { return false };
        self.play_clip(clip, now, secs)
    }

    /// Puts `clip` (from any layer's row) on this layer with a `secs` transition.
    pub fn play_clip(&mut self, clip: Clip, now: f32, secs: f32) -> bool {
        if self.active_clip == Some(clip.id) && self.source == clip.source {
            return true;
        }
        self.start_transition(now, secs);
        if let Some(own) = &clip.own {
            self.apply_clip_settings(own);
        }
        self.source = clip.source;
        self.generator_values = clip.generator_values;
        self.generator_mods = clip.mods;
        if !clip.name.is_empty() {
            self.name = clip.name;
        }
        self.active_clip = Some(clip.id);
        self.playing = true;
        true
    }

    pub fn clip_slot_mut(&mut self, slot: usize) -> &mut Clip {
        if self.clips.len() <= slot {
            self.clips.resize_with(slot + 1, Clip::default);
        }
        &mut self.clips[slot]
    }

    pub fn expire_transition(&mut self, now: f32) {
        if self.transition.as_ref().is_some_and(|t| t.progress(now) >= 1.0) {
            self.transition = None;
        }
    }

    pub fn set_generator(&mut self, id: &str) {
        if let Some(desc) = shaders::generator(id) {
            self.source = SourceSpec::Generator(id.to_string());
            self.generator_values = desc.defaults();
            self.generator_mods.clear();
        }
    }
}

#[cfg(test)]
mod transform_3d_tests {
    use super::*;

    /// Flat, the 3D corners are exactly where the composite puts the picture; pushed back it shrinks round the
    /// middle; tilted round the vertical axis one side gets taller than the other (perspective).
    #[test]
    fn depth_and_tilt_place_the_corners_in_perspective() {
        let t = LayerTransform::default();
        let flat = t.corners_3d(16.0 / 9.0, 1.0, 0.0, 0.0, 0.0);
        for (got, want) in flat.iter().zip([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]) {
            assert!((got[0] - want[0]).abs() < 1e-5 && (got[1] - want[1]).abs() < 1e-5, "{flat:?}");
        }
        let back = LayerTransform { z: 2.0, ..Default::default() }.corners_3d(16.0 / 9.0, 1.0, 0.0, 0.0, 0.0);
        assert!((back[1][0] - back[0][0] - 0.5).abs() < 1e-4, "half as wide two heights further away: {back:?}");
        let tilted = LayerTransform { tilt_y: 30.0, ..Default::default() }.corners_3d(16.0 / 9.0, 1.0, 0.0, 0.0, 0.0);
        let left = tilted[3][1] - tilted[0][1];
        let right = tilted[2][1] - tilted[1][1];
        assert!((left - right).abs() > 0.05, "the edges differ in height: {tilted:?}");
        assert!(LayerTransform { tilt_x: 10.0, ..Default::default() }.is_3d() && !LayerTransform::default().is_3d());
    }
}
