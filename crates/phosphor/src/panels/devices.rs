//! Devices: everything connected, with disconnect/block/disable.

use super::*;

pub(crate) fn devices(ui: &mut egui::Ui, st: &mut State) {
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        // Phones, tablets and browsers on the session.
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{} Phones & browsers", icon::DEVICE_MOBILE)).strong());
            if st.net.running() {
                hint(ui, st.net.url());
            } else {
                hint(ui, "session is off (Session panel)");
            }
        });
        #[cfg(not(target_arch = "wasm32"))]
        {
            let peers = st.net.peers();
            if peers.is_empty() {
                hint(ui, "Nobody connected.");
            }
            egui::Grid::new("peers").num_columns(5).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
                for p in &peers {
                    let (glyph, what) = match p.kind {
                        crate::net::PeerKind::Remote if p.controlling => (icon::HAND_POINTING, "controlling"),
                        crate::net::PeerKind::Remote => (icon::LOCK, "remote, locked (watching)"),
                        crate::net::PeerKind::Viewer => (icon::EYE, "watching"),
                        crate::net::PeerKind::Audio => (icon::MICROPHONE, "sending audio"),
                        crate::net::PeerKind::Guest => (icon::GAME_CONTROLLER, "audience (play page)"),
                    };
                    ui.label(format!("{glyph} {}", p.device));
                    ui.monospace(&p.ip);
                    if p.kind == crate::net::PeerKind::Audio {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(what).small());
                            ui.add(egui::ProgressBar::new(p.level).desired_width(60.0).desired_height(8.0)).on_hover_text("Level of the audio this phone is sending");
                        });
                    } else {
                        ui.label(RichText::new(what).small());
                    }
                    hint(ui, ago(p.since));
                    ui.horizontal(|ui| {
                        if ui.small_button("Disconnect").on_hover_text("Drops it now; it can reconnect").clicked() {
                            st.net.disconnect(p.id);
                        }
                        if ui.small_button(label(icon::PROHIBIT, "Block")).on_hover_text("Drops everything from this address and refuses it until unblocked").clicked() {
                            st.net.block(&p.ip);
                        }
                    });
                    ui.end_row();
                }
            });
            let blocked = st.net.blocked();
            if !blocked.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new("Blocked addresses").small().color(Color32::GRAY));
                for ip in blocked {
                    ui.horizontal(|ui| {
                        ui.monospace(&ip);
                        if ui.small_button("Unblock").clicked() {
                            st.net.unblock(&ip);
                        }
                    });
                }
            }
        }
        ui.separator();

        // MIDI inputs.
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{} MIDI inputs", icon::PIANO_KEYS)).strong());
            if ui.small_button(label(icon::ARROWS_CLOCKWISE, "Rescan")).clicked() {
                st.control.reconnect();
            }
        });
        let all = st.control.midi_all.clone();
        if all.is_empty() {
            hint(ui, "No MIDI devices found.");
        }
        let seen = st.control.activity.lock().unwrap().midi_seen.clone();
        let mut changed = false;
        for name in all {
            ui.horizontal(|ui| {
                activity_dot(ui, seen.get(&name).copied());
                let mut on = !st.control.midi_disabled.contains(&name);
                let text = RichText::new(if on { "ON" } else { "OFF" }).small();
                if ui.toggle_value(&mut on, text).changed() {
                    if on {
                        st.control.midi_disabled.remove(&name);
                    } else {
                        st.control.midi_disabled.insert(name.clone());
                    }
                    changed = true;
                }
                ui.label(&name);
            });
        }
        if changed {
            st.control.reconnect();
        }
        ui.separator();

        // OSC senders.
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{} OSC senders", icon::BROADCAST)).strong());
            hint(ui, &st.control.osc_status);
        });
        let (senders, blocked) = {
            let a = st.control.activity.lock().unwrap();
            (a.osc.values().cloned().collect::<Vec<_>>(), a.osc_blocked.clone())
        };
        if senders.is_empty() {
            hint(ui, "No OSC received yet.");
        }
        for s in senders {
            ui.horizontal(|ui| {
                activity_dot(ui, Some(s.last_seen));
                ui.monospace(&s.ip);
                hint(ui, format!("{} msgs · {}", s.count, s.last_address));
                let is_blocked = blocked.contains(&s.ip);
                let text = if is_blocked { "Unblock" } else { "Block" };
                if ui.small_button(text).clicked() {
                    let mut a = st.control.activity.lock().unwrap();
                    if is_blocked {
                        a.osc_blocked.remove(&s.ip);
                    } else {
                        a.osc_blocked.insert(s.ip.clone());
                    }
                }
            });
        }
        ui.separator();

        // Audio input and outputs, for the full picture.
        ui.label(RichText::new(format!("{} Audio in", icon::MICROPHONE)).strong());
        if st.audio.is_open() {
            ui.label(st.audio.current().unwrap_or("Default input"));
        } else {
            hint(ui, "off (Audio panel)");
        }
        ui.separator();
        ui.label(RichText::new(format!("{} Outputs", icon::PROJECTOR_SCREEN)).strong());
        #[cfg(target_os = "macos")]
        syphon_row(ui, st);
        #[cfg(target_os = "macos")]
        ndi_row(ui, st);
        let windows: Vec<(u64, bool)> = st.output.lock().unwrap().iter().map(|(id, o)| (*id, o.open)).collect();
        for o in &st.comp.outputs {
            let open = windows.iter().any(|(id, on)| *id == o.id && *on);
            ui.horizontal(|ui| {
                ui.label(&o.name);
                hint(ui, if open { "window open" } else { "closed" });
            });
        }
    });
}
