//! BC7 (BPTC) blocks for Hap R, Hap's best quality. Only mode 6 is written: one RGBA line per 4×4 block with 7-bit
//! endpoints plus a shared low bit each, and 16 steps between them. That's one mode out of BC7's eight, but it's
//! the one that suits video (smooth colour, any alpha) and it's quick to search. Spec: Khronos Data Format, BPTC.

/// One 4×4 block of RGBA pixels.
pub type Block = [[u8; 4]; 16];

/// Mode 6's 4-bit interpolation weights (out of 64).
const WEIGHTS: [i32; 16] = [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64];

fn mix(a: u8, b: u8, w: i32) -> i32 {
    ((64 - w) * a as i32 + w * b as i32 + 32) >> 6
}

/// The 16 colours between two endpoints.
fn palette(a: [u8; 4], b: [u8; 4]) -> [[i32; 4]; 16] {
    std::array::from_fn(|k| std::array::from_fn(|c| mix(a[c], b[c], WEIGHTS[k])))
}

fn dist(p: [u8; 4], q: [i32; 4]) -> i32 {
    (0..4).map(|c| (p[c] as i32 - q[c]).pow(2)).sum()
}

/// Each pixel's nearest step (found from where it falls along the line, then checked against its neighbours) and
/// the block's total squared error.
fn pick(px: &Block, a: [u8; 4], b: [u8; 4]) -> ([u8; 16], i32) {
    let pal = palette(a, b);
    let d: [f32; 4] = std::array::from_fn(|c| b[c] as f32 - a[c] as f32);
    let len2 = d.iter().map(|x| x * x).sum::<f32>().max(1e-6);
    let mut idx = [0u8; 16];
    let mut err = 0;
    for (i, p) in px.iter().enumerate() {
        let t = (0..4).map(|c| (p[c] as f32 - a[c] as f32) * d[c]).sum::<f32>() / len2;
        let guess = (t * 15.0).round().clamp(0.0, 15.0) as usize;
        let (k, e) = (guess.saturating_sub(1)..=(guess + 1).min(15)).map(|k| (k, dist(*p, pal[k]))).min_by_key(|x| x.1).unwrap_or((0, 0));
        idx[i] = k as u8;
        err += e;
    }
    (idx, err)
}

/// An endpoint rounded to 7 bits per channel plus the shared low bit `p`.
fn quantize(e: [f32; 4], p: u8) -> [u8; 4] {
    std::array::from_fn(|c| (((e[c] - p as f32) / 2.0).round().clamp(0.0, 127.0) as u8) << 1 | p)
}

/// The best of the four low-bit choices for these endpoints.
fn best_fit(px: &Block, e0: [f32; 4], e1: [f32; 4]) -> ([u8; 4], [u8; 4], [u8; 16], i32) {
    let mut best = ([0; 4], [0; 4], [0; 16], i32::MAX);
    for p0 in 0..2 {
        for p1 in 0..2 {
            let (a, b) = (quantize(e0, p0), quantize(e1, p1));
            let (idx, err) = pick(px, a, b);
            if err < best.3 {
                best = (a, b, idx, err);
            }
        }
    }
    best
}

/// Endpoints that best fit these steps (least squares, each channel on its own).
fn refit(px: &Block, idx: &[u8; 16]) -> Option<([f32; 4], [f32; 4])> {
    let (mut aa, mut ab, mut bb) = (0f32, 0f32, 0f32);
    let (mut pa, mut pb) = ([0f32; 4], [0f32; 4]);
    for (p, &k) in px.iter().zip(idx) {
        let w = WEIGHTS[k as usize] as f32 / 64.0;
        let v = 1.0 - w;
        aa += v * v;
        ab += v * w;
        bb += w * w;
        for c in 0..4 {
            pa[c] += v * p[c] as f32;
            pb[c] += w * p[c] as f32;
        }
    }
    let det = aa * bb - ab * ab;
    if det.abs() < 1e-6 {
        return None;
    }
    let e0 = std::array::from_fn(|c| (bb * pa[c] - ab * pb[c]) / det);
    let e1 = std::array::from_fn(|c| (aa * pb[c] - ab * pa[c]) / det);
    Some((e0, e1))
}

/// The line the block's colours run along: the mean and the main direction (power iteration on the covariance).
fn line(px: &Block) -> ([f32; 4], [f32; 4], f32, f32) {
    let mut mean = [0f32; 4];
    for p in px {
        for c in 0..4 {
            mean[c] += p[c] as f32 / 16.0;
        }
    }
    let mut cov = [[0f32; 4]; 4];
    for p in px {
        let d: [f32; 4] = std::array::from_fn(|c| p[c] as f32 - mean[c]);
        for i in 0..4 {
            for j in 0..4 {
                cov[i][j] += d[i] * d[j];
            }
        }
    }
    let mut axis = [1f32, 1.0, 1.0, 0.0];
    for _ in 0..8 {
        let next: [f32; 4] = std::array::from_fn(|i| (0..4).map(|j| cov[i][j] * axis[j]).sum());
        let n = next.iter().map(|x| x * x).sum::<f32>().sqrt();
        if n < 1e-6 {
            break;
        }
        axis = next.map(|x| x / n);
    }
    let (mut lo, mut hi) = (0f32, 0f32);
    for p in px {
        let t = (0..4).map(|c| (p[c] as f32 - mean[c]) * axis[c]).sum::<f32>();
        lo = lo.min(t);
        hi = hi.max(t);
    }
    (mean, axis, lo, hi)
}

/// One BC7 mode-6 block.
pub fn encode(px: &Block) -> [u8; 16] {
    let (mean, axis, lo, hi) = line(px);
    let e0 = std::array::from_fn(|c| mean[c] + axis[c] * lo);
    let e1 = std::array::from_fn(|c| mean[c] + axis[c] * hi);
    let mut best = best_fit(px, e0, e1);
    // Two rounds of fitting the endpoints to the steps the pixels picked.
    for _ in 0..2 {
        if best.3 == 0 {
            break;
        }
        let Some((e0, e1)) = refit(px, &best.2) else { break };
        let next = best_fit(px, e0, e1);
        if next.3 >= best.3 {
            break;
        }
        best = next;
    }
    let (mut a, mut b, mut idx, _) = best;
    // The first pixel's step is stored in 3 bits, so it must be in the lower half: swap the ends if it isn't.
    if idx[0] >= 8 {
        std::mem::swap(&mut a, &mut b);
        idx = idx.map(|k| 15 - k);
    }
    pack(a, b, &idx)
}

fn pack(a: [u8; 4], b: [u8; 4], idx: &[u8; 16]) -> [u8; 16] {
    let mut bits: u128 = 1 << 6;
    let mut at = 7;
    let mut put = |v: u128, n: u32| {
        bits |= v << at;
        at += n;
    };
    for c in 0..4 {
        put((a[c] >> 1) as u128, 7);
        put((b[c] >> 1) as u128, 7);
    }
    put((a[0] & 1) as u128, 1);
    put((b[0] & 1) as u128, 1);
    for (i, &k) in idx.iter().enumerate() {
        put(k as u128, if i == 0 { 3 } else { 4 });
    }
    bits.to_le_bytes()
}

/// Decodes a mode-6 block (for tests).
#[cfg(test)]
pub fn decode(block: &[u8; 16]) -> Block {
    let bits = u128::from_le_bytes(*block);
    assert_eq!(bits & 0x7F, 1 << 6, "mode 6");
    let mut at = 7;
    let mut get = |n: u32| {
        let v = (bits >> at) & ((1 << n) - 1);
        at += n;
        v as u8
    };
    let (mut a, mut b) = ([0u8; 4], [0u8; 4]);
    for c in 0..4 {
        a[c] = get(7) << 1;
        b[c] = get(7) << 1;
    }
    let (p0, p1) = (get(1), get(1));
    for c in 0..4 {
        a[c] |= p0;
        b[c] |= p1;
    }
    let pal = palette(a, b);
    std::array::from_fn(|i| {
        let k = get(if i == 0 { 3 } else { 4 }) as usize;
        pal[k].map(|v| v as u8)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worst(px: &Block) -> i32 {
        let out = decode(&encode(px));
        px.iter().zip(&out).flat_map(|(p, q)| (0..4).map(move |c| (p[c] as i32 - q[c] as i32).abs())).max().unwrap_or(0)
    }

    #[test]
    fn flat_colours_come_back_nearly_exact() {
        for c in [[255, 0, 0, 255], [0, 0, 0, 0], [37, 140, 201, 255], [128, 128, 128, 77]] {
            assert!(worst(&[c; 16]) <= 1, "{c:?}");
        }
    }

    #[test]
    fn gradients_and_alpha_ramps_stay_close() {
        let ramp: Block = std::array::from_fn(|i| {
            let t = i as u8 * 16;
            [t, 255 - t, 40 + t / 4, t]
        });
        assert!(worst(&ramp) <= 6, "worst {}", worst(&ramp));
        // A subtle dark ramp (where DXT1 bands): BC7 keeps the steps.
        let dark: Block = std::array::from_fn(|i| [20 + (i % 4) as u8 * 3, 30 + (i % 4) as u8 * 2, 45 + (i % 4) as u8 * 2, 255]);
        assert!(worst(&dark) <= 2, "worst {}", worst(&dark));
    }

    #[test]
    fn first_step_is_in_the_low_half() {
        let px: Block = std::array::from_fn(|i| if i == 0 { [255; 4] } else { [0, 0, 0, 255] });
        // Pixel 0 is the bright one, so the ends get swapped; decoding gives the block back.
        assert!(worst(&px) <= 1);
    }
}
