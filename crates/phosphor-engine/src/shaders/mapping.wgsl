// Projection mapping. One draw of 6 vertices per surface, blended onto the black output.
//   u.p[0].xy, u.p[0].zw, u.p[1].xy, u.p[1].zw = corners TL, TR, BR, BL (output space 0..1, y down)
//   u.p[2], u.p[3], u.p[4].x = inverse homography, row-major 3x3, output -> surface (s, t)
//   u.uv_xform = input region (w, h, x, y) of the master frame; u.p[4].y = feather
//   u.opacity = surface opacity * master fade; u.mode = 1 draws the alignment grid
//   u.p[5], u.p[6] = output colour; u.p[7], u.p[8] = surface colour (`ColorGrade::uniforms`)

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
    // The surface's own colour (u.p[7] / u.p[8]), then the output's.
    var rgb = grade(grade_with(textureSampleLevel(tex0, samp, uv, 0.0).rgb, u.p[7], u.p[8]));
    if (u.mode == 1u) {
        rgb = alignment_grid(st);
    }
    let f = max(u.p[4].y, 1e-4);
    // Soft edge, shaped by the blend curve (u.p[4].z + 1) so overlapping projectors even out.
    var edge = smoothstep(0.0, f, st.x) * smoothstep(0.0, f, 1.0 - st.x) * smoothstep(0.0, f, st.y) * smoothstep(0.0, f, 1.0 - st.y);
    // Per side (u.p[12].x on): left, right, top, bottom widths in u.p[11]; a side at 0 stays hard.
    if (u.p[12].x > 0.5) {
        let e = max(u.p[11], vec4f(1e-4));
        edge = smoothstep(0.0, e.x, st.x) * smoothstep(0.0, e.y, 1.0 - st.x) * smoothstep(0.0, e.z, st.y) * smoothstep(0.0, e.w, 1.0 - st.y);
    }
    // Slice shape (u.p[4].w): 1 = ellipse, 2 = triangle (apex at the top), each with the same soft edge.
    let shape = u32(u.p[4].w + 0.5);
    if (shape == 1u) {
        let d = 0.5 - length(st - vec2f(0.5));
        edge = smoothstep(-fwidth(d), max(f, fwidth(d)), d);
    } else if (shape == 2u) {
        // Distance inside the two slanted sides and the base.
        let side = (st.y - 2.0 * abs(st.x - 0.5)) / sqrt(5.0);
        let d = min(side, 1.0 - st.y);
        edge = smoothstep(-fwidth(d), max(f, fwidth(d)), d);
    }
    let soft = pow(edge, 1.0 + u.p[4].z);
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

// Alignment grid over an output (`GridOverlay`): square cells (p[0].x across), a centre crosshair with a circle
// (p[0].z), lines p[0].y pixels wide, colour p[1]. Drawn with alpha blending over the finished output.
@fragment
fn fs_grid(in: VOut) -> @location(0) vec4f {
    let px = in.uv * u.resolution;
    let cell = u.resolution.x / max(u.p[0].x, 1.0);
    let half = u.p[0].y * 0.5;
    // Distance to the nearest grid line, in pixels (both axes).
    let gx = abs(fract(px.x / cell + 0.5) - 0.5) * cell;
    let gy = abs(fract(px.y / cell + 0.5) - 0.5) * cell;
    var a = max(1.0 - smoothstep(half - 0.5, half + 0.5, min(gx, gy)), 0.0) * 0.55;
    if u.p[0].z > 0.5 {
        let c = u.resolution * 0.5;
        let d = px - c;
        // Crosshair through the centre (thicker), a circle of a quarter of the height, and the two diagonals.
        let cross = min(abs(d.x), abs(d.y));
        let ring = abs(length(d) - u.resolution.y * 0.25);
        let diag = min(abs(d.x - d.y), abs(d.x + d.y)) * 0.7071;
        let strong = 1.0 - smoothstep(half + 0.5, half + 1.5, min(cross, ring));
        let faint = (1.0 - smoothstep(half - 0.5, half + 0.5, diag)) * 0.35;
        a = max(a, max(strong, faint));
    }
    // Outer border, so the projector's edge is obvious.
    let edge = min(min(px.x, px.y), min(u.resolution.x - px.x, u.resolution.y - px.y));
    a = max(a, 1.0 - smoothstep(u.p[0].y * 1.5, u.p[0].y * 1.5 + 1.0, edge));
    return vec4f(u.p[1].rgb * a, a);
}
