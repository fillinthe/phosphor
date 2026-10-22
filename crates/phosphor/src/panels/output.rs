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
    #[cfg(not(target_arch = "wasm32"))]
    gpu_row(ui, st);
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

/// Which graphics card renders, on machines with more than one (applies at the next start).
#[cfg(not(target_arch = "wasm32"))]
fn gpu_row(ui: &mut egui::Ui, st: &mut State) {
    let Some(rs) = &st.render_state else { return };
    let current = rs.adapter.get_info().name;
    let mut names: Vec<String> = rs.available_adapters.iter().map(|a| a.get_info().name).collect();
    names.dedup();
    let wanted = crate::gpu_pick::wanted();
    if names.len() < 2 && wanted.is_none() {
        return;
    }
    ui.horizontal(|ui| {
        ui.label("Graphics card");
        let shown = wanted.clone().unwrap_or_else(|| "Automatic".into());
        let mut pick: Option<Option<String>> = None;
        ComboBox::from_id_salt("gpu_pick").width(220.0).selected_text(&shown).show_ui(ui, |ui| {
            if ui.selectable_label(wanted.is_none(), "Automatic").clicked() {
                pick = Some(None);
            }
            for n in &names {
                if ui.selectable_label(wanted.as_deref() == Some(n.as_str()), n).clicked() {
                    pick = Some(Some(n.clone()));
                }
            }
        });
        if let Some(p) = pick {
            st.notice = Some(match crate::gpu_pick::set(p.as_deref()) {
                Ok(()) => "Graphics card saved: it's used from the next time Phosphor starts".into(),
                Err(e) => format!("Couldn't save the graphics card: {e}"),
            });
        }
        hint(ui, format!("using {current}"));
    });
}

/// Every output with its size, screen, window and Identify controls.
pub(crate) fn outputs_list(ui: &mut egui::Ui, st: &mut State) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Outputs").strong());
        if ui.small_button(label(icon::PLUS, "Add output")).clicked() {
            st.mapping_output = st.comp.add_output();
        }
        super::mapping::outputs_actions(ui, st);
        ui.menu_button(label(icon::GRID_FOUR, "Split across"), |ui| {
            hint(ui, "New outputs that each show one part of the picture (a wall of screens or projectors side by side)");
            for (c, r) in [(2, 1), (3, 1), (4, 1), (2, 2), (3, 3)] {
                if ui.button(format!("{c} × {r}")).clicked() {
                    let n = st.comp.split_across(c, r).len();
                    st.notice = Some(format!("{n} new outputs, one tile each: send each to its screen"));
                    ui.close();
                }
            }
        });
        if ui.small_button(label(icon::GRADIENT, "Soft edges")).on_hover_text("Work out the soft-edge blends from where the slices' parts of the picture overlap (projectors side by side or stacked); replaces the edges set by hand").clicked() {
            let n = st.comp.auto_soft_edges();
            st.notice = Some(if n == 0 { "No overlaps found: give neighbouring slices overlapping parts of the picture (Input) first".into() } else { format!("Soft edges set on {n} slices from their overlaps") });
        }
    });
    #[cfg(not(target_arch = "wasm32"))]
    super::projector::all_projectors_row(ui, &st.comp.outputs, &mut st.projectors);
    if st.comp.outputs.iter().any(|o| !o.screen.is_empty()) {
        #[cfg(any(target_os = "macos", windows))]
        {
            let mut at_login = crate::login_item::is_enabled();
            if crate::ui::toggle(ui, &mut at_login, "Start Phosphor when this Mac logs in (installations)").on_hover_text("A LaunchAgent opens Phosphor at login; with the option below, the show comes up on its screens by itself").changed() {
                st.notice = Some(crate::login_item::set(at_login).unwrap_or_else(|e| format!("Couldn't change the login item: {e}")));
            }
        }
        crate::ui::toggle(ui, &mut st.comp.outputs_on_open, "Open outputs on their screens when this project opens")
            .on_hover_text("For a venue: outputs go fullscreen on the screens they were last sent to, at launch and when you open this project");
    }
    #[cfg(target_os = "macos")]
    syphon_row(ui, st);
    #[cfg(windows)]
    spout_row(ui, st);
    #[cfg(any(target_os = "macos", windows))]
    ndi_row(ui, st);
    // Only in a copy that carries the camera extension (until it's signed, release copies don't).
    #[cfg(target_os = "macos")]
    if crate::virtual_camera::bundled() {
        vcam_row(ui, st);
    }
    let canvas = (st.comp.width, st.comp.height);
    let mut remove = None;
    let mut snapshot = None;
    let mut background: Option<u64> = None;
    let mut mirror: Option<u64> = None;
    let count = st.comp.outputs.len();
    for i in 0..count {
        let id = st.comp.outputs[i].id;
        let open = st.output_window_open(id);
        let tex = st.output_tex(id);
        let mut toggle = false;
        let mut send: Option<crate::screens::Display> = None;
        let displays = st.displays.clone();
        let others = super::mapping::picture_sources(&st.comp, id);
        let comps: Vec<(u64, String, bool)> = st.comp.comps.iter().map(|c| (c.id, c.name.clone(), c.in_mix)).collect();
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
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!("{}", i + 1)).strong().color(accent()));
                        ui.add(egui::TextEdit::singleline(&mut o.name).desired_width(110.0));
                        crate::ui::toggle(ui, &mut o.enabled, "ON").on_hover_text("Render this output");
                        grid_toggle(ui, &mut o.grid);
                        program_picker(ui, o, &comps);
                        if ui.small_button(icon::MONITOR).on_hover_text("Preview this output (after mapping) in a window").clicked() {
                            request_preview(ui, MonitorSource::OneOutput(id));
                        }
                        if ui.small_button(icon::CAMERA).on_hover_text("Save a picture of this output as it is now (after mapping)").clicked() {
                            snapshot = Some(id);
                        }
                        if ui.small_button(icon::COPY).on_hover_text("Another output showing exactly this one (the same picture on one more screen)").clicked() {
                            mirror = Some(id);
                        }
                        crate::ui::toggle(ui, &mut o.frozen, "FREEZE").on_hover_text("Hold this output's picture while you change things behind it (not saved: a show opens live)");
                        #[cfg(target_os = "macos")]
                        crate::ui::toggle(ui, &mut o.syphon, "SYPHON")
                            .on_hover_text("Also publish this output (after mapping) as a Syphon source for MadMapper, Resolume, OBS…");
                        if o.mapping.active() {
                            ui.label(RichText::new(format!("{} mapped", icon::PROJECTOR_SCREEN)).small().color(accent()));
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
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
                    ui.horizontal_wrapped(|ui| {
                        crate::ui::toggle(ui, &mut o.flip_x, label(icon::ARROWS_LEFT_RIGHT, "Flip")).on_hover_text("Mirror left-right (rear projection)");
                        crate::ui::toggle(ui, &mut o.flip_y, label(icon::ARROWS_DOWN_UP, "Flip")).on_hover_text("Mirror top-bottom (projector hung upside down)");
                        crate::ui::toggle(ui, &mut o.smooth, "Smooth edges").on_hover_text("Anti-aliasing: slice edges without stair steps (draws this output at twice the size; turn off while lining up with the grid)");
                        guarded!(ui, o.delay_frames, egui::DragValue::new(&mut o.delay_frames).range(0..=phosphor_engine::MAX_OUTPUT_DELAY).prefix("delay ").suffix(" frames"))
                            .on_hover_text("Show this output a few frames late, to line it up with a slower projector or LED processor");
                        #[cfg(not(target_arch = "wasm32"))]
                        if o.background.is_empty() {
                            if ui.small_button(label(icon::IMAGE, "Background…")).on_hover_text("A picture behind this output's slices (a backdrop; only with mapping)").clicked() {
                                background = Some(id);
                            }
                        } else {
                            Level::percent(&mut o.background_opacity, "Background").default(1.0).width(110.0).show(ui);
                            if ui.small_button(icon::X).on_hover_text("No background picture").clicked() {
                                o.background.clear();
                            }
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
                            crate::ui::toggle_mapped(ui, &mut s.enabled, "ON", ControlTarget::SliceEnabled(s.id));
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
    if snapshot.is_some() {
        st.snapshot_output = snapshot;
    }
    if let Some(id) = mirror
        && st.comp.mirror_output(id).is_some()
    {
        st.notice = Some("New output showing the same picture: send it to its screen".into());
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(id) = background {
        st.dialogs.pick_file(crate::dialogs::Purpose::OutputBackground(id), Some(("Picture", &["png", "jpg", "jpeg", "webp", "tif", "tiff", "psd", "heic", "exr", "bmp"])));
    }
    #[cfg(target_arch = "wasm32")]
    let _ = background;
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

/// Phosphor as a camera ("Phosphor") in Zoom, OBS, Teams…: on/off and what macOS said about installing it.
#[cfg(target_os = "macos")]
pub fn vcam_row(ui: &mut egui::Ui, st: &mut State) {
    ui.horizontal(|ui| {
        let was = st.vcam.on;
        crate::ui::toggle(ui, &mut st.vcam.on, label(icon::WEBCAM, "Virtual camera"))
            .on_hover_text("The composition as a camera called \"Phosphor\" in video calls, OBS and other apps (1920×1080, 30 fps)");
        if st.vcam.on && !was {
            crate::virtual_camera::activate();
        }
        let said = crate::virtual_camera::status();
        let status = if st.vcam.connected() {
            OutputStatus::Receiving(1)
        } else if !st.vcam.on {
            OutputStatus::Off
        } else if said.starts_with("macOS didn't") || said.starts_with("This copy") {
            OutputStatus::Error(&said)
        } else {
            OutputStatus::Waiting("waiting for the camera")
        };
        output_status(ui, status);
    });
    let said = crate::virtual_camera::status();
    if st.vcam.on && !st.vcam.connected() && !said.is_empty() {
        hint(ui, said);
    }
}

/// Colour correction for an output or a mapping surface: match projectors, screens and materials.
/// What an output shows when it isn't mapped: the main mix, or one comp as its own program (masters). Only offered
/// when the project has comps (`comps` = id, name, in the main mix).
pub(crate) fn program_picker(ui: &mut egui::Ui, o: &mut phosphor_engine::OutputConfig, comps: &[(u64, String, bool)]) {
    if comps.len() < 2 {
        return;
    }
    ui.horizontal(|ui| {
        ui.label("Shows");
        let current = o.program.and_then(|id| comps.iter().find(|(c, _, _)| *c == id)).map_or("Main mix".to_string(), |(_, n, _)| format!("Comp: {n}"));
        ComboBox::from_id_salt(("output_program", o.id)).width(180.0).selected_text(current).show_ui(ui, |ui| {
            ui.selectable_value(&mut o.program, None, "Main mix");
            for (id, name, _) in comps {
                ui.selectable_value(&mut o.program, Some(*id), format!("Comp: {name}"));
            }
        })
        .response
        .on_hover_text("A comp as this output's whole picture: each output its own mix (masters). With mapping on, its slices decide instead");
    });
    if o.program.is_some_and(|id| comps.iter().any(|(c, _, mixed)| *c == id && *mixed)) {
        hint(ui, "That comp is also in the main mix: switch its \"In the master mix\" off (Comp settings) to show it only here.");
    }
}

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
    use phosphor_engine::Choice;
    let name = if g.pattern == phosphor_engine::TestPattern::Grid { "GRID".to_string() } else { g.pattern.label().to_uppercase() };
    let r = toggle(ui, &mut g.on, label(icon::GRID_FOUR, &name)).on_hover_text("Alignment grid or test pattern on this output to line up the projector · right-click to pick the pattern and its settings · off for the show");
    r.right_menu(|ui| {
        ui.label(RichText::new("Test pattern").strong());
        for &p in phosphor_engine::TestPattern::ALL.iter() {
            if ui.radio_value(&mut g.pattern, p, p.label()).clicked() {
                g.on = true;
            }
        }
        hint(ui, "Grid shows over the show; the others cover it. Test card: red top left, green top right, blue bottom right, yellow bottom left.");
        ui.separator();
        ui.label(RichText::new("Grid").strong());
        ui.horizontal(|ui| {
            ui.label("Cells across");
            guarded!(ui, g.cells, egui::DragValue::new(&mut g.cells).range(2..=64));
        });
        crate::ui::toggle(ui, &mut g.crosshair, "Crosshair, circle and diagonals");
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
