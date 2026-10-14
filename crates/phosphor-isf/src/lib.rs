pub fn proto(src: &str) -> String {
    let mut fe = naga::front::glsl::Frontend::default();
    let opts = naga::front::glsl::Options::from(naga::ShaderStage::Fragment);
    let module = match fe.parse(&opts, src) {
        Ok(m) => m,
        Err(e) => return format!("PARSE ERR: {}", e.emit_to_string(src)),
    };
    let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
        .validate(&module).map_err(|e| format!("{e:?}")).unwrap();
    naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty()).unwrap()
}
