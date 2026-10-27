//! Saving settings to disk.

use super::*;

impl PhosphorApp {
    pub(crate) fn save_settings(&mut self, storage: &mut dyn eframe::Storage) {
        if self.bench_session {
            return;
        }
        storage.set_string(KEY_COMPOSITION, self.state.comp.to_json());
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
        storage.set_string(KEY_LANGUAGE, crate::i18n::chosen().code().into());
        storage.set_string(KEY_TOOLTIPS, if crate::theme::tooltips_on() { "1" } else { "0" }.into());
        storage.set_string(KEY_PROJECTS_AT_START, if self.state.projects.at_start { "1" } else { "0" }.into());
        storage.set_string(KEY_UPDATE_SKIP, self.state.updater.skipped.clone());
        storage.set_string(KEY_UPDATE_AUTO, if self.state.updater.auto { "1" } else { "0" }.into());
        storage.set_string(KEY_SEND_CRASHES, if self.state.bug.send_crashes { "1" } else { "0" }.into());
        #[cfg(not(target_arch = "wasm32"))]
        storage.set_string(KEY_SOUND_OUT, crate::media::video::sound::output_device().unwrap_or_default());
        storage.set_string(KEY_FOLLOW_AT_LAUNCH, self.state.follow_at_launch.clone());
        storage.set_string(KEY_LOCK, self.lock.to_setting());
        storage.set_string(KEY_PANELS_LOCKED, if self.panels_locked { "1" } else { "0" }.into());
        if let Some(t) = layout::to_text(&self.state.render_rate) {
            storage.set_string(KEY_RENDER_RATE, t);
        }
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
