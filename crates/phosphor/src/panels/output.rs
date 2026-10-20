//! Outputs: sizes, windows, screens, Syphon and NDI.

use super::*;

pub(crate) const RESOLUTIONS: &[(u32, u32, &str)] = &[
    (1280, 720, "1280×720 (720p)"),
    (1920, 1080, "1920×1080 (1080p)"),
    (2560, 1440, "2560×1440"),
    (3840, 2160, "3840×2160 (4K)"),
];

pub(crate) fn output(ui: &mut egui::Ui, st: &mut State) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Screens").strong());
        if ui.small_button(label(icon::ARROWS_CLOCKWISE, "Rescan")).clicked() {
            st.displays = crate::screens::displays();
        }
    });
    if st.displays.is_empty() {
        hint(ui, "No displays found.");
    }
    for d in st.displays.clone() {
        ui.horizontal(|ui| {
            let icon_name = if d.main { icon::LAPTOP } else { icon::PROJECTOR_SCREEN };
            ui.label(format!("{icon_name} {}", d.label()));
            if d.main {
                hint(ui, "(this screen)");
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Send Output 1 here").on_hover_text("Opens the first output fullscreen on this display, no menu bar").clicked() {
                    st.send_output_to(d.clone());
                }
            });
        });
    }
    ui.separator();
    outputs_list(ui, st);
    ui.separator();
    egui::Grid::new("output_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        ui.label("Canvas");
        let current = format!("{}×{}", st.comp.width, st.comp.height);
        ComboBox::from_id_salt("resolution").selected_text(current).show_ui(ui, |ui| {
            for (w, h, label) in RESOLUTIONS {
                if ui.selectable_label(st.comp.width == *w && st.comp.height == *h, *label).clicked() {
                    st.comp.width = *w;
                    st.comp.height = *h;
                }
            }
        });
        ui.end_row();

        ui.label("Master");
        ui.horizontal(|ui| {
            Level::percent(&mut st.comp.master_fade, "Fade").default(1.0).show(ui);
        });
        ui.end_row();

        ui.label("Render at");
        ui.horizontal(|ui| {
            crate::ui::choice_combo(ui, "render_rate", &mut st.render_rate, 110.0, false);
            crate::ui::hint(ui, "Projectors are 60 Hz; on a 120 Hz Mac display, 60 fps halves the work and heat.");
        });
        ui.end_row();

        ui.label("All output windows");
        let text = if st.output_open() { "Close" } else { "Open" };
        if ui.button(text).clicked() {
            st.toggle_output();
        }
        ui.end_row();
    });
    hint(ui, "In the output window: F = fullscreen, E = mapping handles, Esc = exit. Drag it to a projector first.");
    ui.separator();

    let s = st.stats.latest;
    egui::Grid::new("stats_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
        let row = |ui: &mut egui::Ui, k: &str, v: String| {
            ui.label(k);
            ui.monospace(v);
            ui.end_row();
        };
        row(ui, "Frame rate", format!("{:.1} fps ({:.2} ms)", s.fps, s.frame_ms));
        row(ui, "Engine CPU / frame", format!("{:.3} ms", s.render_ms));
        if s.gpu_ms > 0.0 {
            row(ui, "Engine GPU / frame", format!("{:.2} ms", s.gpu_ms));
        }
        row(ui, "Process CPU", format!("{:.1}% of one core", s.cpu_percent));
        row(ui, "Memory", format!("{:.0} MB", s.memory_mb));
        row(ui, "GPU passes / frame", s.passes.to_string());
    });
}

// Monitors: extra preview panels showing the output, the mix, or one layer.

pub(crate) const OUTPUT_SIZES: &[(u32, u32, &str)] = &[
    (0, 0, "Same as canvas"),
    (1280, 720, "1280×720"),
    (1920, 1080, "1920×1080"),
    (1920, 1200, "1920×1200"),
    (2560, 1440, "2560×1440"),
    (3840, 2160, "3840×2160"),
    (1080, 1920, "1080×1920 (portrait)"),
];

/// Every output with its size, screen, window and Identify controls.
pub(crate) fn outputs_list(ui: &mut egui::Ui, st: &mut State) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Outputs").strong());
        if ui.small_button(label(icon::PLUS, "Add output")).clicked() {
            st.mapping_output = st.comp.add_output();
        }
        super::mapping::outputs_actions(ui, st);
    });
    if st.comp.outputs.iter().any(|o| !o.screen.is_empty()) {
        #[cfg(any(target_os = "macos", windows))]
        {
            let mut at_login = crate::login_item::is_enabled();
            if ui.checkbox(&mut at_login, "Start Phosphor when this Mac logs in (installations)").on_hover_text("A LaunchAgent opens Phosphor at login; with the option below, the show comes up on its screens by itself").changed() {
                st.notice = Some(crate::login_item::set(at_login).unwrap_or_else(|e| format!("Couldn't change the login item: {e}")));
            }
        }
        ui.checkbox(&mut st.comp.outputs_on_open, "Open outputs on their screens when this project opens")
            .on_hover_text("For a venue: outputs go fullscreen on the screens they were last sent to, at launch and when you open this project");
    }
    #[cfg(target_os = "macos")]
    syphon_row(ui, st);
    #[cfg(windows)]
    spout_row(ui, st);
    #[cfg(any(target_os = "macos", windows))]
    ndi_row(ui, st);
    let canvas = (st.comp.width, st.comp.height);
    let mut remove = None;
    let count = st.comp.outputs.len();
    for i in 0..count {
        let id = st.comp.outputs[i].id;
        let open = st.output_window_open(id);
        let tex = st.output_tex(id);
        let mut toggle = false;
        let mut send: Option<crate::screens::Display> = None;
        let displays = st.displays.clone();
        let others: Vec<(u64, String)> = st.comp.outputs.iter().filter(|o| o.id != id).map(|o| (o.id, o.name.clone())).collect();
        let (layers, groups) = (&mut st.comp.layers, &mut st.comp.groups);
        let o = &mut st.comp.outputs[i];
        card_frame(ui, false).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(64.0, 36.0), Sense::hover());
                ui.painter().rect_filled(r, 3.0, Color32::BLACK);
                if let Some(t) = tex {
                    let (ow, oh) = o.size(canvas);
                    paint_texture(ui.painter(), t, fit_rect(r, ow as f32 / oh.max(1) as f32));
                }
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{}", i + 1)).strong().color(accent()));
                        ui.add(egui::TextEdit::singleline(&mut o.name).desired_width(110.0));
                        crate::ui::toggle(ui, &mut o.enabled, "ON").on_hover_text("Render this output");
                        grid_toggle(ui, &mut o.grid);
                        #[cfg(target_os = "macos")]
                        crate::ui::toggle(ui, &mut o.syphon, "SYPHON")
                            .on_hover_text("Also publish this output (after mapping) as a Syphon source for MadMapper, Resolume, OBS…");
                        if o.mapping.active() {
                            ui.label(RichText::new(format!("{} mapped", icon::PROJECTOR_SCREEN)).small().color(accent()));
                        }
                    });
                    ui.horizontal(|ui| {
                        let current = OUTPUT_SIZES.iter().find(|(w, h, _)| *w == o.width && *h == o.height).map_or(format!("{}×{}", o.width, o.height), |(_, _, n)| n.to_string());
                        ComboBox::from_id_salt(("out_size", id)).width(130.0).selected_text(current).show_ui(ui, |ui| {
                            for (w, h, name) in OUTPUT_SIZES {
                                if ui.selectable_label(o.width == *w && o.height == *h, *name).clicked() {
                                    o.width = *w;
                                    o.height = *h;
                                }
                            }
                        });
                        let text = if open { label(icon::STOP, "Window") } else { label(icon::PLAY, "Window") };
                        if ui.button(text).clicked() {
                            toggle = true;
                        }
                        ui.menu_button(label(icon::PROJECTOR_SCREEN, "Send to"), |ui| {
                            for d in &displays {
                                if ui.button(d.label()).clicked() {
                                    send = Some(d.clone());
                                    ui.close();
                                }
                            }
                            if displays.is_empty() {
                                hint(ui, "No displays found");
                            }
                        });
                        if count > 1 && ui.small_button(icon::TRASH).on_hover_text("Remove output").clicked() {
                            remove = Some(i);
                        }
                    });
                });
            });
            if !o.screen.is_empty() {
                ui.horizontal(|ui| {
                    let connected = crate::screens::find_screen(&displays, &o.screen).is_some();
                    let (glyph, color) = if connected { (icon::CHECK_CIRCLE, accent()) } else { (icon::WARNING, Color32::from_rgb(230, 160, 60)) };
                    ui.label(RichText::new(format!("{glyph} Screen: {}", o.screen)).small().color(color))
                        .on_hover_text(if connected { "Saved with the project; \"All to their screens\" sends it here" } else { "That screen isn't connected right now" });
                    if ui.small_button(icon::X).on_hover_text("Forget this screen").clicked() {
                        o.screen.clear();
                    }
                });
            }
            if !o.mapping.surfaces.is_empty() {
                let title = format!("{} surfaces{}", o.mapping.surfaces.len(), if o.mapping.enabled { "" } else { " (mapping off)" });
                egui::CollapsingHeader::new(title).id_salt(("out_surfaces", id)).show(ui, |ui| {
                    for s in o.mapping.surfaces.iter_mut() {
                        ui.horizontal(|ui| {
                            crate::ui::toggle(ui, &mut s.enabled, "ON");
                            ui.label(&s.name);
                        });
                        super::mapping::surface_source(ui, s, layers, groups, &others);
                    }
                });
            }
            color_grade_editor(ui, id, &mut o.color);
        });
        if toggle {
            st.toggle_output_window(id);
        }
        if let Some(d) = send {
            st.send_output_id_to(id, d);
        }
    }
    if let Some(i) = remove {
        st.comp.outputs.remove(i);
        st.close_stale_outputs();
    }
}

/// Composition → Syphon toggle with status.
#[cfg(target_os = "macos")]
pub fn syphon_row(ui: &mut egui::Ui, st: &mut State) {
    ui.horizontal(|ui| {
        if !st.syphon.available {
            hint(ui, "Syphon unavailable (framework missing)");
            return;
        }
        let hover = "Publish the composition as \"Phosphor\" — pick it in MadMapper, Resolume, OBS (Syphon Client), VDMX…";
        output_toggle(ui, &mut st.comp.syphon, icon::SHARE_NETWORK, "Syphon", hover, phosphor_engine::AppControl::Syphon);
        let (servers, clients) = (st.syphon.server_count(), st.syphon.clients());
        let waiting = format!("{servers} source(s) published, no app receiving yet");
        output_status(ui, match (servers, clients) {
            (0, _) => OutputStatus::Off,
            (_, 0) => OutputStatus::Waiting(&waiting),
            (_, n) => OutputStatus::Receiving(n),
        });
    });
}

/// Composition → Spout toggle with status (Windows; the same setting as Syphon on the Mac).
#[cfg(windows)]
pub fn spout_row(ui: &mut egui::Ui, st: &mut State) {
    ui.horizontal(|ui| {
        let hover = "Publish the composition as the Spout sender \"Phosphor\": pick it in Resolume, TouchDesigner, OBS (Spout2 plugin), MadMapper…";
        if output_toggle(ui, &mut st.comp.spout, icon::SHARE_NETWORK, "Spout", hover, phosphor_engine::AppControl::Syphon).changed() {
            st.spout.clear_error();
        }
        match (&st.spout.error, st.spout.name()) {
            (Some(e), _) if st.comp.spout => {
                ui.colored_label(WARN, e.as_str());
            }
            (_, Some(name)) => {
                hint(ui, format!("sending as \"{name}\" (Spout can't tell how many apps receive)"));
            }
            _ => output_status(ui, OutputStatus::Off),
        };
    });
}

/// Composition → NDI toggle, name, frame rate and status.
#[cfg(any(target_os = "macos", windows))]
pub fn ndi_row(ui: &mut egui::Ui, st: &mut State) {
    ui.horizontal(|ui| {
        let Some(lib) = crate::ndi::Ndi::library() else {
            ui.label(RichText::new(format!("{} NDI", icon::BROADCAST)).color(Color32::GRAY));
            hint(ui, if cfg!(windows) { "needs NDI Tools or the NDI Runtime (then restart Phosphor)" } else { "needs NDI Tools for Mac (then restart Phosphor, not the Mac)" });
            if ui.small_button("Get NDI Tools…").on_hover_text("ndi.video/tools — installs the NDI runtime; then restart Phosphor").clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab("https://ndi.video/tools/"));
            }
            return;
        };
        let hover = format!("Send the composition over the network as NDI (MadMapper, Resolume, OBS, vMix, other computers)\nNDI library: {}", lib.display());
        output_toggle(ui, &mut st.comp.ndi.enabled, icon::BROADCAST, "NDI", &hover, phosphor_engine::AppControl::Ndi);
        ui.add(egui::TextEdit::singleline(&mut st.comp.ndi.name).desired_width(90.0)).on_hover_text("Name receivers see");
        ComboBox::from_id_salt("ndi_fps").width(60.0).selected_text(format!("{} fps", st.comp.ndi.fps)).show_ui(ui, |ui| {
            for f in [25, 30, 50, 60] {
                ui.selectable_value(&mut st.comp.ndi.fps, f, format!("{f} fps"));
            }
        });
        let status = match (&st.ndi.error, st.ndi.running(), st.ndi.connections()) {
            (Some(e), false, _) if st.comp.ndi.enabled => OutputStatus::Error(e),
            (_, true, 0) => OutputStatus::Waiting("on the network, nobody receiving yet"),
            (_, true, n) => OutputStatus::Receiving(n as usize),
            _ => OutputStatus::Off,
        };
        output_status(ui, status);
    });
}

/// Colour correction for an output or a mapping surface: match projectors, screens and materials.
pub(crate) fn color_grade_editor(ui: &mut egui::Ui, id: u64, g: &mut phosphor_engine::ColorGrade) {
    let title = if g.is_neutral() { label(icon::PALETTE, "Colour") } else { label(icon::PALETTE, "Colour (adjusted)") };
    egui::CollapsingHeader::new(title).id_salt(("color_grade", id)).show(ui, |ui| {
        ui.spacing_mut().slider_width = (ui.available_width() - 150.0).max(80.0);
        ui.horizontal(|ui| {
            Level::range(&mut g.brightness, -1.0, 1.0, "Brightness").default(0.0).show(ui);
        });
        ui.horizontal(|ui| {
            Level::range(&mut g.contrast, 0.0, 3.0, "Contrast").default(1.0).show(ui);
        });
        ui.horizontal(|ui| {
            Level::range(&mut g.gamma, 0.2, 3.0, "Gamma").default(1.0).log().show(ui);
        });
        ui.horizontal(|ui| {
            Level::range(&mut g.saturation, 0.0, 2.0, "Saturation").default(1.0).show(ui);
        });
        for (k, name) in ["Red", "Green", "Blue"].iter().enumerate() {
            ui.horizontal(|ui| {
                Level::range(&mut g.gains[k], 0.0, 2.0, *name).default(1.0).show(ui);
            });
        }
        if ui.add_enabled(!g.is_neutral(), egui::Button::new(label(icon::ARROW_COUNTER_CLOCKWISE, "Reset colour"))).clicked() {
            *g = Default::default();
        }
    });
}

/// The alignment grid toggle for an output (GRID), with its settings on right-click: cells across, crosshair,
/// line width, colour. Used in the Outputs panel and the Mapping Studio.
pub(crate) fn grid_toggle(ui: &mut egui::Ui, g: &mut phosphor_engine::GridOverlay) {
    let r = toggle(ui, &mut g.on, label(icon::GRID_FOUR, "GRID")).on_hover_text("Alignment grid over this output (cells, crosshair, circle, border) to line up the projector · right-click for its settings · off for the show");
    r.right_menu(|ui| {
        ui.label(RichText::new("Alignment grid").strong());
        ui.horizontal(|ui| {
            ui.label("Cells across");
            guarded!(ui, g.cells, egui::DragValue::new(&mut g.cells).range(2..=64));
        });
        ui.checkbox(&mut g.crosshair, "Crosshair, circle and diagonals");
        ui.horizontal(|ui| {
            ui.label("Line");
            guarded!(ui, g.width, egui::DragValue::new(&mut g.width).range(0.5..=12.0).speed(0.1).suffix(" px"));
        });
        ui.horizontal(|ui| {
            ui.label("Colour");
            for c in [[255u8, 255, 255], [0, 255, 120], [255, 60, 60], [80, 160, 255], [255, 220, 0]] {
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), Sense::click());
                ui.painter().circle_filled(rect.center(), 7.0, Color32::from_rgb(c[0], c[1], c[2]));
                if g.color == c {
                    ui.painter().circle_stroke(rect.center(), 8.5, Stroke::new(1.5, Color32::WHITE));
                }
                if resp.clicked() {
                    g.color = c;
                }
            }
        });
    });
}
