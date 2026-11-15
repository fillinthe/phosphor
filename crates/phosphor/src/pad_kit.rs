//! The kit for whoever built the 3D world the audience flies through with the game pad (`PadSettings`): a Unity
//! script, an Unreal component and a page of steps, written into one folder to send them. Their project keeps its
//! lighting and look; the scripts only read Phosphor's pad OSC (`State::pad_send`) and move a camera.

use std::path::PathBuf;

const FILES: [(&str, &str); 4] = [
    ("README.md", include_str!("pad_kit/README.md")),
    ("Unity/PhosphorPad.cs", include_str!("pad_kit/PhosphorPad.cs")),
    ("Unreal/PhosphorPadComponent.h", include_str!("pad_kit/PhosphorPadComponent.h")),
    ("Unreal/PhosphorPadComponent.cpp", include_str!("pad_kit/PhosphorPadComponent.cpp")),
];

/// The kit's files with this show's OSC port in them: (path in the kit, text).
pub fn files(port: u16) -> Vec<(&'static str, String)> {
    FILES.iter().map(|(name, text)| (*name, text.replace("{PORT}", &port.to_string()))).collect()
}

/// Writes the kit into Documents/Phosphor Pad Kit (over an older copy) and returns the folder.
pub fn export(port: u16) -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).ok_or("no home folder")?;
    let dir = PathBuf::from(home).join("Documents").join("Phosphor Pad Kit");
    for (name, text) in files(port) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    /// Every file carries the show's port and no placeholder is left.
    #[test]
    fn the_kit_carries_the_port() {
        let files = super::files(9123);
        assert_eq!(files.len(), 4);
        for (name, text) in &files {
            assert!(!text.contains("{PORT}"), "{name}");
        }
        assert!(files.iter().any(|(n, t)| *n == "Unity/PhosphorPad.cs" && t.contains("public int port = 9123;")));
        assert!(files.iter().any(|(n, t)| n.ends_with(".h") && t.contains("int32 Port = 9123;")));
    }
}
