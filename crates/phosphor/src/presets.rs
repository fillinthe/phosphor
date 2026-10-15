//! Things you save by name and recall: looks (whole projects, with built-ins shipped in the binary) and venues
//! (output + mapping setups). One store per kind, each in its own folder next to the app's settings.

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

    /// Output + mapping setups per venue (Mapping panel).
    pub fn venues() -> Self {
        Self { user: Vec::new(), loaded: false, folder: "venues", ext: "venue", builtin: &[] }
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
