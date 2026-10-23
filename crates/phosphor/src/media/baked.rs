//! Files Phosphor draws itself (laser shows, Lottie animations) play as movies: drawn once into a Hap Q Alpha movie
//! (smooth gradients, transparency), which is kept and reused while the file is unchanged, then plays like any other
//! clip (loop, speed, scrub, cues).

/// What a file draws: its size, frame rate and every frame (straight-alpha RGBA, `size`).
pub struct Drawing {
    pub size: (u32, u32),
    pub fps: u32,
    pub frames: Box<dyn Iterator<Item = Vec<u8>>>,
}

/// The movie `path` plays as (kept under `folder` in Phosphor's storage), made by `draw` from the file's bytes the
/// first time. Call off the UI thread.
pub fn cached_movie(path: &str, folder: &str, draw: impl FnOnce(&[u8]) -> Result<Drawing, String>) -> Result<String, String> {
    use std::hash::{Hash, Hasher};
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let meta = std::fs::metadata(path).map_err(|e| format!("{name}: {e}"))?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (path, meta.len(), meta.modified().ok()).hash(&mut h);
    let dir = crate::show::storage_dir().unwrap_or_else(std::env::temp_dir).join(folder);
    let movie = dir.join(format!("{:016x}.mov", h.finish()));
    let movie_path = movie.to_string_lossy().to_string();
    if movie.exists() {
        return Ok(movie_path);
    }
    let d = draw(&std::fs::read(path).map_err(|e| format!("{name}: {e}"))?).map_err(|e| format!("{name}: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // Written under another name and renamed when whole, so a half-made movie is never reused.
    let part = dir.join(format!("{:016x}.part.mov", h.finish()));
    let part_path = part.to_string_lossy().to_string();
    use super::video::hap_encode::{HapKind, HapWriter};
    let mut writer = HapWriter::create(&part_path, d.size.0, d.size.1, d.fps, HapKind::HapQAlpha)?;
    for f in d.frames {
        writer.write(&f)?;
    }
    writer.finish()?;
    std::fs::rename(&part, &movie).map_err(|e| e.to_string())?;
    Ok(movie_path)
}
