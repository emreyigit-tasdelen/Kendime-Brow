//! kendime-brow: browser main process (architecture §3.1).
#![cfg_attr(
    all(not(debug_assertions), not(feature = "sandbox"), target_os = "windows"),
    windows_subsystem = "windows"
)]

use cef::*;
use kb_shell::app::KbApp;

#[cfg(not(all(feature = "sandbox", target_os = "windows")))]
fn main() -> Result<(), &'static str> {
    kb_platform::set_app_user_model_id();
    // Initialize the CEF API version.
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    let args = args::Args::new();
    let Some(cmd_line) = args.as_cmd_line() else {
        return Err("Failed to parse command line arguments");
    };
    let is_browser_process = cmd_line.has_switch(Some(&CefString::from("type"))) != 1;

    // Subprocesses launched from this executable exit here (the dedicated helper binary is
    // preferred; see `browser_subprocess_path`).
    let mut app = KbApp::new();
    let ret = execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    if !is_browser_process {
        return if ret >= 0 {
            Ok(())
        } else {
            Err("subprocess failed")
        };
    }

    // Persistent profile: CEF data under <data dir>/profiles/default/cef (architecture §3.7).
    let cache = kb_shell::shell::profile_dir()
        .map(|d| d.join("cef").to_string_lossy().to_string())
        .unwrap_or_default();
    // Renderer/GPU/utility processes run the small helper binary next to this executable.
    let helper = std::env::current_exe()
        .ok()
        .map(|p| p.with_file_name(format!("kb-shell-helper{}", std::env::consts::EXE_SUFFIX)))
        .filter(|p| p.exists())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let settings = Settings {
        no_sandbox: 1,
        browser_subprocess_path: CefString::from(helper.as_str()),
        root_cache_path: CefString::from(cache.as_str()),
        cache_path: CefString::from(cache.as_str()),
        persist_session_cookies: 1,
        ..Default::default()
    };
    if initialize(
        Some(args.as_main_args()),
        Some(&settings),
        Some(&mut app),
        std::ptr::null_mut(),
    ) != 1
    {
        return Err("CEF initialization failed");
    }

    run_message_loop();
    shutdown();
    Ok(())
}

#[cfg(all(feature = "sandbox", target_os = "windows"))]
fn main() -> Result<(), &'static str> {
    Err("The Windows sandbox build must be launched through bootstrap.exe (not implemented yet).")
}
