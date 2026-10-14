// Rutt-Etra scan processor (after the 1972 analog original and vade's v002 plugin).
// The source is sampled in the vertex stage: a grid of points displaced by brightness (or by
// RGB as XYZ), rotated in 3D, drawn as additive lines or points so overlaps glow like a CRT beam.
//   P(0) scanlines  P(1) points per line  P(2) extrude  P(3) rotate X  P(4) rotate Y
//   P(5) zoom  P(6) line width (handled by target size)  P(7) brightness  P(8) source colour
//   P(9) perspective  P(10) draw: 0 scanlines, 1 mesh, 2 triangle mesh, 3 vertical lines
//   P(11) displace: 0 luminance, 1 RGB -> XYZ   P(12) displace from another layer (tex2) when > 0.5

struct RuttOut {
    @builtin(position) pos: vec4f,
    @location(0) color: vec4f,
};

fn rutt_vertex(row: u32, col: u32, rows: u32, cols: u32) -> RuttOut {
    let uv = vec2f(f32(col) / f32(max(cols, 2u) - 1u), f32(row) / f32(max(rows, 2u) - 1u));
    let c = textureSampleLevel(tex0, samp, uv, 0.0);
    var d = c;
    if (P(12u) > 0.5) {
        d = textureSampleLevel(tex2, samp, uv, 0.0);
    }
    let l = luma(d.rgb);
    let asp = aspect();

    var p = vec3f((uv.x - 0.5) * asp, 0.5 - uv.y, 0.0);
    if (P(11u) > 0.5) {
        p = p + (d.rgb - 0.5) * P(2u) * vec3f(0.5, 0.5, 1.0);
    } else {
        p.y = p.y + l * P(2u) * 0.5;
        p.z = l * P(2u) * 0.5;
    }
    let ax = P(3u) * TAU * 0.5;
    let ay = P(4u) * TAU * 0.5;
    let cx = cos(ax); let sx = sin(ax);
    p = vec3f(p.x, p.y * cx - p.z * sx, p.y * sx + p.z * cx);
    let cy = cos(ay); let sy = sin(ay);
    p = vec3f(p.x * cy + p.z * sy, p.y, -p.x * sy + p.z * cy);
    let k = P(5u) / max(1.0 - p.z * P(9u) * 0.5, 0.2);

    var o: RuttOut;
    o.pos = vec4f(p.x * k / asp * 2.0, p.y * k * 2.0, 0.0, 1.0);
    let beam = mix(vec3f(1.0), c.rgb, P(8u)) * P(7u) * (0.3 + 0.7 * l);
    o.color = vec4f(beam, 1.0);
    return o;
}

// Line list: horizontal segments, then vertical, then diagonals, depending on the draw mode.
@vertex
fn vs_rutt(@builtin(vertex_index) vi: u32) -> RuttOut {
    let rows = max(u32(P(0u)), 2u);
    let cols = max(u32(P(1u)), 2u);
    let mode = u32(P(10u));
    let h_count = select(rows * (cols - 1u), 0u, mode == 3u);
    let v_count = select(0u, cols * (rows - 1u), mode >= 1u);
    let seg = vi / 2u;
    let e = vi & 1u;
    var row = 0u;
    var col = 0u;
    if (seg < h_count) {
        row = seg / (cols - 1u);
        col = seg % (cols - 1u) + e;
    } else if (seg < h_count + v_count) {
        let s = seg - h_count;
        col = s / (rows - 1u);
        row = s % (rows - 1u) + e;
    } else {
        let s = seg - h_count - v_count;
        row = s / (cols - 1u) + e;
        col = s % (cols - 1u) + e;
    }
    return rutt_vertex(row, col, rows, cols);
}

@vertex
fn vs_rutt_points(@builtin(vertex_index) vi: u32) -> RuttOut {
    let rows = max(u32(P(0u)), 2u);
    let cols = max(u32(P(1u)), 2u);
    return rutt_vertex(vi / cols, vi % cols, rows, cols);
}

@fragment
fn fs_rutt(in: RuttOut) -> @location(0) vec4f {
    return in.color;
}

@fragment
fn fs_rutt_points(in: RuttOut) -> @location(0) vec4f {
    return in.color;
}
