//! Image sequences: a folder of numbered pictures (frame_0001.png, frame_0002.png…, as rendered from After Effects,
//! Blender or TouchDesigner) plays as one movie clip. Its path is the FFmpeg pattern ("…/frame_%04d.png");
//! `video::open_input` gives FFmpeg the first frame's number and the frame rate.

/// Fewer numbered pictures than this stay separate pictures.
pub const MIN_FRAMES: usize = 10;
/// Frames per second a sequence plays at.
pub const RATE: &str = "30";

/// Is this path (or file name) an image-sequence pattern?
pub fn is_sequence(path: &str) -> bool {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    file.find("%0").is_some_and(|i| file[i + 2..].trim_start_matches(|c: char| c.is_ascii_digit()).starts_with('d'))
}

/// A sequence's label: its name without the number pattern ("frame_%04d.png" → "frame").
pub fn label(path: &str) -> String {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let stem = file.split("%0").next().unwrap_or(file).trim_end_matches(['_', '-', '.', ' ']);
    if stem.is_empty() { "Sequence".into() } else { stem.to_string() }
}

/// Splits "name0012.png" into ("name", "0012", "png").
fn split(name: &str) -> Option<(&str, &str, &str)> {
    let (stem, ext) = name.rsplit_once('.')?;
    let digits = stem.len() - stem.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    (digits > 0).then(|| (&stem[..stem.len() - digits], &stem[stem.len() - digits..], ext))
}

/// The numbered pictures among `names` (one folder's files) that make sequences: (pattern file name, members).
/// Only picture files, at least `MIN_FRAMES` with the same prefix, digit count and extension.
pub fn find(names: &[String], is_picture: impl Fn(&str) -> bool) -> Vec<(String, Vec<String>)> {
    let mut groups: std::collections::BTreeMap<(String, usize, String), Vec<String>> = Default::default();
    // "._frame_0001.png": metadata macOS writes beside files on FAT/exFAT drives, not frames.
    for n in names.iter().filter(|n| !n.starts_with('.') && is_picture(n)) {
        if let Some((prefix, digits, ext)) = split(n) {
            groups.entry((prefix.to_string(), digits.len(), ext.to_string())).or_default().push(n.clone());
        }
    }
    groups
        .into_iter()
        .filter(|(_, m)| m.len() >= MIN_FRAMES)
        .map(|((prefix, width, ext), members)| (format!("{prefix}%0{width}d.{ext}"), members))
        .collect()
}

/// One folder's listing: takes out the numbered pictures that make sequences and returns one (pattern file name,
/// pattern path) per sequence, so a folder of 300 frames comes in as one clip.
pub fn take_sequences(dir: &std::path::Path, entries: &mut Vec<std::path::PathBuf>) -> Vec<(String, String)> {
    let names: Vec<String> = entries.iter().filter(|p| p.is_file()).filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())).collect();
    let found = find(&names, super::is_picture);
    let members: std::collections::HashSet<&str> = found.iter().flat_map(|(_, m)| m.iter().map(String::as_str)).collect();
    entries.retain(|p| p.file_name().is_none_or(|n| !members.contains(n.to_string_lossy().as_ref())));
    found.iter().map(|(pattern, _)| (pattern.clone(), dir.join(pattern).to_string_lossy().into_owned())).collect()
}

/// The frame files of a pattern path, in order (empty if none are there).
pub fn members(pattern: &str) -> Vec<std::path::PathBuf> {
    let path = std::path::Path::new(pattern);
    let (Some(dir), Some(file)) = (path.parent(), path.file_name().and_then(|f| f.to_str())) else { return Vec::new() };
    let Some((prefix, rest)) = file.split_once("%0") else { return Vec::new() };
    let Some((width, ext)) = rest.split_once("d.") else { return Vec::new() };
    let width: usize = width.parse().unwrap_or(0);
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<std::path::PathBuf> = read
        .flatten()
        .filter(|e| split(&e.file_name().to_string_lossy()).is_some_and(|(p, d, x)| p == prefix && d.len() == width && x == ext))
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

/// Is this media path there? (An image sequence is there when any of its frames is.)
pub fn exists(path: &str) -> bool {
    if is_sequence(path) { !members(path).is_empty() } else { std::path::Path::new(path).exists() }
}

/// The lowest frame number on disk for a pattern path (FFmpeg only looks for frames 0–4 by itself).
pub fn first_number(pattern: &str) -> Option<u32> {
    let path = std::path::Path::new(pattern);
    let dir = path.parent()?;
    let file = path.file_name()?.to_str()?;
    let (prefix, rest) = file.split_once("%0")?;
    let (width, ext) = rest.split_once("d.")?;
    let width: usize = width.parse().ok()?;
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            let (p, digits, x) = split(&n)?;
            (p == prefix && digits.len() == width && x == ext).then(|| digits.parse::<u32>().ok()).flatten()
        })
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbered_pictures_become_one_sequence() {
        let mut names: Vec<String> = (1001..1020).map(|i| format!("shot_{i}.png")).collect();
        names.push("cover.png".into());
        names.push("notes_1.txt".into());
        names.extend((1..4).map(|i| format!("other{i}.jpg")));
        names.extend((1001..1020).map(|i| format!("._shot_{i}.png")));
        let found = find(&names, |n| n.ends_with(".png") || n.ends_with(".jpg"));
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].0, "shot_%04d.png");
        assert_eq!(found[0].1.len(), 19);
        assert!(is_sequence("/a/b/shot_%04d.png") && !is_sequence("/a/100%.png"));
        assert_eq!(label("/a/b/shot_%04d.png"), "shot");
    }

    #[test]
    fn finds_where_the_numbers_start() {
        let dir = std::env::temp_dir().join(format!("phosphor-seq-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for i in [1001, 1002, 1003] {
            std::fs::write(dir.join(format!("f_{i}.png")), b"").unwrap();
        }
        std::fs::write(dir.join("f_12.png"), b"").unwrap();
        let pattern = dir.join("f_%04d.png").to_string_lossy().into_owned();
        assert_eq!(first_number(&pattern), Some(1001));
        assert_eq!(members(&pattern).len(), 3, "f_12.png has other digits");
        std::fs::remove_dir_all(&dir).ok();
    }
}
