//! Colour match: make one picture's colours look like a reference's (per channel: the same average and spread,
//! Reinhard-style). The result is a Colour Match effect's gain and offset per channel.

use super::*;

/// Average and spread (standard deviation) of red, green and blue in an RGBA8 picture, 0..1.
pub fn color_stats(rgba: &[u8]) -> [(f32, f32); 3] {
    let n = (rgba.len() / 4).max(1) as f64;
    let mut sum = [0.0f64; 3];
    let mut sq = [0.0f64; 3];
    for p in rgba.chunks_exact(4) {
        for k in 0..3 {
            let v = p[k] as f64 / 255.0;
            sum[k] += v;
            sq[k] += v * v;
        }
    }
    [0, 1, 2].map(|k| {
        let mean = sum[k] / n;
        ((mean) as f32, ((sq[k] / n - mean * mean).max(0.0).sqrt()) as f32)
    })
}

/// The Colour Match settings that turn a picture with stats `from` into one like `to`: gain R, G, B then offset R, G, B
/// (new = old × gain + offset), kept to the effect's ranges.
pub fn match_values(from: [(f32, f32); 3], to: [(f32, f32); 3]) -> Vec<f32> {
    let gains: [f32; 3] = [0, 1, 2].map(|k| if from[k].1 > 0.004 { (to[k].1 / from[k].1).clamp(0.0, 4.0) } else { 1.0 });
    let offsets: [f32; 3] = [0, 1, 2].map(|k| (to[k].0 - gains[k] * from[k].0).clamp(-1.0, 1.0));
    gains.into_iter().chain(offsets).collect()
}

/// The picture's `k` main colours (k-means over its pixels, 0..1 RGB), darkest first. A few passes over a small
/// picture: call it on a thumbnail.
pub fn palette_of(rgba: &[u8], k: usize) -> Vec<[f32; 3]> {
    let px: Vec<[f32; 3]> = rgba.chunks_exact(4).map(|p| [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]).collect();
    if px.is_empty() || k == 0 {
        return Vec::new();
    }
    let luma = |c: &[f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    // Start from k pixels spread over the picture's brightness range.
    let mut by_luma = px.clone();
    by_luma.sort_by(|a, b| luma(a).total_cmp(&luma(b)));
    let mut centres: Vec<[f32; 3]> = (0..k).map(|i| by_luma[(i * 2 + 1) * by_luma.len() / (2 * k)]).collect();
    let dist = |a: &[f32; 3], b: &[f32; 3]| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2);
    for _ in 0..12 {
        let mut sum = vec![[0.0f32; 4]; k];
        for p in &px {
            let near = (0..k).min_by(|&a, &b| dist(p, &centres[a]).total_cmp(&dist(p, &centres[b]))).unwrap_or(0);
            for c in 0..3 {
                sum[near][c] += p[c];
            }
            sum[near][3] += 1.0;
        }
        for (centre, s) in centres.iter_mut().zip(&sum) {
            if s[3] > 0.0 {
                *centre = [s[0] / s[3], s[1] / s[3], s[2] / s[3]];
            }
        }
    }
    centres.sort_by(|a, b| luma(a).total_cmp(&luma(b)));
    centres
}

impl Composition {
    /// Adds a Palette effect to `owner`'s chain with these colours (up to six): every pixel takes the nearest one.
    pub fn add_palette(&mut self, owner: u64, colours: &[[f32; 3]]) -> Option<u64> {
        let desc = crate::shaders::effect("palette")?;
        let mut values = desc.defaults();
        let n = colours.len().clamp(1, 6);
        for (i, c) in colours.iter().take(6).enumerate() {
            values[i * 3..i * 3 + 3].copy_from_slice(c);
        }
        values[18] = n as f32;
        let id = self.new_id();
        let fx = EffectState { id, effect: desc.id.to_string(), enabled: true, values, mix: 1.0, mods: Vec::new(), blend: Default::default(), other: 0 };
        self.effects_mut(owner)?.push(fx);
        Some(id)
    }

    /// Adds a Colour Match effect to `owner`'s chain that turns pictures like `from` into pictures like `to`.
    pub fn add_color_match(&mut self, owner: u64, from: [(f32, f32); 3], to: [(f32, f32); 3]) -> Option<u64> {
        let desc = crate::shaders::effect("color_match")?;
        let id = self.new_id();
        let fx = EffectState { id, effect: desc.id.to_string(), enabled: true, values: match_values(from, to), mix: 1.0, mods: Vec::new(), blend: Default::default(), other: 0 };
        self.effects_mut(owner)?.push(fx);
        Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_main_colours_are_found() {
        // Half red, a third blue, the rest near-white.
        let px: Vec<u8> = (0..600).flat_map(|i| if i < 300 { [220, 20, 20, 255] } else if i < 500 { [10, 10, 200, 255] } else { [250, 250, 245, 255] }).collect();
        let pal = palette_of(&px, 3);
        assert_eq!(pal.len(), 3);
        let has = |c: [f32; 3]| pal.iter().any(|p| (0..3).all(|k| (p[k] - c[k]).abs() < 0.02));
        assert!(has([10.0 / 255.0, 10.0 / 255.0, 200.0 / 255.0]) && has([220.0 / 255.0, 20.0 / 255.0, 20.0 / 255.0]) && has([250.0 / 255.0, 250.0 / 255.0, 245.0 / 255.0]), "{pal:?}");
        let mut c = Composition::default();
        c.layers.push(LayerState { id: 1, ..Default::default() });
        c.add_palette(1, &pal).unwrap();
        assert_eq!(c.layers[0].effects[0].values[18], 3.0);
    }

    #[test]
    fn colours_meet_the_reference() {
        // A dull picture (0.4..0.6) and a livelier one (0.3..0.7).
        let pic = |lo: u8, hi: u8| -> Vec<u8> { (0..100).flat_map(|i| { let v = if i % 2 == 0 { lo } else { hi }; [v, v, v, 255] }).collect() };
        let (dull, bright) = (color_stats(&pic(102, 153)), color_stats(&pic(77, 179)));
        assert!((dull[0].0 - 0.5).abs() < 0.01 && (dull[0].1 - 0.1).abs() < 0.01);
        let v = match_values(dull, bright);
        // The dull picture's dark and light come out where the bright one's are.
        let apply = |x: f32| x * v[0] + v[3];
        assert!((apply(0.4) - 0.3).abs() < 0.02 && (apply(0.6) - 0.7).abs() < 0.02, "{v:?}");
        // Flat colour can't be stretched: gain stays 1.
        assert_eq!(match_values([(0.5, 0.0); 3], bright)[0], 1.0);
    }
}
