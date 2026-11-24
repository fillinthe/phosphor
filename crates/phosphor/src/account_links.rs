//! Which project file on this computer has a copy in the account: file path → the account project's id, kept next to
//! the settings. A project opened from its file knows its account copy again, so Save keeps that copy up to date
//! instead of making a second one.

use std::collections::BTreeMap;

fn path() -> Option<std::path::PathBuf> {
    crate::show::storage_dir().map(|d| d.join("account-links.json"))
}

fn read(text: &str) -> BTreeMap<String, String> {
    serde_json::from_str(text).unwrap_or_default()
}

/// The account id `file` is linked to in the saved links `text`.
fn linked(text: &str, file: &str) -> Option<String> {
    read(text).remove(file).filter(|id| !id.is_empty())
}

/// The saved links with `file` linked to `id`.
fn with_link(text: &str, file: &str, id: &str) -> String {
    let mut all = read(text);
    all.insert(file.to_string(), id.to_string());
    serde_json::to_string(&all).unwrap_or_default()
}

/// The account project this file is a copy of, if it was ever saved there from this computer.
pub fn get(file: &str) -> Option<String> {
    linked(&std::fs::read_to_string(path()?).ok()?, file)
}

/// Remembers that `file` has the account copy `id`.
pub fn set(file: &str, id: &str) {
    let Some(p) = path() else { return };
    let text = std::fs::read_to_string(&p).unwrap_or_default();
    if linked(&text, file).as_deref() != Some(id) {
        let _ = crate::project_files::write_whole(&p, with_link(&text, file, id));
    }
}

#[cfg(test)]
mod tests {
    use super::{linked, with_link};

    #[test]
    fn a_file_remembers_its_account_copy() {
        assert_eq!(linked("", "/a/show.phosphor"), None);
        let one = with_link("", "/a/show.phosphor", "p1");
        let two = with_link(&one, "/b/other.phosphor", "p2");
        assert_eq!(linked(&two, "/a/show.phosphor").as_deref(), Some("p1"));
        assert_eq!(linked(&two, "/b/other.phosphor").as_deref(), Some("p2"));
        assert_eq!(linked(&two, "/c/none.phosphor"), None);
        // Saved as new: the file follows the new copy. A damaged links file reads as no links.
        assert_eq!(linked(&with_link(&two, "/a/show.phosphor", "p9"), "/a/show.phosphor").as_deref(), Some("p9"));
        assert_eq!(linked("not json", "/a/show.phosphor"), None);
    }
}
