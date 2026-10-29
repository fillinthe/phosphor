//! Visuals in one click: shuffle parameters, add audio/beat motion ("react"), a random look for a layer or the
//! whole composition ("surprise"), fill a clip row with variations, and a ready-to-play instant set.
//! Pure rules on the model; the app supplies the seed.

use super::*;
use crate::shaders::{self, Param, ShaderDesc};

/// A small seeded random source (xorshift), so the model needs no random crate and tests are repeatable.
#[derive(Clone, Debug)]
pub struct Dice(u64);

impl Dice {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    /// 0..1.
    pub fn unit(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 40) as f32 / (1u64 << 24) as f32
    }

    /// 0..n (0 when n is 0).
    pub fn below(&mut self, n: usize) -> usize {
        ((self.unit() * n as f32) as usize).min(n.saturating_sub(1))
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        (!items.is_empty()).then(|| &items[self.below(items.len())])
    }
}

/// Generators that are test patterns or fills, never picked at random.
const PLAIN_GENERATORS: &[&str] = &["checker", "color_bars", "solid"];

/// Effects that look good dropped on anything (no keys, thresholds or heavy stylizers).
pub const LIVELY_EFFECTS: &[&str] = &[
    "kaleidoscope",
    "mirror",
    "glow",
    "edge_glow",
    "rgb_shift",
    "zoom_rotate",
    "wave",
    "tile",
    "trails",
    "color_adjust",
    "pixelate",
    "halftone",
    "crt",
    "glitch",
    "zoom_blur",
    "posterize",
    "technicolor",
    "slitscan",
];

/// Blends for layers above the bottom one: they all let the layers below show.
const LIVELY_BLENDS: &[BlendMode] = &[BlendMode::Screen, BlendMode::Add, BlendMode::Lighten, BlendMode::Difference, BlendMode::Overlay];

/// Generators worth landing on at random: (built in, imported / yours). Imported ones that only draw
/// sound (FFT, waveform) are left out: they sit dark without audio.
pub fn lively_generators() -> (Vec<&'static ShaderDesc>, Vec<&'static ShaderDesc>) {
    // (Points generators make point data, not a picture: they need a Points render to show anything.)
    let builtin: Vec<_> = shaders::GENERATORS.iter().filter(|d| !PLAIN_GENERATORS.contains(&d.id) && !shaders::points_out(d.id)).collect();
    let other = shaders::all_generators()
        .into_iter()
        .filter(|d| !shaders::GENERATORS.iter().any(|b| b.id == d.id))
        .filter(|d| {
            let n = format!("{} {}", d.id, d.name).to_ascii_lowercase();
            !["fft", "audio", "spectrum", "waveform", "sound", "test", "color_bars", "calibrat"].iter().any(|w| n.contains(w))
        })
        .collect();
    (builtin, other)
}

/// Brightness-like parameters (alpha, opacity, gain…): shuffling never takes them much below their default,
/// so a shuffle doesn't fade the picture out.
fn is_level(p: &Param) -> bool {
    let n = p.name.to_ascii_lowercase();
    ["alpha", "opacity", "bright", "intensity", "gain", "level", "amount", "exposure"].iter().any(|w| n.contains(w))
}

/// Parameters whose changes jump the picture (time is multiplied by them) rather than move it.
fn is_speed(p: &Param) -> bool {
    p.name.eq_ignore_ascii_case("speed")
}

fn is_hue(p: &Param) -> bool {
    p.name.to_ascii_lowercase().contains("hue")
}

/// Moves each value to a random spot near its default: `amount` 0 = unchanged, 1 = anywhere in range.
/// Speeds stay the same sign and never stop, so a shuffled picture keeps moving the same way.
pub fn shuffle_values(params: &[Param], values: &mut Vec<f32>, amount: f32, dice: &mut Dice) {
    values.resize(params.len(), 0.0);
    for (p, v) in params.iter().zip(values.iter_mut()) {
        let span = p.max - p.min;
        // Two dice averaged: values near the default come up more often than the extremes.
        let r = (dice.unit() + dice.unit()) - 1.0;
        let mut x = if is_hue(p) { p.min + dice.unit() * span } else { p.default + r * span * amount };
        if is_level(p) {
            x = x.max(p.default * 0.8);
        }
        if is_speed(p) && p.default != 0.0 {
            let lo = p.default.abs() * 0.4;
            x = x.abs().max(lo).copysign(p.default);
        }
        // Imported shaders can have min above max (or NaN): never panic on them.
        let (lo, hi) = (p.min.min(p.max), p.min.max(p.max));
        *v = if x.is_finite() { x.max(lo).min(hi) } else { p.default };
    }
}

/// Motion for a shader's parameters that follows the music: a hue drift over 16 beats, one parameter
/// pumping on the kick and one breathing over 8 beats. Speeds are left alone.
pub fn react_mods(params: &[Param], dice: &mut Dice) -> Vec<ParamMod> {
    let mut out = Vec::new();
    let mut free: Vec<usize> = Vec::new();
    for (i, p) in params.iter().enumerate() {
        if is_hue(p) {
            out.push(ParamMod { index: i, m: Modulation { source: ModSource::LfoSaw, rate: 16.0, sync: true, range: true, min: 0.0, max: 1.0, ..Default::default() } });
        } else if !is_speed(p) && p.max > p.min && p.max.is_finite() && p.min.is_finite() {
            free.push(i);
        }
    }
    let kick = Modulation { source: ModSource::AudioLow, amount: 0.25 + dice.unit() * 0.2, ..Default::default() };
    let breathe = Modulation { source: ModSource::LfoSine, amount: 0.15 + dice.unit() * 0.15, rate: 8.0, sync: true, ..Default::default() };
    for m in [kick, breathe] {
        if free.is_empty() {
            break;
        }
        let index = free.remove(dice.below(free.len()));
        out.push(ParamMod { index, m });
    }
    out
}

/// A generator with shuffled values: usually a built-in one; sometimes one of yours / an imported ISF, kept
/// close to the defaults its author chose (random settings often blank those out).
fn shuffled_generator(dice: &mut Dice, amount: f32) -> Option<(&'static ShaderDesc, Vec<f32>)> {
    let (builtin, other) = lively_generators();
    let (desc, amount) = if other.is_empty() || dice.chance(0.7) { (*dice.pick(&builtin)?, amount) } else { (*dice.pick(&other)?, amount * 0.2) };
    let mut values = desc.defaults();
    shuffle_values(desc.params, &mut values, amount, dice);
    Some((desc, values))
}

impl LayerState {
    /// New values for the generator and every effect.
    pub fn shuffle(&mut self, amount: f32, dice: &mut Dice) {
        if let SourceSpec::Generator(id) = &self.source
            && let Some(desc) = shaders::generator(id)
        {
            shuffle_values(desc.params, &mut self.generator_values, amount, dice);
        }
        for fx in &mut self.effects {
            if let Some(desc) = shaders::effect(&fx.effect) {
                shuffle_values(desc.params, &mut fx.values, amount, dice);
            }
        }
    }

    /// Adds music-following motion to the generator and effects (replacing what's on those parameters).
    pub fn react(&mut self, dice: &mut Dice) {
        if let SourceSpec::Generator(id) = &self.source
            && let Some(desc) = shaders::generator(id)
        {
            merge_mods(&mut self.generator_mods, react_mods(desc.params, dice));
        }
        for fx in &mut self.effects {
            if let Some(desc) = shaders::effect(&fx.effect) {
                let mut mods = react_mods(desc.params, dice);
                mods.truncate(2);
                merge_mods(&mut fx.mods, mods);
            }
        }
    }
}

impl LayerState {
    /// Takes every modulation off the layer's generator and effects: the look stays, nothing moves on its own.
    pub fn still(&mut self) {
        self.generator_mods.clear();
        for fx in &mut self.effects {
            fx.mods.clear();
        }
    }
}

fn merge_mods(into: &mut Vec<ParamMod>, new: Vec<ParamMod>) {
    for m in new {
        into.retain(|o| o.index != m.index);
        into.push(m);
    }
}

impl Composition {
    /// 0–2 random lively effects with shuffled values and some motion.
    fn random_effects(&mut self, dice: &mut Dice, max: usize) -> Vec<EffectState> {
        let n = dice.below(max + 1);
        let mut out = Vec::new();
        let mut ids: Vec<&str> = LIVELY_EFFECTS.iter().copied().filter(|id| shaders::effect(id).is_some()).collect();
        for _ in 0..n {
            if ids.is_empty() {
                break;
            }
            let id = ids.remove(dice.below(ids.len()));
            let Some(mut fx) = self.new_effect(id) else { continue };
            if let Some(desc) = shaders::effect(id) {
                shuffle_values(desc.params, &mut fx.values, 0.5, dice);
                if dice.chance(0.6) {
                    fx.mods = react_mods(desc.params, dice);
                    fx.mods.truncate(1);
                }
            }
            out.push(fx);
        }
        out
    }

    /// A random look on one layer: a new generator (unless it plays a movie, picture or live source), fresh effects
    /// and motion that follows the music, faded in with the launch fade. `top` layers get a see-through blend.
    pub fn surprise_layer(&mut self, layer: u64, now: f32, dice: &mut Dice) {
        let effects = self.random_effects(dice, 2);
        let secs = self.launch_secs().max(0.3);
        let bottom = self.layers.first().map(|l| l.id) == Some(layer);
        let Some(l) = self.layer_mut(layer) else { return };
        l.start_transition(now, secs);
        if matches!(l.source, SourceSpec::None | SourceSpec::Generator(_))
            && let Some((desc, values)) = shuffled_generator(dice, 0.6)
        {
            l.source = SourceSpec::Generator(desc.id.to_string());
            l.generator_values = values;
            l.generator_mods.clear();
            l.name = desc.name.to_string();
            l.active_clip = None;
            l.playing = true;
        }
        l.effects = effects;
        if !bottom {
            l.blend = *dice.pick(LIVELY_BLENDS).unwrap_or(&BlendMode::Screen);
            l.opacity = 0.55 + dice.unit() * 0.35;
        }
        l.react(dice);
    }

    /// A random look on every layer that's on (two new layers if there are none).
    pub fn surprise(&mut self, now: f32, dice: &mut Dice) {
        if !self.layers.iter().any(|l| self.in_open_comp(l)) {
            self.add_layer();
            self.add_layer();
        }
        // The open comp's layers (every layer without comps).
        let ids: Vec<u64> = self.layers.iter().filter(|l| l.enabled && self.in_open_comp(l)).map(|l| l.id).collect();
        for id in ids {
            self.surprise_layer(id, now, dice);
        }
    }

    /// Fills the layer's empty clip slots in the first `columns` with generator variations: each its own
    /// generator, values, motion and (sometimes) an effect stored on the clip. Returns how many were made.
    pub fn fill_variations(&mut self, layer: u64, columns: usize, dice: &mut Dice) -> usize {
        let Some(index) = self.layers.iter().position(|l| l.id == layer) else { return 0 };
        let empty: Vec<usize> = (0..columns).filter(|&s| self.layers[index].clips.get(s).is_none_or(|c| c.is_empty())).collect();
        let base = self.layers[index].clip_settings();
        let mut made = 0;
        for slot in empty {
            let Some((desc, values)) = shuffled_generator(dice, 0.7) else { break };
            let effects = self.random_effects(dice, 1);
            let id = self.new_id();
            let mut mods = react_mods(desc.params, dice);
            mods.truncate(2);
            let clip = self.layers[index].clip_slot_mut(slot);
            *clip = Clip {
                id,
                name: desc.name.to_string(),
                source: SourceSpec::Generator(desc.id.to_string()),
                generator_values: values,
                mods,
                own: Some(base.clone()),
                effects,
                ..Default::default()
            };
            made += 1;
        }
        made
    }

    /// Replaces the layers with a set ready to play: three layers with a row of variations each, the first column
    /// launched and the top layer moving on by itself every 8 beats. Outputs, mapping and controls stay.
    /// Hit columns, tap the tempo, done.
    pub fn instant_set(&mut self, now: f32, dice: &mut Dice) {
        self.layers.clear();
        self.groups.clear();
        self.launch.fade = true;
        let names = ["Base", "Shapes", "Accents"];
        let mut ids = Vec::new();
        for (i, name) in names.iter().enumerate() {
            let l = self.add_layer();
            l.name = name.to_string();
            if i > 0 {
                l.blend = if i == 1 { BlendMode::Screen } else { BlendMode::Add };
                l.opacity = if i == 1 { 0.75 } else { 0.6 };
            }
            ids.push(l.id);
        }
        let columns = self.clip_columns.max(1);
        for &id in &ids {
            self.fill_variations(id, columns, dice);
        }
        if let Some(top) = self.layer_mut(ids[2]) {
            top.autopilot = Autopilot::Random;
            top.autopilot_beats = 8.0;
        }
        self.trigger_column(0, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dice_stay_in_range() {
        let mut d = Dice::new(7);
        for _ in 0..10_000 {
            let u = d.unit();
            assert!((0.0..1.0).contains(&u));
            assert!(d.below(5) < 5);
        }
        assert_eq!(d.below(0), 0);
    }

    #[test]
    fn surprise_never_lands_on_point_data() {
        let (builtin, other) = lively_generators();
        assert!(builtin.iter().chain(&other).all(|d| !shaders::points_out(d.id)));
        assert!(!builtin.is_empty());
    }

    #[test]
    fn shuffle_keeps_values_in_range_and_speeds_moving() {
        let mut d = Dice::new(3);
        for desc in shaders::all_generators() {
            for _ in 0..50 {
                let mut v = desc.defaults();
                shuffle_values(desc.params, &mut v, 1.0, &mut d);
                for (p, x) in desc.params.iter().zip(&v) {
                    assert!(*x >= p.min && *x <= p.max, "{} {} = {x}", desc.id, p.name);
                    if is_speed(p) && p.default != 0.0 {
                        assert_eq!(x.signum(), p.default.signum(), "{} speed flipped", desc.id);
                        assert!(x.abs() >= p.default.abs() * 0.39);
                    }
                }
            }
        }
    }

    #[test]
    fn shuffle_survives_odd_ranges() {
        let params = [Param::new("Backwards", 1.0, 0.0, 0.5), Param::new("Speed", f32::NAN, 2.0, 1.0)];
        let mut v = Vec::new();
        shuffle_values(&params, &mut v, 1.0, &mut Dice::new(5));
        assert!((0.0..=1.0).contains(&v[0]));
        assert!(v[1].is_finite());
    }

    #[test]
    fn react_drifts_hue_and_skips_speed() {
        let plasma = shaders::generator("plasma").unwrap();
        let mods = react_mods(plasma.params, &mut Dice::new(1));
        let hue = plasma.params.iter().position(|p| p.name == "Hue").unwrap();
        let speed = plasma.params.iter().position(|p| p.name == "Speed").unwrap();
        assert!(mods.iter().any(|m| m.index == hue && m.m.sync && m.m.range));
        assert!(mods.iter().all(|m| m.index != speed));
        assert!(mods.iter().any(|m| m.m.source == ModSource::AudioLow));
    }

    #[test]
    fn surprise_gives_every_layer_a_look() {
        let mut comp = Composition::default();
        let mut d = Dice::new(42);
        comp.surprise(0.0, &mut d);
        assert_eq!(comp.layers.len(), 2);
        for l in &comp.layers {
            assert!(matches!(&l.source, SourceSpec::Generator(id) if !PLAIN_GENERATORS.contains(&id.as_str())));
            assert!(!l.generator_mods.is_empty() || l.effects.iter().any(|f| !f.mods.is_empty()));
        }
        // Movie layers keep their movie.
        let movie = SourceSpec::Movie { path: Some("a.mov".into()), name: "a".into() };
        comp.layers[0].source = movie.clone();
        let id = comp.layers[0].id;
        comp.surprise_layer(id, 1.0, &mut d);
        assert_eq!(comp.layers[0].source, movie);
    }

    #[test]
    fn instant_set_is_ready_to_play() {
        let mut comp = Composition::starter();
        comp.ensure_outputs();
        comp.instant_set(0.0, &mut Dice::new(9));
        assert_eq!(comp.outputs.len(), 1);
        assert_eq!(comp.layers.len(), 3);
        for l in &comp.layers {
            assert_eq!(l.clips.iter().filter(|c| !c.is_empty()).count(), comp.clip_columns);
            // Column 1 is playing, with the clip's motion.
            assert_eq!(l.active_slot(), Some(0));
            assert_eq!(l.generator_mods, l.clips[0].mods);
        }
        assert_eq!(comp.layers[2].autopilot, Autopilot::Random);
        // Every clip id is unique.
        let mut ids: Vec<u64> = comp.layers.iter().flat_map(|l| l.clips.iter().map(|c| c.id)).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), comp.clip_columns * 3);
    }
}
