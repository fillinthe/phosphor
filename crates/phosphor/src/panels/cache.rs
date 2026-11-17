//! Cache settings (brightness menu): how much Phosphor keeps on this computer, clearing it, and when what went
//! unused is cleaned up by itself. The housekeeping is `cache_store`.

use eframe::egui;
use egui_phosphor::regular as icon;

use crate::app::State;
use crate::ui::guarded;

pub(crate) fn cache_rows(ui: &mut egui::Ui, st: &mut State) {
    crate::ui::field(ui, &format!("{} Cache", icon::HARD_DRIVES), |ui| {
        let size = crate::cache_store::usage().map_or_else(|| "…".to_string(), crate::cache_store::size_text);
        ui.label(size).on_hover_text("What Phosphor keeps on this computer: copies of clips from other drives (they load fast, and play with the drive unplugged), thumbnails, and a copy of each project");
        if crate::ui::command(ui, format!("{} Clear", icon::BROOM)).on_hover_text("Removes the clip copies and thumbnails. Nothing of yours is lost: they're made again from your files as you use them. Project copies stay").clicked() {
            crate::cache_store::clear();
            st.notice = Some("Cache cleared: clip copies and thumbnails are made again as you use your files".into());
        }
    });
    crate::ui::field(ui, "Clean up after", |ui| {
        let mut days = crate::cache_store::days();
        if guarded!(ui, days, egui::DragValue::new(&mut days).range(0..=365).suffix(" days")).on_hover_text("Each time Phosphor starts, clip copies and thumbnails not used for this many days are removed. 0 = never").changed() {
            crate::cache_store::set_days(days);
        }
        let mut gb = crate::media::clip_cache::cap_gb();
        ui.label("Up to");
        if guarded!(ui, gb, egui::DragValue::new(&mut gb).range(1..=500).suffix(" GB")).on_hover_text("The most the clip copies may take. When it's full, the ones used longest ago make room").changed() {
            crate::media::clip_cache::set_cap_gb(gb);
        }
    });
}
