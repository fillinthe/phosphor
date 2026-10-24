//! Devices: everything connected, with disconnect/block/disable.

use super::*;

/// What macOS lets Phosphor use, each with a button to its System Settings page. Read every 2 s (it can change
/// while Phosphor runs).
#[cfg(target_os = "macos")]
fn permissions(ui: &mut egui::Ui) {
    let id = egui::Id::new("permissions");
    let now = ui.ctx().input(|i| i.time);
    let cached = ui.ctx().data(|d| d.get_temp::<(f64, std::sync::Arc<Vec<(String, String, crate::permissions::Status, String)>>)>(id));
    let list = match cached {
        Some((t, list)) if now - t < 2.0 => list,
        _ => {
            let list = std::sync::Arc::new(crate::permissions::all().into_iter().map(|p| (p.name.to_string(), p.why.to_string(), p.status, p.settings.to_string())).collect::<Vec<_>>());
            ui.ctx().data_mut(|d| d.insert_temp(id, (now, list.clone())));
            list
        }
    };
    ui.label(RichText::new(format!("{} Permissions", icon::SHIELD_CHECK)).strong());
    egui::Grid::new("permissions").num_columns(3).spacing([10.0, 4.0]).show(ui, |ui| {
        use crate::permissions::Status;
        for (name, why, status, settings) in list.iter() {
            ui.label(name).on_hover_text(why.as_str());
            let (glyph, color) = match status {
                Status::Allowed => (icon::CHECK_CIRCLE, accent()),
                Status::NotAsked => (icon::QUESTION, Color32::GRAY),
                Status::Denied | Status::Restricted => (icon::WARNING, Color32::from_rgb(230, 160, 60)),
            };
            ui.label(RichText::new(format!("{glyph} {}", status.label())).color(color));
            if *status != Status::Allowed && ui.small_button("System Settings…").on_hover_text("Open the page where you can allow it").clicked() {
                crate::os::open(settings.as_str());
            }
            ui.end_row();
        }
    });
    ui.separator();
}

pub(crate) fn devices(ui: &mut egui::Ui, st: &mut State) {
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        #[cfg(target_os = "macos")]
        permissions(ui);
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
                        crate::net::PeerKind::Follower => (icon::MONITOR_PLAY, "following this show"),
                    };
                    ui.label(format!("{glyph} {}", p.device));
                    ui.monospace(&p.ip);
                    if p.kind == crate::net::PeerKind::Audio {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(what).small());
                            ui.add(crate::ui::meter(p.level).desired_width(60.0).desired_height(8.0)).on_hover_text("Level of the audio this phone is sending");
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
            if !st.web_log.is_empty() {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Changes from the web").small().strong());
                    if ui.small_button("Clear").clicked() {
                        st.web_log.clear();
                    }
                });
                egui::ScrollArea::vertical().id_salt("web_log").max_height(160.0).stick_to_bottom(true).show(ui, |ui| {
                    egui::Grid::new("web_log_grid").num_columns(3).spacing([10.0, 2.0]).show(ui, |ui| {
                        for l in &st.web_log {
                            ui.monospace(RichText::new(&l.wall).small());
                            ui.label(RichText::new(&l.who).small().strong());
                            ui.label(RichText::new(&l.what).small());
                            ui.end_row();
                        }
                    });
                });
            }
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
                // (The light shows on / off: one word keeps it one width.)
                if toggle(ui, &mut on, "On").changed() {
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
                if steady_small_button(ui, text, &["Unblock", "Block"]).clicked() {
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

        // Other Phosphors on the network (Bonjour), e.g. the computers of a multi-room show.
        #[cfg(not(target_arch = "wasm32"))]
        {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{} Other Phosphors", icon::DESKTOP_TOWER)).strong());
                hint(ui, format!("this one is \"{}\"", crate::control::discovery::identity().0));
            });
            let players = crate::control::discovery::players();
            if players.is_empty() {
                hint(ui, "None heard on this network.");
            }
            for p in &players {
                ui.horizontal(|ui| {
                    ui.label(&p.name);
                    hint(ui, format!("OSC {}:{}{}", p.address, p.port, if p.version.is_empty() { String::new() } else { format!(" · {}", p.version) }));
                    if ui.small_button(icon::COPY).on_hover_text("Copy its OSC address (to send it cues or values)").clicked() {
                        ui.ctx().copy_text(format!("{}:{}", p.address, p.port));
                    }
                });
            }
            ui.separator();
        }

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
        #[cfg(windows)]
        spout_row(ui, st);
        #[cfg(any(target_os = "macos", windows))]
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
