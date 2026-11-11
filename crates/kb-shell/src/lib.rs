//! kb-shell library: code shared by the browser binary and the helper (subprocess) binary.

pub mod app;
pub mod kb_scheme;
pub mod scheme;
pub mod shell;
pub mod tabs;

use cef::*;

wrap_app! {
    // App used by helper (renderer/GPU/utility) processes. Custom schemes must be registered
    // identically in every process.
    pub struct KbSubprocessApp;

    impl App {
        fn on_register_custom_schemes(&self, registrar: Option<&mut SchemeRegistrar>) {
            kb_scheme::register_custom_scheme(registrar);
        }
    }
}
