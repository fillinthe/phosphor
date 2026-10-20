//! Colour-grading LUTs (`.cube` files, as cameras and grading apps export them) as effects: each file in a LUT folder
//! becomes an effect with an Amount. The 3D table is packed into a picture (N² wide, N high: red across a slice,
//! blue picks the slice, green down) that the effect's shader reads with trilinear blending between slices.

/// A parsed 3D LUT: `size`³ RGB entries, red changing fastest (the .cube order).
pub struct Lut {
    pub size: usize,
    pub data: Vec<[f32; 3]>,
}

/// Reads a `.cube` file (LUT_3D_SIZE, optional DOMAIN_MIN / DOMAIN_MAX, then size³ lines of "r g b").
pub fn parse_cube(text: &str) -> Result<Lut, String> {
    let mut size = 0usize;
    let (mut lo, mut hi) = ([0.0f32; 3], [1.0f32; 3]);
    let mut data = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') || t.starts_with("TITLE") {
            continue;
        }
        let mut words = t.split_whitespace();
        match words.next() {
            Some("LUT_3D_SIZE") => size = words.next().and_then(|n| n.parse().ok()).ok_or("bad LUT_3D_SIZE")?,
            Some("LUT_1D_SIZE") => return Err("1D LUTs aren't supported (only 3D .cube)".into()),
            Some("DOMAIN_MIN") => lo = three(words).ok_or("bad DOMAIN_MIN")?,
            Some("DOMAIN_MAX") => hi = three(words).ok_or("bad DOMAIN_MAX")?,
            Some(first) if first.parse::<f32>().is_ok() => {
                let v = three(t.split_whitespace()).ok_or_else(|| format!("bad line: {t}"))?;
                data.push([0, 1, 2].map(|k| ((v[k] - lo[k]) / (hi[k] - lo[k]).max(1e-6)).clamp(0.0, 1.0)));
            }
            _ => {}
        }
    }
    if !(2..=128).contains(&size) {
        return Err(format!("LUT size {size} (2 to 128)"));
    }
    if data.len() != size * size * size {
        return Err(format!("{} entries for a {size}³ LUT (needs {})", data.len(), size * size * size));
    }
    Ok(Lut { size, data })
}

fn three<'a>(mut w: impl Iterator<Item = &'a str>) -> Option<[f32; 3]> {
    Some([w.next()?.parse().ok()?, w.next()?.parse().ok()?, w.next()?.parse().ok()?])
}

impl Lut {
    /// The table as a picture: (width, height, RGBA8).
    pub fn pack(&self) -> (u32, u32, Vec<u8>) {
        let n = self.size;
        let (w, h) = (n * n, n);
        let mut rgba = vec![255u8; w * h * 4];
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    let v = self.data[r + g * n + b * n * n];
                    let at = ((g * w) + b * n + r) * 4;
                    for k in 0..3 {
                        rgba[at + k] = (v[k] * 255.0).round() as u8;
                    }
                }
            }
        }
        (w as u32, h as u32, rgba)
    }
}

/// The effect's shader: the layer's colour looked up in the packed table (buf0), mixed in by Amount (P(0)).
pub fn wgsl(entry: &str, size: usize) -> String {
    format!(
        "@fragment
fn {entry}(in: VOut) -> @location(0) vec4f {{
    let c = src(in.uv);
    let n = {size}.0;
    let col = clamp(c.rgb, vec3f(0.0), vec3f(1.0)) * (n - 1.0);
    let b0 = floor(col.b);
    let b1 = min(b0 + 1.0, n - 1.0);
    let w = n * n;
    let uv0 = vec2f((col.r + 0.5 + b0 * n) / w, (col.g + 0.5) / n);
    let uv1 = vec2f((col.r + 0.5 + b1 * n) / w, (col.g + 0.5) / n);
    let graded = mix(textureSampleLevel(buf0, samp, uv0, 0.0).rgb, textureSampleLevel(buf0, samp, uv1, 0.0).rgb, col.b - b0);
    return vec4f(mix(c.rgb, graded, P(0u)), c.a);
}}
"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2³ "invert" LUT in .cube text.
    fn invert_cube() -> String {
        let mut s = String::from("TITLE \"invert\"\nLUT_3D_SIZE 2\n");
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    s.push_str(&format!("{} {} {}\n", 1 - r, 1 - g, 1 - b));
                }
            }
        }
        s
    }

    #[test]
    fn a_cube_file_reads_and_packs() {
        let lut = parse_cube(&invert_cube()).unwrap();
        assert_eq!(lut.size, 2);
        let (w, h, rgba) = lut.pack();
        assert_eq!((w, h), (4, 2));
        // Black (r=g=b=0) is the first pixel of slice 0 → white.
        assert_eq!(&rgba[0..3], &[255, 255, 255]);
        // r=1, g=0, b=1 → x = 1 + 1*2 = 3, y = 0 → (0, 255, 0).
        assert_eq!(&rgba[3 * 4..3 * 4 + 3], &[0, 255, 0]);
        assert!(parse_cube("LUT_3D_SIZE 2\n0 0 0\n").is_err(), "too few entries");
        assert!(parse_cube("LUT_1D_SIZE 4\n").is_err());
    }

    #[test]
    fn the_shader_validates() {
        // The same prelude the engine compiles imported shaders with.
        let full = format!("{}\n{}", phosphor_engine::shaders::common_source(), wgsl("isf_lut_test", 33));
        let module = naga::front::wgsl::parse_str(&full).expect("parses");
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::default()).validate(&module).expect("valid");
    }
}
