//! CEF application objects for the browser process: App, BrowserProcessHandler, Client and
//! the Views delegates. Behaviour lives in `shell.rs`; this file only wires callbacks.
//!
//! Note: `wrap_*!` macros clone struct fields, so shared state must be `Rc`/`Arc`-like.

use std::cell::RefCell;
use std::rc::Rc;

use cef::*;

use crate::{kb_scheme, shell};

wrap_app! {
    pub struct KbApp;

    impl App {
        fn on_register_custom_schemes(&self, registrar: Option<&mut SchemeRegistrar>) {
            kb_scheme::register_custom_scheme(registrar);
        }

        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(KbBrowserProcessHandler::new())
        }
    }
}

wrap_browser_process_handler! {
    struct KbBrowserProcessHandler;

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            assert!(
                kb_scheme::install_handler_factory(),
                "failed to register the kb:// scheme handler"
            );

            let command_line = command_line_get_global().expect("no global command line");
            let url = CefString::from(&command_line.switch_value(Some(&CefString::from("url"))))
                .to_string();
            let start_url = if url.is_empty() { shell::NEW_TAB_URL.to_string() } else { url };

            let mut client = KbClient::new();
            let mut ui_delegate = UiViewDelegate::new();
            let ui_view = browser_view_create(
                Some(&mut client),
                Some(&CefString::from(shell::UI_URL)),
                Some(&BrowserSettings::default()),
                None,
                None,
                Some(&mut ui_delegate),
            );

            let mut window_delegate = KbWindowDelegate::new(
                Rc::new(RefCell::new(ui_view)),
                client,
                start_url,
            );
            window_create_top_level(Some(&mut window_delegate));
        }
    }
}

wrap_window_delegate! {
    struct KbWindowDelegate {
        ui_view: Rc<RefCell<Option<BrowserView>>>,
        client: Client,
        start_url: String,
    }

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            let (width, height) = shell::DEFAULT_WINDOW_SIZE;
            Size { width, height }
        }
    }

    impl PanelDelegate {}

    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            let (Some(window), Some(ui_view)) = (window, self.ui_view.borrow().clone()) else {
                return;
            };
            shell::init_window(window, ui_view, self.client.clone());
            shell::new_tab(&self.start_url);
            window.show();
        }

        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            self.ui_view.borrow_mut().take();
        }

        fn can_close(&self, _window: Option<&mut Window>) -> i32 {
            if std::env::var_os("KB_DEBUG").is_some() {
                eprintln!("[kb] window can_close");
            }
            shell::can_close_window(self.ui_view.borrow().as_ref()) as i32
        }

        fn on_key_event(&self, _window: Option<&mut Window>, event: Option<&KeyEvent>) -> i32 {
            event.map_or(0, |e| handle_key(e) as i32)
        }

        fn on_accelerator(&self, _window: Option<&mut Window>, command_id: i32) -> i32 {
            shell::run_command(command_id) as i32
        }

        fn window_runtime_style(&self) -> RuntimeStyle {
            // CEF limits (verified in 1.6): a Chrome-style BrowserView cannot be added to an
            // Alloy-style window, and a window cannot hold more than one Chrome-style
            // BrowserView. Multi-tab windows therefore use Alloy style throughout.
            RuntimeStyle::ALLOY
        }
    }
}

// The kb://ui strip: Alloy style, fixed height.
wrap_browser_view_delegate! {
    struct UiViewDelegate {}

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: 0, height: shell::UI_STRIP_HEIGHT }
        }
    }

    impl BrowserViewDelegate {
        fn browser_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }
    }
}

// Web tabs: Alloy style (see `window_runtime_style`). Popups become new tabs.
wrap_browser_view_delegate! {
    pub struct TabViewDelegate {}

    impl ViewDelegate {}

    impl BrowserViewDelegate {
        fn on_popup_browser_view_created(
            &self,
            _browser_view: Option<&mut BrowserView>,
            popup_browser_view: Option<&mut BrowserView>,
            is_devtools: i32,
        ) -> i32 {
            if is_devtools != 0 {
                return 0; // Let CEF open DevTools in its own window.
            }
            match popup_browser_view {
                Some(view) => {
                    shell::add_tab_view(view.clone());
                    1
                }
                None => 0,
            }
        }

        fn browser_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }
    }
}

wrap_client! {
    pub struct KbClient;

    impl Client {
        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(KbDisplayHandler::new())
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(KbLifeSpanHandler::new())
        }

        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            Some(KbKeyboardHandler::new())
        }
    }
}

/// Matches a key-down against `shell::ACCELERATORS` and runs the command.
fn handle_key(event: &KeyEvent) -> bool {
    if std::env::var_os("KB_DEBUG").is_some() {
        eprintln!(
            "[kb] key type={:?} code={} mods={}",
            event.type_, event.windows_key_code, event.modifiers
        );
    }
    if event.type_ != KeyEventType::RAWKEYDOWN && event.type_ != KeyEventType::KEYDOWN {
        return false;
    }
    let flag = |f: sys::cef_event_flags_t| event.modifiers & (f.0 as u32) != 0;
    let shift = flag(sys::cef_event_flags_t::EVENTFLAG_SHIFT_DOWN);
    let ctrl = flag(sys::cef_event_flags_t::EVENTFLAG_CONTROL_DOWN);
    let alt = flag(sys::cef_event_flags_t::EVENTFLAG_ALT_DOWN);
    shell::command_for_key(event.windows_key_code, shift, ctrl, alt).is_some_and(shell::run_command)
}

// The native event type in `on_pre_key_event` differs per platform in the cef bindings.
#[cfg(target_os = "windows")]
type OsKeyEvent<'a> = Option<&'a mut sys::MSG>;
#[cfg(target_os = "linux")]
type OsKeyEvent<'a> = Option<&'a mut sys::XEvent>;
#[cfg(target_os = "macos")]
type OsKeyEvent<'a> = *mut u8;

// Browser shortcuts. Window accelerators do not fire while an Alloy browser has focus, so
// key-down events are matched against `shell::ACCELERATORS` before the page sees them.
wrap_keyboard_handler! {
    struct KbKeyboardHandler;

    impl KeyboardHandler {
        fn on_pre_key_event(
            &self,
            _browser: Option<&mut Browser>,
            event: Option<&KeyEvent>,
            _os_event: OsKeyEvent<'_>,
            _is_keyboard_shortcut: Option<&mut i32>,
        ) -> i32 {
            event.map_or(0, |e| handle_key(e) as i32)
        }
    }
}

wrap_display_handler! {
    struct KbDisplayHandler;

    impl DisplayHandler {
        fn on_title_change(&self, browser: Option<&mut Browser>, _title: Option<&CefString>) {
            if let Some(browser) = browser {
                shell::on_title_change(browser);
            }
        }
    }
}

wrap_life_span_handler! {
    struct KbLifeSpanHandler;

    impl LifeSpanHandler {
        fn on_after_created(&self, _browser: Option<&mut Browser>) {
            shell::on_after_created();
        }

        fn do_close(&self, browser: Option<&mut Browser>) -> i32 {
            browser.is_some_and(shell::do_close) as i32
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                shell::on_before_close(browser);
            }
        }
    }
}
