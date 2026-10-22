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
    /// The colour this table gives `c` (0..1 RGB), blending between its entries (trilinear).
    pub fn sample(&self, c: [f32; 3]) -> [f32; 3] {
        let n = self.size;
        let f = c.map(|v| v.clamp(0.0, 1.0) * (n - 1) as f32);
        let i0 = f.map(|v| (v.floor() as usize).min(n - 1));
        let i1 = i0.map(|v| (v + 1).min(n - 1));
        let t = [f[0] - i0[0] as f32, f[1] - i0[1] as f32, f[2] - i0[2] as f32];
        let at = |r: usize, g: usize, b: usize| self.data[r + g * n + b * n * n];
        let mut out = [0.0; 3];
        for (k, o) in out.iter_mut().enumerate() {
            let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
            let c00 = lerp(at(i0[0], i0[1], i0[2])[k], at(i1[0], i0[1], i0[2])[k], t[0]);
            let c10 = lerp(at(i0[0], i1[1], i0[2])[k], at(i1[0], i1[1], i0[2])[k], t[0]);
            let c01 = lerp(at(i0[0], i0[1], i1[2])[k], at(i1[0], i0[1], i1[2])[k], t[0]);
            let c11 = lerp(at(i0[0], i1[1], i1[2])[k], at(i1[0], i1[1], i1[2])[k], t[0]);
            *o = lerp(lerp(c00, c10, t[1]), lerp(c01, c11, t[1]), t[2]);
        }
        out
    }

    /// The same grade on an `n`³ table (so tables of different sizes can share one picture).
    pub fn resample(&self, n: usize) -> Lut {
        if n == self.size {
            return Lut { size: n, data: self.data.clone() };
        }
        let step = 1.0 / (n - 1) as f32;
        let mut data = Vec::with_capacity(n * n * n);
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    data.push(self.sample([r as f32 * step, g as f32 * step, b as f32 * step]));
                }
            }
        }
        Lut { size: n, data }
    }

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

/// Size every table in the LUT mixer is resampled to.
pub const MIX_SIZE: usize = 33;

/// The LUT mixer's picture: every table at `MIX_SIZE`³, stacked top to bottom (one band of `MIX_SIZE` rows each).
pub fn pack_all(luts: &[Lut]) -> (u32, u32, Vec<u8>) {
    let mut rgba = Vec::new();
    let mut w = 0;
    for l in luts {
        let (lw, _, px) = l.resample(MIX_SIZE).pack();
        w = lw;
        rgba.extend(px);
    }
    (w, (MIX_SIZE * luts.len()) as u32, rgba)
}

/// The LUT mixer's shader: LUT A (P(0)) and LUT B (P(1)) from the stacked picture, blended by Mix (P(2)), mixed
/// into the picture by Amount (P(3)). With `split`, the blend is per pixel instead: P(2) = what decides (0 brightness,
/// 1 left to right, 2 top to bottom, 3 distance from the centre), P(3) = where it changes, P(4) = how softly,
/// P(5) = Amount.
pub fn wgsl_mix(entry: &str, count: usize, split: bool) -> String {
    let blend = if split {
        "let by = round(P(2u));
    var m = dot(c.rgb, vec3f(0.2126, 0.7152, 0.0722));
    if (by == 1.0) { m = in.uv.x; }
    if (by == 2.0) { m = in.uv.y; }
    if (by == 3.0) { m = length(in.uv - vec2f(0.5)) * 1.4142; }
    let soft = max(P(4u), 0.001);
    let graded = mix(a, b, smoothstep(P(3u) - soft, P(3u) + soft, m));
    return vec4f(mix(c.rgb, graded, P(5u)), c.a);"
    } else {
        "let graded = mix(a, b, P(2u));
    return vec4f(mix(c.rgb, graded, P(3u)), c.a);"
    };
    let n = MIX_SIZE;
    format!(
        "fn lut_band(c: vec3f, band: f32) -> vec3f {{
    let n = {n}.0;
    let rows = n * {count}.0;
    let col = clamp(c, vec3f(0.0), vec3f(1.0)) * (n - 1.0);
    let b0 = floor(col.b);
    let b1 = min(b0 + 1.0, n - 1.0);
    let w = n * n;
    let y = (band * n + col.g + 0.5) / rows;
    let a = textureSampleLevel(buf0, samp, vec2f((col.r + 0.5 + b0 * n) / w, y), 0.0).rgb;
    let b = textureSampleLevel(buf0, samp, vec2f((col.r + 0.5 + b1 * n) / w, y), 0.0).rgb;
    return mix(a, b, col.b - b0);
}}

@fragment
fn {entry}(in: VOut) -> @location(0) vec4f {{
    let c = src(in.uv);
    let last = {count}.0 - 1.0;
    let a = lut_band(c.rgb, clamp(round(P(0u)), 0.0, last));
    let b = lut_band(c.rgb, clamp(round(P(1u)), 0.0, last));
    {blend}
}}
"
    )
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
    fn resampling_keeps_the_grade() {
        let lut = parse_cube(&invert_cube()).unwrap();
        let big = lut.resample(5);
        assert_eq!((big.size, big.data.len()), (5, 125));
        let v = big.sample([0.25, 0.5, 1.0]);
        assert!((v[0] - 0.75).abs() < 1e-5 && (v[1] - 0.5).abs() < 1e-5 && v[2].abs() < 1e-5, "{v:?}");
        let (w, h, px) = pack_all(&[lut.resample(3), big]);
        assert_eq!((w as usize, h as usize, px.len()), (MIX_SIZE * MIX_SIZE, MIX_SIZE * 2, MIX_SIZE * MIX_SIZE * MIX_SIZE * 2 * 4));
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
    fn the_shipped_looks_parse() {
        let dir = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/luts"));
        let mut n = 0;
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let lut = parse_cube(&std::fs::read_to_string(e.path()).unwrap()).unwrap_or_else(|err| panic!("{}: {err}", e.path().display()));
            assert_eq!(lut.size, 17);
            n += 1;
        }
        assert!(n >= 8, "{n} looks");
    }

    #[test]
    fn the_shader_validates() {
        // The same prelude the engine compiles imported shaders with.
        let full = format!("{}\n{}", phosphor_engine::shaders::common_source(), wgsl("isf_lut_test", 33));
        let module = naga::front::wgsl::parse_str(&full).expect("parses");
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::default()).validate(&module).expect("valid");
        for split in [false, true] {
            let mix = format!("{}\n{}", phosphor_engine::shaders::common_source(), wgsl_mix("isf_lut_mixer", 3, split));
            let module = naga::front::wgsl::parse_str(&mix).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&mix)));
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::default()).validate(&module).expect("mixer valid");
        }
    }
}
