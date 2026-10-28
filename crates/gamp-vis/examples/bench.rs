use gamp_vis::GDrop;
fn main() {
    let mut g = GDrop::new(356, 198);
    g.auto = false;
    let wave: Vec<f32> = (0..1024).map(|k| (k as f32 * 0.05).sin() * 0.5).collect();
    let spec = vec![0.05f32; 512];
    let t = std::time::Instant::now();
    for _ in 0..600 {
        g.render(&wave, &spec, 48000, 1.0 / 60.0);
    }
    println!("{:.2} ms per frame", t.elapsed().as_secs_f64() * 1000.0 / 600.0);
}
