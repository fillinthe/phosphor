//! Saved projects keep opening: every project file version ever committed (tests/old_projects, named by the date it
//! first appeared) must load in this version, and keep everything through a save and a reload. New fields need a
//! default and old files a `migrate` step: this test fails otherwise.
//!
//! `PHOSPHOR_WRITE_FIXTURE=<date>_templates_<app version> cargo test -p phosphor-engine --test old_projects` adds a
//! project saved by this version (every template), so later versions are checked against it too. Do it each release.

use phosphor_engine::{Composition, TEMPLATES};

fn fixtures() -> Vec<(String, String)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/old_projects");
    let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
        .expect("tests/old_projects")
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "phosphor"))
        .map(|e| (e.file_name().to_string_lossy().to_string(), std::fs::read_to_string(e.path()).expect("readable")))
        .collect();
    out.sort();
    out
}

#[test]
fn every_old_project_still_opens() {
    let all = fixtures();
    assert!(all.len() >= 12, "the fixtures are there");
    for (name, json) in all {
        let comp = Composition::from_json(&json).unwrap_or_else(|e| panic!("{name} no longer opens: {e}"));
        let again = Composition::from_json(&comp.to_json()).unwrap_or_else(|e| panic!("{name} saved by this version doesn't reopen: {e}"));
        assert_eq!(comp, again, "{name}: something changed through a save and reload");
        assert!(!comp.made_by_newer(), "{name}");
    }
}

#[test]
fn write_this_versions_project() {
    let Ok(name) = std::env::var("PHOSPHOR_WRITE_FIXTURE") else { return };
    // Every template's layers, outputs and settings in one project.
    let mut comp = Composition::default();
    for (_, _, make) in TEMPLATES {
        comp.merge(make());
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/old_projects");
    let name = format!("{name}.phosphor");
    std::fs::write(dir.join(name), comp.to_json()).expect("written");
}
