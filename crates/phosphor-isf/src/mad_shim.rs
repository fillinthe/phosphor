//! Shared machinery for the clean-room MadMapper include stand-ins (`MadNoise.glsl`, `MadSDF.glsl`):
//! a shim is a block of private helpers plus a table of public functions, and only the functions the
//! shader calls without defining itself are emitted (naga drops anything unused anyway, but the names
//! must not collide with the shader's own).

/// `name(` appears in `body` as a call that the body doesn't also define.
pub(crate) fn called_not_defined(body: &str, name: &str) -> bool {
    let mut called = false;
    let mut i = 0;
    while let Some(pos) = body[i..].find(name) {
        let s = i + pos;
        let e = s + name.len();
        i = e;
        let before_ok = s == 0 || !is_ident(body.as_bytes()[s - 1]);
        let after = body[e..].trim_start();
        if !before_ok || !after.starts_with('(') {
            continue;
        }
        // A definition: a type name right before, e.g. `float noise(`.
        let head = body[..s].trim_end();
        let ty = head.rsplit(|c: char| !c.is_ascii_alphanumeric() && c != '_').next().unwrap_or("");
        if matches!(ty, "float" | "vec2" | "vec3" | "vec4" | "int" | "void" | "mat2" | "mat3") {
            return false;
        }
        called = true;
    }
    called
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The shim for one shader body: `helpers` plus every function in `functions` it calls but doesn't define.
pub(crate) fn shim(body: &str, helpers: &str, functions: &[(&str, &str)]) -> String {
    let needed: Vec<&str> = functions.iter().filter(|(n, _)| called_not_defined(body, n)).map(|(_, g)| *g).collect();
    if needed.is_empty() {
        return String::new();
    }
    let mut s = String::from(helpers);
    for g in needed {
        s.push_str(g);
    }
    s
}
