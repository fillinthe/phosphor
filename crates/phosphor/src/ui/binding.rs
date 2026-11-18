//! Editors for what drives what: pick a target, type an input (MIDI/OSC/key), set a value range,
//! and edit a list of bindings. Used by the Mappings editor, the "Map to…" menu and the Control
//! Surface.

use super::*;
use phosphor_engine::{Binding, ControlSource, ValueRange};

/// A searchable list of everything that can be controlled. `targets` comes from
/// `control::all_targets` (compute it once per frame, not per row). Returns the new pick.
pub(crate) fn target_picker(ui: &mut egui::Ui, id_salt: impl std::hash::Hash + std::fmt::Debug, targets: &[(ControlTarget, String)], current: Option<&ControlTarget>, width: f32) -> Option<ControlTarget> {
    let id = egui::Id::new(("target_picker", id_salt));
    let text = current.and_then(|c| targets.iter().find(|(t, _)| t == c)).map(|(_, n)| n.clone()).unwrap_or_else(|| {
        if current.is_some() { "(removed)".into() } else { "Pick what to control…".into() }
    });
    let mut picked = None;
    ComboBox::from_id_salt(id).width(width).selected_text(text).height(320.0).show_ui(ui, |ui| {
        let (q, _) = crate::ui::search_field(ui, id, "Search…", width.max(200.0));
        for (t, name) in targets.iter().filter(|(_, n)| q.is_empty() || n.to_lowercase().contains(&q)) {
            if crate::ui::pick_item(ui, current == Some(t), name).clicked() {
                picked = Some(t.clone());
            }
        }
    });
    picked
}

const SOURCE_KINDS: [&str; 9] = ["MIDI CC", "MIDI Note", "OSC", "Key", "Gamepad", "DMX", "Arduino", "Stream Deck", "Deck dial"];

fn source_kind(s: &ControlSource) -> usize {
    match s {
        ControlSource::MidiCc { .. } => 0,
        ControlSource::MidiNote { .. } => 1,
        ControlSource::Osc { .. } => 2,
        ControlSource::Key { .. } => 3,
        ControlSource::Gamepad { .. } => 4,
        ControlSource::Dmx { .. } => 5,
        ControlSource::Arduino { .. } => 6,
        ControlSource::StreamDeck { .. } => 7,
        ControlSource::StreamDeckDial { .. } => 8,
    }
}

/// Type in an input by hand: MIDI CC / note (channel 1–16, number 0–127), an OSC address or a key.
/// Returns true when it changed.
pub(crate) fn source_editor(ui: &mut egui::Ui, id_salt: impl std::hash::Hash + std::fmt::Debug, source: &mut ControlSource) -> bool {
    let before = source.clone();
    let id = egui::Id::new(("source_editor", id_salt));
    let mut kind = source_kind(source);
    crate::ui::dropdown(ui, id, 82.0).selected_text(SOURCE_KINDS[kind]).show_ui(ui, |ui| {
        for (i, k) in SOURCE_KINDS.iter().enumerate() {
            crate::ui::pick_value(ui, &mut kind, i, *k);
        }
    });
    let (ch, num) = match source {
        ControlSource::MidiCc { channel, cc } => (*channel, *cc),
        ControlSource::MidiNote { channel, note } => (*channel, *note),
        _ => (0, 0),
    };
    if kind != source_kind(source) {
        *source = match kind {
            0 => ControlSource::MidiCc { channel: ch, cc: num },
            1 => ControlSource::MidiNote { channel: ch, note: num },
            2 => ControlSource::Osc { address: "/".into() },
            3 => ControlSource::Key { key: "Q".into() },
            4 => ControlSource::Gamepad { input: "South".into() },
            5 => ControlSource::Dmx { universe: 1, channel: 1 },
            6 => ControlSource::Arduino { pin: 0, analog: true },
            7 => ControlSource::StreamDeck { key: 0 },
            _ => ControlSource::StreamDeckDial { dial: 0 },
        };
    }
    match source {
        ControlSource::MidiCc { channel, cc: n } | ControlSource::MidiNote { channel, note: n } => {
            let mut c = *channel + 1;
            guarded!(ui, c, egui::DragValue::new(&mut c).range(1..=16).prefix("ch "));
            *channel = c - 1;
            guarded!(ui, *n, egui::DragValue::new(n).range(0..=127));
        }
        ControlSource::Osc { address } => {
            crate::ui::text_field(ui, address, "/touchosc/fader1", Some(150.0));
        }
        ControlSource::Key { key } => {
            if ui.add(egui::TextEdit::singleline(key).char_limit(12).desired_width(50.0)).changed() {
                *key = key.to_uppercase();
            }
        }
        ControlSource::Gamepad { input } => {
            crate::ui::text_field(ui, input, "South", Some(110.0))
                .on_hover_text("Button or stick name (South, East, LeftStickX, RightTrigger2…); easiest: press Listen and move it");
        }
        ControlSource::Dmx { universe, channel } => {
            guarded!(ui, *universe, egui::DragValue::new(universe).range(0..=32767).prefix("universe "))
                .on_hover_text("The universe number your desk sends (Art-Net counts from 0, sACN from 1)");
            guarded!(ui, *channel, egui::DragValue::new(channel).range(1..=512).prefix("ch "));
        }
        ControlSource::Arduino { pin, analog } => {
            crate::ui::toggle(ui, analog, "Analog").on_hover_text("On: an analog input (A0…, a knob or sensor). Off: a digital pin (a button)");
            guarded!(ui, *pin, egui::DragValue::new(pin).range(0..=63).prefix(if *analog { "A" } else { "pin " }));
        }
        ControlSource::StreamDeck { key } => {
            // Shown from 1, as on the deck (left to right, top to bottom).
            let mut k = *key as u32 + 1;
            guarded!(ui, k, egui::DragValue::new(&mut k).range(1..=200).prefix("key "))
                .on_hover_text("Keys count from 1, left to right, top to bottom; a Stream Deck + dial's press is key 101, 102…");
            *key = (k - 1) as u8;
        }
        ControlSource::StreamDeckDial { dial } => {
            let mut d = *dial as u32 + 1;
            guarded!(ui, d, egui::DragValue::new(&mut d).range(1..=8).prefix("dial "));
            *dial = (d - 1) as u8;
        }
    }
    *source != before
}

/// Min / max / invert for how a 0..1 value lands on its target.
pub(crate) fn range_editor(ui: &mut egui::Ui, range: &mut ValueRange) {
    guarded!(ui, range.min, egui::DragValue::new(&mut range.min).range(0.0..=1.0).speed(0.005).fixed_decimals(2).prefix("from "));
    guarded!(ui, range.max, egui::DragValue::new(&mut range.max).range(0.0..=1.0).speed(0.005).fixed_decimals(2).prefix("to "));
    crate::ui::icon_toggle(ui, &mut range.invert, icon::ARROWS_DOWN_UP).on_hover_text("Invert (up = down)");
}

/// One target's own settings: the part of the fader it listens to, its value effects, its range.
pub(crate) fn binding_editor(ui: &mut egui::Ui, b: &mut Binding) {
    let part = &mut b.part;
    let whole = part.is_whole();
    let text = if whole { RichText::new(icon::ARROWS_HORIZONTAL) } else { RichText::new(format!("{} {:.0}–{:.0}%", icon::ARROWS_HORIZONTAL, part.from * 100.0, part.to * 100.0)).color(accent()) };
    crate::ui::field_menu(ui, text, |ui| {
        ui.set_min_width(240.0);
        crate::ui::heading_help(ui, "Part of the fader", "This target runs through its whole range over just this part of the fader's travel, and holds below and above it. Give each target its own part and one knob plays them one after another.");
        ui.horizontal(|ui| {
            let (mut from, mut to) = (part.from * 100.0, part.to * 100.0);
            guarded!(ui, from, egui::DragValue::new(&mut from).range(0.0..=100.0).speed(0.5).max_decimals(0).prefix("from ").suffix("%"));
            guarded!(ui, to, egui::DragValue::new(&mut to).range(0.0..=100.0).speed(0.5).max_decimals(0).prefix("to ").suffix("%"));
            (part.from, part.to) = (from / 100.0, to / 100.0);
            if !whole && crate::ui::small_command(ui, "Whole").on_hover_text("Listen to the whole fader again").clicked() {
                *part = Default::default();
            }
        });
    })
    .response
    .on_hover_text("The part of the fader this target listens to");
    value_fx_menu(ui, &mut b.fx, false);
    range_editor(ui, &mut b.range);
}

/// The targets one control drives, each with its own part of the fader, value effects and range: pick, adjust,
/// remove, add more.
pub(crate) fn binding_list(ui: &mut egui::Ui, id_salt: impl std::hash::Hash + std::fmt::Debug + Copy, targets: &[(ControlTarget, String)], bindings: &mut Vec<Binding>) {
    let mut remove = None;
    for (i, b) in bindings.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            if let Some(t) = target_picker(ui, (id_salt, i), targets, Some(&b.target), 200.0) {
                b.target = t;
            }
            binding_editor(ui, b);
            if crate::ui::small_danger(ui, icon::TRASH).on_hover_text("Remove").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        bindings.remove(i);
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new(icon::PLUS).color(accent()));
        if let Some(t) = target_picker(ui, (id_salt, "add"), targets, None, 200.0) {
            bindings.push(Binding::new(t));
        }
    });
}

/// The ~ menu on a mapping: effects its value goes through (smooth, steps, curve, formula), in order.
/// `timed` = also offer the ones that work over time (smooth, delay, fall, endless knob, dead band): a controller
/// mapping has those; a target of a fader or of another control takes the instant ones only.
pub(crate) fn value_fx_menu(ui: &mut egui::Ui, fx: &mut Vec<phosphor_engine::ValueFx>, timed: bool) {
    use phosphor_engine::ValueFx;
    let text = if fx.is_empty() { RichText::new(icon::WAVE_SINE) } else { RichText::new(format!("{} {}", icon::WAVE_SINE, fx.len())).color(accent()) };
    crate::ui::field_menu(ui, text, |ui| {
        ui.set_min_width(260.0);
        crate::ui::heading_help(ui, "On the way in", "The controller's value goes through these, top to bottom, before its range.");
        let mut remove = None;
        for (k, f) in fx.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                crate::ui::heading(ui, f.name());
                match f {
                    ValueFx::Smooth(s) => {
                        guarded!(ui, *s, egui::DragValue::new(s).range(0.02..=10.0).speed(0.01).suffix(" s")).on_hover_text("Glide to each new value over this long");
                    }
                    ValueFx::Steps(n) => {
                        guarded!(ui, *n, egui::DragValue::new(n).range(2..=64)).on_hover_text("Snap to this many steps");
                    }
                    ValueFx::Curve(c) => {
                        guarded!(ui, *c, egui::DragValue::new(c).range(0.1..=8.0).speed(0.02)).on_hover_text("Above 1: slow then fast; below 1: fast then slow");
                    }
                    ValueFx::Formula(f) => {
                        text_field(ui, f, "1 - $VAL", Some(140.0)).on_hover_text("$VAL is the value (0..1); + - * / %, pow, sin, cos (a turn per 1.0), abs, min, max, clamp, mix, fract, floor…");
                    }
                    ValueFx::Delay(s) => {
                        guarded!(ui, *s, egui::DragValue::new(s).range(0.02..=30.0).speed(0.01).suffix(" s")).on_hover_text("Each move lands this much later");
                    }
                    ValueFx::Threshold(t) => {
                        guarded!(ui, *t, egui::DragValue::new(t).range(0.0..=1.0).speed(0.005).max_decimals(2)).on_hover_text("On (full) at or above this, off below: a fader becomes a switch");
                    }
                    ValueFx::Clamp(lo, hi) => {
                        guarded!(ui, *lo, egui::DragValue::new(lo).range(0.0..=1.0).speed(0.005).max_decimals(2).prefix("from ")).on_hover_text("Lowest value it can send");
                        guarded!(ui, *hi, egui::DragValue::new(hi).range(0.0..=1.0).speed(0.005).max_decimals(2).prefix("to ")).on_hover_text("Highest value it can send");
                    }
                    ValueFx::Repeat(n) => {
                        guarded!(ui, *n, egui::DragValue::new(n).range(1..=32).suffix("×")).on_hover_text("The range repeats this many times along the fader");
                    }
                    ValueFx::Randomize(a) => {
                        guarded!(ui, *a, egui::DragValue::new(a).range(0.0..=1.0).speed(0.005).max_decimals(2)).on_hover_text("Up to this much random wobble on each value");
                    }
                    ValueFx::Fall(s) => {
                        guarded!(ui, *s, egui::DragValue::new(s).range(0.02..=10.0).speed(0.01).suffix(" s")).on_hover_text("Rises land at once; drops glide down over this long (peak hold)");
                    }
                    ValueFx::Endless(s) => {
                        guarded!(ui, *s, egui::DragValue::new(s).range(0.001..=0.25).speed(0.0005).max_decimals(3).prefix("per click ")).on_hover_text("For knobs that turn forever (relative MIDI: 1–63 up, 65–127 down): how far one click moves the value (0.01 = 1%)");
                    }
                    ValueFx::DeadBand(d) => {
                        guarded!(ui, *d, egui::DragValue::new(d).range(0.001..=0.25).speed(0.0005).max_decimals(3)).on_hover_text("Changes smaller than this are ignored (a jittery fader or sensor)");
                    }
                    ValueFx::Lookup(points) => {
                        // Draw the output: left = fader at 0, right = fader at full.
                        crate::ui::step_bars(ui, points, None, false, 40.0);
                    }
                }
                if crate::ui::small_command(ui, icon::X).clicked() {
                    remove = Some(k);
                }
            });
        }
        if let Some(k) = remove {
            fx.remove(k);
        }
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            let fresh = [
                ValueFx::Smooth(0.3),
                ValueFx::Steps(4),
                ValueFx::Curve(2.0),
                ValueFx::Formula("1 - $VAL".into()),
                ValueFx::Delay(0.5),
                ValueFx::Threshold(0.5),
                ValueFx::Clamp(0.0, 1.0),
                ValueFx::Repeat(2),
                ValueFx::Randomize(0.1),
                ValueFx::Fall(0.5),
                ValueFx::Lookup((0..9).map(|i| i as f32 / 8.0).collect()),
                ValueFx::Endless(0.01),
                ValueFx::DeadBand(0.02),
            ];
            for f in fresh.into_iter().filter(|f| timed || !matches!(f, ValueFx::Smooth(_) | ValueFx::Delay(_) | ValueFx::Fall(_) | ValueFx::Endless(_) | ValueFx::DeadBand(_))) {
                let name = f.name();
                if crate::ui::small_command(ui, label(icon::PLUS, name)).clicked() {
                    fx.push(f);
                }
            }
        });
    })
    .response
    .on_hover_text(if timed { "Smooth, steps, curve or a formula on this mapping's value" } else { "This target's own steps, curve or formula" });
}

