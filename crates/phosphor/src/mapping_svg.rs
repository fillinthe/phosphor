//! The mapping as SVG: `export` draws every slice and mask of an output (for the venue's tech team, a laser cutter,
//! or another app), `import` reads shapes back as polygons (0..1 of the picture) to become masks.
use phosphor_engine::Mapping;

/// An SVG drawing of `mapping` at `w` × `h`: slices as outlined shapes with their names, masks filled grey.
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
    for s in mapping.surfaces.iter().filter(|s| s.enabled) {
        let pts = s.outline(16);
        out += &format!("<path id=\"slice-{}\" d=\"{}\"><title>{}</title></path>\n", s.id, path(&pts), esc(&s.name));
    }
    out += "</g>\n<g id=\"masks\" fill=\"#808080\" fill-opacity=\"0.6\">\n";
    for m in mapping.placed_masks() {
        out += &format!("<path id=\"mask-{}\" d=\"{}\"><title>{}</title></path>\n", m.id, path(&m.outline()), esc(&m.name));
    }
    out += "</g>\n<g id=\"names\" fill=\"white\" font-family=\"sans-serif\" font-size=\"14\" text-anchor=\"middle\">\n";
    for s in mapping.surfaces.iter().filter(|s| s.enabled) {
        let pts = s.outline(1);
        let n = pts.len().max(1) as f32;
        let (cx, cy) = (pts.iter().map(|p| p[0]).sum::<f32>() / n * fw, pts.iter().map(|p| p[1]).sum::<f32>() / n * fh);
        out += &format!("<text x=\"{cx:.1}\" y=\"{cy:.1}\">{}</text>\n", esc(&s.name));
    }
    out + "</g>\n</svg>\n"
}

/// Every closed shape in an SVG (polygon, polyline, rect, path; curves follow their end points), in 0..1 of the
/// drawing's size (its viewBox, else width × height).
pub fn import(text: &str) -> Result<Vec<Vec<[f32; 2]>>, String> {
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
    let mut shapes = Vec::new();
    // Phosphor's own exports have a background rectangle: not a shape.
    for n in root.descendants().filter(|n| n.is_element() && n.attribute("id") != Some("background")) {
        let pts: Vec<[f32; 2]> = match n.tag_name().name() {
            "polygon" | "polyline" => nums(n.attribute("points").unwrap_or("")).chunks_exact(2).map(|p| unit(p[0], p[1])).collect(),
            "rect" => {
                let a = |k: &str| n.attribute(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
                let (x, y, w, h) = (a("x"), a("y"), a("width"), a("height"));
                vec![unit(x, y), unit(x + w, y), unit(x + w, y + h), unit(x, y + h)]
            }
            "path" => path_points(n.attribute("d").unwrap_or("")).into_iter().map(|p| unit(p[0], p[1])).collect(),
            _ => continue,
        };
        if pts.len() >= 3 {
            shapes.push(pts);
        }
    }
    Ok(shapes)
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
        let back = import(&svg).unwrap();
        // Only the slice outline (not the background).
        assert!(back.iter().all(|p| p.iter().all(|q| q[0] <= 0.501)), "{back:?}");
        let shapes = import(r#"<svg viewBox="0 0 100 100"><path d="M10 10 h80 v80 H10z"/><polygon points="0,0 50,0 50,50"/><path d="m10,10 l20-0 c0,0 0,0 0,20z"/></svg>"#).unwrap();
        assert_eq!(shapes[0], vec![[0.1, 0.1], [0.9, 0.1], [0.9, 0.9], [0.1, 0.9]]);
        assert_eq!(shapes[1].len(), 3);
        assert_eq!(shapes[2], vec![[0.1, 0.1], [0.3, 0.1], [0.3, 0.3]]);
        assert!(import("<svg/>").is_err(), "no size");
    }
}
