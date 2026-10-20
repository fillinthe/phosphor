//! Modulators panel: shared, named signal sources any control can follow (Modulate menu → Shared modulators),
//! chained into each other. VDMX-style data sources, the Phosphor way.

use phosphor_engine::{ChainMode, ModChain, Modulator};

use super::*;

pub(crate) fn modulators(ui: &mut egui::Ui, st: &mut State) {
    let inputs = st.mod_inputs;
    ui.horizontal(|ui| {
        if ui.button(label(icon::PLUS, "Modulator")).on_hover_text("A shared signal: pick it in any control's Modulate menu").clicked() && st.comp.modulators.len() < phosphor_engine::control::MAX_SHARED {
            let id = st.comp.new_id();
            let n = st.comp.modulators.len() + 1;
            st.comp.modulators.push(Modulator { id, name: format!("Mod {n}"), ..Default::default() });
        }
        hint(ui, "One source drives many controls: right-click any slider, Modulate, then pick it. Chain one into the next.");
    });
    let mut remove = None;
    let mut moves: Option<(usize, usize)> = None;
    let earlier: Vec<(u64, String)> = st.comp.modulators.iter().map(|m| (m.id, m.name.clone())).collect();
    let n = st.comp.modulators.len();
    egui::ScrollArea::vertical().show(ui, |ui| {
        for (i, m) in st.comp.modulators.iter_mut().enumerate() {
            let value = inputs.shared.iter().find(|(id, _)| *id == m.id).map_or(0.0, |x| x.1);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut m.name).desired_width(120.0));
                    ui.add(egui::ProgressBar::new(value).desired_width(110.0).fill(accent().gamma_multiply(0.7)));
                    if i > 0 && ui.small_button(icon::ARROW_UP).on_hover_text("Earlier in the chain").clicked() {
                        moves = Some((i, i - 1));
                    }
                    if i + 1 < n && ui.small_button(icon::ARROW_DOWN).on_hover_text("Later in the chain").clicked() {
                        moves = Some((i, i + 1));
                    }
                    if ui.small_button(icon::TRASH).on_hover_text("Delete (controls following it stop moving)").clicked() {
                        remove = Some(i);
                    }
                });
                // Hand / MIDI knob: a fader that any knob, OSC message or the remote can move (right-click it: Learn).
                if m.signal.source == phosphor_engine::ModSource::Hand {
                    ui.horizontal(|ui| {
                        Level::percent(&mut m.value, "Value").default(0.0).target(phosphor_engine::ControlTarget::Modulator(m.id)).show(ui);
                        hint(ui, "right-click: Learn a MIDI knob");
                    });
                }
                // Its signal: the same editor as any control's Modulate menu.
                let mut sig = Some(m.signal);
                ui.push_id(("modsig", m.id), |ui| modulation_editor_signal(ui, &mut sig));
                if let Some(s) = sig {
                    m.signal = s;
                }
                // Chain: take from an earlier modulator.
                ui.horizontal(|ui| {
                    let mut on = m.chain.is_some();
                    if ui.checkbox(&mut on, "Chain:").on_hover_text("Combine with an earlier modulator").changed() {
                        m.chain = on.then(|| ModChain { from: earlier.get(i.saturating_sub(1)).map_or(0, |e| e.0), ..Default::default() });
                    }
                    if let Some(c) = &mut m.chain {
                        choice_combo(ui, ("chain_mode", m.id), &mut c.mode, 120.0, true);
                        let current = earlier[..i].iter().find(|(id, _)| *id == c.from).map_or("pick one".to_string(), |(_, n)| n.clone());
                        egui::ComboBox::from_id_salt(("chain_from", m.id)).width(100.0).selected_text(current).show_ui(ui, |ui| {
                            for (id, name) in &earlier[..i] {
                                ui.selectable_value(&mut c.from, *id, name);
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
        hint(ui, "No modulators yet. Add one, set its signal (an LFO on the beat, the bass, a step sequence…), then pick it from any slider's Modulate menu.");
    }
}
