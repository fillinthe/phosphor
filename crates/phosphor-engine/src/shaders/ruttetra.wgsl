// Rutt-Etra scan processor (after the 1972 analog original and vade's v002 plugin).
// The source is sampled in the vertex stage: each scanline is a strip of points displaced by
// brightness, rotated in 3D, drawn as additive lines so overlaps glow like a CRT beam.
//   P(0) scanlines  P(1) points per line  P(2) extrude  P(3) rotate X  P(4) rotate Y
//   P(5) zoom  P(6) line width (handled by target size)  P(7) brightness  P(8) source colour
//   P(9) perspective

struct RuttOut {
    @builtin(position) pos: vec4f,
    @location(0) color: vec4f,
};

@vertex
fn vs_rutt(@builtin(vertex_index) vi: u32) -> RuttOut {
    let rows = max(u32(P(0u)), 2u);
    let cols = max(u32(P(1u)), 2u);
    let segs = cols - 1u;
    let seg = vi / 2u;
    let row = seg / segs;
    let col = seg % segs + (vi & 1u);
    let uv = vec2f(f32(col) / f32(cols - 1u), f32(row) / f32(rows - 1u));
    let c = textureSampleLevel(tex0, samp, uv, 0.0);
    let l = luma(c.rgb);
    let asp = aspect();

    var p = vec3f((uv.x - 0.5) * asp, 0.5 - uv.y, 0.0);
    p.y = p.y + l * P(2u) * 0.5;
    p.z = l * P(2u) * 0.5;
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

@fragment
fn fs_rutt(in: RuttOut) -> @location(0) vec4f {
    return in.color;
}
