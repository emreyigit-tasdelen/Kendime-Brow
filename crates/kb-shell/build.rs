// Embeds the Windows application manifest (Windows 10+ compatibility, per-monitor DPI awareness).
// CEF expects it; without it the page flickered between two device scale factors.
fn main() {
    println!("cargo:rerun-if-changed=app.manifest");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let path = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("app.manifest");
        // Executable icon (Explorer, pinned taskbar button, Start menu). Precompiled resource:
        // icon.res is built from icon.ico (RT_ICON + RT_GROUP_ICON), so no rc.exe is needed.
        println!("cargo:rerun-if-changed=assets/icon.res");
        let res = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("assets")
            .join("icon.res");
        println!("cargo:rustc-link-arg-bins={}", res.display());
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}",
            path.display()
        );
    }
}
