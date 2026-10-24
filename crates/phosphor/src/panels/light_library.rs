//! "A real light…" (LED / DMX panel): search the Open Fixture Library by name, pick the light and the DMX mode it's
//! set to, and it's added with its channels in order (`crate::ofl`).

use phosphor_engine::{Fixture, FixtureShape};

use super::*;

/// Temp-data key: the window is open.
const LIGHT_WINDOW: &str = "phosphor_light_window";

pub(crate) fn open_light_window(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(LIGHT_WINDOW), true));
}

/// The window, while open.
pub(crate) fn light_window(ctx: &egui::Context, st: &mut State) {
    if !ctx.data(|d| d.get_temp::<bool>(egui::Id::new(LIGHT_WINDOW))).unwrap_or(false) {
        return;
    }
    st.ofl.poll(ctx);
    let query_id = egui::Id::new("light_query");
    let mut query: String = ctx.data(|d| d.get_temp(query_id)).unwrap_or_default();
    let mut open = true;
    let mut add: Option<Fixture> = None;
    egui::Window::new(label(icon::LIGHTBULB, "A real light")).open(&mut open).default_width(440.0).default_height(420.0).show(ctx, |ui| {
        hint(ui, "Type the maker or model, pick your light, then the DMX mode it's set to (on its menu or DIP switches). Lights from the Open Fixture Library.");
        crate::ui::text_field(ui, &mut query, "e.g. Chauvet Intimidator, ADJ par, Stairville", None);
        let lib = &mut st.ofl;
        match &lib.lights {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    hint(ui, "Getting the list of lights…");
                });
            }
            Some(Err(e)) => {
                ui.colored_label(WARN, e.as_str());
                if ui.button("Try again").clicked() {
                    lib.retry();
                }
            }
            Some(Ok(all)) => {
                let found = crate::ofl::search(all, &query);
                hint(ui, format!("{} of {} lights", found.len(), all.len()));
                let mut look = None;
                egui::ScrollArea::vertical().id_salt("light_list").max_height(180.0).auto_shrink([false, true]).show(ui, |ui| {
                    for l in found.iter().take(300) {
                        let on = lib.open.as_ref().is_some_and(|(o, _)| o.key == l.key);
                        if crate::ui::pick_item(ui, on, format!("{} · {}", l.maker, l.name)).clicked() {
                            look = Some((*l).clone());
                        }
                    }
                });
                if let Some(l) = look {
                    lib.look_at(&l, ui.ctx());
                }
            }
        }
        if let Some((light, def)) = &lib.open {
            ui.separator();
            match def {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        hint(ui, format!("Getting {}…", light.name));
                    });
                }
                Some(Err(e)) => {
                    ui.colored_label(WARN, e.as_str());
                }
                Some(Ok(d)) => {
                    ui.label(RichText::new(format!("{} {}", light.maker, d.name)).strong());
                    if !d.categories.is_empty() {
                        hint(ui, d.categories.join(", "));
                    }
                    for (i, (name, _)) in d.modes.iter().enumerate() {
                        ui.horizontal(|ui| {
                            if ui.button(label(icon::PLUS, &format!("Add · {name}"))).on_hover_text(d.mode_summary(i)).clicked() {
                                add = Some(d.fixture(&light.maker, i));
                            }
                            ui.add(egui::Label::new(RichText::new(d.mode_summary(i)).weak()).truncate());
                        });
                    }
                }
            }
        }
    });
    if let Some(mut f) = add {
        let n = st.comp.dmx.fixtures.iter().filter(|x| x.shape == FixtureShape::MovingHead).count();
        let next = super::leds::next_free(&st.comp.dmx.fixtures);
        f.id = st.comp.new_id();
        (f.universe, f.channel) = next;
        let x = 0.15 + 0.14 * (n % 6) as f32;
        f.points = [[x, 0.3], [x, 0.3], [x, 0.3], [x, 0.3]];
        st.notice = Some(format!("Added {} at universe {} channel {} ({})", f.name, f.universe, f.channel, f.span()));
        st.comp.dmx.fixtures.push(f);
    }
    ctx.data_mut(|d| d.insert_temp(query_id, query));
    if !open {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(LIGHT_WINDOW), false));
    }
}
