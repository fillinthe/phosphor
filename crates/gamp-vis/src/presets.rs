//! G-Drop's presets: original, each a small function of the music.

use crate::{Features, Params, Point, Preset};

fn hue(h: f32) -> [f32; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let x = 1.0 - ((h % 2.0) - 1.0).abs();
    match h as u32 {
        0 => [1.0, x, 0.0],
        1 => [x, 1.0, 0.0],
        2 => [0.0, 1.0, x],
        3 => [0.0, x, 1.0],
        4 => [x, 0.0, 1.0],
        _ => [1.0, 0.0, x],
    }
}

const AMBER: [f32; 3] = [1.0, 0.7, 0.12];
const GREEN: [f32; 3] = [0.17, 0.93, 0.24];

pub static PRESETS: &[Preset] = &[
    Preset {
        name: "Amber Tunnel",
        frame: |f, p| {
            p.zoom = 1.035 + 0.02 * (f.bass_att - 1.0);
            p.rot = 0.012 * (f.time * 0.4).sin();
            p.decay = 0.965;
            p.tint = [1.0, 0.97, 0.9];
            p.wave_mode = 0;
            p.wave = AMBER;
            p.wave_alpha = 0.9;
            p.wave_thick = true;
        },
        point: None,
    },
    Preset {
        name: "Green Scope Drift",
        frame: |f, p| {
            p.zoom = 1.008;
            p.dy = -0.004;
            p.warp = 0.6;
            p.decay = 0.95;
            p.wave_mode = 1;
            p.wave = GREEN;
            p.wave_y = 0.5 + 0.1 * (f.time * 0.7).sin();
            p.wave_scale = 1.0 + 0.4 * f.mid;
        },
        point: None,
    },
    Preset {
        name: "Kaleido Bloom",
        frame: |f, p| {
            p.zoom = 0.985;
            p.decay = 0.97;
            p.wave_mode = 3;
            p.wave = hue(f.time * 0.05);
            p.wave_alpha = 0.7;
            p.shapes[0].on = true;
            p.shapes[0].sides = 6;
            p.shapes[0].rad = 0.05 + 0.08 * f.bass;
            p.shapes[0].ang = f.time;
            p.shapes[0].color = hue(f.time * 0.05 + 0.5);
            p.shapes[0].alpha = 0.35;
        },
        point: Some(|f, _p, _x, _y, rad, ang, pt| {
            pt.rot = 0.04 * (ang * 6.0 + f.time).sin() * rad;
        }),
    },
    Preset {
        name: "Spiral Galaxy",
        frame: |f, p| {
            p.zoom = 1.02;
            p.zoom_exp = 1.3;
            p.rot = 0.03 + 0.02 * f.treb_att;
            p.decay = 0.975;
            p.wave_mode = 3;
            p.wave = hue(f.time * 0.03);
            p.wave_scale = 1.4;
        },
        point: None,
    },
    Preset {
        name: "Bass Pulse",
        frame: |f, p| {
            p.zoom = 1.0 + 0.07 * (f.bass - 0.8).max(0.0);
            p.decay = 0.93;
            p.wave_mode = 0;
            p.wave = [1.0, 0.3, 0.2];
            p.shapes[0].on = true;
            p.shapes[0].rad = 0.06 + 0.12 * f.bass;
            p.shapes[0].color = [1.0, 0.25, 0.15];
            p.shapes[0].alpha = if f.beat { 0.9 } else { 0.25 };
        },
        point: None,
    },
    Preset {
        name: "Ocean Waves",
        frame: |f, p| {
            p.warp = 2.5;
            p.warp_speed = 0.6;
            p.zoom = 1.005;
            p.decay = 0.96;
            p.tint = [0.9, 0.97, 1.0];
            p.wave_mode = 1;
            p.wave = [0.3, 0.6, 1.0];
            p.wave_y = 0.35 + 0.05 * f.bass_att;
            p.wave_thick = true;
        },
        point: None,
    },
    Preset {
        name: "Fire Rise",
        frame: |f, p| {
            p.dy = 0.011;
            p.warp = 1.4;
            p.decay = 0.955;
            p.tint = [1.0, 0.88, 0.65];
            p.wave_mode = 2;
            p.wave = [1.0, 0.45, 0.1];
            p.wave_y = 0.12;
            p.wave_scale = 1.0 + 0.3 * f.bass_att;
        },
        point: None,
    },
    Preset {
        name: "Hypno Rings",
        frame: |f, p| {
            p.zoom = 0.97;
            p.decay = 0.985;
            p.wave_mode = 0;
            p.wave = hue(f.time * 0.08);
            p.wave_scale = 0.6;
            p.wave_alpha = 0.8;
        },
        point: None,
    },
    Preset {
        name: "Butterfly",
        frame: |f, p| {
            p.zoom = 1.02;
            p.rot = 0.02 * (f.time * 0.5).sin();
            p.decay = 0.955;
            p.wave_mode = 5;
            p.wave = [0.95, 0.35, 0.9];
            p.wave_scale = 1.0 + 0.3 * f.treb;
        },
        point: None,
    },
    Preset {
        name: "Neon Grid",
        frame: |f, p| {
            p.sx = 1.0 + 0.012 * (f.time * 0.9).sin();
            p.sy = 1.0 + 0.012 * (f.time * 0.7).cos();
            p.decay = 0.97;
            p.wave_mode = 3;
            p.wave = [0.2, 1.0, 0.95];
            p.border = 0.01;
            p.border_color = [0.0, 0.2, 0.25];
        },
        point: None,
    },
    Preset {
        name: "Vortex",
        frame: |f, p| {
            p.zoom = 1.03;
            p.decay = 0.97;
            p.wave_mode = 0;
            p.wave = hue(f.time * 0.04 + 0.6);
            p.wave_thick = true;
        },
        point: Some(|f, _p, _x, _y, rad, _ang, pt| {
            pt.rot = (0.12 + 0.05 * f.mid_att) * (1.0 - rad).max(0.0);
        }),
    },
    Preset {
        name: "Starfield",
        frame: |_f, p| {
            p.zoom = 1.05;
            p.zoom_exp = 1.6;
            p.warp = 0.2;
            p.decay = 0.93;
            p.wave_mode = 3;
            p.wave = [1.0, 1.0, 1.0];
            p.wave_scale = 2.2;
        },
        point: None,
    },
    Preset {
        name: "Liquid Mercury",
        frame: |f, p| {
            p.warp = 3.2;
            p.warp_speed = 0.5;
            p.decay = 0.975;
            p.gamma = 1.4;
            p.wave_mode = 0;
            p.wave = [0.8, 0.85, 0.9];
            p.wave_scale = 0.9 + 0.3 * f.vol;
        },
        point: None,
    },
    Preset {
        name: "Heartbeat",
        frame: |f, p| {
            p.zoom = 0.99;
            p.decay = 0.94;
            p.wave_mode = 1;
            p.wave = [1.0, 0.2, 0.3];
            p.shapes[0].on = f.beat || f.bass > 1.2;
            p.shapes[0].sides = 6;
            p.shapes[0].rad = 0.18;
            p.shapes[0].color = [1.0, 0.1, 0.2];
            p.shapes[0].alpha = 0.6;
        },
        point: None,
    },
    Preset {
        name: "Aurora",
        frame: |f, p| {
            p.dy = -0.003;
            p.warp = 2.0;
            p.decay = 0.975;
            p.tint = [0.96, 1.0, 0.98];
            p.wave_mode = 1;
            p.wave = hue(0.35 + 0.15 * (f.time * 0.2).sin());
            p.wave_y = 0.72;
            p.wave_thick = true;
        },
        point: None,
    },
    Preset {
        name: "Pinwheel",
        frame: |f, p| {
            p.zoom = 1.01;
            p.decay = 0.965;
            p.wave_mode = 3;
            p.wave = AMBER;
            p.shapes[0].on = true;
            p.shapes[0].sides = 3;
            p.shapes[0].ang = f.time * 1.5;
            p.shapes[0].rad = 0.12 + 0.06 * f.treb;
            p.shapes[0].color = hue(f.time * 0.06);
            p.shapes[0].alpha = 0.4;
        },
        point: Some(|f, _p, _x, _y, rad, ang, pt| {
            pt.rot = 0.03 * (ang * 4.0 - f.time).sin() * rad;
        }),
    },
    Preset {
        name: "Deep Space",
        frame: |f, p| {
            p.zoom = 1.015;
            p.rot = 0.005;
            p.decay = 0.99;
            p.wave_mode = 3;
            p.wave = hue(0.6 + 0.1 * f.treb_att);
            p.wave_alpha = 0.5;
        },
        point: None,
    },
    Preset {
        name: "Plasma Pool",
        frame: |f, p| {
            p.warp = 4.0;
            p.warp_scale = 0.5;
            p.decay = 0.96;
            p.wave_mode = 0;
            p.wave_alpha = 0.3;
            p.wave = hue(f.time * 0.1 + 0.3);
            p.shapes[0].on = true;
            p.shapes[0].x = 0.5 + 0.25 * (f.time * 0.4).sin();
            p.shapes[0].y = 0.5 + 0.25 * (f.time * 0.31).cos();
            p.shapes[0].rad = 0.1 + 0.05 * f.bass;
            p.shapes[0].color = hue(f.time * 0.1);
            p.shapes[0].alpha = 0.5;
        },
        point: None,
    },
    Preset {
        name: "Equalizer Rain",
        frame: |_f, p| {
            // Content falls: each frame pulls from just above.
            p.dy = 0.012;
            p.decay = 0.95;
            p.wave_mode = 2;
            p.wave = GREEN;
            p.wave_y = 0.92;
            p.wave_scale = 0.8;
            p.wrap = false;
        },
        point: None,
    },
    Preset {
        name: "Mirror Hall",
        frame: |f, p| {
            p.zoom = 1.012;
            p.decay = 0.965;
            p.wave_mode = 4;
            p.wave = hue(f.time * 0.07 + 0.2);
            p.wave_x = 0.5;
        },
        point: Some(|_f, _p, x, _y, _rad, _ang, pt| {
            pt.dx = (x - 0.5).signum() * 0.006;
        }),
    },
    Preset {
        name: "Solar Flare",
        frame: |f, p| {
            p.zoom = 1.03 + 0.03 * f.treb_att;
            p.decay = 0.96;
            p.wave_mode = 0;
            p.wave = AMBER;
            p.wave_thick = true;
            p.wave_scale = 1.3;
            p.border = 0.006;
            p.border_color = [0.25, 0.12, 0.0];
        },
        point: None,
    },
    Preset {
        name: "Wobble Scope",
        frame: |f, p| {
            p.warp = 1.6;
            p.rot = 0.008 * (f.time * 0.3).cos();
            p.decay = 0.955;
            p.wave_mode = 4;
            p.wave = [0.6, 0.9, 1.0];
            p.wave_x = 0.5 + 0.2 * (f.time * 0.6).sin();
        },
        point: None,
    },
    Preset {
        name: "Night Drive",
        frame: |f, p| {
            p.zoom = 1.025;
            p.cx = 0.5 + 0.1 * (f.time * 0.25).sin();
            p.cy = 0.5 + 0.06 * (f.time * 0.21).cos();
            p.decay = 0.97;
            p.tint = [0.98, 0.94, 1.0];
            p.wave_mode = 1;
            p.wave = [0.75, 0.4, 1.0];
        },
        point: None,
    },
    Preset {
        name: "Crystal Shards",
        frame: |f, p| {
            p.zoom = 0.985;
            p.decay = 0.975;
            p.wave_mode = 5;
            p.wave = [0.7, 0.95, 1.0];
            for (k, s) in p.shapes.iter_mut().enumerate() {
                s.on = true;
                s.sides = 5;
                s.ang = f.time * if k == 0 { 0.8 } else { -1.1 };
                s.rad = 0.08 + 0.05 * f.mid;
                s.x = 0.5 + 0.2 * (f.time * 0.5 + k as f32 * 3.1).sin();
                s.y = 0.5 + 0.2 * (f.time * 0.4 + k as f32 * 1.7).cos();
                s.color = hue(f.time * 0.05 + k as f32 * 0.5);
                s.alpha = 0.3;
            }
        },
        point: None,
    },
    Preset {
        name: "Llama Glow",
        frame: |f, p| {
            p.zoom = 1.02;
            p.rot = 0.01 * (f.time * 0.2).sin();
            p.decay = 0.97;
            p.tint = [1.0, 0.98, 0.92];
            p.wave_mode = if (f.time / 8.0) as u32 % 2 == 0 { 1 } else { 0 };
            p.wave = if p.wave_mode == 1 { GREEN } else { AMBER };
            p.wave_thick = true;
        },
        point: None,
    },
];

#[allow(dead_code)]
fn unused(_: &Features, _: &mut Params, _: &mut Point) {}
