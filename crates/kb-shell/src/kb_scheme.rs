//! CEF glue for the `kb://` scheme (architecture §3.4). Resolution logic is in `scheme.rs`.

use std::sync::{Arc, Mutex};

use cef::*;

use crate::scheme;

/// Registers `kb` as a standard, secure, CORS-enabled scheme.
/// Must run in EVERY process (browser + helpers) via `App::on_register_custom_schemes`.
pub fn register_custom_scheme(registrar: Option<&mut SchemeRegistrar>) {
    let Some(registrar) = registrar else {
        return;
    };
    let options = SchemeOptions::STANDARD.get_raw()
        | SchemeOptions::SECURE.get_raw()
        | SchemeOptions::CORS_ENABLED.get_raw()
        | SchemeOptions::FETCH_ENABLED.get_raw();
    // `as _`: the raw option type is i32 on Windows and u32 on Linux in the cef bindings.
    #[allow(clippy::unnecessary_cast)]
    registrar.add_custom_scheme(Some(&CefString::from(scheme::SCHEME)), options as _);
}

/// Installs the `kb://` handler factory in the browser process (call after context init).
pub fn install_handler_factory() -> bool {
    let mut factory = KbSchemeHandlerFactory::new();
    register_scheme_handler_factory(
        Some(&CefString::from(scheme::SCHEME)),
        None,
        Some(&mut factory),
    ) == 1
}

/// Security headers sent with every `kb://` response.
pub const SECURITY_HEADERS: &[(&str, &str)] = &[
    ("Content-Security-Policy", scheme::CSP),
    ("X-Content-Type-Options", "nosniff"),
    ("Cache-Control", "no-store"),
];

wrap_scheme_handler_factory! {
    pub struct KbSchemeHandlerFactory;

    impl SchemeHandlerFactory {
        fn create(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _scheme_name: Option<&CefString>,
            request: Option<&mut Request>,
        ) -> Option<ResourceHandler> {
            let url = CefString::from(&request?.url()).to_string();
            Some(KbResourceHandler::new(scheme::resolve(&url), Arc::new(Mutex::new(0))))
        }
    }
}

// Serves one embedded body. Headers are written directly in `response_headers`
// (the stock StreamResourceHandler drops its header map, so the CSP never arrived).
wrap_resource_handler! {
    pub struct KbResourceHandler {
        response: scheme::Response,
        offset: Arc<Mutex<usize>>,
    }

    impl ResourceHandler {
        fn open(
            &self,
            _request: Option<&mut Request>,
            handle_request: Option<&mut i32>,
            _callback: Option<&mut Callback>,
        ) -> i32 {
            if let Some(handle_request) = handle_request {
                *handle_request = 1;
            }
            1
        }

        fn response_headers(
            &self,
            response: Option<&mut Response>,
            response_length: Option<&mut i64>,
            _redirect_url: Option<&mut CefString>,
        ) {
            let Some(response) = response else {
                return;
            };
            response.set_status(self.response.status);
            response.set_status_text(Some(&CefString::from(self.response.status_text)));
            response.set_mime_type(Some(&CefString::from(self.response.mime)));
            for (name, value) in SECURITY_HEADERS {
                response.set_header_by_name(
                    Some(&CefString::from(*name)),
                    Some(&CefString::from(*value)),
                    1,
                );
            }
            if let Some(response_length) = response_length {
                *response_length = self.response.body.len() as i64;
            }
        }

        #[allow(clippy::not_unsafe_ptr_arg_deref)]
        fn read(
            &self,
            data_out: *mut u8,
            bytes_to_read: i32,
            bytes_read: Option<&mut i32>,
            _callback: Option<&mut ResourceReadCallback>,
        ) -> i32 {
            let Some(bytes_read) = bytes_read else {
                return 0;
            };
            *bytes_read = 0;
            let Ok(mut offset) = self.offset.lock() else {
                return 0;
            };
            let remaining = &self.response.body[*offset..];
            let n = remaining.len().min(bytes_to_read.max(0) as usize);
            if n == 0 || data_out.is_null() {
                return 0;
            }
            // SAFETY: CEF guarantees `data_out` points to at least `bytes_to_read` bytes.
            unsafe { std::ptr::copy_nonoverlapping(remaining.as_ptr(), data_out, n) };
            *offset += n;
            *bytes_read = n as i32;
            1
        }
    }
}
