//! Shadertoy (single pass) → ISF, so a shader pasted from shadertoy.com plays in Phosphor.
//!
//! Shadertoy's `void mainImage(out vec4 fragColor, in vec2 fragCoord)` with `iTime`, `iResolution`, `iFrame`,
//! `iTimeDelta`, `iDate`, `iMouse` (a Mouse point + Mouse down, moved from the preview) and `iChannel0..3` becomes an ISF shader: a generator, or an effect on the
//! layer's picture when it reads `iChannel0`. Other channels are blank pictures (their textures aren't part of the
//! code). Shaders with several buffers (Buffer A…D) need each pass pasted separately; only the Image pass is read.

/// The ISF source for a Shadertoy shader, or why it can't be converted.
pub fn to_isf(source: &str, credit: &str) -> Result<String, String> {
    if !source.contains("mainImage") {
        return Err("no `void mainImage(out vec4 fragColor, in vec2 fragCoord)`: paste the Image tab's code".into());
    }
    let uses = |name: &str| source.match_indices(name).any(|(k, _)| !source[k + name.len()..].starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_'));
    let effect = uses("iChannel0");
    let mut inputs: Vec<String> = Vec::new();
    if effect {
        inputs.push(r#"{"NAME": "inputImage", "TYPE": "image"}"#.into());
    }
    for n in 1..4 {
        if uses(&format!("iChannel{n}")) {
            inputs.push(format!(r#"{{"NAME": "iChannel{n}", "TYPE": "image"}}"#));
        }
    }
    // iMouse: a point the preview's mouse moves (Mouse X / Y) and a button held while dragging (Mouse down).
    let mouse = uses("iMouse");
    if mouse {
        inputs.push(r#"{"NAME": "phosphorMouse", "LABEL": "Mouse", "TYPE": "point2D", "DEFAULT": [0.5, 0.5], "MIN": [0.0, 0.0], "MAX": [1.0, 1.0]}"#.into());
        inputs.push(r#"{"NAME": "phosphorMouseDown", "LABEL": "Mouse down", "TYPE": "bool", "DEFAULT": false}"#.into());
    }
    let header = format!(
        "/*{{\n\"DESCRIPTION\": \"Imported from Shadertoy\",\n\"CREDIT\": {},\n\"ISFVSN\": \"2\",\n\"CATEGORIES\": [\"Shadertoy\"],\n\"INPUTS\": [\n{}\n]\n}}*/\n",
        json_str(credit),
        inputs.join(",\n")
    );
    let mut defines = String::from(
        "#define iTime TIME\n#define iResolution vec3(RENDERSIZE, 1.0)\n#define iFrame FRAMEINDEX\n#define iTimeDelta TIMEDELTA\n#define iDate DATE\n#define iSampleRate 44100.0\n#define iChannelResolution (vec3[4](vec3(RENDERSIZE, 1.0), vec3(RENDERSIZE, 1.0), vec3(RENDERSIZE, 1.0), vec3(RENDERSIZE, 1.0)))\n",
    );
    // Shadertoy's iMouse: xy = where the mouse is (pixels), zw = where it was pressed, negative once let go.
    defines.push_str(if mouse {
        "#define iMouse vec4(phosphorMouse * RENDERSIZE, (phosphorMouseDown ? 1.0 : -1.0) * phosphorMouse * RENDERSIZE)\n"
    } else {
        "#define iMouse vec4(0.0)\n"
    });
    if effect {
        defines.push_str("#define iChannel0 inputImage\n");
    }
    // Pictures: Shadertoy calls `texture(iChannelN, uv)` (uv 0..1, origin bottom left, like ISF's normalized pixel).
    let body = source.replace("texture(", "IMG_NORM_PIXEL(").replace("textureLod(", "phosphor_lod(");
    Ok(format!(
        "{header}{defines}#define phosphor_lod(img, uv, lod) IMG_NORM_PIXEL(img, uv)\n{body}\nvoid main() {{\n    vec4 c = vec4(0.0, 0.0, 0.0, 1.0);\n    mainImage(c, gl_FragCoord.xy);\n    gl_FragColor = vec4(c.rgb, 1.0);\n}}\n"
    ))
}

fn json_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', " "))
}

#[cfg(test)]
mod tests {
    /// Shadertoy's default "new shader".
    const DEFAULT: &str = "void mainImage( out vec4 fragColor, in vec2 fragCoord )\n{\n    vec2 uv = fragCoord/iResolution.xy;\n    vec3 col = 0.5 + 0.5*cos(iTime+uv.xyx+vec3(0,2,4));\n    fragColor = vec4(col,1.0);\n}\n";

    const EFFECT: &str = "void mainImage(out vec4 o, in vec2 p) {\n    vec2 uv = p / iResolution.xy;\n    uv.x += sin(uv.y * 20.0 + iTime) * 0.01;\n    o = texture(iChannel0, uv) * (0.8 + 0.2 * sin(float(iFrame)));\n}\n";

    #[test]
    fn the_default_shader_becomes_a_generator() {
        let isf = super::to_isf(DEFAULT, "Shadertoy").unwrap();
        let sh = crate::import("st_default", &isf, None).expect("imports");
        assert!(!sh.is_filter && sh.unsupported.is_none(), "{:?}", sh.unsupported);
        crate::validate_wgsl(&sh.wgsl).expect("valid WGSL");
    }

    #[test]
    fn a_shader_reading_channel_0_becomes_an_effect() {
        let isf = super::to_isf(EFFECT, "someone").unwrap();
        let sh = crate::import("st_effect", &isf, None).expect("imports");
        assert!(sh.is_filter && sh.unsupported.is_none(), "{:?}", sh.unsupported);
        crate::validate_wgsl(&sh.wgsl).expect("valid WGSL");
    }

    /// Typical Shadertoy code: macros, consts, out parameters, a raymarching loop, rotations, textureLod.
    const RAYMARCH: &str = "#define MAX_STEPS 80\n#define R(a) mat2(cos(a), -sin(a), sin(a), cos(a))\nconst float EPS = 0.001;\n\nfloat map(vec3 p) {\n    p.xy *= R(iTime * 0.3);\n    vec3 q = mod(p, 2.0) - 1.0;\n    return length(q) - 0.35;\n}\n\nvoid normal(in vec3 p, out vec3 n) {\n    vec2 e = vec2(EPS, 0.0);\n    n = normalize(vec3(map(p + e.xyy) - map(p - e.xyy), map(p + e.yxy) - map(p - e.yxy), map(p + e.yyx) - map(p - e.yyx)));\n}\n\nvoid mainImage(out vec4 fragColor, in vec2 fragCoord) {\n    vec2 uv = (fragCoord - 0.5 * iResolution.xy) / iResolution.y;\n    vec3 ro = vec3(0.0, 0.0, iTime), rd = normalize(vec3(uv, 1.0));\n    float t = 0.0;\n    for (int i = 0; i < MAX_STEPS; i++) {\n        float d = map(ro + rd * t);\n        if (d < EPS) break;\n        t += d;\n    }\n    vec3 n;\n    normal(ro + rd * t, n);\n    vec3 col = 0.5 + 0.5 * n;\n    col *= textureLod(iChannel1, uv, 0.0).r + 1.0;\n    fragColor = vec4(col * exp(-0.05 * t), 1.0);\n}\n";

    #[test]
    fn typical_shadertoy_code_converts() {
        let isf = super::to_isf(RAYMARCH, "test").unwrap();
        let sh = crate::import("st_ray", &isf, None).expect("imports");
        assert!(!sh.is_filter && sh.unsupported.is_none(), "{:?}", sh.unsupported);
        crate::validate_wgsl(&sh.wgsl).expect("valid WGSL");
    }

    /// iMouse: the shader gets a Mouse point and a Mouse down button (the preview moves and holds them).
    #[test]
    fn imouse_becomes_a_mouse_point_and_button() {
        const MOUSE: &str = "void mainImage(out vec4 o, in vec2 p) {\n    vec2 m = iMouse.xy / iResolution.xy;\n    o = vec4(m, iMouse.z > 0.0 ? 1.0 : 0.0, 1.0);\n}\n";
        let isf = super::to_isf(MOUSE, "test").unwrap();
        let sh = crate::import("st_mouse", &isf, None).expect("imports");
        assert!(sh.unsupported.is_none(), "{:?}", sh.unsupported);
        crate::validate_wgsl(&sh.wgsl).expect("valid WGSL");
        let labels: Vec<&str> = sh.inputs.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["Mouse", "Mouse down"]);
        assert!(!super::to_isf(DEFAULT, "").unwrap().contains("phosphorMouse"), "only shaders that use it get one");
    }

    #[test]
    fn other_code_is_refused() {
        assert!(super::to_isf("void main() {}", "").is_err());
    }
}
