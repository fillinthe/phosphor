use super::*;

const GENERATOR: &str = r#"/*{
    "DESCRIPTION": "test generator",
    "CREDIT": "phosphor",
    "CATEGORIES": ["Generator", "Test"],
    "INPUTS": [
        { "NAME": "speed", "TYPE": "float", "MIN": 0.0, "MAX": 4.0, "DEFAULT": 1.0 },
        { "NAME": "invert", "TYPE": "bool", "DEFAULT": true },
    ]
}*/
// comment with /* tricky */ bits
float wave(float x) { return 0.5 + 0.5 * sin(x); }
void main() {
    vec2 uv = isf_FragNormCoord;
    float v = wave(uv.x * 10.0 + TIME * speed);
    if (invert) v = 1.0 - v;
    gl_FragColor = vec4(vec3(v) * gl_FragCoord.y / RENDERSIZE.y, 1.0);
}
"#;

const FILTER: &str = r#"/*{
    "ISFVSN": "2",
    "INPUTS": [
        { "NAME": "inputImage", "TYPE": "image" },
        { "NAME": "center", "TYPE": "point2D", "DEFAULT": [0.5, 0.5], "MIN": [0, 0], "MAX": [1, 1] },
        { "NAME": "amount", "TYPE": "float", "DEFAULT": 0.25 }
    ],
    "PASSES": [ { } ]
}*/
void main() {
    vec2 d = isf_FragNormCoord - center;
    vec4 a = IMG_NORM_PIXEL(inputImage, isf_FragNormCoord - d * amount);
    vec4 b = IMG_PIXEL(inputImage, gl_FragCoord.xy);
    vec4 c = IMG_THIS_PIXEL(inputImage);
    vec2 sz = IMG_SIZE(inputImage);
    gl_FragColor = mix(a, b + c, amount) * step(1.0, sz.x);
}
"#;

const MENU_COLOR: &str = r#"/*{
    "INPUTS": [
        { "NAME": "tint", "TYPE": "color", "DEFAULT": [1.0, 0.5, 0.25, 1.0] },
        { "NAME": "mode", "TYPE": "long", "VALUES": [0, 1, 2], "LABELS": ["A", "B", "C"], "DEFAULT": 1 },
        { "NAME": "mixAmt", "TYPE": "float", "MIN": 0., "MAX": 1., "DEFAULT": .5 },
    ],
}*/
void main() {
    vec4 c = tint;
    if (mode == 1) c.rgb = c.bgr;
    else if (mode == 2) c.rgb = vec3(1.0) - c.rgb;
    gl_FragColor = mix(vec4(isf_FragNormCoord, 0.0, 1.0), c, mixAmt);
}
"#;

#[test]
fn header_parser_handles_relaxed_json() {
    let (json, body) = header::split_header(MENU_COLOR).unwrap();
    assert!(body.trim_start().starts_with("void main"));
    let h = header::parse_header(json).unwrap();
    assert_eq!(h.inputs.len(), 3);
    assert_eq!(h.inputs[0].kind, IsfInputKind::Color { default: [1.0, 0.5, 0.25, 1.0] });
    assert_eq!(
        h.inputs[1].kind,
        IsfInputKind::Long { values: vec![0, 1, 2], labels: vec!["A".into(), "B".into(), "C".into()], default: 1 }
    );
    assert_eq!(h.inputs[2].kind, IsfInputKind::Float { min: 0.0, max: 1.0, default: 0.5 });
    assert_eq!(h.inputs[2].label, "mixAmt");

    let (json, _) = header::split_header(GENERATOR).unwrap();
    let h = header::parse_header(json).unwrap();
    assert_eq!(h.description, "test generator");
    assert_eq!(h.credit, "phosphor");
    assert_eq!(h.categories, vec!["Generator".to_string(), "Test".to_string()]);
    assert_eq!(h.inputs[1].kind, IsfInputKind::Bool { default: true });
    assert_eq!(h.passes, 0);

    assert_eq!(header::clean_json(r#"{"a": 1., "b": .5, "c": [1,2,], // x
    }"#), "{\"a\": 1.0, \"b\": 0.5, \"c\": [1,2] \n    }");
    assert!(matches!(header::split_header("void main() {}"), Err(IsfError::Header(_))));
    assert!(matches!(header::split_header("/* not json */ void main() {}"), Err(IsfError::Header(_))));
}

#[test]
fn generator_converts_and_validates() {
    let s = import("Test Gen", GENERATOR, None).unwrap();
    assert_eq!(s.entry, "isf_test_gen");
    assert!(s.unsupported.is_none(), "{:?}", s.unsupported);
    assert!(!s.is_filter);
    assert_eq!(s.passes, 1);
    assert_eq!(s.param_count(), 2);
    assert_eq!(s.inputs[0].offset, Some(0));
    assert_eq!(s.inputs[1].offset, Some(1));
    assert_eq!(&s.default_params()[..2], &[1.0, 1.0]);
    validate_wgsl(&s.wgsl).unwrap();
    assert!(s.wgsl.contains("@fragment\nfn isf_test_gen(in: VOut) -> @location(0) vec4f"));
    // Helpers carry the entry prefix; Phosphor's globals are referenced unprefixed and not redeclared.
    assert!(s.wgsl.contains("fn isf_test_gen_wave("));
    assert!(s.wgsl.contains("u.p[0]"));
    assert!(!s.wgsl.contains("struct U"));
    assert!(!s.wgsl.contains("@binding"));
    // Two copies of the same shader under different names coexist in one module.
    let t = import("Other", GENERATOR, None).unwrap();
    validate_wgsl(&format!("{}\n{}", s.wgsl, t.wgsl)).unwrap();
}

#[test]
fn filter_with_input_image_converts_and_validates() {
    let s = import("blur-ish", FILTER, None).unwrap();
    assert!(s.unsupported.is_none(), "{:?}", s.unsupported);
    assert!(s.is_filter);
    assert_eq!(s.passes, 1);
    assert_eq!(s.inputs[0].texture, Some(0));
    assert_eq!(s.inputs[1].offset, Some(0));
    assert_eq!(s.inputs[2].offset, Some(2));
    assert_eq!(s.param_count(), 3);
    validate_wgsl(&s.wgsl).unwrap();
    assert!(s.wgsl.contains("textureSampleLevel(tex0, samp,"));
    assert!(s.wgsl.contains("textureDimensions(tex0,"));
}

#[test]
fn long_menu_and_color_convert_and_validate() {
    let s = import("menu", MENU_COLOR, None).unwrap();
    assert!(s.unsupported.is_none(), "{:?}", s.unsupported);
    assert_eq!(s.param_count(), 6);
    assert_eq!(s.inputs[0].offset, Some(0));
    assert_eq!(s.inputs[1].offset, Some(4));
    assert_eq!(s.inputs[2].offset, Some(5));
    assert_eq!(&s.default_params()[..6], &[1.0, 0.5, 0.25, 1.0, 1.0, 0.5]);
    validate_wgsl(&s.wgsl).unwrap();
    assert!(s.wgsl.contains("u.p[1]"));
}

#[test]
fn unsupported_features_are_reported_not_errors() {
    // Multi-pass and persistent buffers are supported: passes and buffers come through.
    let multipass = GENERATOR.replace("\"INPUTS\"", "\"PASSES\": [{\"TARGET\": \"a\", \"WIDTH\": \"$WIDTH/2\"}, {}], \"INPUTS\"");
    let s = import("mp", &multipass, None).unwrap();
    assert_eq!(s.passes, 2);
    assert!(s.unsupported.is_none(), "{:?}", s.unsupported);
    assert_eq!(s.buffers, vec!["a".to_string()]);
    assert_eq!(s.pass_plan[0].width.as_deref(), Some("$WIDTH/2"));
    assert!(s.pass_plan[1].target.is_none());
    assert_eq!(s.inputs.len(), 2);

    let persistent = GENERATOR.replace("\"INPUTS\"", "\"PASSES\": [{\"TARGET\": \"a\", \"PERSISTENT\": true}], \"INPUTS\"");
    let s = import("fb", &persistent, None).unwrap();
    assert!(s.unsupported.is_none(), "{:?}", s.unsupported);
    assert!(s.pass_plan[0].persistent);

    // More buffers than bindings is still reported.
    let many_bufs: String = (0..=crate::MAX_BUFFERS).map(|i| format!("{{\"TARGET\": \"b{i}\"}},")).collect();
    let too_many_bufs = GENERATOR.replace("\"INPUTS\"", &format!("\"PASSES\": [{many_bufs} {{}}], \"INPUTS\""));
    let s = import("bufs", &too_many_bufs, None).unwrap();
    assert!(s.unsupported.as_deref().unwrap_or("").contains("buffers"), "{:?}", s.unsupported);

    // One more colour (4 floats each) than fits.
    let many: String = (0..crate::MAX_PARAMS / 4 + 1).map(|i| format!("{{ \"NAME\": \"c{i}\", \"TYPE\": \"color\" }},")).collect();
    let too_many = GENERATOR.replace("\"INPUTS\": [", &format!("\"INPUTS\": [{many}"));
    let s = import("big", &too_many, None).unwrap();
    assert!(s.unsupported.as_deref().unwrap_or("").contains(&format!("max {}", crate::MAX_PARAMS)), "{:?}", s.unsupported);
}

#[test]
fn vertex_shader_is_inlined() {
    let vs = "varying vec2 off;\nvoid main(void) { isf_vertShaderInit(); off = isf_FragNormCoord + vec2(0.01, 0.0); }\n";
    let fs = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ] }*/\n\
#if __VERSION__ <= 120\nvarying vec2 off;\n#else\nin vec2 off;\n#endif\n\
void main() { gl_FragColor = IMG_NORM_PIXEL(inputImage, off); }\n";
    let s = import("vs", fs, Some(vs)).unwrap();
    assert!(s.unsupported.is_none(), "{:?}", s.unsupported);
    validate_wgsl(&s.wgsl).unwrap();
    assert!(s.wgsl.contains("fn isf_vs_phosphor_vs_main("));

    // A vertex shader that moves the quad gets its own vertex stage.
    let geometry = "void main() { isf_vertShaderInit(); gl_Position = gl_Position * 0.5; }";
    let s = import("vs2", fs, Some(geometry)).unwrap();
    assert!(s.unsupported.is_none(), "{:?}", s.unsupported);
    validate_wgsl(&s.wgsl).unwrap();
    let v = s.vertex_entry.as_deref().expect("vertex entry");
    assert!(s.wgsl.contains(&format!("@vertex\nfn {v}(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> VOut")), "{}", s.wgsl);
    // Its own varyings reach the fragment stage by location; copies drawn by `RENDER_SETTINGS.INSTANCE_COUNT`.
    let own = "varying vec2 off;\nvoid main() { isf_vertShaderInit(); off = vec2(float(gl_InstanceID), 0.0); gl_Position = gl_Position * 0.5; }";
    let fs_off = "/*{\"INPUTS\":[{\"NAME\":\"count\",\"TYPE\":\"float\",\"DEFAULT\":4}],\"RENDER_SETTINGS\":{\"INSTANCE_COUNT\":\"count\"}}*/\nvarying vec2 off;\nvoid main() { gl_FragColor = vec4(off, 0.0, 1.0); }";
    let s = import("vs3", fs_off, Some(own)).unwrap();
    assert!(s.unsupported.is_none(), "{:?}", s.unsupported);
    validate_wgsl(&s.wgsl).unwrap();
    assert!(s.wgsl.contains("@location(1) v1: vec2<f32>") && s.wgsl.contains("in.v1"), "{}", s.wgsl);
    assert_eq!(s.instances, Some(crate::Instances::Param(0)));
}


#[test]
fn curve_lut_is_identity_for_a_straight_default() {
    let lut = curve_lut(&[0.0, 0.0, 0.5, 0.5, 1.0, 1.0], 256);
    assert_eq!(lut.len(), 256 * 4);
    assert_eq!(lut[0], 0);
    assert_eq!(lut[255 * 4], 255);
    assert!((lut[128 * 4] as i32 - 128).abs() <= 1);
    // A bent curve passes through its control point.
    let bent = curve_lut(&[0.0, 0.0, 0.5, 0.9, 1.0, 1.0], 256);
    assert!((bent[128 * 4] as i32 - 230).abs() <= 2);
}


/// ISF 1: inputs declared again as uniforms, and `vv_vertShaderInit` in the vertex shader.
#[test]
fn isf1_redeclared_uniforms_and_vv_names() {
    let fs = "/*{ \"INPUTS\": [ {\"NAME\": \"inputImage\", \"TYPE\": \"image\"}, {\"NAME\": \"amount\", \"TYPE\": \"float\", \"DEFAULT\": 0.5} ] }*/\nuniform float amount;\nvarying vec2 off;\nvoid main() { gl_FragColor = IMG_NORM_PIXEL(inputImage, off) * amount; }\n";
    let vs = "varying vec2 off;\nuniform float amount;\nvoid main() { vv_vertShaderInit(); off = vv_FragNormCoord + vec2(amount * 0.01, 0.0); }\n";
    let sh = crate::import("isf1", fs, Some(vs)).expect("converts");
    assert!(sh.unsupported.is_none(), "{:?}", sh.unsupported);
    crate::validate_wgsl(&sh.wgsl).expect("valid WGSL");
}

/// MadMapper Surface FX: notes before the header, a header split over `.fs` and `.vs`, and a `fxVsFunc` that moves
/// the vertices (Extend, Vertex Noise, Transform Matrix's layout).
#[test]
fn mad_fx_headers_and_vertex_functions() {
    let fs = "/* Available uniforms: notes, not JSON */\n/*{ \"INPUTS\": [ {\"NAME\": \"fx_mix\", \"TYPE\": \"float\", \"DEFAULT\": 1.0} ] }*/\nvec4 fxColorForPixel(vec2 uv) { return FX_NORM_PIXEL(uv) * fx_mix * fx_zoom; }\n";
    let vs = "/*{ \"INPUTS\": [ {\"NAME\": \"fx_zoom\", \"TYPE\": \"float\", \"DEFAULT\": 2.0, \"MIN\": 1.0, \"MAX\": 5.0} ] }*/\nvoid fxVsFunc() {\n    mm_FragNormCoord = (mm_TextureMatrix*vec4(mm_TexCoord0.xy,0,1)).xy;\n    gl_Position = mm_ModelViewProjectionMatrix * vec4(mm_Vertex.xy * fx_zoom, 0, 1);\n}\n";
    let sh = crate::import("madfx", fs, Some(vs)).expect("converts");
    assert!(sh.unsupported.is_none(), "{:?}", sh.unsupported);
    let names: Vec<&str> = sh.inputs.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["fx_mix", "fx_zoom"], "both headers' inputs");
    assert!(sh.vertex_entry.is_some(), "the vertices move: a vertex stage");
    crate::validate_wgsl(&sh.wgsl).expect("valid WGSL");
    // MadMapper's boilerplate vertex function needs no vertex stage; a header only in the .vs is read from there.
    let plain_vs = "/*{ \"INPUTS\": [] }*/\nvoid fxVsFunc() {\n    mm_FragNormCoord = (mm_TextureMatrix*vec4(mm_TexCoord0.xy,0,1)).xy;\n    gl_Position = mm_ModelViewProjectionMatrix * vec4(mm_Vertex.xy,0,1);\n}\n";
    let plain_fs = "/* notes only */\nvec4 fxColorForPixel(vec2 uv) { return FX_NORM_PIXEL(uv); }\n";
    let sh = crate::import("madfx2", plain_fs, Some(plain_vs)).expect("converts");
    assert!(sh.unsupported.is_none() && sh.vertex_entry.is_none(), "{:?}", sh.unsupported);
}

/// Old GLSL names that GLSL 4.50 reserves (`sample`, `filter`…) are renamed; an input with such a name keeps it.
#[test]
fn reserved_words_used_as_names() {
    let fs = "/*{ \"INPUTS\": [ {\"NAME\": \"inputImage\", \"TYPE\": \"image\"}, {\"NAME\": \"filter\", \"TYPE\": \"float\", \"DEFAULT\": 0.5} ] }*/\nvoid main() { vec4 sample = IMG_NORM_PIXEL(inputImage, isf_FragNormCoord); vec4 output = sample * filter; gl_FragColor = output; }\n";
    let sh = crate::import("reserved", fs, None).expect("converts");
    assert!(sh.unsupported.is_none(), "{:?}", sh.unsupported);
    crate::validate_wgsl(&sh.wgsl).expect("valid WGSL");
}
