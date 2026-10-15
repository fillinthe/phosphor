// Projection mapping. One draw of 6 vertices per surface, blended onto the black output.
//   u.p[0].xy, u.p[0].zw, u.p[1].xy, u.p[1].zw = corners TL, TR, BR, BL (output space 0..1, y down)
//   u.p[2], u.p[3], u.p[4].x = inverse homography, row-major 3x3, output -> surface (s, t)
//   u.uv_xform = input region (w, h, x, y) of the master frame; u.p[4].y = feather
//   u.opacity = surface opacity * master fade; u.mode = 1 draws the alignment grid

@vertex
fn vs_surface(@builtin(vertex_index) vi: u32) -> VOut {
    var order = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);
    let k = order[vi];
    var c = u.p[0].xy;
    if (k == 1u) { c = u.p[0].zw; }
    if (k == 2u) { c = u.p[1].xy; }
    if (k == 3u) { c = u.p[1].zw; }
    var o: VOut;
    o.pos = vec4f(c.x * 2.0 - 1.0, 1.0 - c.y * 2.0, 0.0, 1.0);
    o.uv = c;
    return o;
}

fn alignment_grid(st: vec2f) -> vec3f {
    let cells = st * 8.0;
    let g = abs(fract(cells - 0.5) - 0.5) / fwidth(cells);
    let line = 1.0 - min(min(g.x, g.y), 1.0);
    let edge = min(min(st.x, 1.0 - st.x), min(st.y, 1.0 - st.y));
    let border = step(edge, 0.012);
    let cross = max(step(abs(st.x - 0.5), 0.004), step(abs(st.y - 0.5), 0.004));
    var c = mix(vec3f(0.10, 0.10, 0.12), vec3f(0.6), line);
    // Top-left corner marker so you can tell orientation.
    let marker = step(length(st - vec2f(0.08, 0.08)), 0.035);
    c = mix(c, vec3f(0.47, 1.0, 0.67), clamp(border + cross, 0.0, 1.0));
    c = mix(c, vec3f(1.0, 0.45, 0.3), marker);
    return c;
}

/// What a surface shows at surface coordinate `st` (shared by corner-pinned and mesh surfaces):
/// its input region of the frame (or the alignment grid), feathered edges, premultiplied.
fn surface_color(st: vec2f) -> vec4f {
    let uv = u.uv_xform.zw + st * u.uv_xform.xy;
    var rgb = grade(textureSampleLevel(tex0, samp, uv, 0.0).rgb);
    if (u.mode == 1u) {
        rgb = alignment_grid(st);
    }
    let f = max(u.p[4].y, 1e-4);
    // Soft edge, shaped by the blend curve (u.p[4].z + 1) so overlapping projectors even out.
    let soft = pow(smoothstep(0.0, f, st.x) * smoothstep(0.0, f, 1.0 - st.x)
        * smoothstep(0.0, f, st.y) * smoothstep(0.0, f, 1.0 - st.y), 1.0 + u.p[4].z);
    let inside = select(0.0, 1.0, all(st >= vec2f(-1e-4)) && all(st <= vec2f(1.0 + 1e-4)));
    let a = u.opacity * soft * inside;
    return vec4f(rgb * a, a);
}

@fragment
fn fs_surface(in: VOut) -> @location(0) vec4f {
    let q = in.pos.xy / u.resolution;
    let h = mat3x3f(
        vec3f(u.p[2].x, u.p[2].w, u.p[3].z),
        vec3f(u.p[2].y, u.p[3].x, u.p[3].w),
        vec3f(u.p[2].z, u.p[3].y, u.p[4].x),
    );
    let r = h * vec3f(q, 1.0);
    return surface_color(r.xy / r.z);
}

// Mesh warp and masks: triangles from a vertex buffer, (x, y) in output space 0..1 (y down) and
// (s, t) the surface coordinate.
struct MeshIn {
    @location(0) pos: vec2f,
    @location(1) st: vec2f,
};

@vertex
fn vs_mesh(v: MeshIn) -> VOut {
    var o: VOut;
    o.pos = vec4f(v.pos.x * 2.0 - 1.0, 1.0 - v.pos.y * 2.0, 0.0, 1.0);
    o.uv = v.st;
    return o;
}

@fragment
fn fs_mesh(in: VOut) -> @location(0) vec4f {
    return surface_color(in.uv);
}

@fragment
fn fs_mask(in: VOut) -> @location(0) vec4f {
    // Premultiplied black at the vertex's darkness (1 inside, fading across a feathered edge).
    return vec4f(0.0, 0.0, 0.0, clamp(in.uv.x, 0.0, 1.0));
}
