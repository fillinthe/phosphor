//! Live data section (Modulators panel): switch on the weather somewhere, the ISS and earthquakes; their readings
//! become modulators (`crate::live_data`) that any control can follow (right-click › Modulate).

use crate::live_data::Feed;

use super::*;

pub(crate) fn live_data_section(ui: &mut egui::Ui, st: &mut State) {
    let on = st.comp.live_data.readings();
    egui::CollapsingHeader::new(RichText::new(label(icon::GLOBE_HEMISPHERE_WEST, "Live data")).strong()).id_salt("live_data").default_open(!on.is_empty()).show(ui, |ui| {
        hint(ui, "Things happening in the world move your visuals: each reading is a modulator (0 to 1) any control can follow. Needs the internet; free, no sign-up.");
        // A place found by name becomes the weather's place.
        if let Some(Ok((name, lat, lon))) = st.feeds.found.take() {
            let d = &mut st.comp.live_data;
            (d.place, d.latitude, d.longitude, d.weather) = (name, lat, lon, true);
        }
        let d = &mut st.comp.live_data;
        ui.horizontal(|ui| {
            toggle(ui, &mut d.weather, "Weather").on_hover_text("Temperature, wind, clouds and rain where you pick (Open-Meteo, every 10 minutes)");
            let id = egui::Id::new("live_data_place");
            let mut name: String = ui.ctx().data(|t| t.get_temp(id)).unwrap_or_default();
            let r = crate::ui::text_field(ui, &mut name, if d.place.is_empty() { "A town or city" } else { &d.place }, Some(150.0));
            let go = (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) || crate::ui::command_if(ui, !name.trim().is_empty() && !st.feeds.finding, icon::MAGNIFYING_GLASS).on_hover_text("Find this place").clicked();
            if go && !name.trim().is_empty() {
                st.feeds.find_place(&name);
                name.clear();
            }
            if st.feeds.finding {
                ui.spinner();
            }
            ui.ctx().data_mut(|t| t.insert_temp(id, name));
        });
        if let Some(Err(e)) = &st.feeds.found {
            ui.colored_label(WARN, format!("Couldn't find that place: {e}"));
        }
        if !d.place.is_empty() {
            hint(ui, format!("Weather in {}", d.place));
        } else if d.weather {
            hint(ui, "Type a place and press Enter.");
        }
        crate::ui::toolbar(ui, |ui| {
            toggle(ui, &mut d.iss, "Space station").on_hover_text("Where the International Space Station is over the Earth (every 5 seconds; it goes round in 90 minutes)");
            toggle(ui, &mut d.quakes, "Earthquakes").on_hover_text("The newest earthquake's size and how many there were in the last hour, worldwide (USGS, every minute)");
        });
        let names = st.comp.live_data.readings();
        if names.is_empty() {
            return;
        }
        let free = phosphor_engine::control::MAX_SHARED.saturating_sub(st.comp.modulators.len());
        let missing = names.iter().filter(|n| st.comp.hand(n).is_none()).count();
        if missing > free {
            ui.colored_label(WARN, format!("Only {} shared modulators fit: remove some to get every reading.", phosphor_engine::control::MAX_SHARED));
        }
        for name in names {
            ui.horizontal(|ui| {
                let (value, shown) = st.feeds.reading(name).map_or((0.0, "waiting…".to_string()), |r| (r.value, r.shown.clone()));
                ui.add(crate::ui::meter(value).desired_width(90.0));
                ui.label(name);
                hint(ui, shown);
            });
        }
        for (feed, e) in &st.feeds.problems {
            let what = match feed {
                Feed::Weather => "Weather",
                Feed::Iss => "Space station",
                Feed::Quakes => "Earthquakes",
            };
            ui.colored_label(WARN, format!("{what}: {e} (trying again soon)"));
        }
    });
}
