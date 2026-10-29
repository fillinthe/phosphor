//! How a layer looks in another app (name, opacity, blend, fit, its effects) and the effects those apps use, turned
//! into Phosphor's: an ISF effect (.fs) by its file (the same shader, its settings by name), anything else (Quartz
//! Composer, Core Image, Resolume's own) by the nearest built-in effect, or left out when there's none.

use phosphor_engine::{BlendMode, Composition, EffectState, FitMode, shaders};

/// One effect on a layer in the other app.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LookEffect {
    /// Its name there ("Bad TV", "CIKaleidoscope", "Mirror.qtz").
    pub name: String,
    /// The shader file it comes from, if it's one (an ISF .fs).
    pub path: Option<String>,
    pub enabled: bool,
    /// Dry / wet, 0..1.
    pub mix: f32,
    /// Its settings by name, in the shader's own units.
    pub values: Vec<(String, f32)>,
}

/// A layer as the other app had it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayerLook {
    pub name: String,
    pub opacity: Option<f32>,
    pub blend: Option<BlendMode>,
    pub fit: Option<FitMode>,
    pub effects: Vec<LookEffect>,
}

/// The built-in generator nearest to another app's (MadMapper's Madam / materials, Resolume's generators).
pub fn generator_for(name: &str) -> &'static str {
    let n = name.to_lowercase();
    let has = |w: &str| n.contains(w);
    if has("solid") {
        "solid"
    } else if has("test") || has("color pattern") || has("colorpattern") || has("bars") {
        "color_bars"
    } else if has("grid") || has("checker") || has("square") || has("brick") {
        "checker"
    } else if has("gradient") {
        "gradient"
    } else if has("particle") {
        "particles"
    } else if has("spectrum") {
        "spectrum"
    } else if has("waveform") || has("wave synth") || has("sine") || has("oscillo") {
        "waveform"
    } else if has("cellular") || has("voronoi") || has("caustic") {
        "voronoi"
    } else if has("noise") || has("cloud") || has("dune") || has("simplex") || has("field") {
        "noise"
    } else if has("tunnel") {
        "tunnel"
    } else if has("line") || has("diagonal") || has("bar code") || has("barcode") || has("stripe") {
        "lines"
    } else if has("circle") || has("ring") || has("arc") || has("siren") {
        "rings"
    } else if has("star") {
        "starfield"
    } else if has("spiral") {
        "spiral"
    } else {
        "plasma"
    }
}

/// Another app's blend mode by name (VDMX "OpenGL- Add" / "OpenGL- Over", Resolume "Add" / "Alpha"…); Normal
/// for anything else, None for no name.
pub fn blend_named(mode: &str) -> Option<BlendMode> {
    let mode = mode.to_lowercase();
    let has = |w: &str| mode.contains(w);
    Some(if mode.is_empty() {
        return None;
    } else if has("add") || has("plus") {
        BlendMode::Add
    } else if has("multiply") {
        BlendMode::Multiply
    } else if has("screen") {
        BlendMode::Screen
    } else if has("overlay") {
        BlendMode::Overlay
    } else if has("difference") {
        BlendMode::Difference
    } else if has("subtract") {
        BlendMode::Subtract
    } else if has("lighten") || has("lighter") {
        BlendMode::Lighten
    } else if has("darken") || has("darker") {
        BlendMode::Darken
    } else if has("exclusion") {
        BlendMode::Exclusion
    } else {
        BlendMode::Normal
    })
}

/// The built-in effect nearest to one of another app's, by its name (None: nothing close, or it only crops).
pub fn builtin_for(name: &str) -> Option<&'static str> {
    let n = name.to_lowercase().replace(".qtz", "").replace(".fs", "").replace("v002 ", "");
    let n = n.strip_prefix("ci").filter(|_| name.starts_with("CI")).unwrap_or(&n).to_string();
    let has = |w: &str| n.contains(w);
    Some(if has("crop") || has("straighten") || has("clamp") || has("transform") || has("keystone") {
        return None;
    } else if has("kaleid") || has("scope") || has("cloner") {
        "kaleidoscope"
    } else if has("mirror") || has("reflect") || n == "flip" {
        "mirror"
    } else if has("dot screen") || has("dotscreen") || has("halftone") || has("polkadot") || has("point grid") || has("pointgrid") {
        "halftone"
    } else if has("rutt") {
        "rutt_etra"
    } else if has("pixel") || has("boxinator") || has("lorez") || has("lo-rez") || has("triangulate") {
        "pixelate"
    } else if has("color controls") || has("colorcontrols") || has("hue") || has("saturation") || has("brightness") || has("monochrome") || has("levels") {
        "color_adjust"
    } else if has("false color") || has("falsecolor") || has("palette") || has("tint") {
        "palette"
    } else if has("edge") {
        "edges"
    } else if has("posterize") || has("comic") {
        "posterize"
    } else if has("zoom blur") {
        "zoom_blur"
    } else if has("blur") || has("depth of field") || has("depthoffield") {
        "blur"
    } else if has("glow") || has("bloom") {
        "glow"
    } else if has("chroma zoom") || has("rgb") || has("chromatic") || has("convergence") {
        "rgb_shift"
    } else if has("bad tv") || has("glitch") || has("twitch") || has("fragment") {
        "glitch"
    } else if has("crt") || has("scanline") {
        "crt"
    } else if has("tile") || has("multiframe") || has("compound eye") || has("video wall") || has("videowall") {
        "tile"
    } else if has("invert") {
        "invert"
    } else if has("threshold") {
        "threshold"
    } else if has("emboss") {
        "emboss"
    } else if has("sharpen") {
        "sharpen"
    } else if has("vignette") {
        "vignette"
    } else if has("wave") || has("ripple") || has("warp") || has("twist") {
        "wave"
    } else if has("trail") || has("ghost") || has("feedback") || has("time glitch") {
        "trails"
    } else if has("tunnel") || has("zoom") || has("rotate") {
        "zoom_rotate"
    } else if has("ascii") {
        "pixelate"
    } else if has("grain") || has("noise") {
        "grain"
    } else {
        return None;
    })
}

/// The Phosphor effect for `fx`: the ISF shader of the same file when Phosphor has it (or no built-in is close),
/// else the nearest built-in one. Returns (effect id, whether it's a stand-in rather than the same shader).
pub fn effect_id(fx: &LookEffect) -> Option<(String, bool)> {
    if let Some(path) = fx.path.as_deref().filter(|p| p.to_lowercase().ends_with(".fs")) {
        let stem = std::path::Path::new(path).file_stem().map_or_else(String::new, |s| s.to_string_lossy().to_string());
        let id = format!("{}{}", shaders::ISF_EFFECT, phosphor_engine::custom::slug(&stem));
        if shaders::effect(&id).is_some() || builtin_for(&fx.name).is_none() {
            return Some((id, false));
        }
    }
    builtin_for(&fx.name).map(|b| (b.to_string(), true))
}

/// `fx` as an effect on a Phosphor layer (settings matched by name when it's the same shader). None: left out.
pub fn effect_state(comp: &mut Composition, fx: &LookEffect) -> Option<(EffectState, bool)> {
    let (id, stand_in) = effect_id(fx)?;
    let mut state = comp.new_effect(&id).unwrap_or_else(|| EffectState { id: comp.new_id(), effect: id.clone(), enabled: true, values: Vec::new(), mix: 1.0, mods: Vec::new(), blend: Default::default(), other: 0, light: 0 });
    if !stand_in && let Some(desc) = shaders::effect(&id) {
        let key = |s: &str| s.to_lowercase().replace([' ', '_', '-'], "");
        for (name, v) in &fx.values {
            if let Some(i) = desc.params.iter().position(|p| key(p.name) == key(name))
                && let Some(slot) = state.values.get_mut(i)
            {
                *slot = v.clamp(desc.params[i].min, desc.params[i].max);
            }
        }
    }
    state.enabled = fx.enabled;
    state.mix = fx.mix.clamp(0.0, 1.0);
    Some((state, stand_in))
}

/// Gives layer `layer` the look: its name, opacity, blend, fit and effects (added after any it has). Returns the
/// names of effects drawn by a stand-in and of those left out.
pub fn apply(comp: &mut Composition, layer: u64, look: &LayerLook) -> (Vec<String>, Vec<String>) {
    let (mut stand_ins, mut left) = (Vec::new(), Vec::new());
    let mut effects = Vec::new();
    for fx in &look.effects {
        match effect_state(comp, fx) {
            Some((state, stand_in)) => {
                if stand_in {
                    stand_ins.push(fx.name.clone());
                }
                effects.push(state);
            }
            None => left.push(fx.name.clone()),
        }
    }
    if let Some(l) = comp.layer_mut(layer) {
        if !look.name.is_empty() {
            l.name = look.name.clone();
        }
        if let Some(o) = look.opacity {
            l.opacity = o.clamp(0.0, 1.0);
        }
        if let Some(b) = look.blend {
            l.blend = b;
        }
        if let Some(f) = look.fit {
            l.fit = f;
        }
        l.effects.extend(effects);
    }
    (stand_ins, left)
}

/// The effects that will be drawn by a stand-in, and those left out (names), for the import's message.
pub fn report(looks: &[LayerLook], master: &[LookEffect]) -> (Vec<String>, Vec<String>) {
    let (mut stand_ins, mut left) = (Vec::new(), Vec::new());
    for fx in looks.iter().flat_map(|l| &l.effects).chain(master) {
        match effect_id(fx) {
            Some((_, true)) => stand_ins.push(fx.name.clone()),
            Some(_) => {}
            None => left.push(fx.name.clone()),
        }
    }
    (stand_ins, left)
}

/// The master effects (the whole picture's) from the other app's main output.
pub fn apply_master(comp: &mut Composition, effects: &[LookEffect]) -> (Vec<String>, Vec<String>) {
    let (mut stand_ins, mut left) = (Vec::new(), Vec::new());
    for fx in effects {
        match effect_state(comp, fx) {
            Some((state, stand_in)) => {
                if stand_in {
                    stand_ins.push(fx.name.clone());
                }
                comp.master_effects.push(state);
            }
            None => left.push(fx.name.clone()),
        }
    }
    (stand_ins, left)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_by_file_or_by_the_nearest_built_in() {
        let fx = |name: &str, path: Option<&str>| LookEffect { name: name.into(), path: path.map(Into::into), enabled: true, mix: 1.0, values: Vec::new() };
        // An ISF file Phosphor doesn't have and no built-in is close to: the same shader by id (it loads when found).
        assert_eq!(effect_id(&fx("Optical Flow Generator", Some("/Library/Graphics/ISF/Optical Flow Generator.fs"))), Some(("isf:optical_flow_generator".into(), false)));
        // Quartz Composer and Core Image: the nearest built-in.
        assert_eq!(effect_id(&fx("Kaleidescope.qtz", None)), Some(("kaleidoscope".into(), true)));
        assert_eq!(effect_id(&fx("v002 Rutt Etra.qtz", None)), Some(("rutt_etra".into(), true)));
        assert_eq!(effect_id(&fx("CIColorControls", None)), Some(("color_adjust".into(), true)));
        assert_eq!(effect_id(&fx("CICrop", None)), None, "a crop is left out");
        // An ISF file Phosphor doesn't have but a built-in is close to: the built-in.
        assert_eq!(effect_id(&fx("Edges", Some("/Library/Graphics/ISF/Edges.fs"))), Some(("edges".into(), true)));
        let mut comp = Composition::default();
        let id = comp.add_layer().id;
        let look = LayerLook {
            name: "Headliner".into(),
            opacity: Some(0.5),
            blend: Some(BlendMode::Add),
            fit: Some(FitMode::Fit),
            effects: vec![LookEffect { mix: 0.4, enabled: false, ..fx("Mirror.qtz", None) }, fx("CICrop", None)],
        };
        let (stand_ins, left) = apply(&mut comp, id, &look);
        assert_eq!((stand_ins, left), (vec!["Mirror.qtz".to_string()], vec!["CICrop".to_string()]));
        let l = comp.layer_mut(id).unwrap();
        assert_eq!((l.name.as_str(), l.opacity, l.blend, l.fit), ("Headliner", 0.5, BlendMode::Add, FitMode::Fit));
        assert_eq!((l.effects[0].effect.as_str(), l.effects[0].mix, l.effects[0].enabled), ("mirror", 0.4, false));
    }
}
