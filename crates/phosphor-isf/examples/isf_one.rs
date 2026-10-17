//! Imports one ISF / MadMapper shader and prints the full error or the WGSL.
//!
//! Usage: `cargo run -p phosphor-isf --example isf_one -- path/to/shader.fs [--wgsl]`

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.iter().find(|a| !a.starts_with("--")) else {
        eprintln!("usage: isf_one <shader.fs> [--wgsl]");
        return;
    };
    let path = std::path::Path::new(path);
    let fs = std::fs::read_to_string(path).expect("read shader");
    let vs = std::fs::read_to_string(path.with_extension("vs")).ok();
    let slug = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    match phosphor_isf::import(&slug, &fs, vs.as_deref()) {
        Err(e) => println!("ERROR {e:?}"),
        Ok(sh) => {
            if let Some(r) = &sh.unsupported {
                println!("UNSUPPORTED {r}");
            } else if let Err(e) = phosphor_isf::validate_wgsl(&sh.wgsl) {
                println!("INVALID WGSL\n{e}");
            } else {
                println!("OK {} params, {} buffers, {} passes", sh.param_count(), sh.buffers.len(), sh.pass_plan.len());
            }
            if args.iter().any(|a| a == "--wgsl") {
                println!("{}", sh.wgsl);
            }
        }
    }
}
