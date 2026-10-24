//! LED / DMX panel: place LED strips and matrices on the composition (drag their ends / corners on the
//! picture), set their wiring (pixels, order, universe, channel) and send them over Art-Net or sACN.

use phosphor_engine::{AppControl, Fixture, FixtureShape};

use super::*;

pub(crate) fn leds(ui: &mut egui::Ui, st: &mut State) {
    let aspect = st.aspect();
    let selected_id = egui::Id::new("led_fixture_selected");
    let mut selected: Option<u64> = ui.ctx().data(|d| d.get_temp(selected_id)).flatten();
    ui.horizontal_wrapped(|ui| {
        let d = &mut st.comp.dmx;
        output_toggle(ui, &mut d.enabled, icon::LIGHTBULB, "Send", "Send the LED pixels over the network", AppControl::Dmx);
        choice_combo(ui, "dmx_protocol", &mut d.protocol, 110.0, false);
        ui.label("to");
        let hint_text = match d.protocol {
            phosphor_engine::DmxProtocol::ArtNet => "everyone (broadcast)",
            phosphor_engine::DmxProtocol::Sacn => "multicast",
            phosphor_engine::DmxProtocol::UsbPro | phosphor_engine::DmxProtocol::OpenDmx => "its port",
        };
        let usb = matches!(d.protocol, phosphor_engine::DmxProtocol::UsbPro | phosphor_engine::DmxProtocol::OpenDmx);
        if usb {
            // The interface's USB serial port (the list is read when the menu opens).
            #[cfg(not(target_arch = "wasm32"))]
            ComboBox::from_id_salt("dmx_usb_port").width(180.0).selected_text(if d.target.is_empty() { "(choose the port)".to_string() } else { d.target.clone() }).show_ui(ui, |ui| {
                let ports = crate::usb_dmx::ports();
                if ports.is_empty() {
                    ui.label("No USB interface found: plug it in (FTDI driver on older Macs)");
                }
                for p in ports {
                    crate::ui::pick_value(ui, &mut d.target, p.clone(), p);
                }
            })
            .response
            .on_hover_text(if d.protocol == phosphor_engine::DmxProtocol::OpenDmx {
                "An Enttec Open DMX or another plain FTDI DMX cable: one universe, the first your fixtures use. Phosphor makes the DMX signal itself (40 frames a second)"
            } else {
                "An Enttec DMX USB Pro or a compatible (DMXking ultraDMX…): one universe, the first your fixtures use"
            });
        } else {
            ui.add(egui::TextEdit::singleline(&mut d.target).desired_width(120.0).hint_text(hint_text))
                .on_hover_text("The controller's IP address (e.g. 2.0.0.10), or leave empty to send to the whole network");
        }
        guarded!(ui, d.fps, egui::DragValue::new(&mut d.fps).range(1.0..=60.0).speed(0.2).suffix(" fps"));
        Level::percent(&mut d.master, "Master").default(1.0).width(120.0).target(ControlTarget::App(phosphor_engine::AppControl::DmxMaster)).tip("Every fixture's level at once").show(ui);
        #[cfg(not(target_arch = "wasm32"))]
        {
            let status = match (&st.dmx.error, d.enabled) {
                (Some(e), true) => OutputStatus::Error(e),
                (None, true) if st.dmx.universes > 0 => OutputStatus::Waiting(""),
                (None, true) => OutputStatus::Waiting("starting…"),
                _ => OutputStatus::Off,
            };
            output_status(ui, status);
            if d.enabled && st.dmx.universes > 0 && st.dmx.error.is_none() {
                ui.label(RichText::new(format!("{} {} universes", icon::CHECK_CIRCLE, st.dmx.universes)).small().color(accent()));
            }
        }
    });

    #[cfg(not(target_arch = "wasm32"))]
    dmx_monitor(ui, st);
    #[cfg(not(target_arch = "wasm32"))]
    dmx_recorder(ui, st);

    // The picture with every fixture on it.
    let w = ui.available_width().max(160.0);
    let h = (w / aspect).min(ui.available_height() * 0.5).max(90.0);
    let w = h * aspect;
    let (frame, bg) = ui.allocate_exact_size(egui::vec2(w, h), Sense::click());
    match st.texture {
        Some(t) => paint_texture(ui.painter(), t, frame),
        None => {
            ui.painter().rect_filled(frame, 2.0, Color32::from_gray(20));
        }
    }
    if bg.clicked() {
        selected = None;
    }
    let to_screen = |p: [f32; 2]| egui::pos2(frame.left() + p[0] * frame.width(), frame.top() + p[1] * frame.height());
    for f in &mut st.comp.dmx.fixtures {
        let is_sel = selected == Some(f.id);
        let color = if !f.enabled { Color32::GRAY } else if is_sel { accent() } else { accent().gamma_multiply(0.6) };
        let corners = match f.shape {
            // Found by camera: no ends or corners, just its lights (the middle moves them all).
            FixtureShape::Scanned if !f.positions.is_empty() => 0,
            FixtureShape::MovingHead => 1,
            FixtureShape::Strip | FixtureShape::Ring | FixtureShape::Scanned => 2,
            FixtureShape::Grid | FixtureShape::Curve => 4,
        };
        let pts: Vec<egui::Pos2> = f.points[..corners].iter().map(|p| to_screen(*p)).collect();
        let p = ui.painter();
        if corners == 0 {
            let line: Vec<egui::Pos2> = f.positions.iter().map(|p| to_screen(*p)).collect();
            p.add(egui::Shape::line(line, Stroke::new(if is_sel { 1.5 } else { 0.75 }, color.gamma_multiply(0.6))));
        } else if corners == 2 {
            p.line_segment([pts[0], pts[1]], Stroke::new(if is_sel { 2.0 } else { 1.0 }, color));
        } else {
            p.add(egui::Shape::closed_line(pts.clone(), Stroke::new(if is_sel { 2.0 } else { 1.0 }, color)));
        }
        // Pixel dots (thinned out on big fixtures).
        let samples = f.sample_points();
        let step = (samples.len() / 400).max(1);
        for s in samples.iter().step_by(step) {
            p.circle_filled(to_screen(*s), if is_sel { 2.0 } else { 1.5 }, color);
        }
        // Drag the ends / corners, or the middle to move the whole fixture.
        let mut moved = false;
        for (i, at) in pts.iter().enumerate() {
            let mut d = [0.0, 0.0];
            let r = point_handle(ui, ui.id().with(("led_pt", f.id, i)), *at, frame, color, false, &mut d);
            if r.drag_started() || r.clicked() {
                selected = Some(f.id);
            }
            if d != [0.0, 0.0] {
                f.points[i] = [(f.points[i][0] + d[0]).clamp(0.0, 1.0), (f.points[i][1] + d[1]).clamp(0.0, 1.0)];
                moved = true;
            }
        }
        let centre = if corners == 0 {
            samples.iter().fold(egui::Vec2::ZERO, |a, p| a + to_screen(*p).to_vec2()) / samples.len().max(1) as f32
        } else {
            pts.iter().fold(egui::Vec2::ZERO, |a, p| a + p.to_vec2()) / pts.len() as f32
        };
        let mut d = [0.0, 0.0];
        let r = point_handle(ui, ui.id().with(("led_mid", f.id)), centre.to_pos2(), frame, color, true, &mut d);
        if r.drag_started() || r.clicked() {
            selected = Some(f.id);
        }
        if !moved && d != [0.0, 0.0] {
            for p in f.points.iter_mut().chain(f.positions.iter_mut()) {
                *p = [(p[0] + d[0]).clamp(0.0, 1.0), (p[1] + d[1]).clamp(0.0, 1.0)];
            }
        }
    }

    ui.horizontal_wrapped(|ui| {
        if ui.button(label(icon::PLUS, "LED strip")).clicked() {
            let id = st.comp.new_id();
            let n = st.comp.dmx.fixtures.len();
            let next = next_free(&st.comp.dmx.fixtures);
            let y = 0.2 + 0.15 * (n % 5) as f32;
            st.comp.dmx.fixtures.push(Fixture { id, name: format!("Strip {}", n + 1), points: [[0.1, y], [0.9, y], [0.9, 0.9], [0.1, 0.9]], universe: next.0, channel: next.1, ..Default::default() });
            selected = Some(id);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if ui.button(label(icon::TABLE, "Import CSV…")).on_hover_text("Fixtures from a patch list: name, shape, pixels, universe, channel, x1, y1, x2, y2 (0–1 of the picture)").clicked() {
            st.dialogs.pick_file(crate::dialogs::Purpose::ImportFixtures, Some(("Patch list", &["csv", "txt", "tsv"])));
        }
        if ui.button(label(icon::GRID_FOUR, "LED matrix")).clicked() {
            let id = st.comp.new_id();
            let n = st.comp.dmx.fixtures.len();
            let next = next_free(&st.comp.dmx.fixtures);
            st.comp.dmx.fixtures.push(Fixture {
                id,
                name: format!("Matrix {}", n + 1),
                shape: FixtureShape::Grid,
                points: [[0.25, 0.25], [0.75, 0.25], [0.75, 0.75], [0.25, 0.75]],
                cols: 16,
                rows: 16,
                universe: next.0,
                channel: next.1,
                ..Default::default()
            });
            selected = Some(id);
        }
        video_wall_menu(ui, st);
        #[cfg(not(target_arch = "wasm32"))]
        if ui.button(label(icon::LIGHTBULB, "A real light…")).on_hover_text("Pick your light by name from the Open Fixture Library (moving heads, pars, bars…): its channels are set up for you").clicked() {
            open_light_window(ui.ctx());
        }
        #[cfg(not(target_arch = "wasm32"))]
        if ui.button(label(icon::SCAN, "Find with a camera…")).on_hover_text("The lights blink codes while a camera watches; each light is placed where the camera sees it").clicked() {
            open_led_scan_window(ui.ctx());
        }
        hint(ui, "Drag the dots on the picture to place lights; the square moves the whole fixture.");
    });

    let mut remove = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for (i, f) in st.comp.dmx.fixtures.iter_mut().enumerate() {
            let is_sel = selected == Some(f.id);
            let r = card_frame(ui, is_sel).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    toggle(ui, &mut f.enabled, "ON");
                    ui.add(egui::TextEdit::singleline(&mut f.name).desired_width(120.0));
                    hint(ui, f.span());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(icon::TRASH).on_hover_text("Delete").clicked() {
                            remove = Some(i);
                        }
                    });
                });
                ui.horizontal_wrapped(|ui| {
                    choice_buttons(ui, &mut f.shape, |_, _| {});
                    match f.shape {
                        FixtureShape::Scanned if !f.positions.is_empty() => {
                            hint(ui, format!("{} lights where the camera saw them", f.positions.len()));
                        }
                        FixtureShape::Strip | FixtureShape::Ring | FixtureShape::Curve | FixtureShape::Scanned => {
                            guarded!(ui, f.cols, egui::DragValue::new(&mut f.cols).range(1..=4096).prefix("pixels "));
                        }
                        FixtureShape::Grid => {
                            guarded!(ui, f.cols, egui::DragValue::new(&mut f.cols).range(1..=512).suffix(" wide"));
                            guarded!(ui, f.rows, egui::DragValue::new(&mut f.rows).range(1..=512).suffix(" high"));
                            crate::ui::toggle(ui, &mut f.zigzag, "Zig-zag").on_hover_text("Every other row is wired backwards (most LED matrices)");
                            crate::ui::toggle(ui, &mut f.by_columns, "By columns").on_hover_text("Wired column by column (top to bottom) instead of row by row");
                        }
                        FixtureShape::MovingHead if f.head == phosphor_engine::HeadLayout::Custom => {
                            let names: Vec<String> = f.roles.iter().enumerate().map(|(i, r)| format!("{}: {}", i + 1, custom_channel_name(f, *r))).collect();
                            ui.label(format!("{} channels (its own mode)", f.roles.len())).on_hover_text(names.join("\n"));
                        }
                        FixtureShape::MovingHead => {
                            choice_combo(ui, ("head_layout", f.id), &mut f.head, 230.0, false).on_hover_text("Set the head to the DMX mode with these channels");
                        }
                    }
                    if f.shape != FixtureShape::MovingHead {
                        choice_combo(ui, ("led_order", f.id), &mut f.order, 90.0, false);
                        crate::ui::toggle(ui, &mut f.average, "Average").on_hover_text("Each light takes the average of its own patch of the picture (9 samples) instead of the one point at its centre: steadier on detailed or moving pictures");
                    }
                });
                if f.shape == FixtureShape::MovingHead {
                    // One row each: a level fills its row.
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            Level::percent(&mut f.pan, "Pan").default(0.5).target(ControlTarget::Fixture { fixture: f.id, param: 0 }).show(ui);
                        });
                        ui.horizontal(|ui| {
                            Level::percent(&mut f.tilt, "Tilt").default(0.5).target(ControlTarget::Fixture { fixture: f.id, param: 1 }).show(ui);
                        });
                        ui.horizontal(|ui| {
                            Level::percent(&mut f.dimmer, "Dimmer").default(1.0).target(ControlTarget::Fixture { fixture: f.id, param: 2 }).show(ui);
                        });
                    });
                    ui.horizontal_wrapped(|ui| {
                        choice_buttons(ui, &mut f.motion, |_, _| {});
                        if f.motion != phosphor_engine::HeadMotion::Still {
                            ui.spacing_mut().slider_width = 80.0;
                            ui.horizontal(|ui| {
                                Level::percent(&mut f.motion_size, "Size").default(0.2).width(140.0).target(ControlTarget::Fixture { fixture: f.id, param: 3 }).show(ui);
                            });
                            guarded!(ui, f.motion_beats, egui::DragValue::new(&mut f.motion_beats).range(0.25..=64.0).speed(0.25).suffix(" beats"));
                        }
                    });
                    hint(ui, "Its colour is the picture under the dot; it moves on the beat. Drag the dot to choose the colour.");
                    // Channels after the layout's (gobo, prism, zoom, strobe…), each a control of its own.
                    let mut gone = None;
                    let custom = f.head == phosphor_engine::HeadLayout::Custom;
                    for (i, e) in f.extra.iter_mut().enumerate() {
                        if custom {
                            // A library light's own settings: named by the library, in its channel order.
                            ui.horizontal(|ui| {
                                Level::percent(&mut e.value, &e.name).width(240.0).target(ControlTarget::Fixture { fixture: f.id, param: 5 + i as u8 }).show(ui);
                            });
                            continue;
                        }
                        ui.horizontal(|ui| {
                            crate::ui::text_field(ui, &mut e.name, &format!("channel {}", f.head.channels() + i + 1), Some(80.0));
                            let name = if e.name.is_empty() { format!("Ch {}", f.head.channels() + i + 1) } else { e.name.clone() };
                            Level::percent(&mut e.value, &name).default(0.0).width(160.0).target(ControlTarget::Fixture { fixture: f.id, param: 5 + i as u8 }).show(ui);
                            if ui.small_button(icon::TRASH).on_hover_text("Remove this channel (the ones after it move up one)").clicked() {
                                gone = Some(i);
                            }
                        });
                    }
                    if let Some(i) = gone {
                        f.extra.remove(i);
                    }
                    if !custom && f.extra.len() < 32 && ui.small_button(label(icon::PLUS, "Channel")).on_hover_text("A channel after the layout's: gobo, prism, zoom, strobe… (in the order your light's manual lists them)").clicked() {
                        f.extra.push(phosphor_engine::ExtraChannel::default());
                    }
                }
                ui.horizontal_wrapped(|ui| {
                    guarded!(ui, f.universe, egui::DragValue::new(&mut f.universe).range(0..=32767).prefix("universe "));
                    guarded!(ui, f.channel, egui::DragValue::new(&mut f.channel).range(1..=512).prefix("channel "));
                    ui.spacing_mut().slider_width = 90.0;
                    ui.horizontal(|ui| {
                        Level::percent(&mut f.brightness, "Bright").default(1.0).width(140.0).target(ControlTarget::Fixture { fixture: f.id, param: 4 }).show(ui);
                    });
                    ui.horizontal(|ui| {
                        Level::range(&mut f.gamma, 1.0, 3.0, "Gamma").default(2.2).width(140.0).tip("2.2 makes fades look even on most LEDs").show(ui);
                    });
                });
            });
            if r.response.clicked() {
                selected = Some(f.id);
            }
        }
        if st.comp.dmx.fixtures.is_empty() {
            hint(ui, "Add an LED strip or matrix, place it on the picture, set its universe and channel, then turn on Send. Works with Art-Net and sACN controllers (e.g. PixLite, Enttec, WLED, Falcon).");
        }
    });
    if let Some(i) = remove {
        st.comp.dmx.fixtures.remove(i);
    }
    ui.ctx().data_mut(|d| d.insert_temp(selected_id, selected));
    #[cfg(not(target_arch = "wasm32"))]
    led_scan_window(&ui.ctx().clone(), st);
}

/// The first universe / channel after the last fixture, so new fixtures don't overlap.
/// Video wall: a grid of LED tiles covering the whole picture, patched one after another (each on fresh universes).
fn video_wall_menu(ui: &mut egui::Ui, st: &mut State) {
    let key = ui.id().with("video_wall");
    let mut v: [u32; 4] = ui.data_mut(|d| d.get_temp(key)).unwrap_or([4, 3, 32, 32]);
    ui.menu_button(label(icon::SQUARES_FOUR, "Video wall…"), |ui| {
        ui.horizontal(|ui| {
            ui.label("Tiles");
            guarded!(ui, v[0], egui::DragValue::new(&mut v[0]).range(1..=32).suffix(" across"));
            guarded!(ui, v[1], egui::DragValue::new(&mut v[1]).range(1..=32).suffix(" down"));
        });
        ui.horizontal(|ui| {
            ui.label("Each tile");
            guarded!(ui, v[2], egui::DragValue::new(&mut v[2]).range(1..=512).suffix(" px wide"));
            guarded!(ui, v[3], egui::DragValue::new(&mut v[3]).range(1..=512).suffix(" px high"));
        });
        let start = next_free(&st.comp.dmx.fixtures);
        hint(ui, format!("{} tiles of {} pixels, from universe {} (each tile starts a fresh universe). Set the wiring (zig-zag, order) on each after.", v[0] * v[1], v[2] * v[3], start.0));
        if ui.button(label(icon::PLUS, "Add the wall")).clicked() {
            let comp = &mut st.comp;
            let mut ids = Vec::new();
            for _ in 0..v[0] * v[1] {
                ids.push(comp.new_id());
            }
            let mut ids = ids.into_iter();
            comp.dmx.add_video_wall(v[0], v[1], v[2], v[3], start, || ids.next().unwrap_or_default());
            ui.close();
        }
    });
    ui.data_mut(|d| d.insert_temp(key, v));
}

/// A library light's channel, as its role or its setting's name.
fn custom_channel_name(f: &Fixture, role: phosphor_engine::Role) -> String {
    match role {
        phosphor_engine::Role::Extra(i) => f.extra.get(i as usize).map_or_else(|| "?".into(), |e| e.name.clone()),
        r => r.name().to_string(),
    }
}

pub(crate) fn next_free(fixtures: &[Fixture]) -> (u16, u16) {
    fixtures.last().map_or((0, 1), |f| f.next_free())
}

/// What's being sent: one universe's 512 channels as a grid (brighter = higher), value on hover. Only copied while
/// this is open.
#[cfg(not(target_arch = "wasm32"))]
fn dmx_monitor(ui: &mut egui::Ui, st: &mut State) {
    egui::CollapsingHeader::new(label(icon::MONITOR, "Monitor (what's being sent)")).id_salt("dmx_monitor").show(ui, |ui| {
        st.dmx.watching = true;
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        let unis: Vec<u16> = st.dmx.sent.keys().copied().collect();
        if unis.is_empty() {
            hint(ui, "Nothing going out (LED / DMX off, or no fixtures on).");
            return;
        }
        let id = ui.id().with("dmx_monitor_uni");
        let mut uni = ui.data(|d| d.get_temp::<u16>(id)).filter(|u| unis.contains(u)).unwrap_or(unis[0]);
        ui.horizontal_wrapped(|ui| {
            ui.label("Universe");
            for u in &unis {
                if crate::ui::pick_item(ui, *u == uni, u.to_string()).clicked() {
                    uni = *u;
                }
            }
        });
        ui.data_mut(|d| d.insert_temp(id, uni));
        let Some(data) = st.dmx.sent.get(&uni) else { return };
        let cols = 32usize;
        let cell = ((ui.available_width() - 4.0) / cols as f32).clamp(6.0, 22.0);
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(cell * cols as f32, cell * (512 / cols) as f32), Sense::hover());
        let p = ui.painter();
        for (i, v) in data.iter().enumerate() {
            let r = egui::Rect::from_min_size(rect.min + egui::vec2((i % cols) as f32 * cell, (i / cols) as f32 * cell), egui::vec2(cell - 1.0, cell - 1.0));
            p.rect_filled(r, 0.0, if *v == 0 { Color32::from_gray(24) } else { accent().gamma_multiply(0.15 + 0.85 * *v as f32 / 255.0) });
        }
        if let Some(pos) = resp.hover_pos() {
            let (cx, cy) = (((pos.x - rect.left()) / cell) as usize, ((pos.y - rect.top()) / cell) as usize);
            let i = (cy * cols + cx).min(511);
            resp.on_hover_text(format!("Universe {uni} · channel {} = {}", i + 1, data[i]));
        }
    });
}

/// Record what the lights are sent and play it back (a chase you can reuse without the picture), save and open takes.
#[cfg(not(target_arch = "wasm32"))]
fn dmx_recorder(ui: &mut egui::Ui, st: &mut State) {
    let now = ui.input(|i| i.time);
    ui.horizontal_wrapped(|ui| {
        let r = &mut st.dmx_rec;
        if r.is_recording() {
            if action_button(ui, Act::Record, label(icon::STOP, &format!("Stop ({:.0} s)", r.elapsed(now)))).clicked() {
                r.stop();
            }
        } else if ui.button(label(icon::RECORD, "Record DMX")).on_hover_text("Keep everything the lights are sent from now on (Stop ends the take)").clicked() {
            r.start_recording(now);
        }
        let has_take = r.take.as_ref().is_some_and(|t| !t.frames.is_empty());
        if r.is_playing() {
            if ui.button(label(icon::STOP, &format!("Stop playback ({:.0} s)", r.elapsed(now)))).clicked() {
                r.stop();
            }
        } else if crate::ui::command_if(ui, has_take && !r.is_recording(), label(icon::PLAY, "Play")).on_hover_text("Send the take to the lights instead of the picture").clicked() {
            r.play(now);
        }
        crate::ui::toggle(ui, &mut r.looping, "Loop");
        if let Some(t) = &r.take {
            hint(ui, &format!("take: {:.1} s, {} frames", t.length(), t.frames.len()));
        }
        if crate::ui::command_if(ui, has_take, label(icon::FLOPPY_DISK, "Save…")).clicked() {
            st.dialogs.save_file(crate::dialogs::Purpose::SaveDmxTake, ("DMX take", &["phdmx"]), "Lights.phdmx");
        }
        if ui.button(label(icon::FOLDER_OPEN, "Open…")).on_hover_text("A saved take (.phdmx)").clicked() {
            st.dialogs.pick_file(crate::dialogs::Purpose::OpenDmxTake, Some(("DMX take", &["phdmx"])));
        }
    });
}
