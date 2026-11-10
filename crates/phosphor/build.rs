fn main() {
    // Windows: the app icon and version details inside Phosphor.exe (Start menu, desktop shortcut, Explorer, taskbar,
    // .phosphor files and Task Manager all show them; without it Windows shows a blank program icon).
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=../../assets/icon.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/icon.ico");
        res.set("ProductName", "Phosphor VJ");
        res.set("FileDescription", "Phosphor VJ");
        if let Err(e) = res.compile() {
            println!("cargo:warning=Windows icon not embedded: {e}");
        }
    }
    // ScreenCaptureKit's Swift bridge loads the Swift runtime through @rpath; macOS 12+ ships it in /usr/lib/swift.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg-bins=-Wl,-rpath,/usr/lib/swift");
    }
}
