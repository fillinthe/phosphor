//! Modulators panel: shared, named signal sources any control can follow (Modulate menu → Shared modulators),
//! chained into each other. VDMX-style data sources, the Phosphor way.

use phosphor_engine::{ChainMode, ModChain, Modulator};

use super::*;

pub(crate) fn modulators(ui: &mut egui::Ui, st: &mut State) {
    let beats = st.mod_inputs.beats;
    let inputs = st.mod_inputs;
    crate::ui::toolbar(ui, |ui| {
        if crate::ui::command(ui, label(icon::WAVE_SINE, "LFO")).on_hover_text("A shared LFO (change its shape, speed or source on it): any control can follow it from its Modulate menu").clicked() && st.comp.modulators.len() < phosphor_engine::control::MAX_SHARED {
            let id = st.comp.new_id();
            let n = st.comp.modulators.iter().filter(|m| m.name.starts_with("LFO")).count() + 1;
            let signal = phosphor_engine::Modulation { source: phosphor_engine::ModSource::LfoSine, rate: 0.5, ..Default::default() };
            st.comp.modulators.push(Modulator { id, name: format!("LFO {n}"), signal, ..Default::default() });
        }
        if crate::ui::command(ui, label(icon::SQUARES_FOUR, "Step track")).on_hover_text("A step sequencer track on the beat (levels or on/off gates) that any control can follow").clicked() && st.comp.modulators.len() < phosphor_engine::control::MAX_SHARED {
            let id = st.comp.new_id();
            let n = st.comp.modulators.iter().filter(|m| m.sequence.is_some()).count() + 1;
            let signal = phosphor_engine::Modulation { source: phosphor_engine::ModSource::Steps, ..Default::default() };
            st.comp.modulators.push(Modulator { id, name: format!("Track {n}"), signal, sequence: Some(super::sequencer::new_track()), ..Default::default() });
        }
        if crate::ui::command(ui, label(icon::WAVEFORM, "Sound band")).on_hover_text("How loud the sound is between two frequencies (a kick, a snare, a voice): any control can follow it").clicked() && st.comp.modulators.len() < phosphor_engine::control::MAX_SHARED {
            let id = st.comp.new_id();
            let n = st.comp.modulators.iter().filter(|m| m.band.is_some()).count() + 1;
            let signal = phosphor_engine::Modulation { source: phosphor_engine::ModSource::Hand, ..Default::default() };
            st.comp.modulators.push(Modulator { id, name: format!("Band {n}"), signal, band: Some([40.0, 120.0]), value: 0.0, ..Default::default() });
        }
        help(ui, "One source drives many controls: right-click any slider, Modulate, then pick it. Chain one into the next.");
    });
    #[cfg(not(target_arch = "wasm32"))]
    super::tracking::tracking_section(ui, st);
    #[cfg(not(target_arch = "wasm32"))]
    super::live_data::live_data_section(ui, st);
    let mut remove = None;
    let mut moves: Option<(usize, usize)> = None;
    let earlier: Vec<(u64, String)> = st.comp.modulators.iter().map(|m| (m.id, m.name.clone())).collect();
    // What gate tracks can fire (only worked out when there's a gate track to show).
    let targets = if st.comp.modulators.iter().any(|m| m.sequence.as_ref().is_some_and(|t| t.gate)) { crate::control::all_targets(&st.comp) } else { Vec::new() };
    // Colour tracks paint colour settings: every colour's red (its green and blue follow).
    let colour_reds: Vec<(phosphor_engine::ControlTarget, String)> = if st.comp.modulators.iter().any(|m| m.sequence.as_ref().is_some_and(|t| t.is_colour())) {
        let all = if targets.is_empty() { crate::control::all_targets(&st.comp) } else { targets.clone() };
        all.into_iter()
            .filter(|(t, _)| crate::control::colour_siblings(&st.comp, t).is_some())
            .map(|(t, n)| (t, n.trim_end_matches(" R").trim_end_matches("Red").trim_end_matches(" · ").to_string()))
            .collect()
    } else {
        Vec::new()
    };
    let n = st.comp.modulators.len();
    crate::ui::column_scroll(ui).show(ui, |ui| {
        for (i, m) in st.comp.modulators.iter_mut().enumerate() {
            let value = inputs.shared.iter().find(|(id, _)| *id == m.id).map_or(0.0, |x| x.1);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut m.name).desired_width(100.0));
                    ui.add(crate::ui::meter(value).desired_width(70.0).fill(accent().gamma_multiply(0.7)));
                    if i > 0 && crate::ui::small_command(ui, icon::ARROW_UP).on_hover_text("Earlier in the chain").clicked() {
                        moves = Some((i, i - 1));
                    }
                    if i + 1 < n && crate::ui::small_command(ui, icon::ARROW_DOWN).on_hover_text("Later in the chain").clicked() {
                        moves = Some((i, i + 1));
                    }
                    if crate::ui::small_command(ui, icon::TRASH).on_hover_text("Delete (controls following it stop moving)").clicked() {
                        remove = Some(i);
                    }
                });
                // Hand / MIDI knob: a fader that any knob, OSC message or the remote can move (right-click it: Learn).
                if let Some(band) = &mut m.band {
                    // A sound band: its value follows the sound (the audio input or the clips' sound).
                    crate::ui::toolbar(ui, |ui| {
                        ui.label("Sound from");
                        guarded!(ui, band[0], egui::DragValue::new(&mut band[0]).range(20.0..=16_000.0).speed(2.0).suffix(" Hz"));
                        ui.label("to");
                        guarded!(ui, band[1], egui::DragValue::new(&mut band[1]).range(20.0..=16_000.0).speed(2.0).suffix(" Hz"));
                        for (name, lo, hi) in [("Kick", 40.0, 120.0), ("Snare", 150.0, 400.0), ("Voice", 300.0, 3000.0), ("Hats", 6000.0, 14000.0)] {
                            if crate::ui::small_command(ui, name).clicked() {
                                *band = [lo, hi];
                            }
                        }
                    });
                } else if m.signal.source == phosphor_engine::ModSource::Hand {
                    ui.horizontal(|ui| {
                        Level::percent(&mut m.value, "Value").default(0.0).target(phosphor_engine::ControlTarget::Modulator(m.id)).show(ui);
                        hint(ui, "right-click: Learn a MIDI knob");
                    });
                    looper_row(ui, &mut m.looper, beats);
                }
                // A step track, or its signal: the same editor as any control's Modulate menu.
                if let (phosphor_engine::ModSource::Steps, Some(track)) = (m.signal.source, &mut m.sequence) {
                    let seed = (inputs.time.to_bits() as u64) << 16 ^ m.id;
                    if ui.push_id(("seq", m.id), |ui| super::sequencer::track_editor(ui, track, beats, seed)).inner {
                        m.sequence = None;
                        m.signal.source = phosphor_engine::ModSource::LfoSine;
                    } else if track.is_colour() {
                        ui.label(RichText::new("Paints its colour into:").small());
                        super::sequencer::colour_targets(ui, ("paint", m.id), &colour_reds, &mut track.paint);
                    } else if track.gate {
                        // Lit steps fire clips, columns or anything else.
                        ui.label(RichText::new("Each lit step fires (a clip plays and lets go, a column fires, a control switches on):").small());
                        crate::ui::binding_list(ui, ("fires", m.id), &targets, &mut track.fires);
                    }
                } else {
                    let mut sig = Some(m.signal);
                    ui.push_id(("modsig", m.id), |ui| modulation_editor_signal(ui, &mut sig));
                    if let Some(s) = sig {
                        m.signal = s;
                    }
                    if m.signal.source == phosphor_engine::ModSource::Steps && crate::ui::small_command(ui, label(icon::SQUARES_FOUR, "More steps…")).on_hover_text("Turn it into a step sequencer track (4 to 64 steps on the beat grid, gates)").clicked() {
                        let s = &m.signal;
                        let per_beat = if s.sync { 8.0 / s.rate.max(0.25) } else { 2.0 };
                        m.sequence = Some(phosphor_engine::StepTrack { steps: s.steps.to_vec(), per_beat, ..Default::default() });
                    }
                }
                // Chain: take from an earlier modulator.
                ui.horizontal(|ui| {
                    let mut on = m.chain.is_some();
                    if crate::ui::toggle(ui, &mut on, "Chain:").on_hover_text("Combine with an earlier modulator").changed() {
                        m.chain = on.then(|| ModChain { from: earlier.get(i.saturating_sub(1)).map_or(0, |e| e.0), ..Default::default() });
                    }
                    if let Some(c) = &mut m.chain {
                        choice_combo(ui, ("chain_mode", m.id), &mut c.mode, 120.0, true);
                        let current = earlier[..i].iter().find(|(id, _)| *id == c.from).map_or("pick one".to_string(), |(_, n)| n.clone());
                        crate::ui::dropdown(ui, ("chain_from", m.id), 100.0).selected_text(current).show_ui(ui, |ui| {
                            for (id, name) in &earlier[..i] {
                                crate::ui::pick_value(ui, &mut c.from, *id, name);
                            }
                        });
                        ui.horizontal(|ui| {
                            Level::percent(&mut c.amount, "Amount").default(1.0).show(ui);
                        });
                        if i == 0 || c.mode == ChainMode::Multiply && c.from == 0 {
                            hint(ui, "chains take from modulators above this one");
                        }
                    }
                });
            });
        }
    });
    if let Some((a, b)) = moves {
        st.comp.modulators.swap(a, b);
    }
    if let Some(i) = remove {
        let id = st.comp.modulators.remove(i).id;
        for m in &mut st.comp.modulators {
            if m.chain.is_some_and(|c| c.from == id) {
                m.chain = None;
            }
        }
    }
    if st.comp.modulators.is_empty() {
        help(ui, "No modulators yet. Add one, set its signal (an LFO on the beat, the bass, a step sequence…), then pick it from any slider's Modulate menu.");
    }
}

/// A Hand modulator's data looper: loop length, record (from the next loop start), play / stop, clear, and the take.
fn looper_row(ui: &mut egui::Ui, l: &mut phosphor_engine::Looper, beats: f32) {
    ui.horizontal_wrapped(|ui| {
        crate::ui::heading(ui, format!("{} Loop", icon::REPEAT));
        let mut len = l.len();
        let lengths: Vec<(f32, String)> = [1.0f32, 2.0, 4.0, 8.0, 16.0].into_iter().map(|n| (n, format!("{n:.0}"))).collect();
        if segmented(ui, &mut len, &lengths, |_, r, n| {
            r.clone().on_hover_text(format!("{n:.0} beats"));
        }) && l.recording.is_none()
        {
            l.beats = len;
        }
        match l.recording {
            Some(start) if beats < start => {
                ui.label(RichText::new(format!("{} waiting for the beat…", icon::RECORD)).color(crate::theme::WARN));
            }
            Some(_) => {
                ui.label(RichText::new(format!("{} recording", icon::RECORD)).color(crate::theme::WARN));
            }
            None => {
                if action_button(ui, Act::Record, format!("{} Rec", icon::RECORD)).on_hover_text("Record what this knob does for the loop's length, from the next loop start; then it plays back by itself").clicked() {
                    l.arm(beats);
                }
                if !l.samples.is_empty() {
                    let (stop, start) = (label(icon::STOP, "Stop"), label(icon::PLAY, "Play"));
                    let play = if l.playing { stop.clone() } else { start.clone() };
                    if steady_action_button(ui, Act::Go, play, &[&stop, &start]).on_hover_text("Play the take back, looped with the beat (stopped: the knob drives it again)").clicked() {
                        l.playing = !l.playing;
                    }
                    if crate::ui::small_command(ui, icon::TRASH).on_hover_text("Forget the take").clicked() {
                        l.samples.clear();
                        l.playing = false;
                    }
                }
            }
        }
    });
    if !l.samples.is_empty() {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(260.0), 28.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);
        let n = l.samples.len().max(2);
        let pts: Vec<egui::Pos2> = l.samples.iter().enumerate().map(|(i, v)| egui::pos2(rect.left() + rect.width() * i as f32 / (n - 1) as f32, rect.bottom() - v * rect.height())).collect();
        p.add(egui::Shape::line(pts, Stroke::new(1.5, accent())));
        if l.playing {
            let x = rect.left() + rect.width() * (beats.rem_euclid(l.len()) / l.len());
            p.vline(x, rect.y_range(), Stroke::new(1.0, Color32::WHITE));
        }
    }
}
