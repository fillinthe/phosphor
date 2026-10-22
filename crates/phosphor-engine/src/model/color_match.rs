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

impl Composition {
    /// Adds a Colour Match effect to `owner`'s chain that turns pictures like `from` into pictures like `to`.
    pub fn add_color_match(&mut self, owner: u64, from: [(f32, f32); 3], to: [(f32, f32); 3]) -> Option<u64> {
        let desc = crate::shaders::effect("color_match")?;
        let id = self.new_id();
        let fx = EffectState { id, effect: desc.id.to_string(), enabled: true, values: match_values(from, to), mix: 1.0, mods: Vec::new(), blend: Default::default() };
        self.effects_mut(owner)?.push(fx);
        Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
