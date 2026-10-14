//! Exported shows: a copy of Phosphor.app that carries a project, its media, shaders and
//! mappings in `Contents/Resources/Show`. When that folder is present, Phosphor opens straight
//! into the show and keeps its settings separate from the main app.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use phosphor_engine::Composition;
use serde::{Deserialize, Serialize};

/// Written next to the project as `show.json`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Manifest {
    pub name: String,
    pub exported: String,
    pub phosphor_version: String,
    /// Output id → screen name it was on when exported, so the show lands on the same screens.
    pub screens: Vec<(u64, String)>,
    /// Open the output windows full screen as soon as the show starts.
    pub autostart: bool,
    /// The panel layout at export time (opens the same way).
    /// RON text of the app's SavedLayout.
    pub layout: Option<String>,
}

pub struct Bundled {
    pub dir: PathBuf,
    pub manifest: Manifest,
}

pub const PROJECT: &str = "show.phosphor";
pub const MANIFEST: &str = "show.json";

/// The show inside this app, if it is an exported show.
pub fn bundled() -> Option<&'static Bundled> {
    static B: OnceLock<Option<Bundled>> = OnceLock::new();
    B.get_or_init(|| {
        let exe = std::env::current_exe().ok()?;
        let dir = exe.parent()?.join("../Resources/Show");
        let manifest: Manifest = serde_json::from_str(&std::fs::read_to_string(dir.join(MANIFEST)).ok()?).ok()?;
        dir.join(PROJECT).is_file().then(|| Bundled { dir: dir.canonicalize().unwrap_or(dir), manifest })
    })
    .as_ref()
}

/// Name for settings/recovery storage: separate per exported show.
pub fn storage_name() -> &'static str {
    static N: OnceLock<String> = OnceLock::new();
    N.get_or_init(|| match bundled() {
        Some(b) => format!("Phosphor Show {}", slug(&b.manifest.name)),
        None => "Phosphor".into(),
    })
}

pub fn storage_dir() -> Option<PathBuf> {
    eframe::storage_dir(storage_name())
}

pub fn slug(name: &str) -> String {
    let s: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "show".into() } else { s }
}

/// The show's project with its media paths pointing into the bundle.
pub fn load(b: &Bundled) -> Result<Composition, String> {
    let json = std::fs::read_to_string(b.dir.join(PROJECT)).map_err(|e| e.to_string())?;
    let mut comp = Composition::from_json(&json).map_err(|e| e.to_string())?;
    comp.map_paths(|p| if Path::new(p).is_relative() { b.dir.join(p).to_string_lossy().into_owned() } else { p.to_string() });
    Ok(comp)
}

/// Extra shader folders carried by the show.
pub fn shader_dir(kind: &str) -> Option<PathBuf> {
    bundled().map(|b| b.dir.join(kind))
}
