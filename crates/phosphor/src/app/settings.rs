//! Saving settings to disk.

use super::*;

/// How often hidden tooltips, menus and pop-ups are forgotten (`forget_areas`).
const FORGET_AREAS_EVERY: f64 = 60.0;

/// Areas egui needn't remember once they're off screen: tooltips, menus and pop-ups (placed by their code each time
/// they show). Windows the user moved (`Order::Middle`) keep their place.
fn ephemeral(layer: &egui::LayerId) -> bool {
    matches!(layer.order, egui::Order::Foreground | egui::Order::Tooltip | egui::Order::Debug)
}

/// Forgets the tooltips, menus and pop-ups egui has kept since they were last shown (one entry each, forever, and all
/// saved with the settings: 1,185 in one owner's file). Returns how many.
pub(crate) fn forget_areas(ctx: &egui::Context) -> usize {
    ctx.memory_mut(|m| m.forget_hidden_areas(ephemeral))
}

impl PhosphorApp {
    /// On the first frame and once a minute: `forget_areas` (cheap: one walk over the list).
    pub(crate) fn forget_areas_now_and_then(&mut self, ctx: &egui::Context, now: f64) {
        if now < self.areas_forget_at {
            return;
        }
        self.areas_forget_at = now + FORGET_AREAS_EVERY;
        let n = forget_areas(ctx);
        if n > 0 {
            log::info!("forgot {n} hidden tooltips / menus");
        }
    }

    /// Where the project is kept between runs. Native: the recovery snapshot (written every 2 s off the UI thread,
    /// and on quit) holds it, so the settings only say so; while that can't be written they carry a copy (compact).
    /// The settings used to hold it as pretty JSON, made on the UI thread at every autosave.
    fn save_project(&self, storage: &mut dyn eframe::Storage) {
        #[cfg(not(target_arch = "wasm32"))]
        let in_recovery = self.saver.holds_project();
        #[cfg(target_arch = "wasm32")]
        let in_recovery = false;
        if in_recovery {
            storage.remove_string(KEY_COMPOSITION);
            storage.set_string(KEY_PROJECT_IN_RECOVERY, "1".into());
        } else {
            storage.set_string(KEY_COMPOSITION, self.state.comp.to_json_compact());
            storage.remove_string(KEY_PROJECT_IN_RECOVERY);
        }
    }

    pub(crate) fn save_settings(&mut self, storage: &mut dyn eframe::Storage) {
        // Keys from settings that were renamed or retired: dropped so the file holds only what the app reads.
        for key in ["help_icons", "areas_v1", "use_areas"] {
            storage.remove_string(key);
        }
        // (PHOSPHOR_BENCH_KEEP_SETTINGS=1: a bench saves too, for tests across a restart.)
        if self.bench_session && std::env::var_os("PHOSPHOR_BENCH_KEEP_SETTINGS").is_none() {
            return;
        }
        self.save_project(storage);
        // The interface's memory (window places, open sections), without what it no longer shows.
        if cfg!(not(target_arch = "wasm32")) {
            forget_areas(&self.egui_ctx);
            self.egui_ctx.memory(|m| eframe::set_value(storage, KEY_EGUI_MEMORY, m));
        }
        storage.set_string(KEY_STACK_RETIRED, "1".into());
        if let Some(text) = to_text(&self.dock) {
            storage.set_string(KEY_DOCK, text);
        }
        if let Some(text) = to_text(&self.layouts) {
            storage.set_string(KEY_LAYOUTS, text);
        }
        if let Ok(json) = serde_json::to_string(&self.state.stream.settings) {
            storage.set_string(KEY_STREAM, json);
        }
        if let Ok(json) = serde_json::to_string(&self.state.recorder_settings()) {
            storage.set_string(KEY_RECORDERS, json);
        }
        if let Ok(json) = serde_json::to_string(&Tab::ALL) {
            storage.set_string(KEY_KNOWN_TABS, json);
        }
        if let Ok(json) = serde_json::to_string(&self.detached) {
            storage.set_string(KEY_DETACHED, json);
        }
        if let Ok(json) = serde_json::to_string(&self.state.explorer.favorites) {
            storage.set_string(KEY_FAVORITES, json);
        }
        if let Ok(json) = serde_json::to_string(&self.state.explorer.hidden) {
            storage.set_string(KEY_HIDDEN_FX, json);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Ok(json) = serde_json::to_string(&self.state.folders.folders) {
            storage.set_string(KEY_WATCHED, json);
        }
        storage.set_string(KEY_LIBRARY_DEFAULT, "1".into());
        if let Ok(json) = serde_json::to_string(&self.state.recent) {
            storage.set_string(KEY_RECENT_PROJECTS, json);
        }
        storage.set_string(KEY_PROJECT_PATH, self.state.project_path.clone().unwrap_or_default());
        storage.set_string(KEY_AUTOSAVE, if self.state.autosave { "1" } else { "0" }.into());
        storage.set_string(KEY_HELP_ICONS, if crate::ui::help_icons() { "1" } else { "0" }.into());
        storage.set_string(KEY_SLIDERS_JUMP, if crate::ui::sliders_jump() { "1" } else { "0" }.into());
        storage.set_string(KEY_DONATE_SHOWN, if crate::support::donate_shown() { "1" } else { "0" }.into());
        storage.set_string(KEY_UNDO_PLAYBACK, if crate::history::undo_playback() { "1" } else { "0" }.into());
        storage.set_string(KEY_CONFIRM_REMOVES, if crate::ui::confirm_removes() { "1" } else { "0" }.into());
        storage.set_string(KEY_MEDIA_CLICK_PLAYS, if crate::panels::media_click_plays() { "1" } else { "0" }.into());
        storage.set_string(KEY_LANGUAGE, crate::i18n::chosen().code().into());
        storage.set_string(KEY_TOOLTIPS, if crate::theme::tooltips_on() { "1" } else { "0" }.into());
        storage.set_string(KEY_PROJECTS_AT_START, if self.state.projects.at_start { "1" } else { "0" }.into());
        storage.set_string(KEY_UPDATE_SKIP, self.state.updater.skipped.clone());
        storage.set_string(KEY_UPDATE_AUTO, if self.state.updater.auto { "1" } else { "0" }.into());
        #[cfg(not(target_arch = "wasm32"))]
        {
            storage.set_string(KEY_CACHE_DAYS, crate::cache_store::days().to_string());
            storage.set_string(KEY_CACHE_GB, crate::media::clip_cache::cap_gb().to_string());
        }
        storage.set_string(KEY_SEND_PROBLEMS, if self.state.bug.send_problems { "1" } else { "0" }.into());
        storage.set_string(KEY_PRIVACY_NOTICE, if self.state.bug.notice_seen { "1" } else { "0" }.into());
        #[cfg(not(target_arch = "wasm32"))]
        storage.set_string(KEY_SOUND_OUT, crate::media::video::sound::output_device().unwrap_or_default());
        storage.set_string(KEY_FOLLOW_AT_LAUNCH, self.state.follow_at_launch.clone());
        // (Only a player that follows at launch keeps the leader's password.)
        storage.set_string(KEY_FOLLOW_PASSWORD, if self.state.follow_at_launch.is_empty() { String::new() } else { self.state.follow_password.clone() });
        storage.set_string(KEY_LOCK, self.lock.to_setting());
        storage.set_string(KEY_PANELS_LOCKED, if self.panels_locked { "1" } else { "0" }.into());
        if let Some(t) = layout::to_text(&self.state.render_rate) {
            storage.set_string(KEY_RENDER_RATE, t);
        }
        storage.set_string(KEY_SCREENS_DONT_WAIT, if self.state.screens_dont_wait { "1" } else { "0" }.into());
        storage.set_string(KEY_RENDER_THREAD, if self.state.render_thread { "1" } else { "0" }.into());
        storage.set_string(KEY_REMOTE_PASSWORD, if self.state.require_password { self.state.remote_password.clone() } else { String::new() });
        if let Ok(json) = serde_json::to_string(&self.state.explorer.recent) {
            storage.set_string(KEY_RECENT, json);
        }
        if let Ok(json) = serde_json::to_string(&self.state.monitors) {
            storage.set_string(KEY_MONITORS, json);
        }
        if self.state.account_hello_never {
            storage.set_string(KEY_ACCOUNT_HELLO, "1".into());
        }
        storage.set_string(KEY_ACCOUNT_TAB_MOVED, "1".into());
        storage.set_string(KEY_AREAS, "1".into());
        let saved: Vec<(usize, DockState<Tab>, Vec<Tab>)> = self.workspace_docks.iter().map(|(i, (d, t))| (*i, d.clone(), t.clone())).collect();
        if let Some(text) = to_text(&saved) {
            storage.set_string(KEY_AREA_DOCKS, text);
        }
        storage.set_string(KEY_AREA, self.workspace.unwrap_or(0).to_string());
        storage.set_string(KEY_BRIGHTNESS, self.ui_brightness.to_string());
        storage.set_string(KEY_ACCENT, theme::accent_index().to_string());
        if let Ok(json) = serde_json::to_string(&self.state.control.settings()) {
            storage.set_string(KEY_CONTROL, json);
        }
    }
}
