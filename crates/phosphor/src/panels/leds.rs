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
        };
        ui.add(egui::TextEdit::singleline(&mut d.target).desired_width(120.0).hint_text(hint_text))
            .on_hover_text("The controller's IP address (e.g. 2.0.0.10), or leave empty to send to the whole network");
        guarded!(ui, d.fps, egui::DragValue::new(&mut d.fps).range(1.0..=60.0).speed(0.2).suffix(" fps"));
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
            FixtureShape::MovingHead => 1,
            FixtureShape::Strip => 2,
            FixtureShape::Grid => 4,
        };
        let pts: Vec<egui::Pos2> = f.points[..corners].iter().map(|p| to_screen(*p)).collect();
        let p = ui.painter();
        if corners == 2 {
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
        let centre = pts.iter().fold(egui::Vec2::ZERO, |a, p| a + p.to_vec2()) / pts.len() as f32;
        let mut d = [0.0, 0.0];
        let r = point_handle(ui, ui.id().with(("led_mid", f.id)), centre.to_pos2(), frame, color, true, &mut d);
        if r.drag_started() || r.clicked() {
            selected = Some(f.id);
        }
        if !moved && d != [0.0, 0.0] {
            for p in f.points.iter_mut() {
                *p = [(p[0] + d[0]).clamp(0.0, 1.0), (p[1] + d[1]).clamp(0.0, 1.0)];
            }
        }
    }

    ui.horizontal(|ui| {
        if ui.button(label(icon::PLUS, "LED strip")).clicked() {
            let id = st.comp.new_id();
            let n = st.comp.dmx.fixtures.len();
            let next = next_free(&st.comp.dmx.fixtures);
            let y = 0.2 + 0.15 * (n % 5) as f32;
            st.comp.dmx.fixtures.push(Fixture { id, name: format!("Strip {}", n + 1), points: [[0.1, y], [0.9, y], [0.9, 0.9], [0.1, 0.9]], universe: next.0, channel: next.1, ..Default::default() });
            selected = Some(id);
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
                        FixtureShape::Strip => {
                            guarded!(ui, f.cols, egui::DragValue::new(&mut f.cols).range(1..=4096).prefix("pixels "));
                        }
                        FixtureShape::Grid => {
                            guarded!(ui, f.cols, egui::DragValue::new(&mut f.cols).range(1..=512).suffix(" wide"));
                            guarded!(ui, f.rows, egui::DragValue::new(&mut f.rows).range(1..=512).suffix(" high"));
                            crate::ui::toggle(ui, &mut f.zigzag, "Zig-zag").on_hover_text("Every other row is wired backwards (most LED matrices)");
                        }
                        FixtureShape::MovingHead => {
                            choice_combo(ui, ("head_layout", f.id), &mut f.head, 230.0, false).on_hover_text("Set the head to the DMX mode with these channels");
                        }
                    }
                    if f.shape != FixtureShape::MovingHead {
                        choice_combo(ui, ("led_order", f.id), &mut f.order, 90.0, false);
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
                    for (i, e) in f.extra.iter_mut().enumerate() {
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
                    if f.extra.len() < 32 && ui.small_button(label(icon::PLUS, "Channel")).on_hover_text("A channel after the layout's: gobo, prism, zoom, strobe… (in the order your light's manual lists them)").clicked() {
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
}

/// The first universe / channel after the last fixture, so new fixtures don't overlap.
fn next_free(fixtures: &[Fixture]) -> (u16, u16) {
    fixtures.last().map_or((0, 1), |f| f.next_free())
}
