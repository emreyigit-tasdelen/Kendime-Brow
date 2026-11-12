//! CEF application objects for the browser process: App, BrowserProcessHandler, Client and
//! the Views delegates. Behaviour lives in `shell.rs`; this file only wires callbacks.
//!
//! Note: `wrap_*!` macros clone struct fields, so shared state must be `Rc`/`Arc`-like.

use std::cell::RefCell;
use std::rc::Rc;

use cef::*;

use cef::wrapper::message_router::MessageRouterBrowserSideHandlerCallbacks;

use crate::{bridge, kb_scheme, shell};

wrap_app! {
    pub struct KbApp;

    impl App {
        fn on_register_custom_schemes(&self, registrar: Option<&mut SchemeRegistrar>) {
            kb_scheme::register_custom_scheme(registrar);
        }

        fn on_before_command_line_processing(
            &self,
            process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            // Browser process only (empty process type).
            let is_browser = process_type.is_none_or(|t| t.to_string().is_empty());
            if std::env::var_os("KB_DEBUG").is_some() {
                eprintln!(
                    "[kb] command line processing: type={:?} browser={is_browser}",
                    process_type.map(CefString::to_string)
                );
            }
            let Some(cl) = command_line.filter(|_| is_browser) else {
                return;
            };
            for s in crate::privacy_flags::SWITCHES {
                cl.append_switch(Some(&CefString::from(*s)));
            }
            for (s, v) in crate::privacy_flags::valued_switches() {
                cl.append_switch_with_value(
                    Some(&CefString::from(s)),
                    Some(&CefString::from(v.as_str())),
                );
            }
        }

        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(KbBrowserProcessHandler::new())
        }

        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            Some(bridge::new_render_process_handler())
        }
    }
}

wrap_browser_process_handler! {
    struct KbBrowserProcessHandler;

    impl BrowserProcessHandler {
        fn on_before_child_process_launch(&self, command_line: Option<&mut CommandLine>) {
            if let Some(cl) = command_line {
                crate::privacy_flags::add_child_switches(cl);
            }
        }

        fn on_context_initialized(&self) {
            assert!(
                kb_scheme::install_handler_factory(),
                "failed to register the kb:// scheme handler"
            );
            bridge::init_browser_side();
            let failed = crate::privacy_flags::apply_prefs();
            if !failed.is_empty() && std::env::var_os("KB_DEBUG").is_some() {
                eprintln!("[kb] privacy prefs not applied: {failed:?}");
            }
            let store = shell::profile_dir()
                .map(|dir| crate::store::Store::open(&dir.join("kb.sqlite")))
                .unwrap_or_else(crate::store::Store::open_in_memory);
            if let Some(dir) = shell::profile_dir() {
                let lists = crate::content_blocking::reload(&dir);
                if std::env::var_os("KB_DEBUG").is_some() {
                    eprintln!("[kb] filter lists loaded: {lists}");
                }
            }
            match store {
                Ok(store) => shell::init_store(store),
                Err(e) => eprintln!("[kb] profile database unavailable: {e}"),
            }
            crate::privacy_flags::init_fingerprint(kb_privacy::fingerprint::Level::parse(
                &shell::setting("fingerprint_protection"),
            ));
            if shell::setting_bool("filters_auto_update")
                && let Some(dir) = shell::profile_dir()
            {
                crate::content_blocking::start_update(dir, false);
            }

            let command_line = command_line_get_global().expect("no global command line");
            let url = CefString::from(&command_line.switch_value(Some(&CefString::from("url"))))
                .to_string();
            let start_tabs = if url.is_empty() {
                shell::startup_tabs()
            } else {
                vec![crate::store::SessionTab {
                    url,
                    active: true,
                    container: crate::containers::DEFAULT.into(),
                }]
            };

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
                start_tabs,
            );
            window_create_top_level(Some(&mut window_delegate));
        }
    }
}

wrap_window_delegate! {
    struct KbWindowDelegate {
        ui_view: Rc<RefCell<Option<BrowserView>>>,
        client: Client,
        start_tabs: Vec<crate::store::SessionTab>,
    }

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            let (width, height) = shell::DEFAULT_WINDOW_SIZE;
            Size { width, height }
        }

        fn on_layout_changed(&self, view: Option<&mut View>, _new_bounds: Option<&Rect>) {
            // Children are positioned relative to the client area.
            if let Some(window) = view.and_then(|v| v.window()) {
                let b = window.client_area_bounds_in_screen();
                shell::apply_layout(b.width, b.height);
            }
        }
    }

    impl PanelDelegate {}

    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            let (Some(window), Some(ui_view)) = (window, self.ui_view.borrow().clone()) else {
                return;
            };
            shell::init_window(window, ui_view, self.client.clone());
            shell::open_startup_tabs(&self.start_tabs);
            window.show();
            shell::debug_bounds();
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

        fn minimum_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: 0, height: shell::UI_STRIP_HEIGHT }
        }

        fn maximum_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: 0, height: shell::UI_STRIP_HEIGHT }
        }

        fn height_for_width(&self, _view: Option<&mut View>, _width: i32) -> i32 {
            shell::UI_STRIP_HEIGHT
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
            browser_view: Option<&mut BrowserView>,
            popup_browser_view: Option<&mut BrowserView>,
            is_devtools: i32,
        ) -> i32 {
            if is_devtools != 0 {
                return 0; // Let CEF open DevTools in its own window.
            }
            match popup_browser_view {
                Some(view) => {
                    let container = browser_view
                        .map_or(crate::containers::DEFAULT, |o| shell::container_of_view(o));
                    shell::add_tab_view(view.clone(), container);
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

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(KbLoadHandler::new())
        }

        fn request_handler(&self) -> Option<RequestHandler> {
            Some(KbRequestHandler::new())
        }

        fn download_handler(&self) -> Option<DownloadHandler> {
            Some(KbDownloadHandler::new())
        }

        fn on_process_message_received(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> i32 {
            bridge::browser_router().is_some_and(|r| {
                r.on_process_message_received(
                    browser.cloned(),
                    frame.cloned(),
                    source_process,
                    message.cloned(),
                )
            }) as i32
        }
    }
}

wrap_download_handler! {
    struct KbDownloadHandler;

    impl DownloadHandler {
        fn can_download(
            &self,
            _browser: Option<&mut Browser>,
            _url: Option<&CefString>,
            _request_method: Option<&CefString>,
        ) -> i32 {
            1
        }

        fn on_before_download(
            &self,
            _browser: Option<&mut Browser>,
            download_item: Option<&mut DownloadItem>,
            suggested_name: Option<&CefString>,
            callback: Option<&mut BeforeDownloadCallback>,
        ) -> i32 {
            let (Some(item), Some(callback)) = (download_item, callback) else {
                return 0;
            };
            let name = suggested_name.map(CefString::to_string).unwrap_or_default();
            let handled = crate::downloads::on_before_download(item, &name, callback);
            shell::refresh();
            handled as i32
        }

        fn on_download_updated(
            &self,
            _browser: Option<&mut Browser>,
            download_item: Option<&mut DownloadItem>,
            callback: Option<&mut DownloadItemCallback>,
        ) {
            if let Some(item) = download_item {
                crate::downloads::on_download_updated(item, callback);
                shell::refresh();
            }
        }
    }
}

wrap_load_handler! {
    struct KbLoadHandler;

    impl LoadHandler {
        fn on_load_start(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _transition_type: TransitionType,
        ) {
            let Some(frame) = frame else { return };
            let url = CefString::from(&frame.url()).to_string();
            let js = crate::content_blocking::cosmetic_script(&url);
            if !js.is_empty() {
                frame.execute_java_script(
                    Some(&CefString::from(js.as_str())),
                    Some(&CefString::from("kb://cosmetic")),
                    0,
                );
            }
        }

        fn on_loading_state_change(
            &self,
            browser: Option<&mut Browser>,
            is_loading: i32,
            can_go_back: i32,
            can_go_forward: i32,
        ) {
            if let Some(browser) = browser {
                shell::update_tab(
                    browser,
                    shell::TabUpdate {
                        loading: Some((is_loading != 0, can_go_back != 0, can_go_forward != 0)),
                        ..Default::default()
                    },
                );
            }
        }
    }
}

wrap_request_handler! {
    struct KbRequestHandler;

    impl RequestHandler {
        fn on_before_browse(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _user_gesture: i32,
            _is_redirect: i32,
        ) -> i32 {
            if let (Some(b), Some(f)) = (browser.as_deref(), frame.as_deref())
                && f.is_main() != 0
            {
                crate::content_blocking::reset_count(b.identifier());
            }
            if let Some(r) = bridge::browser_router() {
                r.on_before_browse(browser.cloned(), frame.cloned());
            }
            0
        }

        fn resource_request_handler(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _is_navigation: i32,
            _is_download: i32,
            _request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut i32>,
        ) -> Option<ResourceRequestHandler> {
            Some(crate::content_blocking::KbResourceRequestHandler::new())
        }

        fn on_render_process_terminated(
            &self,
            browser: Option<&mut Browser>,
            _status: TerminationStatus,
            _error_code: i32,
            _error_string: Option<&CefString>,
        ) {
            if let Some(r) = bridge::browser_router() {
                r.on_render_process_terminated(browser.cloned());
            }
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
    // The flag type is i32 on Windows and u32 on Linux in the cef bindings.
    #[allow(clippy::unnecessary_cast)]
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
        fn on_title_change(&self, browser: Option<&mut Browser>, title: Option<&CefString>) {
            if let Some(browser) = browser {
                let title = title.map(CefString::to_string).unwrap_or_default();
                shell::on_title_change(browser, title);
            }
        }

        fn on_address_change(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            url: Option<&CefString>,
        ) {
            let (Some(browser), Some(frame)) = (browser, frame) else {
                return;
            };
            if frame.is_main() == 0 {
                return;
            }
            shell::update_tab(
                browser,
                shell::TabUpdate {
                    url: Some(url.map(CefString::to_string).unwrap_or_default()),
                    ..Default::default()
                },
            );
        }
    }
}

wrap_life_span_handler! {
    struct KbLifeSpanHandler;

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            if std::env::var_os("KB_DEBUG").is_some()
                && let Some(ctx) = browser.and_then(|b| b.host()).and_then(|h| h.request_context())
            {
                eprintln!(
                    "[kb] browser created: global context = {}, cache = {:?}",
                    ctx.is_global(),
                    CefString::from(&ctx.cache_path()).to_string()
                );
            }
            shell::on_after_created();
        }

        fn do_close(&self, browser: Option<&mut Browser>) -> i32 {
            browser.is_some_and(shell::do_close) as i32
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            if let Some(r) = bridge::browser_router() {
                r.on_before_close(browser.as_deref().cloned());
            }
            if let Some(browser) = browser {
                shell::on_before_close(browser);
            }
        }
    }
}
