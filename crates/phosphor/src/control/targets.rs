//! Every control as a target: set it, read it back, list them, and their built-in OSC addresses.

use super::*;

fn scaled(p: &shaders::Param, v: f32) -> f32 {
    p.min + v.clamp(0.0, 1.0) * (p.max - p.min)
}

pub fn find_effect(comp: &Composition, id: u64) -> Option<&EffectState> {
    comp.all_effects().find(|e| e.id == id)
}

pub fn find_effect_mut(comp: &mut Composition, id: u64) -> Option<&mut EffectState> {
    comp.all_effects_mut().find(|e| e.id == id)
}

/// Inverse of `unit_to`: where `x` sits in the range, 0..1.
pub fn to_unit(x: f32, min: f32, max: f32, log: bool) -> f32 {
    let v = if log { (x / min).ln() / (max / min).ln() } else { (x - min) / (max - min) };
    if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 }
}

fn param_unit(p: &shaders::Param, x: f32) -> f32 {
    to_unit(x, p.min, p.max, false)
}

/// The current 0..1 value of a target, the inverse of `set_target` (controller feedback, surfaces, remote).
/// None for triggers and for controls that live outside the composition (tempo, audio).
pub fn get_target(comp: &Composition, target: &ControlTarget) -> Option<f32> {
    let on = |b: bool| if b { 1.0 } else { 0.0 };
    let layer = |id: &u64| comp.layers.iter().find(|l| l.id == *id);
    let effect = |id: &u64| find_effect(comp, *id);
    Some(match target {
        ControlTarget::MasterFade => comp.master_fade,
        // Opacity, on/off and bypass also drive groups (same id space as layers).
        ControlTarget::LayerOpacity(id) => comp.mix_controls(*id)?.0,
        ControlTarget::LayerEnabled(id) => on(comp.mix_controls(*id)?.1),
        // A slice group's id reaches all its slices (the first one's value is shown).
        // A group (or layout) has its own on/off and opacity, on top of its slices'.
        ControlTarget::SliceOpacity(id) => match comp.slice_group(*id) {
            Some(g) => g.opacity,
            None => comp.slice(*id)?.opacity,
        },
        ControlTarget::SliceEnabled(id) => match comp.slice_group(*id) {
            Some(g) => on(g.enabled),
            None => on(comp.slice(*id)?.enabled),
        },
        ControlTarget::Layout(id) => comp.slice_group(*id).map(|g| on(g.enabled && g.opacity > 0.5))?,
        ControlTarget::StopLayer(_) => 0.0,
        ControlTarget::MaskTransform { mask, part } => comp.mask(*mask)?.transform.unit(*part as usize),
        ControlTarget::SliceTransform { slice, part } => comp.slice(*slice)?.transform.unit(*part as usize),
        ControlTarget::MaskFeather(id) => comp.mask(*id)?.feather / phosphor_engine::mapping::MASK_FEATHER_MAX,
        ControlTarget::MaskEnabled(id) => on(comp.mask(*id)?.enabled),
        ControlTarget::MaskInvert(id) => on(comp.mask(*id)?.invert),
        ControlTarget::SliceFeather(id) => comp.slice(*id)?.feather / phosphor_engine::mapping::SLICE_FEATHER_MAX,
        ControlTarget::Color { owner, part } => comp.color(*owner)?.unit(*part as usize),
        ControlTarget::Fixture { fixture, param } => comp.dmx.fixtures.iter().find(|f| f.id == *fixture)?.param(*param)?,
        ControlTarget::LayerFxBypass(id) => on(comp.mix_controls(*id)?.2),
        ControlTarget::LayerSpeed(id) => to_unit(layer(id)?.rate, 0.1, 4.0, true),
        ControlTarget::LayerVolume(id) => layer(id)?.volume / 2.0,
        ControlTarget::LayerX(id) => (layer(id)?.transform.x + 1.0) / 2.0,
        ControlTarget::LayerY(id) => (layer(id)?.transform.y + 1.0) / 2.0,
        ControlTarget::LayerScale(id) => to_unit(layer(id)?.transform.scale, 0.05, 4.0, true),
        ControlTarget::LayerRotation(id) => (layer(id)?.transform.rotation + 180.0) / 360.0,
        ControlTarget::LayerCorner { layer: id, point } => layer(id)?.transform.corners?[(*point as usize / 2).min(3)][*point as usize % 2].clamp(0.0, 1.0),
        ControlTarget::LayerLoopIn(id) => layer(id)?.loop_in,
        ControlTarget::LayerLoopOut(id) => layer(id)?.loop_out,
        ControlTarget::LayerDirection(id) => {
            layer(id)?.direction.unit()
        }
        ControlTarget::GeneratorParam { layer: lid, index } => {
            let l = layer(lid)?;
            let phosphor_engine::SourceSpec::Generator(g) = &l.source else { return None };
            let p = shaders::generator(g)?.params.get(*index)?;
            param_unit(p, l.generator_values.get(*index).copied().unwrap_or(p.default))
        }
        ControlTarget::EffectParam { effect: eid, index } => {
            let fx = effect(eid)?;
            let p = shaders::effect(&fx.effect)?.params.get(*index)?;
            param_unit(p, fx.values.get(*index).copied().unwrap_or(p.default))
        }
        ControlTarget::EffectMix(id) => effect(id)?.mix,
        ControlTarget::EffectEnabled(id) => on(effect(id)?.enabled),
        ControlTarget::Surface(id) => comp.surface.widgets.iter().find(|w| w.id == *id)?.value,
        ControlTarget::SurfaceY(id) => comp.surface.widgets.iter().find(|w| w.id == *id)?.value_y,
        ControlTarget::SurfaceBar { widget, bar } => comp.surface.widgets.iter().find(|w| w.id == *widget)?.bars.get(*bar as usize)?.value,
        ControlTarget::Modulator(id) => comp.modulators.iter().find(|m| m.id == *id)?.value,
        ControlTarget::App(a) => match a {
            AppControl::LaunchFade => on(comp.launch.fade),
            AppControl::FadeTime => to_unit(comp.launch.fade_secs, 0.05, 10.0, true),
            AppControl::LaunchTiming => comp.launch.timing.unit(),
            AppControl::LaunchStyle => comp.launch.style.unit(),
            AppControl::MasterVolume => comp.volume / 2.0,
            AppControl::MuteSound => on(comp.sound_muted),
            AppControl::AutoLevel => on(comp.sound_auto_level),
            AppControl::WebControl => on(!comp.web_control_off),
            AppControl::Dmx => on(comp.dmx.enabled),
            // (Nothing to freeze without an output: no value to show.)
            AppControl::DmxMaster => comp.dmx.master,
            AppControl::MasterSpeed => to_unit(comp.master_speed, 0.25, 4.0, true),
            AppControl::FreezeOutputs if !comp.outputs.iter().any(|o| o.enabled) => return None,
            AppControl::FreezeOutputs => on(comp.outputs_frozen()),
            AppControl::Blackout => on(comp.blackout_on),
            AppControl::MasterFxBypass => on(comp.master_fx_bypass),
            AppControl::Syphon => on(comp.share_out()),
            AppControl::Ndi => on(comp.ndi.enabled),
            AppControl::Crossfader => comp.crossfader,
            AppControl::SurfacePage => phosphor_engine::pick_unit(comp.surface.current_page(), comp.surface.pages.len()),
            _ => return None,
        },
        ControlTarget::TriggerClip { .. } | ControlTarget::TriggerColumn(_) | ControlTarget::LayerScrub(_) | ControlTarget::LayerJump { .. } | ControlTarget::LayerText(_) | ControlTarget::LayerTextStep { .. } | ControlTarget::LayerClipPick(_) | ControlTarget::LayerRandomClip(_) | ControlTarget::Menu(_) | ControlTarget::Button { .. } | ControlTarget::Slider { .. } => return None,
    })
}

/// Maps 0..1 onto a range, logarithmically when `log`.
pub fn unit_to(v: f32, min: f32, max: f32, log: bool) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if log { min * (max / min).powf(v) } else { min + v * (max - min) }
}

/// Sets any target to a 0..1 value (or `FLIP` for on/off ones). Clip launches and app-wide
/// controls the composition can't do itself are pushed onto `out` for the app.
pub fn set_target(comp: &mut Composition, target: &ControlTarget, v: f32, out: &mut Vec<Deferred>) {
    set_target_depth(comp, target, v, out, 0);
}

/// Surface widgets can drive other widgets; `depth` stops loops (A drives B drives A).
fn set_target_depth(comp: &mut Composition, target: &ControlTarget, v: f32, out: &mut Vec<Deferred>, depth: u8) {
    // Locked controls don't move, whoever sends (controllers, OSC, phones, surface widgets).
    if depth > 6 || comp.control_locked(target) {
        return;
    }
    match target {
        ControlTarget::TriggerClip { layer, slot } => {
            out.push(if v > 0.5 { Deferred::Clip(*layer, *slot) } else { Deferred::ClipRelease(*layer, *slot) });
        }
        ControlTarget::TriggerColumn(col) => {
            if v > 0.5 {
                out.push(Deferred::Column(*col));
            }
        }
        ControlTarget::Modulator(id) => {
            if let Some(m) = comp.modulators.iter_mut().find(|m| m.id == *id) {
                m.value = v.clamp(0.0, 1.0);
            }
        }
        ControlTarget::SurfaceBar { widget, bar } => {
            let Some(b) = comp.surface.widget_mut(*widget).and_then(|w| w.bars.get_mut(*bar as usize)) else { return };
            b.value = v.clamp(0.0, 1.0);
            let (value, bindings) = (b.value, b.bindings.clone());
            for b in bindings {
                set_target_depth(comp, &b.target, b.range.apply(value), out, depth + 1);
            }
        }
        ControlTarget::Surface(id) | ControlTarget::SurfaceY(id) => {
            let y = matches!(target, ControlTarget::SurfaceY(_));
            let Some(w) = comp.surface.widget_mut(*id) else { return };
            if w.kind == phosphor_engine::WidgetKind::Multi && !y {
                // The widget itself (a controller on it): every bar at once.
                w.value = v.clamp(0.0, 1.0);
                for bar in 0..w.bars.len() as u8 {
                    set_target_depth(comp, &ControlTarget::SurfaceBar { widget: *id, bar }, v, out, depth + 1);
                }
                return;
            }
            if w.kind == phosphor_engine::WidgetKind::Color {
                // Hue (X) or saturation (Y) moved: red, green and blue go to their targets.
                if y {
                    w.value_y = v.clamp(0.0, 1.0);
                } else {
                    w.value = v.clamp(0.0, 1.0);
                }
                let rgb = w.rgb();
                let lists = [w.bindings.clone(), w.bindings_y.clone(), w.bindings_z.clone()];
                for (c, list) in rgb.into_iter().zip(lists) {
                    for b in list {
                        set_target_depth(comp, &b.target, b.range.apply(c), out, depth + 1);
                    }
                }
                return;
            }
            let value = match w.kind {
                phosphor_engine::WidgetKind::Toggle if !y => toggled(w.value > 0.5, v) as u8 as f32,
                // A controller on a Choices widget lands on whole choices.
                phosphor_engine::WidgetKind::Choices if !y => w.choice_value(phosphor_engine::pick(v.clamp(0.0, 1.0), w.choice_count())),
                _ => v.clamp(0.0, 1.0),
            };
            let bindings = if y {
                w.value_y = value;
                w.bindings_y.clone()
            } else {
                w.value = value;
                w.bindings.clone()
            };
            for b in bindings {
                set_target_depth(comp, &b.target, b.range.apply(value), out, depth + 1);
            }
        }
        ControlTarget::LayerFxBypass(id) => {
            if let Some(m) = comp.mix_controls_mut(*id) {
                *m.fx_bypass = toggled(*m.fx_bypass, v);
            }
        }
        ControlTarget::LayerVolume(id) => {
            if let Some(l) = comp.layer_mut(*id) {
                l.volume = v.clamp(0.0, 1.0) * 2.0;
            }
        }
        ControlTarget::LayerX(id) | ControlTarget::LayerY(id) | ControlTarget::LayerScale(id) | ControlTarget::LayerRotation(id) => {
            if let Some(l) = comp.layer_mut(*id) {
                let v = v.clamp(0.0, 1.0);
                let t = &mut l.transform;
                match target {
                    ControlTarget::LayerX(_) => t.x = v * 2.0 - 1.0,
                    ControlTarget::LayerY(_) => t.y = v * 2.0 - 1.0,
                    ControlTarget::LayerScale(_) => t.scale = unit_to(v, 0.05, 4.0, true),
                    _ => t.rotation = v * 360.0 - 180.0,
                }
            }
        }
        ControlTarget::MaskTransform { mask, part } => {
            if let Some(m) = comp.mask_mut(*mask) {
                m.transform.set_unit(*part as usize, v);
            }
        }
        ControlTarget::SliceTransform { slice, part } => {
            if let Some(s) = comp.slice_mut(*slice) {
                s.transform.set_unit(*part as usize, v);
            }
        }
        ControlTarget::MaskFeather(id) => {
            if let Some(m) = comp.mask_mut(*id) {
                m.feather = v.clamp(0.0, 1.0) * phosphor_engine::mapping::MASK_FEATHER_MAX;
            }
        }
        ControlTarget::MaskEnabled(id) | ControlTarget::MaskInvert(id) => {
            if let Some(m) = comp.mask_mut(*id) {
                let flag = if matches!(target, ControlTarget::MaskEnabled(_)) { &mut m.enabled } else { &mut m.invert };
                *flag = toggled(*flag, v);
            }
        }
        ControlTarget::SliceFeather(id) => {
            if let Some(s) = comp.slice_mut(*id) {
                s.feather = v.clamp(0.0, 1.0) * phosphor_engine::mapping::SLICE_FEATHER_MAX;
            }
        }
        ControlTarget::Color { owner, part } => {
            if let Some(c) = comp.color_mut(*owner) {
                c.set_unit(*part as usize, v);
            }
        }
        ControlTarget::LayerCorner { layer, point } => {
            if let Some(c) = comp.layer_mut(*layer).and_then(|l| l.transform.corners.as_mut()) {
                c[(*point as usize / 2).min(3)][*point as usize % 2] = v.clamp(0.0, 1.0);
            }
        }
        ControlTarget::LayerSpeed(id) => {
            if let Some(l) = comp.layer_mut(*id) {
                l.rate = unit_to(v, 0.1, 4.0, true);
            }
        }
        ControlTarget::LayerLoopIn(id) => {
            if let Some(l) = comp.layer_mut(*id) {
                l.loop_in = v.clamp(0.0, 1.0);
            }
        }
        ControlTarget::LayerLoopOut(id) => {
            if let Some(l) = comp.layer_mut(*id) {
                l.loop_out = v.clamp(0.0, 1.0);
            }
        }
        ControlTarget::LayerDirection(id) => {
            if let Some(l) = comp.layer_mut(*id) {
                l.direction = Choice::from_unit(v);
            }
        }
        ControlTarget::LayerScrub(id) => out.push(Deferred::Scrub(*id, v.clamp(0.0, 1.0))),
        // Words, not numbers: set by OSC text (`ControlHub::apply_texts`).
        ControlTarget::LayerText(_) => {}
        ControlTarget::LayerClipPick(layer) => {
            // Among the clips that are there; only when the pick changes (a fader sends many values).
            if let Some(l) = comp.layers.iter().find(|l| l.id == *layer) {
                let slots: Vec<usize> = l.clips.iter().enumerate().filter(|(_, c)| !c.is_empty()).map(|(i, _)| i).collect();
                if !slots.is_empty() && v != FLIP {
                    let slot = slots[phosphor_engine::pick(v.clamp(0.0, 1.0), slots.len())];
                    if l.clips.get(slot).is_some_and(|c| l.active_clip != Some(c.id)) {
                        out.push(Deferred::Clip(*layer, slot));
                    }
                }
            }
        }
        ControlTarget::Button { key, .. } => {
            if v > 0.5 || v == FLIP {
                out.push(Deferred::Button(*key));
            }
        }
        ControlTarget::Slider { key, .. } => {
            if v != FLIP {
                out.push(Deferred::Slider(*key, v.clamp(0.0, 1.0)));
            }
        }
        ControlTarget::Menu(key) => {
            if v > 0.5 || v == FLIP {
                out.push(Deferred::Menu(key.clone()));
            }
        }
        ControlTarget::LayerRandomClip(layer) => {
            // On a press (not its release): another of the row's clips, picked at random.
            if (v > 0.5 || v == FLIP)
                && let Some(l) = comp.layers.iter().find(|l| l.id == *layer)
            {
                let slots: Vec<usize> = l.clips.iter().enumerate().filter(|(_, c)| !c.is_empty() && l.active_clip != Some(c.id)).map(|(i, _)| i).collect();
                if let Some(slot) = crate::app::dice().pick(&slots).copied() {
                    out.push(Deferred::Clip(*layer, slot));
                }
            }
        }
        ControlTarget::LayerTextStep { layer, forward } => {
            if (v > 0.5 || v == FLIP)
                && let Some(phosphor_engine::SourceSpec::Text(style)) = comp.layer_mut(*layer).map(|l| &mut l.source)
            {
                style.step_part(*forward);
            }
        }
        ControlTarget::LayerJump { layer, forward } => {
            if v > 0.5 || v == FLIP {
                out.push(Deferred::Jump(*layer, *forward));
            }
        }
        ControlTarget::App(a) => match a {
            AppControl::LaunchFade => comp.launch.fade = toggled(comp.launch.fade, v),
            AppControl::FadeTime => comp.launch.fade_secs = unit_to(v, 0.05, 10.0, true),
            AppControl::LaunchTiming => comp.launch.timing = Choice::from_unit(v),
            AppControl::LaunchStyle => comp.set_launch_style(Choice::from_unit(v)),
            AppControl::MasterVolume => comp.volume = v.clamp(0.0, 1.0) * 2.0,
            AppControl::MuteSound => comp.sound_muted = toggled(comp.sound_muted, v),
            AppControl::AutoLevel => comp.sound_auto_level = toggled(comp.sound_auto_level, v),
            AppControl::WebControl => comp.web_control_off = !toggled(!comp.web_control_off, v),
            AppControl::Dmx => comp.dmx.enabled = toggled(comp.dmx.enabled, v),
            AppControl::DmxMaster => comp.dmx.master = v.clamp(0.0, 1.0),
            AppControl::MasterSpeed => comp.master_speed = unit_to(v, 0.25, 4.0, true),
            AppControl::Blackout => comp.blackout_on = toggled(comp.blackout_on, v),
            AppControl::FreezeOutputs => {
                let frozen = toggled(comp.outputs_frozen(), v);
                comp.freeze_outputs(frozen);
            }
            AppControl::MasterFxBypass => comp.master_fx_bypass = toggled(comp.master_fx_bypass, v),
            AppControl::Syphon => *comp.share_out_mut() = toggled(comp.share_out(), v),
            AppControl::Ndi => comp.ndi.enabled = toggled(comp.ndi.enabled, v),
            AppControl::Crossfader => comp.crossfader = v.clamp(0.0, 1.0),
            AppControl::SurfacePage => comp.surface.page = phosphor_engine::pick(v, comp.surface.pages.len()),
            _ => out.push(Deferred::App(*a, v)),
        },
        ControlTarget::MasterFade => comp.master_fade = v.clamp(0.0, 1.0),
        ControlTarget::LayerOpacity(id) => {
            if let Some(m) = comp.mix_controls_mut(*id) {
                *m.opacity = v.clamp(0.0, 1.0);
            }
        }
        ControlTarget::LayerEnabled(id) => {
            if let Some(m) = comp.mix_controls_mut(*id) {
                *m.enabled = toggled(*m.enabled, v);
            }
        }
        ControlTarget::Fixture { fixture, param } => {
            if let Some(x) = comp.dmx.fixtures.iter_mut().find(|f| f.id == *fixture).and_then(|f| f.param_mut(*param)) {
                *x = v.clamp(0.0, 1.0);
            }
        }
        ControlTarget::SliceOpacity(id) => {
            if let Some(g) = comp.slice_group_mut(*id) {
                g.opacity = v.clamp(0.0, 1.0);
            } else if let Some(s) = comp.slice_mut(*id) {
                s.opacity = v.clamp(0.0, 1.0);
            }
        }
        ControlTarget::Layout(id) => {
            if v > 0.5 {
                out.push(Deferred::Layout(*id));
            }
        }
        ControlTarget::StopLayer(id) => {
            if v > 0.5 {
                out.push(Deferred::Stop(*id));
            }
        }
        ControlTarget::SliceEnabled(id) if comp.slice_group(*id).is_some() => {
            if let Some(g) = comp.slice_group_mut(*id) {
                g.enabled = toggled(g.enabled, v);
            }
        }
        ControlTarget::SliceEnabled(id) => {
            let ids = comp.slices_reached(*id);
            if let Some(on) = ids.first().and_then(|f| comp.slice(*f)).map(|s| toggled(s.enabled, v)) {
                for sid in ids {
                    if let Some(s) = comp.slice_mut(sid) {
                        s.enabled = on;
                    }
                }
            }
        }
        ControlTarget::GeneratorParam { layer, index } => {
            if let Some(l) = comp.layer_mut(*layer)
                && let phosphor_engine::SourceSpec::Generator(g) = &l.source
                && let Some(desc) = shaders::generator(g)
                && let Some(p) = desc.params.get(*index)
            {
                if l.generator_values.len() <= *index {
                    l.generator_values.resize(index + 1, p.default);
                }
                l.generator_values[*index] = scaled(p, v);
            }
        }
        ControlTarget::EffectParam { effect, index } => {
            if let Some(fx) = find_effect_mut(comp, *effect)
                && let Some(desc) = shaders::effect(&fx.effect)
                && let Some(p) = desc.params.get(*index)
            {
                if fx.values.len() <= *index {
                    fx.values.resize(index + 1, p.default);
                }
                fx.values[*index] = scaled(p, v);
            }
        }
        ControlTarget::EffectMix(id) => {
            if let Some(fx) = find_effect_mut(comp, *id) {
                fx.mix = v.clamp(0.0, 1.0);
            }
        }
        ControlTarget::EffectEnabled(id) => {
            if let Some(fx) = find_effect_mut(comp, *id) {
                fx.enabled = toggled(fx.enabled, v);
            }
        }
    }
}

/// Every bindable target in the composition, with a readable name.
pub fn all_targets(comp: &Composition) -> Vec<(ControlTarget, String)> {
    let mut out = vec![(ControlTarget::MasterFade, "Master fade".to_string())];
    out.extend(AppControl::ALL.iter().map(|a| (ControlTarget::App(*a), a.name().to_string())));
    for w in &comp.surface.widgets {
        if w.kind == phosphor_engine::WidgetKind::Label {
            continue;
        }
        out.push((ControlTarget::Surface(w.id), format!("Surface · {}", w.label)));
        if matches!(w.kind, phosphor_engine::WidgetKind::XyPad | phosphor_engine::WidgetKind::Color) {
            out.push((ControlTarget::SurfaceY(w.id), format!("Surface · {} (Y)", w.label)));
        }
        for i in 0..w.bars.len() {
            out.push((ControlTarget::SurfaceBar { widget: w.id, bar: i as u8 }, format!("Surface · {} · bar {}", w.label, i + 1)));
        }
    }
    for m in comp.modulators.iter().filter(|m| m.signal.source == phosphor_engine::ModSource::Hand) {
        out.push((ControlTarget::Modulator(m.id), format!("Modulator · {}", m.name)));
    }
    for col in 0..comp.clip_columns {
        out.push((ControlTarget::TriggerColumn(col), format!("Column {} (trigger)", col + 1)));
    }
    let effects = |prefix: &str, effects: &[EffectState], out: &mut Vec<(ControlTarget, String)>| {
        for fx in effects {
            let Some(desc) = shaders::effect(&fx.effect) else { continue };
            out.push((ControlTarget::EffectEnabled(fx.id), format!("{prefix} · {} on/off", desc.name)));
            out.push((ControlTarget::EffectMix(fx.id), format!("{prefix} · {} mix", desc.name)));
            for (i, p) in desc.params.iter().enumerate() {
                out.push((ControlTarget::EffectParam { effect: fx.id, index: i }, format!("{prefix} · {} · {}", desc.name, p.name)));
            }
        }
    };
    effects("MASTER", &comp.master_effects, &mut out);
    let colors = |name: &str, owner: u64, out: &mut Vec<(ControlTarget, String)>| {
        for (i, part) in phosphor_engine::ColorGrade::PARTS.iter().enumerate() {
            out.push((ControlTarget::Color { owner, part: i as u8 }, format!("{name} · colour {}", part.name.to_lowercase())));
        }
    };
    for o in &comp.outputs {
        colors(&o.name, o.id, &mut out);
        for s in &o.mapping.surfaces {
            out.push((ControlTarget::SliceOpacity(s.id), format!("{} · {} · opacity", o.name, s.name)));
            out.push((ControlTarget::SliceEnabled(s.id), format!("{} · {} · on/off", o.name, s.name)));
            for (i, part) in phosphor_engine::mapping::ShapeTransform::PARTS.iter().enumerate() {
                out.push((ControlTarget::SliceTransform { slice: s.id, part: i as u8 }, format!("{} · {} · {}", o.name, s.name, part.name.to_lowercase())));
            }
            out.push((ControlTarget::SliceFeather(s.id), format!("{} · {} · soft edge", o.name, s.name)));
            colors(&format!("{} · {}", o.name, s.name), s.id, &mut out);
            effects(&format!("{} · {}", o.name, s.name), &s.effects, &mut out);
        }
        for m in &o.mapping.masks {
            out.push((ControlTarget::MaskEnabled(m.id), format!("{} · {} · on/off", o.name, m.name)));
            out.push((ControlTarget::MaskInvert(m.id), format!("{} · {} · invert", o.name, m.name)));
            out.push((ControlTarget::MaskFeather(m.id), format!("{} · {} · soft edge", o.name, m.name)));
            for (i, part) in phosphor_engine::mapping::ShapeTransform::PARTS.iter().enumerate() {
                out.push((ControlTarget::MaskTransform { mask: m.id, part: i as u8 }, format!("{} · {} · {}", o.name, m.name, part.name.to_lowercase())));
            }
        }
        for g in o.mapping.layouts() {
            out.push((ControlTarget::Layout(g.id), format!("{} · {} (layout) · switch to it", o.name, g.name)));
        }
        for g in &o.mapping.groups {
            out.push((ControlTarget::SliceOpacity(g.id), format!("{} · {} (group) · opacity", o.name, g.name)));
            out.push((ControlTarget::SliceEnabled(g.id), format!("{} · {} (group) · on/off", o.name, g.name)));
        }
    }
    for f in &comp.dmx.fixtures {
        for p in f.params() {
            out.push((ControlTarget::Fixture { fixture: f.id, param: p }, format!("Light {} · {}", f.name, f.param_name(p))));
        }
    }
    for c in comp.comps.iter().filter(|_| comp.comps.len() > 1) {
        out.push((ControlTarget::LayerOpacity(c.id), format!("Comp {} · opacity", c.name)));
        out.push((ControlTarget::LayerEnabled(c.id), format!("Comp {} · on/off", c.name)));
        out.push((ControlTarget::LayerFxBypass(c.id), format!("Comp {} · effects bypass", c.name)));
        effects(&c.name, &c.effects, &mut out);
    }
    for g in &comp.groups {
        out.push((ControlTarget::LayerOpacity(g.id), format!("{} · opacity", g.name)));
        out.push((ControlTarget::LayerEnabled(g.id), format!("{} · on/off", g.name)));
        out.push((ControlTarget::LayerFxBypass(g.id), format!("{} · effects bypass", g.name)));
        effects(&g.name, &g.effects, &mut out);
    }
    for l in &comp.layers {
        out.push((ControlTarget::LayerOpacity(l.id), format!("{} · opacity", l.name)));
        out.push((ControlTarget::LayerEnabled(l.id), format!("{} · on/off", l.name)));
        out.push((ControlTarget::LayerSpeed(l.id), format!("{} · speed", l.name)));
        out.push((ControlTarget::LayerFxBypass(l.id), format!("{} · effects bypass", l.name)));
        out.push((ControlTarget::LayerX(l.id), format!("{} · position X", l.name)));
        out.push((ControlTarget::LayerY(l.id), format!("{} · position Y", l.name)));
        out.push((ControlTarget::LayerScale(l.id), format!("{} · scale", l.name)));
        out.push((ControlTarget::LayerRotation(l.id), format!("{} · rotation", l.name)));
        if l.transform.corners.is_some() {
            for (point, name) in ["top-left X", "top-left Y", "top-right X", "top-right Y", "bottom-right X", "bottom-right Y", "bottom-left X", "bottom-left Y"].iter().enumerate() {
                out.push((ControlTarget::LayerCorner { layer: l.id, point: point as u8 }, format!("{} · corner {name}", l.name)));
            }
        }
        if matches!(l.source, phosphor_engine::SourceSpec::Text(_)) {
            out.push((ControlTarget::LayerText(l.id), format!("{} · text (OSC words)", l.name)));
            out.push((ControlTarget::LayerTextStep { layer: l.id, forward: true }, format!("{} · next text part", l.name)));
            out.push((ControlTarget::LayerTextStep { layer: l.id, forward: false }, format!("{} · previous text part", l.name)));
        }
        if matches!(l.source, phosphor_engine::SourceSpec::Movie { .. }) {
            out.push((ControlTarget::LayerScrub(l.id), format!("{} · scrub", l.name)));
            out.push((ControlTarget::LayerJump { layer: l.id, forward: false }, format!("{} · jump back", l.name)));
            out.push((ControlTarget::LayerJump { layer: l.id, forward: true }, format!("{} · jump forward", l.name)));
            out.push((ControlTarget::LayerVolume(l.id), format!("{} · volume", l.name)));
            out.push((ControlTarget::LayerLoopIn(l.id), format!("{} · loop in", l.name)));
            out.push((ControlTarget::LayerLoopOut(l.id), format!("{} · loop out", l.name)));
            out.push((ControlTarget::LayerDirection(l.id), format!("{} · direction", l.name)));
        }
        if l.clips.iter().any(|c| !c.is_empty()) {
            out.push((ControlTarget::LayerClipPick(l.id), format!("{} · pick a clip (by value)", l.name)));
            out.push((ControlTarget::LayerRandomClip(l.id), format!("{} · random clip", l.name)));
        }
        for (slot, clip) in l.clips.iter().enumerate() {
            if !clip.is_empty() {
                out.push((ControlTarget::TriggerClip { layer: l.id, slot }, format!("{} · clip {} ({})", l.name, slot + 1, clip.name)));
            }
        }
        if let phosphor_engine::SourceSpec::Generator(g) = &l.source
            && let Some(desc) = shaders::generator(g)
        {
            for (i, p) in desc.params.iter().enumerate() {
                out.push((ControlTarget::GeneratorParam { layer: l.id, index: i }, format!("{} · {} · {}", l.name, desc.name, p.name)));
            }
        }
        effects(&format!("{} · clip", l.name), &l.clip_effects, &mut out);
        effects(&l.name, &l.effects, &mut out);
    }
    // Every menu item (File › Save, Quick › Surprise…).
    out.extend(crate::menu::mappable().into_iter().map(|(a, name)| (ControlTarget::Menu(crate::menu::action_key(a)), format!("Menu · {name}"))));
    out
}

/// The OSC address every control answers to without a mapping (values 0..1; 1 = press for triggers).
pub fn default_osc_address(target: &ControlTarget) -> String {
    match target {
        ControlTarget::MasterFade => "/phosphor/master".into(),
        ControlTarget::LayerOpacity(id) => format!("/phosphor/layer/{id}/opacity"),
        ControlTarget::LayerEnabled(id) => format!("/phosphor/layer/{id}/on"),
        ControlTarget::SliceOpacity(id) => format!("/phosphor/slice/{id}/opacity"),
        ControlTarget::SliceEnabled(id) => format!("/phosphor/slice/{id}/on"),
        ControlTarget::MaskTransform { mask: id, part } | ControlTarget::SliceTransform { slice: id, part } => {
            let kind = if matches!(target, ControlTarget::MaskTransform { .. }) { "mask" } else { "slice" };
            let name = phosphor_engine::mapping::ShapeTransform::PARTS.get(*part as usize).map_or("part", |p| p.name);
            format!("/phosphor/{kind}/{id}/{}", name.to_lowercase().replace(' ', "_"))
        }
        ControlTarget::Layout(id) => format!("/phosphor/layout/{id}"),
        ControlTarget::StopLayer(id) if *id == phosphor_engine::MASTER_ID => "/phosphor/stop".into(),
        ControlTarget::StopLayer(id) => format!("/phosphor/layer/{id}/stop"),
        ControlTarget::MaskFeather(id) => format!("/phosphor/mask/{id}/soft_edge"),
        ControlTarget::MaskEnabled(id) => format!("/phosphor/mask/{id}/on"),
        ControlTarget::MaskInvert(id) => format!("/phosphor/mask/{id}/invert"),
        ControlTarget::SliceFeather(id) => format!("/phosphor/slice/{id}/soft_edge"),
        ControlTarget::Color { owner, part } => {
            let name = phosphor_engine::ColorGrade::PARTS.get(*part as usize).map_or("part", |p| p.name);
            format!("/phosphor/colour/{owner}/{}", name.to_lowercase())
        }
        ControlTarget::Fixture { fixture, param } => match phosphor_engine::Fixture::PARAMS.get(*param as usize) {
            Some(name) => format!("/phosphor/light/{fixture}/{}", name.replace(' ', "_")),
            None => format!("/phosphor/light/{fixture}/extra/{}", param - 4),
        },
        ControlTarget::GeneratorParam { layer, index } => format!("/phosphor/layer/{layer}/source/{}", index + 1),
        ControlTarget::EffectParam { effect, index } => format!("/phosphor/fx/{effect}/{}", index + 1),
        ControlTarget::EffectMix(id) => format!("/phosphor/fx/{id}/mix"),
        ControlTarget::EffectEnabled(id) => format!("/phosphor/fx/{id}/on"),
        ControlTarget::TriggerClip { layer, slot } => format!("/phosphor/layer/{layer}/clip/{}", slot + 1),
        ControlTarget::TriggerColumn(col) => format!("/phosphor/column/{}", col + 1),
        ControlTarget::LayerSpeed(id) => format!("/phosphor/layer/{id}/speed"),
        ControlTarget::LayerFxBypass(id) => format!("/phosphor/layer/{id}/fxbypass"),
        ControlTarget::LayerLoopIn(id) => format!("/phosphor/layer/{id}/loop/in"),
        ControlTarget::LayerLoopOut(id) => format!("/phosphor/layer/{id}/loop/out"),
        ControlTarget::LayerDirection(id) => format!("/phosphor/layer/{id}/direction"),
        ControlTarget::LayerScrub(id) => format!("/phosphor/layer/{id}/scrub"),
        ControlTarget::LayerText(id) => format!("/phosphor/layer/{id}/text"),
        ControlTarget::LayerClipPick(id) => format!("/phosphor/layer/{id}/clip/pick"),
        ControlTarget::LayerRandomClip(id) => format!("/phosphor/layer/{id}/clip/random"),
        ControlTarget::Button { key, .. } => format!("/phosphor/button/{key:x}"),
        ControlTarget::Slider { key, .. } => format!("/phosphor/slider/{key:x}"),
        ControlTarget::Menu(key) => format!("/phosphor/menu/{}", key.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect::<String>().trim_matches('_')),
        ControlTarget::LayerTextStep { layer, forward } => format!("/phosphor/layer/{layer}/text/{}", if *forward { "next" } else { "previous" }),
        ControlTarget::LayerJump { layer, forward } => format!("/phosphor/layer/{layer}/jump/{}", if *forward { "forward" } else { "back" }),
        ControlTarget::LayerVolume(id) => format!("/phosphor/layer/{id}/volume"),
        ControlTarget::LayerX(id) => format!("/phosphor/layer/{id}/x"),
        ControlTarget::LayerY(id) => format!("/phosphor/layer/{id}/y"),
        ControlTarget::LayerScale(id) => format!("/phosphor/layer/{id}/scale"),
        ControlTarget::LayerRotation(id) => format!("/phosphor/layer/{id}/rotation"),
        ControlTarget::LayerCorner { layer, point } => format!("/phosphor/layer/{layer}/corner/{}/{}", point / 2 + 1, if point % 2 == 0 { "x" } else { "y" }),
        ControlTarget::Surface(id) => format!("/phosphor/surface/{id}"),
        ControlTarget::SurfaceY(id) => format!("/phosphor/surface/{id}/y"),
        ControlTarget::SurfaceBar { widget, bar } => format!("/phosphor/surface/{widget}/bar/{}", bar + 1),
        ControlTarget::Modulator(id) => format!("/phosphor/modulator/{id}"),
        ControlTarget::App(a) => format!("/phosphor/{}", a.osc()),
    }
}

pub fn target_label(comp: &Composition, target: &ControlTarget) -> String {
    all_targets(comp).into_iter().find(|(t, _)| t == target).map(|(_, n)| n).unwrap_or_else(|| "(removed)".into())
}
