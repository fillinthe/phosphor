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
    // Settings newer than the templates use, so they're checked too: a credited file, live data, a light with its own
    // channels, a news-headlines text layer.
    comp.add_media("aurora.jpg", "/media/aurora.jpg");
    if let Some(m) = comp.media_bin.last_mut() {
        m.credit = Some(phosphor_engine::Credit { title: "Aurora".into(), author: "NASA".into(), licence: "Public domain".into(), source: "NASA".into(), ..Default::default() });
    }
    comp.live_data = phosphor_engine::LiveData { weather: true, place: "Reykjavik".into(), latitude: 64.1, longitude: -21.9, iss: true, quakes: false };
    let id = comp.new_id();
    comp.dmx.fixtures.push(phosphor_engine::Fixture {
        id,
        name: "Library spot".into(),
        shape: phosphor_engine::FixtureShape::MovingHead,
        head: phosphor_engine::HeadLayout::Custom,
        roles: vec![phosphor_engine::Role::Pan, phosphor_engine::Role::Dimmer, phosphor_engine::Role::Extra(0)],
        extra: vec![phosphor_engine::ExtraChannel { name: "Gobo".into(), value: 0.0 }],
        ..Default::default()
    });
    let style = phosphor_engine::TextStyle { live: phosphor_engine::LiveText::Headlines(8), live_source: "https://feeds.bbci.co.uk/news/world/rss.xml".into(), ..Default::default() };
    comp.add_layer().source = phosphor_engine::SourceSpec::Text(style);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/old_projects");
    let name = format!("{name}.phosphor");
    std::fs::write(dir.join(name), comp.to_json()).expect("written");
}
