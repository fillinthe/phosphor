//! Bringing other VJ apps' projects into Phosphor: Resolume Arena / Avenue compositions, VDMX projects, MadMapper
//! projects.

mod keyed;
pub mod looks;
pub mod madmapper;
mod qt_stream;
pub mod resolume;
pub mod resolume_output;
pub mod vdmx;

/// The most layers and clip columns read from another app's file (a damaged file's wild numbers stop here).
pub(crate) const MAX_LAYERS: usize = 256;
pub(crate) const MAX_COLUMNS: usize = 4096;

/// A canvas or output side read from another app's file, kept to a size a graphics card can make.
pub(crate) fn side(v: impl Into<f64>) -> u32 {
    let v: f64 = v.into();
    if v.is_finite() { v.clamp(16.0, 16_384.0) as u32 } else { 16 }
}

#[cfg(test)]
mod limit_tests {
    #[test]
    fn sizes_from_other_apps_stay_sane() {
        assert_eq!(super::side(1920.0), 1920);
        assert_eq!((super::side(1e12), super::side(-4.0), super::side(f64::NAN), super::side(f64::INFINITY)), (16_384, 16, 16, 16));
    }

    #[test]
    fn a_damaged_resolume_file_doesnt_ask_for_billions_of_clips() {
        let xml = r#"<Composition><Deck><Clip layerIndex="18446744073709551615" columnIndex="0"><PreloadData><VideoFile value="/a.mov"/></PreloadData></Clip><Clip layerIndex="0" columnIndex="4000000000"><PreloadData><VideoFile value="/b.mov"/></PreloadData></Clip><Clip layerIndex="1" columnIndex="2"><PreloadData><VideoFile value="/c.mov"/></PreloadData></Clip></Deck></Composition>"#;
        let Ok(file) = super::resolume::parse(xml) else { return };
        let (comp, _) = super::resolume::to_composition(&file, &[0]);
        assert!(comp.layers.len() <= 2, "{}", comp.layers.len());
        assert!(comp.layers.iter().all(|l| l.clips.len() < 16));
    }
}
