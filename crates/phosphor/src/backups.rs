//! Project backups and autosave: every save first copies the file that's there into "<name> Backups" next to it
//! (the newest `KEEP` stay), and a project that has a file can save itself every few minutes when it changed.

use std::path::{Path, PathBuf};

/// Backups kept per project.
pub const KEEP: usize = 10;

/// The folder a project's backups go in: "Show Backups" next to "Show.phosphor".
pub fn folder(project: &Path) -> PathBuf {
    let stem = project.file_stem().map_or_else(|| "Project".into(), |s| s.to_string_lossy().into_owned());
    project.with_file_name(format!("{stem} Backups"))
}

/// Copies the file at `project` (if any) into its backups folder, named by the time (UTC), and drops the oldest
/// beyond `KEEP`. Returns the copy's path.
pub fn back_up(project: &Path, unix_secs: u64) -> std::io::Result<Option<PathBuf>> {
    if !project.exists() {
        return Ok(None);
    }
    let dir = folder(project);
    std::fs::create_dir_all(&dir)?;
    let stem = project.file_stem().map_or_else(|| "Project".into(), |s| s.to_string_lossy().into_owned());
    let ext = project.extension().map_or_else(|| "phosphor".into(), |e| e.to_string_lossy().into_owned());
    let copy = dir.join(format!("{stem} {}.{ext}", stamp(unix_secs)));
    std::fs::copy(project, &copy)?;
    // Oldest first by name (the stamp sorts by time).
    let mut all: Vec<PathBuf> = std::fs::read_dir(&dir)?.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e.to_string_lossy() == ext)).collect();
    all.sort();
    let extra = all.len().saturating_sub(KEEP);
    for old in &all[..extra] {
        let _ = std::fs::remove_file(old);
    }
    Ok(Some(copy))
}

/// "2026-10-22 14-05-33" (UTC) from Unix seconds: sorts by time, and is safe in a file name.
pub fn stamp(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    let secs = unix_secs % 86_400;
    // Days since 1970-01-01 → civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}-{:02}-{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_are_dates() {
        assert_eq!(stamp(0), "1970-01-01 00-00-00");
        assert_eq!(stamp(1_792_677_933), "2026-10-22 14-05-33");
        assert_eq!(stamp(951_782_400), "2000-02-29 00-00-00");
    }

    #[test]
    fn saves_keep_the_last_ten_versions() {
        let dir = std::env::temp_dir().join(format!("phosphor-backups-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let project = dir.join("Show.phosphor");
        assert_eq!(back_up(&project, 1).unwrap(), None, "nothing to back up yet");
        for k in 0..13u64 {
            std::fs::write(&project, format!("version {k}")).unwrap();
            back_up(&project, 1_792_677_933 + k).unwrap();
        }
        let mut kept: Vec<String> = std::fs::read_dir(folder(&project)).unwrap().flatten().map(|e| std::fs::read_to_string(e.path()).unwrap()).collect();
        kept.sort();
        assert_eq!(kept.len(), KEEP);
        assert!(!kept.contains(&"version 0".to_string()) && kept.contains(&"version 12".to_string()), "{kept:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
