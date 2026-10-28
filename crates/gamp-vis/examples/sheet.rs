//! Renders every preset with a synthetic beat and writes PPM files:
//! `cargo run --release -p gamp-vis --example sheet -- out/`
use gamp_vis::{GDrop, PRESETS};

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    std::fs::create_dir_all(&dir).unwrap();
    for (i, p) in PRESETS.iter().enumerate() {
        let (w, h) = std::env::var("SIZE").ok().and_then(|s| { let (a, b) = s.split_once('x')?; Some((a.parse().ok()?, b.parse().ok()?)) }).unwrap_or((240, 180));
        let mut g = GDrop::new(w, h);
        g.auto = false;
        g.select(i, true);
        for f in 0..240 {
            let t = f as f32 / 30.0;
            let wave: Vec<f32> = (0..512).map(|k| (k as f32 * 0.05 + t * 7.0).sin() * 0.5 + (k as f32 * 0.31).sin() * 0.2).collect();
            let mut spec = vec![0.01f32; 256];
            if (t * 2.0).fract() < 0.12 {
                spec[2] = 0.9;
                spec[3] = 0.7;
            }
            for (k, s) in spec.iter_mut().enumerate().skip(10) {
                *s = 0.15 / (k as f32 / 10.0) * (1.0 + (t * 3.0 + k as f32).sin() * 0.3);
            }
            g.render(&wave, &spec, 44100, 1.0 / 30.0);
        }
        let (w, h) = g.size();
        let mut out = format!("P6 {w} {h} 255\n").into_bytes();
        for px in &g.out {
            out.extend_from_slice(&[(px >> 16) as u8, (px >> 8) as u8, *px as u8]);
        }
        std::fs::write(format!("{dir}/{i:02}.ppm"), out).unwrap();
        println!("{i:02} {}", p.name);
    }
}
