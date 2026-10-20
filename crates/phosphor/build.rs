fn main() {
    // ScreenCaptureKit's Swift bridge loads the Swift runtime through @rpath; macOS 12+ ships it in /usr/lib/swift.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg-bins=-Wl,-rpath,/usr/lib/swift");
    }
}
