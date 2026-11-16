//! The mapping as SVG or PDF: `export` / `export_pdf` draw every slice and mask of an output (for the venue's tech
//! team, a laser cutter, or another app), `import` reads shapes back as polygons (0..1 of the picture) to become masks.
use phosphor_engine::Mapping;

/// What a drawing of the mapping shows (all 0..1 of the picture).
struct Shapes {
    /// Every slice that's on: id, name, outline, where its name goes (the middle of its corners).
    slices: Vec<(u64, String, Vec<[f32; 2]>, [f32; 2])>,
    /// Every mask: id, name, outline.
    masks: Vec<(u64, String, Vec<[f32; 2]>)>,
}

fn shapes(mapping: &Mapping) -> Shapes {
    let centre = |pts: Vec<[f32; 2]>| {
        let n = pts.len().max(1) as f32;
        [pts.iter().map(|p| p[0]).sum::<f32>() / n, pts.iter().map(|p| p[1]).sum::<f32>() / n]
    };
    Shapes {
        slices: mapping.surfaces.iter().filter(|s| s.enabled).map(|s| (s.id, s.name.clone(), s.outline(16), centre(s.outline(1)))).collect(),
        masks: mapping.placed_masks().into_iter().map(|m| (m.id, m.name.clone(), m.outline())).collect(),
    }
}

/// A PDF of `mapping` at `w` × `h` points (one per output pixel): black page, masks filled gray, slices outlined
/// with their names. Vector, so it prints and scales sharp.
pub fn export_pdf(mapping: &Mapping, w: u32, h: u32) -> Vec<u8> {
    let (fw, fh) = (w.max(1) as f32, h.max(1) as f32);
    let sh = shapes(mapping);
    // PDF counts y up from the bottom.
    let at = |p: [f32; 2]| format!("{:.2} {:.2}", p[0] * fw, (1.0 - p[1]) * fh);
    let path = |pts: &[[f32; 2]]| {
        let mut d = String::new();
        for (i, p) in pts.iter().enumerate() {
            d += &format!("{} {} ", at(*p), if i == 0 { "m" } else { "l" });
        }
        d + "h"
    };
    // Helvetica speaks Latin-1: other letters become '?'; brackets and backslashes are escaped.
    let text = |s: &str| s.chars().map(|c| if c.is_ascii() && !c.is_ascii_control() { c } else { '?' }).collect::<String>().replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
    let mut c = format!("0 0 0 rg 0 0 {fw} {fh} re f\n0.5 0.5 0.5 rg\n");
    for (_, _, pts) in &sh.masks {
        c += &format!("{} f\n", path(pts));
    }
    c += "0.47 1 0.67 RG 2 w 1 j\n";
    for (_, _, pts, _) in &sh.slices {
        c += &format!("{} S\n", path(pts));
    }
    c += "1 1 1 rg\n";
    for (_, name, _, mid) in &sh.slices {
        // Centered by eye: Helvetica letters are about half as wide as they are tall.
        let size = 14.0;
        let width = name.chars().count() as f32 * size * 0.5;
        c += &format!("BT /F1 {size} Tf {:.2} {:.2} Td ({}) Tj ET\n", mid[0] * fw - width / 2.0, (1.0 - mid[1]) * fh - size / 3.0, text(name));
    }
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {fw} {fh}] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>"),
        format!("<< /Length {} >>\nstream\n{c}endstream", c.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_string(),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for o in offsets {
        out.extend(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    out
}

/// An SVG drawing of `mapping` at `w` × `h`: slices as outlined shapes with their names, masks filled gray.
pub fn export(mapping: &Mapping, w: u32, h: u32) -> String {
    let (fw, fh) = (w as f32, h as f32);
    let path = |pts: &[[f32; 2]]| {
        let mut d = String::new();
        for (i, p) in pts.iter().enumerate() {
            d += &format!("{}{:.2},{:.2} ", if i == 0 { "M" } else { "L" }, p[0] * fw, p[1] * fh);
        }
        d + "Z"
    };
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n<rect id=\"background\" width=\"{w}\" height=\"{h}\" fill=\"black\"/>\n<g id=\"slices\" fill=\"none\" stroke=\"#78ffaa\" stroke-width=\"2\">\n"
    );
    let sh = shapes(mapping);
    for (id, name, pts, _) in &sh.slices {
        out += &format!("<path id=\"slice-{id}\" d=\"{}\"><title>{}</title></path>\n", path(pts), esc(name));
    }
    out += "</g>\n<g id=\"masks\" fill=\"#808080\" fill-opacity=\"0.6\">\n";
    for (id, name, pts) in &sh.masks {
        out += &format!("<path id=\"mask-{id}\" d=\"{}\"><title>{}</title></path>\n", path(pts), esc(name));
    }
    out += "</g>\n<g id=\"names\" fill=\"white\" font-family=\"sans-serif\" font-size=\"14\" text-anchor=\"middle\">\n";
    for (_, name, _, mid) in &sh.slices {
        out += &format!("<text x=\"{:.1}\" y=\"{:.1}\">{}</text>\n", mid[0] * fw, mid[1] * fh, esc(name));
    }
    out + "</g>\n</svg>\n"
}

/// What an SVG holds: closed shapes (for masks) and open lines (for line slices), in 0..1 of the drawing.
#[derive(Debug, Default)]
pub struct Drawing {
    pub shapes: Vec<Vec<[f32; 2]>>,
    pub lines: Vec<Vec<[f32; 2]>>,
}

/// Every closed shape (polygon, rect, closed path) and open line (polyline, line, open path) in an SVG; curves follow
/// their end points. In 0..1 of the drawing's size (its viewBox, else width × height).
pub fn import(text: &str) -> Result<Drawing, String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| e.to_string())?;
    let root = doc.root_element();
    let nums = |s: &str| s.split(|c: char| c == ',' || c.is_whitespace()).filter(|t| !t.is_empty()).filter_map(|t| t.parse::<f32>().ok()).collect::<Vec<_>>();
    let size = root
        .attribute("viewBox")
        .map(nums)
        .filter(|v| v.len() == 4 && v[2] > 0.0 && v[3] > 0.0)
        .map(|v| (v[0], v[1], v[2], v[3]))
        .or_else(|| {
            let len = |a: &str| root.attribute(a).and_then(|v| v.trim_end_matches("px").parse::<f32>().ok());
            Some((0.0, 0.0, len("width")?, len("height")?))
        })
        .ok_or("the drawing has no size (viewBox or width / height)")?;
    let unit = |x: f32, y: f32| [((x - size.0) / size.2).clamp(0.0, 1.0), ((y - size.1) / size.3).clamp(0.0, 1.0)];
    let mut out = Drawing::default();
    // Phosphor's own exports have a background rectangle: not a shape.
    for n in root.descendants().filter(|n| n.is_element() && n.attribute("id") != Some("background")) {
        let (pts, closed): (Vec<[f32; 2]>, bool) = match n.tag_name().name() {
            "polygon" => (nums(n.attribute("points").unwrap_or("")).chunks_exact(2).map(|p| unit(p[0], p[1])).collect(), true),
            "polyline" => (nums(n.attribute("points").unwrap_or("")).chunks_exact(2).map(|p| unit(p[0], p[1])).collect(), false),
            "line" => {
                let a = |k: &str| n.attribute(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
                (vec![unit(a("x1"), a("y1")), unit(a("x2"), a("y2"))], false)
            }
            "rect" => {
                let a = |k: &str| n.attribute(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
                let (x, y, w, h) = (a("x"), a("y"), a("width"), a("height"));
                (vec![unit(x, y), unit(x + w, y), unit(x + w, y + h), unit(x, y + h)], true)
            }
            "path" => {
                let d = n.attribute("d").unwrap_or("");
                (path_points(d).into_iter().map(|p| unit(p[0], p[1])).collect(), d.contains(['Z', 'z']))
            }
            _ => continue,
        };
        if closed && pts.len() >= 3 {
            out.shapes.push(pts);
        } else if !closed && pts.len() >= 2 {
            out.lines.push(pts);
        }
    }
    Ok(out)
}

/// A drawing file's shapes, whichever kind it is: SVG, or EPS / an Illustrator file saved the PostScript way.
pub fn import_file(bytes: &[u8]) -> Result<Drawing, String> {
    let text = String::from_utf8_lossy(bytes);
    let head = &text[..text.len().min(2048)];
    if head.contains("%!PS") || head.contains("%%BoundingBox") {
        import_eps(&text)
    } else if head.starts_with("%PDF") || head.contains("%PDF-") {
        Err("this file is a PDF inside (a newer Illustrator file): save it as SVG or EPS and bring that in".into())
    } else {
        import(&text)
    }
}

/// Every outline in an EPS drawing, in 0..1 of its bounding box: closed ones as shapes, open ones as lines; curves
/// are followed in short steps. It reads the path-building words (moveto / lineto / curveto / closepath, the
/// relative ones, and Illustrator's short forms m l c v y h and re); pictures, text and patterns are passed over.
pub fn import_eps(text: &str) -> Result<Drawing, String> {
    let nums = |s: &str| s.split_whitespace().filter_map(|t| t.parse::<f32>().ok()).collect::<Vec<_>>();
    let bbox = ["%%HiResBoundingBox:", "%%BoundingBox:"]
        .iter()
        .find_map(|key| text.lines().find_map(|l| l.trim().strip_prefix(key)).map(nums).filter(|v| v.len() == 4 && v[2] > v[0] && v[3] > v[1]))
        .ok_or("the drawing doesn't say how big it is (no bounding box)")?;
    let (w, h) = (bbox[2] - bbox[0], bbox[3] - bbox[1]);
    // (PostScript's y runs up the page.)
    let unit = |p: [f32; 2]| [((p[0] - bbox[0]) / w).clamp(0.0, 1.0), (1.0 - (p[1] - bbox[1]) / h).clamp(0.0, 1.0)];
    let mut out = Drawing::default();
    let (mut stack, mut path, mut at, mut start): (Vec<f32>, Vec<[f32; 2]>, [f32; 2], [f32; 2]) = (Vec::new(), Vec::new(), [0.0; 2], [0.0; 2]);
    let finish = |path: &mut Vec<[f32; 2]>, closed: bool, out: &mut Drawing| {
        let mut pts: Vec<[f32; 2]> = std::mem::take(path).into_iter().map(unit).collect();
        pts.dedup();
        if closed && pts.len() > 3 && pts.first() == pts.last() {
            pts.pop();
        }
        // (Nothing bigger than a show could use: a drawing of thousands of shapes is not a set of masks.)
        if out.shapes.len() + out.lines.len() < 512 {
            if closed && pts.len() >= 3 {
                out.shapes.push(pts);
            } else if !closed && pts.len() >= 2 {
                out.lines.push(pts);
            }
        }
    };
    let curve = |path: &mut Vec<[f32; 2]>, a: [f32; 2], b: [f32; 2], c: [f32; 2], d: [f32; 2]| {
        for k in 1..=12 {
            let t = k as f32 / 12.0;
            let u = 1.0 - t;
            path.push([u * u * u * a[0] + 3.0 * u * u * t * b[0] + 3.0 * u * t * t * c[0] + t * t * t * d[0], u * u * u * a[1] + 3.0 * u * u * t * b[1] + 3.0 * u * t * t * c[1] + t * t * t * d[1]]);
        }
    };
    for line in text.lines().filter(|l| !l.trim_start().starts_with('%')) {
        for word in line.split_whitespace() {
            if let Ok(v) = word.parse::<f32>() {
                if v.is_finite() && stack.len() < 64 {
                    stack.push(v);
                }
                continue;
            }
            let n = stack.len();
            let last = |k: usize| stack[n - k];
            match word {
                "moveto" | "m" | "rmoveto" if n >= 2 => {
                    finish(&mut path, false, &mut out);
                    at = if word == "rmoveto" { [at[0] + last(2), at[1] + last(1)] } else { [last(2), last(1)] };
                    start = at;
                    path.push(at);
                }
                "lineto" | "l" | "L" | "rlineto" if n >= 2 && !path.is_empty() => {
                    at = if word == "rlineto" { [at[0] + last(2), at[1] + last(1)] } else { [last(2), last(1)] };
                    path.push(at);
                }
                "curveto" | "c" | "C" if n >= 6 && !path.is_empty() => {
                    let d = [last(2), last(1)];
                    curve(&mut path, at, [last(6), last(5)], [last(4), last(3)], d);
                    at = d;
                }
                // Illustrator: the first handle sits on the point before (v), or the second on the point reached (y).
                "v" | "V" if n >= 4 && !path.is_empty() => {
                    let d = [last(2), last(1)];
                    curve(&mut path, at, at, [last(4), last(3)], d);
                    at = d;
                }
                "y" | "Y" if n >= 4 && !path.is_empty() => {
                    let d = [last(2), last(1)];
                    curve(&mut path, at, [last(4), last(3)], d, d);
                    at = d;
                }
                "re" if n >= 4 => {
                    finish(&mut path, false, &mut out);
                    let (x, y, rw, rh) = (last(4), last(3), last(2), last(1));
                    path.extend([[x, y], [x + rw, y], [x + rw, y + rh], [x, y + rh]]);
                    finish(&mut path, true, &mut out);
                }
                "closepath" | "h" | "cp" => {
                    finish(&mut path, true, &mut out);
                    at = start;
                }
                // Painting words end the outline (the closing ones close it first).
                "s" | "b" | "b*" | "f" | "F" | "f*" | "B" | "B*" | "fill" | "eofill" => finish(&mut path, true, &mut out),
                "S" | "n" | "N" | "stroke" | "newpath" => finish(&mut path, false, &mut out),
                _ => {}
            }
            stack.clear();
        }
    }
    finish(&mut path, false, &mut out);
    if out.shapes.is_empty() && out.lines.is_empty() {
        return Err("no outlines found in that drawing (text and pictures don't count: turn text into outlines first)".into());
    }
    Ok(out)
}

/// The corner points of an SVG path (M, L, H, V, C, S, Q, T, A, Z; absolute or relative). Curves keep their end points.
fn path_points(d: &str) -> Vec<[f32; 2]> {
    // Split into commands and numbers.
    let mut tokens: Vec<Result<char, f32>> = Vec::new();
    let mut num = String::new();
    let flush = |num: &mut String, tokens: &mut Vec<Result<char, f32>>| {
        if let Ok(v) = num.parse::<f32>() {
            tokens.push(Err(v));
        }
        num.clear();
    };
    for c in d.chars() {
        if c.is_ascii_alphabetic() && c != 'e' && c != 'E' {
            flush(&mut num, &mut tokens);
            tokens.push(Ok(c));
        } else if c == '-' && !num.is_empty() && !num.ends_with(['e', 'E']) {
            flush(&mut num, &mut tokens);
            num.push(c);
        } else if c == ',' || c.is_whitespace() {
            flush(&mut num, &mut tokens);
        } else if c == '.' && num.contains('.') {
            flush(&mut num, &mut tokens);
            num.push(c);
        } else {
            num.push(c);
        }
    }
    flush(&mut num, &mut tokens);
    let (mut pts, mut at, mut cmd) = (Vec::new(), [0.0f32, 0.0], 'M');
    let mut i = 0;
    while i < tokens.len() {
        if let Ok(c) = tokens[i] {
            cmd = c;
            i += 1;
            if c == 'Z' || c == 'z' {
                continue;
            }
        }
        let rel = cmd.is_ascii_lowercase();
        let take = match cmd.to_ascii_uppercase() {
            'H' | 'V' => 1,
            'M' | 'L' | 'T' => 2,
            'S' | 'Q' => 4,
            'C' => 6,
            'A' => 7,
            _ => break,
        };
        let args: Vec<f32> = tokens[i..].iter().take(take).map_while(|t| t.err()).collect();
        if args.len() < take {
            break;
        }
        i += take;
        let base = if rel { at } else { [0.0, 0.0] };
        at = match cmd.to_ascii_uppercase() {
            'H' => [args[0] + if rel { at[0] } else { 0.0 }, at[1]],
            'V' => [at[0], args[0] + if rel { at[1] } else { 0.0 }],
            _ => [base[0] + args[take - 2], base[1] + args[take - 1]],
        };
        pts.push(at);
        // After a move, more pairs are lines.
        if cmd == 'M' {
            cmd = 'L';
        } else if cmd == 'm' {
            cmd = 'l';
        }
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_goes_out_and_shapes_come_back() {
        let mut m = Mapping::default();
        let s = m.add_surface(7);
        s.name = "Left <wall>".into();
        s.corners = [[0.0, 0.0], [0.5, 0.0], [0.5, 1.0], [0.0, 1.0]];
        let svg = export(&m, 1000, 500);
        assert!(svg.contains("slice-7") && svg.contains("Left &lt;wall&gt;"), "{svg}");
        let back = import(&svg).unwrap().shapes;
        // Only the slice outline (not the background).
        assert!(back.iter().all(|p| p.iter().all(|q| q[0] <= 0.501)), "{back:?}");
        let drawing = import(r#"<svg viewBox="0 0 100 100"><path d="M10 10 h80 v80 H10z"/><polygon points="0,0 50,0 50,50"/><path d="m10,10 l20-0 c0,0 0,0 0,20z"/><polyline points="0,50 100,50"/><line x1="0" y1="0" x2="100" y2="100"/></svg>"#).unwrap();
        assert_eq!(drawing.lines.len(), 2, "open lines");
        assert_eq!(drawing.lines[1], vec![[0.0, 0.0], [1.0, 1.0]]);
        let shapes = drawing.shapes;
        assert_eq!(shapes[0], vec![[0.1, 0.1], [0.9, 0.1], [0.9, 0.9], [0.1, 0.9]]);
        assert_eq!(shapes[1].len(), 3);
        assert_eq!(shapes[2], vec![[0.1, 0.1], [0.3, 0.1], [0.3, 0.3]]);
        assert!(import("<svg/>").is_err(), "no size");
    }

    /// The PDF: one page the output's size, the cross-reference table pointing at each object, names escaped.
    /// An EPS as a drawing program writes it: a triangle the long way, a square and a curve in Illustrator's short
    /// words, an open line. Shapes come back in 0..1 with y running down.
    #[test]
    fn outlines_come_out_of_an_eps() {
        let eps = "%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 0 0 200 100\n%%EndComments\nnewpath\n0 0 moveto 200 0 lineto 100 100 lineto closepath fill\n50 25 m 150 25 l 150 75 l 50 75 l h f\n0 50 m 0 100 100 100 100 50 c S\n10 10 30 20 re f\n";
        let d = import_file(eps.as_bytes()).unwrap();
        assert_eq!(d.shapes.len(), 3, "triangle, square, rectangle");
        assert_eq!(d.shapes[0], vec![[0.0, 1.0], [1.0, 1.0], [0.5, 0.0]], "y flipped");
        assert_eq!(d.shapes[1], vec![[0.25, 0.75], [0.75, 0.75], [0.75, 0.25], [0.25, 0.25]]);
        assert_eq!(d.lines.len(), 1, "the open curve");
        assert_eq!(d.lines[0].len(), 13, "followed in steps");
        let top = d.lines[0].iter().map(|p| p[1]).fold(1.0f32, f32::min);
        assert!((top - 0.125).abs() < 0.02, "the curve bulges up: {top}");
        // Not a drawing, or one with nothing to cut along: said plainly.
        assert!(import_file(b"%!PS-Adobe-3.0\n%%BoundingBox: 0 0 10 10\n(hello) show\n").is_err());
        assert!(import_file(b"%PDF-1.5 ...").unwrap_err().contains("SVG or EPS"));
        assert!(import_file(b"<svg viewBox=\"0 0 10 10\"><rect x=\"1\" y=\"1\" width=\"5\" height=\"5\"/></svg>").is_ok(), "an SVG still reads");
    }

    #[test]
    fn mapping_as_a_pdf() {
        let mut m = Mapping::default();
        let s = m.add_surface(7);
        s.name = "Left (wall)".into();
        let pdf = export_pdf(&m, 1000, 500);
        // PHOSPHOR_WRITE_PDF=file: keep it, to open in a PDF reader.
        if let Ok(p) = std::env::var("PHOSPHOR_WRITE_PDF") {
            std::fs::write(p, &pdf).unwrap();
        }
        let text = String::from_utf8_lossy(&pdf).to_string();
        assert!(text.starts_with("%PDF-1.4") && text.ends_with("%%EOF\n"));
        assert!(text.contains("/MediaBox [0 0 1000 500]"));
        assert!(text.contains("(Left \\(wall\\)) Tj"), "brackets escaped");
        // Every xref entry points at its "N 0 obj".
        let xref = text.find("xref\n").unwrap();
        for (i, line) in text[xref..].lines().skip(3).take(5).enumerate() {
            let at: usize = line[..10].parse().unwrap();
            assert!(text[at..].starts_with(&format!("{} 0 obj", i + 1)), "object {}", i + 1);
        }
        // The stream's length is right.
        let start = text.find("stream\n").unwrap() + 7;
        let end = text.find("endstream").unwrap();
        assert!(text.contains(&format!("/Length {}", end - start)));
    }
}
