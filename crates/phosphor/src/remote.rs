//! Phone / tablet remote: the project state the remote page shows, and the commands it sends back.

use phosphor_engine::{ControlTarget, MASTER_ID, SourceSpec, shaders};
use serde_json::{Value, json};

use crate::app::{State, Trigger};

/// What the remote page needs: mixer, clip grid, and the selected layer's controls.
pub fn state_json(st: &mut State, now: f64) -> String {
    let play = st.play_json(now);
    let comp = &st.comp;
    let thumbs = st.net.thumb_versions();
    let layers: Vec<Value> = comp
        .layers
        .iter()
        .rev()
        .map(|l| {
            let clips: Vec<Value> = l
                .clips
                .iter()
                .enumerate()
                .filter(|(_, c)| !c.is_empty())
                .map(|(slot, c)| json!({"slot": slot, "name": if c.name.is_empty() { c.source.label() } else { c.name.clone() }, "active": l.active_clip == Some(c.id)}))
                .collect();
            json!({"id": l.id, "name": l.name, "opacity": round(l.opacity), "enabled": l.enabled, "solo": l.solo, "clips": clips,
                "thumb": thumbs.get(&l.id).copied().unwrap_or(0)})
        })
        .collect();
    let mut params: Vec<Value> = Vec::new();
    let selected = st.selected.unwrap_or(0);
    let effects = if selected == MASTER_ID {
        Some(&comp.master_effects)
    } else if let Some(l) = comp.layers.iter().find(|l| l.id == selected) {
        if let SourceSpec::Generator(g) = &l.source
            && let Some(desc) = shaders::generator(g)
        {
            for (i, p) in desc.params.iter().enumerate() {
                let v = l.generator_values.get(i).copied().unwrap_or(p.default);
                params.push(json!({"key": format!("g{}_{i}", l.id), "group": desc.name, "name": p.name, "min": p.min, "max": p.max, "value": round(v),
                    "msg": {"t": "gen", "layer": l.id, "i": i}}));
            }
        }
        Some(&l.effects)
    } else {
        None
    };
    for fx in effects.into_iter().flatten() {
        let Some(desc) = shaders::effect(&fx.effect) else { continue };
        for (i, p) in desc.params.iter().enumerate() {
            let v = fx.values.get(i).copied().unwrap_or(p.default);
            params.push(json!({"key": format!("f{}_{i}", fx.id), "group": desc.name, "fx": fx.id, "fxon": fx.enabled, "name": p.name,
                "min": p.min, "max": p.max, "value": round(v), "msg": {"t": "fx", "effect": fx.id, "i": i}}));
        }
        params.push(json!({"key": format!("m{}", fx.id), "group": desc.name, "fx": fx.id, "fxon": fx.enabled, "name": "Mix",
            "min": 0.0, "max": 1.0, "value": round(fx.mix), "msg": {"t": "mix", "effect": fx.id}}));
    }
    let surface: Vec<Value> = comp
        .surface
        .widgets
        .iter()
        .map(|w| {
            let [r, g, b] = w.color;
            json!({"id": w.id, "kind": format!("{:?}", w.kind), "x": w.x, "y": w.y, "w": w.w, "h": w.h, "label": w.label,
                "color": format!("#{r:02x}{g:02x}{b:02x}"), "value": round(w.value), "value_y": round(w.value_y), "page": w.page})
        })
        .collect();
    json!({
        "surface": {"columns": comp.surface.columns, "widgets": surface, "pages": comp.surface.pages},
        "bpm": comp.bpm,
        "timing": comp.launch.timing,
        "master": round(comp.master_fade),
        "columns": comp.clip_columns,
        "selected": selected,
        "layers": layers,
        "params": params,
        "play": play,
    })
    .to_string()
}

fn round(v: f32) -> f64 {
    (v as f64 * 1000.0).round() / 1000.0
}

/// Applies one command from a remote. Guests (audience play) carry `_from`; they only get through
/// on their turn and for the controls the show allows (`State::guest_may`).
pub fn apply(st: &mut State, cmd: &Value, now: f64) {
    if let Some(from) = cmd.get("_from").and_then(Value::as_u64)
        && !st.guest_may(from, cmd, now)
    {
        return;
    }
    let f = |k: &str| cmd.get(k).and_then(Value::as_f64).unwrap_or(0.0);
    let id = |k: &str| cmd.get(k).and_then(Value::as_u64).unwrap_or(0);
    let flag = |k: &str| cmd.get(k).and_then(Value::as_bool).unwrap_or(false);
    let v = f("v") as f32;
    let on = |b: bool| if b { 1.0 } else { 0.0 };
    // Controls go through the same setter as the Mac's UI and controllers (`State::set_control`).
    let target = match cmd.get("t").and_then(Value::as_str).unwrap_or("") {
        "master" => Some((ControlTarget::MasterFade, v)),
        "opacity" => Some((ControlTarget::LayerOpacity(id("layer")), v)),
        "enabled" => Some((ControlTarget::LayerEnabled(id("layer")), on(flag("v")))),
        "mix" => Some((ControlTarget::EffectMix(id("effect")), v)),
        "fxon" => Some((ControlTarget::EffectEnabled(id("effect")), on(flag("v")))),
        // Parameters arrive in their own units (the page shows min..max).
        "gen" => {
            let (layer, index) = (id("layer"), f("i") as usize);
            let param = st.comp.layers.iter().find(|l| l.id == layer).and_then(|l| match &l.source {
                SourceSpec::Generator(g) => shaders::generator(g).and_then(|d| d.params.get(index)),
                _ => None,
            });
            param.map(|p| (ControlTarget::GeneratorParam { layer, index }, crate::control::to_unit(v, p.min, p.max, false)))
        }
        "fx" => {
            let (effect, index) = (id("effect"), f("i") as usize);
            let param = crate::control::find_effect(&st.comp, effect).and_then(|fx| shaders::effect(&fx.effect)).and_then(|d| d.params.get(index));
            param.map(|p| (ControlTarget::EffectParam { effect, index }, crate::control::to_unit(v, p.min, p.max, false)))
        }
        _ => None,
    };
    if let Some((t, v)) = target {
        st.set_control(&t, v, now);
        return;
    }
    match cmd.get("t").and_then(Value::as_str).unwrap_or("") {
        "solo" => {
            if let Some(l) = st.comp.layer_mut(id("layer")) {
                l.solo = flag("v");
            }
        }
        "select" => st.selected = Some(id("layer")),
        "clip" => st.trigger(Trigger::Clip { layer: id("layer"), slot: f("slot") as usize }, now),
        "column" => st.trigger(Trigger::Column(f("col") as usize), now),
        "tap" => st.clock.tap(now),
        "surprise" => st.quick(crate::app::Quick::Surprise, now),
        // Launch timing (quantize): "Now" / "Beat" / "Bar".
        "timing" => {
            if let Some(t) = cmd.get("v").and_then(|v| serde_json::from_value::<phosphor_engine::LaunchTiming>(v.clone()).ok()) {
                st.comp.launch.timing = t;
            }
        }
        // Any control by target (the Control Surface uses this): same path as the Mac's own UI.
        "set" => {
            if let Some(t) = cmd.get("target").and_then(|t| serde_json::from_value::<phosphor_engine::ControlTarget>(t.clone()).ok()) {
                st.set_control(&t, v, now);
            }
        }
        _ => {}
    }
}
