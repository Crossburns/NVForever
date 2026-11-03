//! The program icon in the Windows executable (Explorer, shortcuts, the taskbar before the
//! window opens). The running window sets the same picture itself (window_icon.rs).
//! Nothing here depends on the checkout's commit, so a commit does not rerun it
//! (frame telemetry reads the commit at run time: frame_telemetry/git_paths.rs, #1312).
fn main() {
    println!("cargo:rerun-if-changed=assets/nvforever.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/nvforever.ico");
        res.compile().expect("embedding the program icon (needs the Windows SDK's rc.exe)");
    }
}
