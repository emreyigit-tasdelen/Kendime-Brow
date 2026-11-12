//! kb-shell library: code shared by the browser binary and the helper (subprocess) binary.

pub mod actions;
pub mod app;
pub mod autofill;
pub mod bridge;
pub mod connections;
pub mod containers;
pub mod content_blocking;
pub mod downloads;
pub mod kb_scheme;
pub mod net;
pub mod net_privacy;
pub mod pdf;
pub mod pdf_url;
pub mod privacy_flags;
pub mod protocol;
pub mod scheme;
pub mod sesslog;
pub mod sftp_svc;
pub mod shell;
pub mod store;
pub mod tabs;
pub mod tasks;
pub mod terminal;
pub mod tunnels;
pub mod updater;
pub mod userscripts;
pub mod vault;

use cef::*;

wrap_app! {
    // App used by helper (renderer/GPU/utility) processes. Custom schemes must be registered
    // identically in every process.
    pub struct KbSubprocessApp;

    impl App {
        fn on_register_custom_schemes(&self, registrar: Option<&mut SchemeRegistrar>) {
            kb_scheme::register_custom_scheme(registrar);
        }

        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            Some(bridge::new_render_process_handler())
        }
    }
}
