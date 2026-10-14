//! Looks you can recall: built-in presets ship inside the binary, user presets live next to the
//! app's settings (and later sync with the Phosphor account).

#[derive(Clone, Debug)]
pub struct Preset {
    pub name: String,
    pub json: String,
}

pub const BUILTIN: &[(&str, &str)] = &[("Neon Kaleido", include_str!("../presets/Neon Kaleido.phosphor"))];

#[derive(Default)]
pub struct PresetStore {
    pub user: Vec<Preset>,
    loaded: bool,
}

impl PresetStore {
    pub fn builtin() -> impl Iterator<Item = Preset> {
        BUILTIN.iter().map(|(name, json)| Preset { name: name.to_string(), json: json.to_string() })
    }

    /// Reads the user preset folder once; call again after saving or deleting.
    pub fn refresh(&mut self) {
        self.loaded = true;
        self.user.clear();
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(dir) = user_dir()
            && let Ok(entries) = std::fs::read_dir(dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "phosphor")
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
            return Err("Give the preset a name first".into());
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let dir = user_dir().ok_or("No settings folder available")?;
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let safe: String = name.chars().map(|c| if matches!(c, '/' | '\\' | ':') { '-' } else { c }).collect();
            std::fs::write(dir.join(format!("{safe}.phosphor")), json).map_err(|e| e.to_string())?;
            self.refresh();
            Ok(())
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = json;
            Err("Saving presets in the browser arrives with account sync".into())
        }
    }

    pub fn delete(&mut self, name: &str) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(dir) = user_dir() {
            let _ = std::fs::remove_file(dir.join(format!("{name}.phosphor")));
            self.refresh();
        }
        #[cfg(target_arch = "wasm32")]
        let _ = name;
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn user_dir() -> Option<std::path::PathBuf> {
    crate::show::storage_dir().map(|d| d.join("presets"))
}
