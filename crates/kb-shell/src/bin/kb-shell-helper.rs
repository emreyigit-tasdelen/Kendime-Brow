#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use cef::{args::Args, *};

fn main() {
    kb_platform::set_app_user_model_id();
    let args = Args::new();

    #[cfg(all(target_os = "macos", feature = "sandbox"))]
    let _sandbox = {
        let mut sandbox = cef::sandbox::Sandbox::new();
        sandbox.initialize(args.as_main_args());
        sandbox
    };

    #[cfg(target_os = "macos")]
    let _loader = {
        let loader = library_loader::LibraryLoader::new(&std::env::current_exe().unwrap(), true);
        assert!(loader.load());
        loader
    };

    // Initialize the CEF API version.
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    execute_process(
        Some(args.as_main_args()),
        Some(&mut kb_shell::KbSubprocessApp::new()),
        std::ptr::null_mut(),
    );
}
