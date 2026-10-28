//! Things you save by name and recall: looks (whole projects, with built-ins shipped in the binary), layers,
//! effect chains, venues (output + mapping setups), Control Surface pages and MIDI/OSC mappings. One store per
//! kind, each in its own folder next to the app's settings; `export_all` / `merge_from_account` carry them
//! through your phosphorfx.com account.

#[derive(Clone, Debug)]
pub struct Preset {
    pub name: String,
    pub json: String,
}

pub const BUILTIN_LOOKS: &[(&str, &str)] = &[("Neon Kaleido", include_str!("../presets/Neon Kaleido.phosphor"))];

pub struct PresetStore {
    pub user: Vec<Preset>,
    loaded: bool,
    /// Sub-folder of the settings folder, file extension, and presets shipped with the app.
    folder: &'static str,
    ext: &'static str,
    builtin: &'static [(&'static str, &'static str)],
}

impl PresetStore {
    /// Whole looks (Presets panel).
    pub fn looks() -> Self {
        Self { user: Vec::new(), loaded: false, folder: "presets", ext: "phosphor", builtin: BUILTIN_LOOKS }
    }

    /// Single layers (source, effects, blend…) to drop into any project.
    pub fn layers() -> Self {
        Self { user: Vec::new(), loaded: false, folder: "layers", ext: "layer", builtin: &[] }
    }

    /// Effect chains to add to any layer or MASTER.
    pub fn chains() -> Self {
        Self { user: Vec::new(), loaded: false, folder: "effect-chains", ext: "chain", builtin: &[] }
    }

    /// Designer boxes made of boxes (`phosphor_engine::SavedBox`), to add to any network.
    pub fn boxes() -> Self {
        Self { user: Vec::new(), loaded: false, folder: "boxes", ext: "box", builtin: &[] }
    }

    /// Output + mapping setups per venue (Mapping panel).
    pub fn venues() -> Self {
        Self { user: Vec::new(), loaded: false, folder: "venues", ext: "venue", builtin: &[] }
    }
    /// Control Surface pages (Control Surface panel).
    pub fn surfaces() -> Self {
        Self { user: Vec::new(), loaded: false, folder: "control-surfaces", ext: "surface", builtin: &[] }
    }
    /// MIDI / OSC / key mappings (MIDI / OSC panel), e.g. one set per controller.
    pub fn mappings() -> Self {
        Self { user: Vec::new(), loaded: false, folder: "mappings", ext: "mappings", builtin: &[] }
    }

    /// Every kind of named save, by the key used in the account copy.
    pub fn all_kinds() -> [(&'static str, PresetStore); 7] {
        [
            ("looks", Self::looks()),
            ("layers", Self::layers()),
            ("chains", Self::chains()),
            ("boxes", Self::boxes()),
            ("venues", Self::venues()),
            ("surfaces", Self::surfaces()),
            ("mappings", Self::mappings()),
        ]
    }

    pub fn builtins(&self) -> impl Iterator<Item = Preset> + '_ {
        self.builtin.iter().map(|(name, json)| Preset { name: name.to_string(), json: json.to_string() })
    }

    /// Reads the folder once; call again after saving or deleting.
    pub fn refresh(&mut self) {
        self.loaded = true;
        self.user.clear();
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(dir) = self.dir()
            && let Ok(entries) = std::fs::read_dir(dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == self.ext)
                    && let Some(stem) = path.file_stem()
                    && let Ok(json) = std::fs::read_to_string(&path)
                {
                    self.user.push(Preset { name: stem.to_string_lossy().to_string(), json });
                }
            }
            self.user.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        }
    }

    pub fn ensure_loaded(&mut self) {
        if !self.loaded {
            self.refresh();
        }
    }

    pub fn save(&mut self, name: &str, json: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Give it a name first".into());
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let dir = self.dir().ok_or("No settings folder available")?;
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            std::fs::write(dir.join(format!("{}.{}", safe(name), self.ext)), json).map_err(|e| e.to_string())?;
            self.refresh();
            Ok(())
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = json;
            Err("Saving these in the browser arrives with account sync".into())
        }
    }

    pub fn delete(&mut self, name: &str) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(dir) = self.dir() {
            let _ = std::fs::remove_file(dir.join(format!("{}.{}", safe(name), self.ext)));
            self.refresh();
        }
        #[cfg(target_arch = "wasm32")]
        let _ = name;
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn dir(&self) -> Option<std::path::PathBuf> {
        crate::show::storage_dir().map(|d| d.join(self.folder))
    }
}

/// A name that's safe as a file name.
fn safe(name: &str) -> String {
    name.chars().map(|c| if matches!(c, '/' | '\\' | ':') { '-' } else { c }).collect()
}

/// All your named saves as one JSON document for the account: {"looks": {"name": "…json…"}, …}.
pub fn export_all() -> String {
    let mut out = serde_json::Map::new();
    for (key, mut store) in PresetStore::all_kinds() {
        store.refresh();
        let entries: serde_json::Map<String, serde_json::Value> = store.user.into_iter().map(|p| (p.name, serde_json::Value::String(p.json))).collect();
        out.insert(key.to_string(), serde_json::Value::Object(entries));
    }
    serde_json::Value::Object(out).to_string()
}

/// The entries in an account copy that this computer doesn't have yet: (kind, name, json). Same name here =
/// yours stays (nothing is overwritten or deleted by syncing).
pub fn missing_from(account: &str, have: &dyn Fn(&str, &str) -> bool) -> Vec<(String, String, String)> {
    let Ok(serde_json::Value::Object(kinds)) = serde_json::from_str::<serde_json::Value>(account) else { return Vec::new() };
    let mut out = Vec::new();
    for (kind, entries) in kinds {
        let serde_json::Value::Object(entries) = entries else { continue };
        for (name, json) in entries {
            if let serde_json::Value::String(json) = json
                && !have(&kind, &name)
            {
                out.push((kind.clone(), name, json));
            }
        }
    }
    out
}

/// Saves what the account has and this computer doesn't. Returns how many were added.
pub fn merge_from_account(account: &str) -> usize {
    let mut stores = PresetStore::all_kinds();
    for (_, s) in stores.iter_mut() {
        s.refresh();
    }
    let missing = missing_from(account, &|kind, name| stores.iter().any(|(k, s)| *k == kind && s.user.iter().any(|p| p.name == name)));
    let mut added = 0;
    for (kind, name, json) in missing {
        if let Some((_, store)) = stores.iter_mut().find(|(k, _)| *k == kind)
            && store.save(&name, &json).is_ok()
        {
            added += 1;
        }
    }
    added
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_entries_you_dont_have_come_in() {
        let account = r#"{"venues": {"Club": "{}", "Barn": "{}"}, "mappings": {"APC40": "[]"}, "nonsense": 3}"#;
        let have = |kind: &str, name: &str| kind == "venues" && name == "Club";
        let mut got = super::missing_from(account, &have);
        got.sort();
        assert_eq!(got, vec![("mappings".into(), "APC40".into(), "[]".into()), ("venues".into(), "Barn".into(), "{}".into())]);
        assert!(super::missing_from("not json", &have).is_empty());
    }
}
