//! MadMapper projects (.mad, Qt 4 data stream: `qt_stream`): outputs, surfaces and the media they show become
//! Phosphor outputs and slices. Quads keep their corners and their part of the picture; circles become ellipse
//! slices, triangles triangle slices, lines line slices, groups slice groups, mask surfaces masks. Each picture a
//! surface shows becomes a layer the slice shows on its own: movies and images by file, Syphon servers (VDMX,
//! Resolume) and cameras as live layers, MadMapper's own generators as the nearest built-in one.
//!
//! MadMapper's stage: one space for every output; an output shows the rectangle `position` ± `stageSize / 2` of
//! it (y grows downward, as in Qt). A surface's `positions` are its corners in that space; its `uvs` its part of
//! the picture, measured in picture heights across (a 16:9 picture spans -0.39..1.39), y downward too.

use phosphor_engine::mapping::{Line, Mask, SliceGroup, SliceShape, Surface};
use phosphor_engine::{Composition, SourceSpec};

use super::qt_stream::{Qv, read_file};

/// One MadMapper output: what part of the stage it shows and at what size.
#[derive(Clone, Debug, PartialEq)]
pub struct MadOutput {
    pub name: String,
    pub centre: [f64; 2],
    pub size: [f64; 2],
    /// Pixels (from the screen it was sent to), when known.
    pub pixels: Option<(u32, u32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MadKind {
    Quad,
    Circle,
    Triangle,
    Group,
    Mask,
    Lines,
    Fixture,
    Other(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct MadSurface {
    pub kind: MadKind,
    pub name: String,
    pub visible: bool,
    pub opacity: f64,
    /// Corners (a quad's, the square around a circle), a triangle's three points, a mask's outline.
    pub points: Vec<[f64; 2]>,
    /// Its part of the picture (four corners).
    pub uvs: Vec<[f64; 2]>,
    pub visual: i64,
    /// The group surface it's in (an index into `surfaces`).
    pub parent: Option<usize>,
    /// 1 = mirrored across, 2 = upside down, 3 = both.
    pub flip: u32,
    pub feather: f64,
    pub inverted: bool,
    /// A Lines surface's lines: points and width (pixels).
    pub lines: Vec<(Vec<[f64; 2]>, f64)>,
}

/// A picture a surface can show.
#[derive(Clone, Debug, PartialEq)]
pub struct MadVisual {
    pub id: i64,
    /// "Movie", "Image", "Syphon", "Live", "Madam" (generators), "" (materials).
    pub kind: String,
    pub url: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MadFile {
    pub name: String,
    pub outputs: Vec<MadOutput>,
    pub surfaces: Vec<MadSurface>,
    pub visuals: Vec<MadVisual>,
    /// Kept out (said in the import's message): cue banks, controller mappings.
    pub cues: usize,
    pub mappings: usize,
}

impl MadFile {
    /// Surfaces of each kind: (slices, groups, masks, lines, LED fixtures, others).
    pub fn counts(&self) -> [usize; 6] {
        let mut c = [0; 6];
        for s in &self.surfaces {
            c[match s.kind {
                MadKind::Quad | MadKind::Circle | MadKind::Triangle => 0,
                MadKind::Group => 1,
                MadKind::Mask => 2,
                MadKind::Lines => 3,
                MadKind::Fixture => 4,
                MadKind::Other(_) => 5,
            }] += 1;
        }
        c
    }

    /// The pictures surfaces show (each once).
    pub fn used_visuals(&self) -> Vec<&MadVisual> {
        let mut out: Vec<&MadVisual> = Vec::new();
        for s in &self.surfaces {
            if let Some(v) = self.visuals.iter().find(|v| v.id == s.visual)
                && !out.iter().any(|o| o.id == v.id)
                && !matches!(s.kind, MadKind::Group | MadKind::Mask | MadKind::Fixture)
            {
                out.push(v);
            }
        }
        out
    }
}

pub fn read(path: &str) -> Result<MadFile, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("can't read it ({e})"))?;
    let name = std::path::Path::new(path).file_stem().map_or_else(|| path.to_string(), |s| s.to_string_lossy().to_string());
    parse(&bytes, &name)
}

pub fn parse(bytes: &[u8], name: &str) -> Result<MadFile, String> {
    let top = read_file(bytes)?;
    let section = |key: &str| top.iter().find(|(k, _)| k == key).map(|(_, v)| v);
    let mut file = MadFile { name: name.to_string(), ..Default::default() };
    let num = |v: &Qv, key: &str, d: f64| v.get(key).and_then(Qv::num).unwrap_or(d);
    let text = |v: &Qv, key: &str| v.get(key).and_then(Qv::str).unwrap_or("").to_string();
    if let Some(outs) = section("outputs").and_then(|o| o.get("outputs")) {
        for o in outs.list() {
            let centre = o.get("position").map(Qv::nums).filter(|n| n.len() == 2).map_or([0.0, 0.0], |n| [n[0], n[1]]);
            let size = o.get("stageSize").map(Qv::nums).filter(|n| n.len() == 2).map_or([2.0, 2.0], |n| [n[0], n[1]]);
            let pixels = o.get("geometry").map(Qv::nums).filter(|n| n.len() == 4).map(|r| ((r[2] - r[0] + 1.0).max(16.0) as u32, (r[3] - r[1] + 1.0).max(16.0) as u32));
            file.outputs.push(MadOutput { name: text(o, "name"), centre, size, pixels });
        }
    }
    if let Some(v) = section("visuals").and_then(|v| v.get("visuals")) {
        for x in v.list() {
            file.visuals.push(MadVisual { id: num(x, "id", -1.0) as i64, kind: text(x, "type"), url: text(x, "url"), name: text(x, "name") });
        }
    }
    let relations: Vec<i64> = section("surfacesRelations").map(|r| r.list().iter().map(|x| x.num().unwrap_or(-1.0) as i64).collect()).unwrap_or_default();
    for (i, s) in section("surfaces").map(Qv::list).unwrap_or(&[]).iter().enumerate() {
        let kind = match s.get("type").and_then(Qv::str).unwrap_or("") {
            "quad" => MadKind::Quad,
            "circle" => MadKind::Circle,
            "triangle" => MadKind::Triangle,
            "group" => MadKind::Group,
            "mask" => MadKind::Mask,
            "lines" => MadKind::Lines,
            "fixture" => MadKind::Fixture,
            other => MadKind::Other(other.to_string()),
        };
        let lines = s
            .get("line")
            .map(Qv::list)
            .unwrap_or(&[])
            .iter()
            .filter(|l| l.get("visible").and_then(Qv::boolean).unwrap_or(true))
            .map(|l| (l.get("positions").map(Qv::points).unwrap_or(&[]).to_vec(), num(l, "width", 2.0)))
            .filter(|(p, _)| p.len() >= 2)
            .collect();
        file.surfaces.push(MadSurface {
            kind,
            name: text(s, "name"),
            visible: s.get("visible").and_then(Qv::boolean).unwrap_or(true),
            opacity: num(s, "opacity", 1.0),
            points: s.get("positions").map(Qv::points).unwrap_or(&[]).to_vec(),
            uvs: s.get("uvs").map(Qv::points).unwrap_or(&[]).to_vec(),
            visual: num(s, "visualId", -1.0) as i64,
            parent: relations.get(i).copied().filter(|p| *p >= 0).map(|p| p as usize),
            flip: num(s, "uvFlip", 0.0) as u32,
            feather: num(s, "feathering", 0.0),
            inverted: s.get("polygonInverted").and_then(Qv::boolean).unwrap_or(false),
            lines,
        });
    }
    file.mappings = section("mappings").map_or(0, |m| m.list().len());
    file.cues = section("cueBanksV4").and_then(|c| c.get("cueBanks")).map_or(0, |b| b.list().iter().map(|bank| bank.get("cues").map_or(0, |c| c.list().len())).sum());
    if file.outputs.is_empty() {
        // (No output saved: the stage's middle, 16:9.)
        file.outputs.push(MadOutput { name: "Output".into(), centre: [0.0, 0.0], size: [3.2, 1.8], pixels: None });
    }
    Ok(file)
}

/// What an import brought in, for its message.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Imported {
    pub outputs: usize,
    pub slices: usize,
    pub groups: usize,
    pub masks: usize,
    pub layers: usize,
    /// MadMapper generators drawn by a built-in one instead (their names).
    pub stand_ins: Vec<String>,
    /// LED fixtures, surfaces of other kinds, cues and controller mappings that didn't come across.
    pub left_out: usize,
    /// Slices saved at no opacity (faded out in MadMapper; its cues bring them up): they come in faded out too.
    pub faded: usize,
}

/// A MadMapper generator's nearest built-in generator.
fn generator_for(name: &str) -> &'static str {
    let n = name.to_lowercase();
    let has = |w: &str| n.contains(w);
    if has("solid") {
        "solid"
    } else if has("test") || has("color pattern") || has("colorpattern") || has("bars") {
        "color_bars"
    } else if has("grid") || has("checker") || has("square") || has("brick") {
        "checker"
    } else if has("gradient") {
        "gradient"
    } else if has("particle") {
        "particles"
    } else if has("spectrum") {
        "spectrum"
    } else if has("waveform") || has("wave synth") {
        "waveform"
    } else if has("cellular") || has("voronoi") || has("caustic") {
        "voronoi"
    } else if has("noise") || has("cloud") || has("dune") || has("simplex") {
        "noise"
    } else if has("line") || has("diagonal") || has("bar code") || has("barcode") || has("stripe") {
        "lines"
    } else if has("circle") || has("ring") || has("arc") || has("siren") {
        "rings"
    } else if has("tunnel") {
        "tunnel"
    } else if has("star") {
        "starfield"
    } else if has("spiral") {
        "spiral"
    } else {
        "plasma"
    }
}

/// The layer source for a picture a surface shows, and whether it stands in for something MadMapper drew itself.
fn source_for(v: &MadVisual) -> (SourceSpec, bool) {
    let url = v.url.trim();
    match v.kind.as_str() {
        "Movie" | "Image" if url.starts_with('/') => (crate::media::spec_for_path(url), false),
        "Syphon" => {
            // "/syphon/App/Server" → "App – Server" (how Phosphor names Syphon servers).
            let mut parts = url.trim_start_matches("/syphon/").splitn(2, '/');
            let app = parts.next().unwrap_or("");
            let label = match parts.next().filter(|s| !s.is_empty()) {
                Some(server) => format!("{app} – {server}"),
                None => app.to_string(),
            };
            (crate::media::live::syphon_source(&label), false)
        }
        "Live" => (crate::media::live::camera_source(&crate::media::live::Camera { name: v.name.clone() }), false),
        "Text" => (SourceSpec::Text(phosphor_engine::TextStyle { text: v.name.clone(), ..crate::text::default_style() }), false),
        _ => {
            // Madam generators and materials: "/madam/Solid Color/", "/materials/Clouds/0 Clouds".
            let name = url.trim_start_matches("/madam/").trim_start_matches("/materials/").split('/').next().filter(|s| !s.is_empty()).unwrap_or(&v.name);
            let id = generator_for(name);
            let generator = phosphor_engine::shaders::generator(id).map_or_else(|| SourceSpec::Generator("plasma".into()), |_| SourceSpec::Generator(id.into()));
            (generator, true)
        }
    }
}

/// Picture width ÷ height for a visual (a file's own; live pictures and generators 16:9).
pub type AspectOf<'a> = &'a dyn Fn(&MadVisual) -> f32;

/// Brings everything in `file` into `comp`: an output per MadMapper output (added after the ones there), its slices,
/// groups and masks, and a layer per picture shown (outside the mix: the slices show them).
pub fn add_to(comp: &mut Composition, file: &MadFile, aspect: AspectOf) -> Imported {
    let mut done = Imported::default();
    // A layer per picture a surface shows.
    let mut layers: Vec<(i64, u64, f32)> = Vec::new();
    for v in file.used_visuals() {
        let (source, stand_in) = source_for(v);
        let generator_values = match &source {
            SourceSpec::Generator(g) => phosphor_engine::shaders::generator(g).map(|d| d.defaults()).unwrap_or_default(),
            _ => Vec::new(),
        };
        if stand_in {
            done.stand_ins.push(v.name.clone());
        }
        if let SourceSpec::Movie { path: Some(path), .. } | SourceSpec::Image { path: Some(path), .. } = &source {
            let name = std::path::Path::new(path).file_name().map_or_else(|| v.name.clone(), |f| f.to_string_lossy().to_string());
            comp.add_media(&name, path);
        }
        let l = comp.add_layer();
        l.name = if v.name.is_empty() { format!("MadMapper {}", v.id) } else { v.name.clone() };
        l.source = source;
        l.generator_values = generator_values;
        l.in_mix = false;
        layers.push((v.id, l.id, aspect(v).max(0.01)));
        done.layers += 1;
    }
    // Which output each surface is on: the one its middle is inside (the nearest one otherwise).
    let output_of = |pts: &[[f64; 2]]| -> usize {
        if pts.is_empty() || file.outputs.len() < 2 {
            return 0;
        }
        let n = pts.len() as f64;
        let mid = [pts.iter().map(|p| p[0]).sum::<f64>() / n, pts.iter().map(|p| p[1]).sum::<f64>() / n];
        let far = |o: &MadOutput| ((mid[0] - o.centre[0]).abs() - o.size[0] / 2.0).max(0.0) + ((mid[1] - o.centre[1]).abs() - o.size[1] / 2.0).max(0.0);
        (0..file.outputs.len()).min_by(|a, b| far(&file.outputs[*a]).total_cmp(&far(&file.outputs[*b]))).unwrap_or(0)
    };
    let first_new = comp.outputs.len();
    for o in &file.outputs {
        let id = comp.add_output();
        let out = comp.output_mut(id).unwrap();
        out.name = if o.name.is_empty() { format!("MadMapper {}", done.outputs + 1) } else { o.name.clone() };
        if let Some((w, h)) = o.pixels {
            (out.width, out.height) = (w, h);
        }
        out.mapping.enabled = true;
        done.outputs += 1;
    }
    // Groups first (their slices go in by id), then slices and masks.
    let mut group_ids: Vec<Option<u64>> = vec![None; file.surfaces.len()];
    for (i, s) in file.surfaces.iter().enumerate() {
        if s.kind == MadKind::Group {
            group_ids[i] = Some(comp.new_id());
        }
    }
    let mut slices_of: Vec<Vec<u64>> = vec![Vec::new(); file.outputs.len()];
    for (i, s) in file.surfaces.iter().enumerate() {
        let points: Vec<[f64; 2]> = if s.kind == MadKind::Lines { s.lines.iter().flat_map(|l| l.0.clone()).collect() } else { s.points.clone() };
        let k = output_of(&points);
        let o = &file.outputs[k];
        let (left, top) = (o.centre[0] - o.size[0] / 2.0, o.centre[1] - o.size[1] / 2.0);
        let to_out = |p: &[f64; 2]| [((p[0] - left) / o.size[0]) as f32, ((p[1] - top) / o.size[1]) as f32];
        let layer = layers.iter().find(|(v, _, _)| *v == s.visual);
        let group = s.parent.and_then(|p| group_ids.get(p).copied().flatten());
        let mut slice = Surface {
            name: if s.name.is_empty() { format!("Surface {}", i + 1) } else { s.name.clone() },
            enabled: s.visible,
            opacity: s.opacity.clamp(0.0, 1.0) as f32,
            layer: layer.map(|l| l.1),
            feather: (s.feather as f32).clamp(0.0, phosphor_engine::mapping::SLICE_FEATHER_MAX),
            flip: [s.flip & 1 != 0, s.flip & 2 != 0],
            group,
            ..Default::default()
        };
        // Its part of the picture: the uvs' box, from picture heights to 0..1 across the picture.
        if let Some((_, _, a)) = layer
            && s.uvs.len() >= 3
        {
            let a = f64::from(*a);
            let xs = s.uvs.iter().map(|p| (p[0] - 0.5) / a + 0.5);
            let (x0, x1) = (xs.clone().fold(f64::MAX, f64::min), xs.fold(f64::MIN, f64::max));
            let (y0, y1) = (s.uvs.iter().map(|p| p[1]).fold(f64::MAX, f64::min), s.uvs.iter().map(|p| p[1]).fold(f64::MIN, f64::max));
            slice.input = [x0 as f32, y0 as f32, (x1 - x0).max(1e-4) as f32, (y1 - y0).max(1e-4) as f32];
        }
        match s.kind {
            MadKind::Quad | MadKind::Circle if s.points.len() == 4 => {
                slice.corners = [to_out(&s.points[0]), to_out(&s.points[1]), to_out(&s.points[2]), to_out(&s.points[3])];
                if s.kind == MadKind::Circle {
                    slice.shape = SliceShape::Ellipse;
                }
            }
            MadKind::Triangle if s.points.len() == 3 => {
                // Phosphor's triangle has its point at the top middle of the slice: a slice whose top edge passes
                // through the first point (parallel to the other two) and whose bottom corners are the other two.
                let [a, b, c] = [to_out(&s.points[0]), to_out(&s.points[1]), to_out(&s.points[2])];
                // (Bottom-left before bottom-right, seen with the point on top, so the picture isn't mirrored.)
                let cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
                let (bl, br) = if cross > 0.0 { (c, b) } else { (b, c) };
                let half = [(br[0] - bl[0]) / 2.0, (br[1] - bl[1]) / 2.0];
                slice.corners = [[a[0] - half[0], a[1] - half[1]], [a[0] + half[0], a[1] + half[1]], br, bl];
                slice.shape = SliceShape::Triangle;
            }
            MadKind::Lines => {
                // A line slice per line (its width from pixels to a share of the output's height).
                let px_h = o.pixels.map_or(1080.0, |p| p.1 as f32);
                for (n, (pts, width)) in s.lines.iter().enumerate() {
                    let id = comp.new_id();
                    let mut line = slice.clone();
                    line.id = id;
                    line.name = format!("{} · {}", slice.name, n + 1);
                    line.line = Some(Line { points: pts.iter().map(to_out).collect(), width: (*width as f32 / px_h).max(0.001) });
                    let mapping = &mut comp.output_mut(comp.outputs[first_new + k].id).unwrap().mapping;
                    mapping.surfaces.push(line);
                    slices_of[k].push(id);
                    done.slices += 1;
                }
                continue;
            }
            MadKind::Mask if s.points.len() >= 3 => {
                let id = comp.new_id();
                let mapping = &mut comp.output_mut(comp.outputs[first_new + k].id).unwrap().mapping;
                mapping.masks.push(Mask { id, name: slice.name.clone(), enabled: s.visible, points: s.points.iter().map(to_out).collect(), invert: s.inverted, group, ..Default::default() });
                done.masks += 1;
                continue;
            }
            MadKind::Group => {
                let id = group_ids[i].unwrap();
                let parent = s.parent.and_then(|p| group_ids.get(p).copied().flatten());
                // (A group goes on the output of its first slice; groups with none stay off.)
                if let Some(at) = file.surfaces.iter().position(|x| x.parent == Some(i) && !matches!(x.kind, MadKind::Group)) {
                    let pts = &file.surfaces[at].points;
                    let k = output_of(pts);
                    let mapping = &mut comp.output_mut(comp.outputs[first_new + k].id).unwrap().mapping;
                    mapping.groups.push(SliceGroup { id, name: slice.name.clone(), parent, enabled: s.visible, opacity: s.opacity.clamp(0.0, 1.0) as f32, ..Default::default() });
                    done.groups += 1;
                }
                continue;
            }
            _ => {
                done.left_out += 1;
                continue;
            }
        }
        slice.id = comp.new_id();
        if slice.opacity <= 0.0 {
            done.faded += 1;
        }
        slices_of[k].push(slice.id);
        let mapping = &mut comp.output_mut(comp.outputs[first_new + k].id).unwrap().mapping;
        mapping.surfaces.push(slice);
        done.slices += 1;
    }
    for k in first_new..comp.outputs.len() {
        comp.outputs[k].mapping.prune_groups();
    }
    done.left_out += file.cues + file.mappings;
    done
}

/// A new project from `file`: its canvas the first output's size.
pub fn to_composition(file: &MadFile, aspect: AspectOf) -> (Composition, Imported) {
    let mut comp = Composition::default();
    if let Some((w, h)) = file.outputs.first().and_then(|o| o.pixels) {
        (comp.width, comp.height) = (w, h);
    }
    let done = add_to(&mut comp, file, aspect);
    (comp, done)
}

/// Picture width ÷ height for a visual: a file's own (read from it), 16:9 for live pictures and generators.
#[cfg(not(target_arch = "wasm32"))]
pub fn file_aspect(v: &MadVisual) -> f32 {
    let url = v.url.trim();
    let own = match v.kind.as_str() {
        "Image" => image::image_dimensions(url).ok().map(|(w, h)| w as f32 / h.max(1) as f32),
        "Movie" => crate::convert::probe(url).ok().filter(|p| p.height > 0).map(|p| p.width as f32 / p.height as f32),
        _ => None,
    };
    own.unwrap_or(16.0 / 9.0)
}

#[cfg(test)]
mod tests {
    use super::super::qt_stream::write as w;
    use super::*;

    /// A made-up project in MadMapper's layout: one 16:9 output showing the stage rectangle centred at (1, -1),
    /// 3.2 × 1.8; a full-screen quad showing a Syphon picture, a circle in a group showing a movie, a triangle, a
    /// two-line Lines surface, an inverted mask and an LED fixture.
    fn sample() -> Vec<u8> {
        let surface = |o: &mut Vec<u8>, kind: &str, name: &str, visual: i32, points: &[[f64; 2]], uvs: &[[f64; 2]], extra: u32, more: &dyn Fn(&mut Vec<u8>)| {
            w::map(o, 7 + extra, |o| {
                w::key(o, "type");
                w::str_v(o, kind);
                w::key(o, "name");
                w::str_v(o, name);
                w::key(o, "visible");
                w::boolean(o, true);
                w::key(o, "opacity");
                w::num(o, 0.8);
                w::key(o, "visualId");
                w::int(o, visual);
                w::key(o, "positions");
                w::points(o, points);
                w::key(o, "uvs");
                w::points(o, uvs);
                more(o);
            });
        };
        let full_uv = [[-7.0 / 18.0, 0.0], [25.0 / 18.0, 0.0], [25.0 / 18.0, 1.0], [-7.0 / 18.0, 1.0]];
        w::file(|o| {
            w::key(o, "visuals");
            w::map(o, 1, |o| {
                w::key(o, "visuals");
                w::list(o, 3, |o| {
                    for (id, kind, url, name) in [(1, "Syphon", "/syphon/VDMX5/vid1", "VDMX5 - vid1"), (2, "Movie", "/footage/loop.mov", "loop.mov"), (3, "Madam", "/madam/Solid Color/", "Solid Color")] {
                        w::map(o, 4, |o| {
                            w::key(o, "id");
                            w::int(o, id);
                            w::key(o, "type");
                            w::str_v(o, kind);
                            w::key(o, "url");
                            w::str_v(o, url);
                            w::key(o, "name");
                            w::str_v(o, name);
                        });
                    }
                });
            });
            w::key(o, "surfacesRelations");
            w::list(o, 7, |o| {
                for p in [-1, -1, 1, -1, -1, -1, -1] {
                    w::int(o, p);
                }
            });
            w::key(o, "surfaces");
            w::list(o, 7, |o| {
                surface(o, "quad", "Screen", 1, &[[-0.6, -1.9], [2.6, -1.9], [2.6, -0.1], [-0.6, -0.1]], &full_uv, 1, &|o| {
                    w::key(o, "uvFlip");
                    w::int(o, 2);
                });
                surface(o, "group", "Circles", 0, &[], &[], 0, &|_| {});
                surface(o, "circle", "Moon", 2, &[[0.6, -1.4], [1.4, -1.4], [1.4, -0.6], [0.6, -0.6]], &[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], 0, &|_| {});
                surface(o, "triangle", "Tri", 3, &[[1.0, -1.9], [1.8, -0.1], [0.2, -0.1]], &full_uv, 0, &|_| {});
                surface(o, "lines", "Edges", 3, &[], &[], 1, &|o| {
                    w::key(o, "line");
                    w::list(o, 2, |o| {
                        for pts in [[[-0.6, -1.0], [2.6, -1.0]], [[1.0, -1.9], [1.0, -0.1]]] {
                            w::map(o, 2, |o| {
                                w::key(o, "positions");
                                w::points(o, &pts);
                                w::key(o, "width");
                                w::num(o, 10.8);
                            });
                        }
                    });
                });
                surface(o, "mask", "Cut", 0, &[[0.0, -1.0], [1.0, -1.0], [1.0, -0.5]], &[], 1, &|o| {
                    w::key(o, "polygonInverted");
                    w::boolean(o, true);
                });
                surface(o, "fixture", "Pixel", 3, &[], &[], 0, &|_| {});
            });
            w::key(o, "outputs");
            w::map(o, 1, |o| {
                w::key(o, "outputs");
                w::list(o, 1, |o| {
                    w::map(o, 4, |o| {
                        w::key(o, "name");
                        w::str_v(o, "Projector-1");
                        w::key(o, "position");
                        w::point(o, 1.0, -1.0);
                        w::key(o, "stageSize");
                        w::size(o, 3.2, 1.8);
                        w::key(o, "geometry");
                        w::rect(o, [1920, 0, 3839, 1079]);
                    });
                });
            });
        })
    }

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4
    }

    #[test]
    fn a_madmapper_project_becomes_outputs_slices_and_layers() {
        let file = parse(&sample(), "Gig").unwrap();
        assert_eq!(file.outputs, vec![MadOutput { name: "Projector-1".into(), centre: [1.0, -1.0], size: [3.2, 1.8], pixels: Some((1920, 1080)) }]);
        assert_eq!(file.counts(), [3, 1, 1, 1, 1, 0]);
        let movie_aspect = |v: &MadVisual| if v.kind == "Movie" { 1.0 } else { 16.0 / 9.0 };
        let (comp, done) = to_composition(&file, &movie_aspect);
        assert_eq!((comp.width, comp.height), (1920, 1080));
        assert_eq!((done.outputs, done.slices, done.groups, done.masks, done.layers), (1, 5, 1, 1, 3));
        assert_eq!(done.stand_ins, vec!["Solid Color".to_string()]);
        assert_eq!(done.left_out, 1, "the LED fixture");
        assert_eq!(done.faded, 0);
        let out = &comp.outputs[0];
        assert_eq!((out.name.as_str(), out.width, out.height), ("Projector-1", 1920, 1080));
        let slice = |name: &str| out.mapping.surfaces.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("{name}"));
        let layer_named = |name: &str| comp.layers.iter().find(|l| l.name == name).unwrap();
        // The full-screen quad: the whole output, the whole Syphon picture, upside down.
        let screen = slice("Screen");
        assert!(close(screen.corners[0], [0.0, 0.0]) && close(screen.corners[2], [1.0, 1.0]), "{:?}", screen.corners);
        assert!(screen.input.iter().zip([0.0, 0.0, 1.0, 1.0]).all(|(a, b)| (a - b).abs() < 1e-4), "{:?}", screen.input);
        assert_eq!(screen.flip, [false, true]);
        assert!((screen.opacity - 0.8).abs() < 1e-6);
        let vdmx = layer_named("VDMX5 - vid1");
        assert_eq!(screen.layer, Some(vdmx.id));
        assert_eq!(vdmx.source, crate::media::live::syphon_source("VDMX5 – vid1"));
        assert!(!vdmx.in_mix, "slices show it; the mix doesn't");
        // The circle: an ellipse in the Circles group, showing the movie's middle square (the movie is square here).
        let moon = slice("Moon");
        assert_eq!(moon.shape, SliceShape::Ellipse);
        assert!(close(moon.corners[0], [0.375, 0.5 / 1.8]), "{:?}", moon.corners);
        assert_eq!(moon.group, out.mapping.groups.iter().find(|g| g.name == "Circles").map(|g| g.id));
        assert_eq!(comp.media_bin.len(), 1);
        // The triangle: its point at the top middle, its base the bottom corners.
        let tri = slice("Tri");
        assert_eq!(tri.shape, SliceShape::Triangle);
        let top_mid = [(tri.corners[0][0] + tri.corners[1][0]) / 2.0, (tri.corners[0][1] + tri.corners[1][1]) / 2.0];
        assert!(close(top_mid, [0.5, 0.0]) && close(tri.corners[3], [0.25, 1.0]) && close(tri.corners[2], [0.75, 1.0]), "{:?}", tri.corners);
        assert_eq!(layer_named("Solid Color").source, SourceSpec::Generator("solid".into()));
        // Lines: one line slice per line, 10.8 px of 1080 = 1% of the height.
        let edge = slice("Edges · 1");
        let line = edge.line.as_ref().unwrap();
        assert!(close(line.points[0], [0.0, 0.5]) && close(line.points[1], [1.0, 0.5]) && (line.width - 0.01).abs() < 1e-5);
        assert!(slice("Edges · 2").line.is_some());
        let mask = &out.mapping.masks[0];
        assert!(mask.invert && mask.points.len() == 3);
        // Old-project check: the result saves and opens.
        let back = Composition::from_json(&comp.to_json()).unwrap();
        assert_eq!(back.outputs[0].mapping.surfaces.len(), 5);
    }

    /// Real MadMapper projects in the folders listed in $PHOSPHOR_MAD_DIRS (separated by ':'): each reads, and becomes
    /// slices.
    #[test]
    fn reads_real_madmapper_projects() {
        let Some(dirs) = std::env::var_os("PHOSPHOR_MAD_DIRS") else { return };
        let mut seen = 0;
        for dir in std::env::split_paths(&dirs) {
            let Ok(entries) = std::fs::read_dir(dir) else { continue };
            for e in entries.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "mad")) {
                let path = e.path();
                let f = read(&path.to_string_lossy()).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
                let (comp, done) = to_composition(&f, &|_| 16.0 / 9.0);
                assert!(done.slices > 0 || f.counts()[0] == 0, "{}", path.display());
                assert!(!comp.outputs.is_empty());
                seen += 1;
            }
        }
        if seen > 0 {
            println!("{seen} MadMapper projects read");
        }
    }
}

/// Converts $PHOSPHOR_MAD_IN to a Phosphor project at $PHOSPHOR_MAD_OUT (for checking by hand).
#[cfg(test)]
#[test]
fn convert_sample() {
    let (Some(input), Some(out)) = (std::env::var_os("PHOSPHOR_MAD_IN"), std::env::var_os("PHOSPHOR_MAD_OUT")) else { return };
    let f = read(&input.to_string_lossy()).unwrap();
    let (comp, done) = to_composition(&f, &file_aspect);
    println!("MAD {done:?} counts {:?} visuals {}", f.counts(), f.used_visuals().len());
    std::fs::write(out, comp.to_json()).unwrap();
}
