//! Browser window and tab management on the CEF UI thread (architecture §2.4, task 1.6).
//!
//! Layout (CEF Views):
//! ```text
//! Window (vertical box)
//!  ├─ UI strip: BrowserView, Alloy style, kb://ui/     (fixed height)
//!  └─ Content panel (flex 1): one Chrome-style BrowserView per tab, only the active one visible
//! ```
//! State lives in a UI-thread `thread_local`. Never call a CEF function that can re-enter our
//! handlers while the state is borrowed: copy what you need out of `with_shell` first.

use std::cell::RefCell;

use cef::*;

use crate::protocol::{self, Command};
use crate::store::{SessionTab, Store};
use crate::tabs::{TabId, Tabs};

pub const UI_URL: &str = "kb://ui/";
pub const NEW_TAB_URL: &str = "kb://newtab/";
pub const UI_STRIP_HEIGHT: i32 = 76;
/// Strip height while the command palette is open.
pub const UI_PALETTE_HEIGHT: i32 = 420;
pub const DEFAULT_WINDOW_SIZE: (i32, i32) = (1280, 800);

/// Window accelerator command ids.
pub mod command {
    pub const NEW_TAB: i32 = 1;
    pub const CLOSE_TAB: i32 = 2;
    pub const NEXT_TAB: i32 = 3;
    pub const PREV_TAB: i32 = 4;
    pub const BACK: i32 = 5;
    pub const FORWARD: i32 = 6;
    pub const RELOAD: i32 = 7;
    pub const PALETTE: i32 = 8;
}

// Windows virtual key codes (CEF uses them on every platform for accelerators).
const VK_TAB: i32 = 0x09;
const VK_LEFT: i32 = 0x25;
const VK_RIGHT: i32 = 0x27;
const VK_T: i32 = 0x54;
const VK_W: i32 = 0x57;
const VK_F5: i32 = 0x74;
const VK_P: i32 = 0x50;

/// (command, key, shift, ctrl, alt)
pub const ACCELERATORS: &[(i32, i32, bool, bool, bool)] = &[
    (command::NEW_TAB, VK_T, false, true, false),
    (command::CLOSE_TAB, VK_W, false, true, false),
    (command::NEXT_TAB, VK_TAB, false, true, false),
    (command::PREV_TAB, VK_TAB, true, true, false),
    (command::BACK, VK_LEFT, false, false, true),
    (command::FORWARD, VK_RIGHT, false, false, true),
    (command::RELOAD, VK_F5, false, false, false),
    (command::PALETTE, VK_P, true, true, false),
];

/// One tab: its view plus the state shown in the UI strip.
#[derive(Clone)]
struct Tab {
    view: BrowserView,
    title: String,
    url: String,
    loading: bool,
    can_go_back: bool,
    can_go_forward: bool,
    container: &'static str,
    /// Last time this tab was the active one (tab lifecycle, 2.8).
    last_active: std::time::Instant,
    /// `Some(url)` while the tab is discarded: its page was replaced by `about:blank` to free
    /// the renderer; the URL is restored when the tab is activated again.
    discarded: Option<String>,
}

impl Tab {
    fn new(view: BrowserView, container: &'static str) -> Self {
        Self {
            view,
            container,
            last_active: std::time::Instant::now(),
            discarded: None,
            title: String::new(),
            url: String::new(),
            loading: true,
            can_go_back: false,
            can_go_forward: false,
        }
    }

    fn is(&self, browser: &mut Browser) -> bool {
        self.view
            .browser()
            .is_some_and(|b| b.is_same(Some(browser)) != 0)
    }
}

struct Shell {
    window: Window,
    client: Client,
    ui: BrowserView,
    tabs: Tabs<Tab>,
}

thread_local! {
    static SHELL: RefCell<Option<Shell>> = const { RefCell::new(None) };
    static BROWSER_COUNT: RefCell<usize> = const { RefCell::new(0) };
    static WINDOW_CLOSING: RefCell<bool> = const { RefCell::new(false) };
    static PALETTE_OPEN: RefCell<bool> = const { RefCell::new(false) };
    static STORE: RefCell<Option<Store>> = const { RefCell::new(None) };
}

/// Opens the profile database (call once on the UI thread before the window is created).
pub fn init_store(store: Store) {
    schedule_lifecycle();
    crate::content_blocking::set_shield_off_hosts(store.shield_off_hosts().unwrap_or_default());
    STORE.with(|s| *s.borrow_mut() = Some(store));
}

fn with_store<R>(f: impl FnOnce(&mut Store) -> R) -> Option<R> {
    STORE.with(|s| s.borrow_mut().as_mut().map(f))
}

/// HTTPS-Only + proxy for one container context.
pub fn apply_context_network(ctx: &RequestContext, container: &str) {
    let https = crate::net_privacy::apply_https_only(ctx, setting_bool("https_only"));
    let proxy_setting = setting(&format!("proxy_{container}"));
    let proxy = crate::net_privacy::apply_proxy(ctx, &proxy_setting);
    debug_log(&format!(
        "network prefs for {container}: https_only={https} proxy({proxy_setting:?})={proxy}"
    ));
}

/// Applies DoH and the network settings of every existing context.
pub fn apply_network_settings() {
    crate::net_privacy::apply_doh(&setting("doh"));
    if let Some(global) = request_context_get_global_context() {
        apply_context_network(&global, crate::containers::DEFAULT);
    }
    for c in crate::containers::CONTAINERS {
        if c.id != crate::containers::DEFAULT
            && let Some(ctx) = crate::containers::existing(c.id)
        {
            apply_context_network(&ctx, c.id);
        }
    }
}

pub fn setting(key: &str) -> String {
    with_store(|s| s.setting(key)).flatten().unwrap_or_default()
}

pub fn setting_bool(key: &str) -> bool {
    with_store(|s| s.setting_bool(key)).unwrap_or(false)
}

wrap_task! {
    // Refreshes the UI state on the UI thread (posted from background threads).
    pub struct RefreshTask;

    impl Task {
        fn execute(&self) {
            refresh();
        }
    }
}

/// Filter list state for kb://settings (catalog merged with on-disk state).
fn filters_json() -> serde_json::Value {
    let Some(dir) = profile_dir().map(|p| crate::content_blocking::filters_dir(&p)) else {
        return serde_json::Value::Array(Vec::new());
    };
    let state = kb_privacy::lists::ListsFile::load(&dir);
    serde_json::Value::Array(
        state
            .lists
            .iter()
            .map(|l| {
                let title = kb_privacy::lists::CATALOG
                    .iter()
                    .find(|e| e.id == l.id)
                    .map_or("", |e| e.title);
                serde_json::json!({
                    "id": l.id, "title": title, "enabled": l.enabled,
                    "updated": l.updated, "rules": l.rules, "error": l.error,
                })
            })
            .collect(),
    )
}

/// Address bar text → URL using the configured search engine.
fn to_url(text: &str) -> String {
    let search = with_store(|s| s.setting("search_url"))
        .flatten()
        .unwrap_or_else(|| protocol::SEARCH_URL.to_string());
    protocol::input_to_url_with(text, &search)
}

/// Tabs to open at startup: the saved session (if enabled and not empty), else the new tab page.
pub fn startup_tabs() -> Vec<SessionTab> {
    let saved = with_store(|s| {
        if s.setting_bool("restore_session") {
            s.load_session().unwrap_or_default()
        } else {
            Vec::new()
        }
    })
    .unwrap_or_default();
    if saved.is_empty() {
        vec![SessionTab {
            url: NEW_TAB_URL.to_string(),
            active: true,
            container: crate::containers::DEFAULT.to_string(),
        }]
    } else {
        saved
    }
}

/// JSON for kb://settings: settings, bookmarks and recent history.
pub fn profile_json(history_query: &str) -> String {
    with_store(|s| {
        let settings: serde_json::Map<String, serde_json::Value> = s
            .settings()
            .into_iter()
            .map(|(k, v)| (k, serde_json::Value::String(v)))
            .collect();
        serde_json::json!({
            "settings": settings,
            "bookmarks": s.bookmarks().unwrap_or_default(),
            "history": s.history(history_query, 200).unwrap_or_default(),
            "filters": filters_json(),
        })
        .to_string()
    })
    .unwrap_or_else(|| "{}".into())
}

fn save_session() {
    let tabs: Vec<SessionTab> = with_shell(|s| {
        let active = s.tabs.active();
        s.tabs
            .iter()
            // Private (non-persistent) tabs never reach the disk.
            .filter(|(_, t)| crate::containers::get(t.container).is_some_and(|c| c.persistent))
            .map(|(id, t)| SessionTab {
                url: t.url.clone(),
                active: Some(id) == active,
                container: t.container.to_string(),
            })
            .collect()
    })
    .unwrap_or_default();
    // Never wipe the saved session while the window is closing (tabs disappear one by one).
    if WINDOW_CLOSING.with(|c| *c.borrow()) {
        return;
    }
    with_store(|s| s.save_session(&tabs));
}

fn with_shell<R>(f: impl FnOnce(&mut Shell) -> R) -> Option<R> {
    SHELL.with(|s| s.borrow_mut().as_mut().map(f))
}

/// Builds the layout inside a freshly created window. Called from `on_window_created`.
pub fn init_window(window: &mut Window, ui_view: BrowserView, client: Client) {
    // Manual layout: CEF's BoxLayout ignored `set_flex_for_view` here (the strip always took
    // the full height, see PROGRESS 1.7), so `apply_layout` positions the views itself after
    // every window layout pass (`WindowDelegate::on_layout_changed`).
    let mut ui = View::from(&ui_view);
    window.add_child_view(Some(&mut ui));

    // Window accelerators (also matched in the key handlers, see app.rs `handle_key`).
    for (cmd, key, shift, ctrl, alt) in ACCELERATORS {
        window.set_accelerator(*cmd, *key, *shift as i32, *ctrl as i32, *alt as i32, 0);
    }

    SHELL.with(|s| {
        *s.borrow_mut() = Some(Shell {
            window: window.clone(),
            client,
            ui: ui_view,
            tabs: Tabs::new(),
        })
    });
}

/// Creates a new Chrome-style tab browser and shows it.
pub fn new_tab(url: &str) {
    let container = active_container();
    new_tab_in(url, container);
}

/// Container of the active tab (new tabs inherit it).
pub fn active_container() -> &'static str {
    with_shell(|s| s.tabs.active_item().map(|t| t.container))
        .flatten()
        .unwrap_or(crate::containers::DEFAULT)
}

/// Container of the tab that shows `view` (for popups).
pub fn container_of_view(view: &BrowserView) -> &'static str {
    let Some(mut b) = view.browser() else {
        return crate::containers::DEFAULT;
    };
    with_shell(|s| {
        s.tabs
            .find(|t| t.is(&mut b))
            .and_then(|id| s.tabs.get(id).map(|t| t.container))
    })
    .flatten()
    .unwrap_or(crate::containers::DEFAULT)
}

/// Opens a tab in the given container.
pub fn new_tab_in(url: &str, container: &str) {
    let Some(mut client) = with_shell(|s| s.client.clone()) else {
        return;
    };
    let container = crate::containers::normalize(container);
    let mut ctx = crate::containers::context(container);
    debug_log(&format!(
        "new tab in container {container}: own context = {}",
        ctx.is_some()
    ));
    let mut delegate = crate::app::TabViewDelegate::new();
    let view = browser_view_create(
        Some(&mut client),
        Some(&CefString::from(url)),
        Some(&BrowserSettings::default()),
        None,
        ctx.as_mut(),
        Some(&mut delegate),
    );
    if std::env::var_os("KB_DEBUG").is_some() {
        eprintln!("[kb] new_tab {url} -> view created: {}", view.is_some());
    }
    if let Some(view) = view {
        add_tab_view(view, container);
    }
}

/// Opens the startup tabs and activates the one marked active.
pub fn open_startup_tabs(tabs: &[SessionTab]) {
    for t in tabs {
        new_tab_in(&t.url, &t.container);
    }
    let active = tabs
        .iter()
        .position(|t| t.active)
        .unwrap_or(tabs.len().saturating_sub(1));
    let id = with_shell(|s| s.tabs.iter().nth(active).map(|(id, _)| id)).flatten();
    if let Some(id) = id {
        with_shell(|s| s.tabs.activate(id));
        refresh();
    }
}

/// Profile directory (`<data dir>/profiles/default`), if the platform data dir is known.
/// `KB_PROFILE_DIR` overrides it (tests, e.g. the zero-connections check with a fresh profile).
pub fn profile_dir() -> Option<std::path::PathBuf> {
    if let Some(dir) = std::env::var_os("KB_PROFILE_DIR").filter(|d| !d.is_empty()) {
        return Some(std::path::PathBuf::from(dir));
    }
    kb_platform::data_dir().map(|root| kb_platform::layout::profile(&root, "default"))
}

/// Adds an existing BrowserView (new tab or popup) as the active tab.
pub fn add_tab_view(view: BrowserView, container: &'static str) {
    let window = with_shell(|s| {
        s.tabs.open(Tab::new(view.clone(), container), true);
        s.window.clone()
    });
    if let Some(window) = window {
        let mut v = View::from(&view);
        // Index 0: tabs stay below the UI strip in z-order (a new BrowserView otherwise painted
        // over the strip until the next resize).
        window.add_child_view_at(Some(&mut v), 0);
        refresh();
    }
}

/// Positions the UI strip and the tab views inside a client area of `width`×`height`.
pub fn apply_layout(width: i32, height: i32) {
    let Some((ui, views)) = with_shell(|s| {
        let views: Vec<BrowserView> = s.tabs.iter().map(|(_, t)| t.view.clone()).collect();
        (s.ui.clone(), views)
    }) else {
        return;
    };
    let wanted = if PALETTE_OPEN.with(|p| *p.borrow()) {
        UI_PALETTE_HEIGHT
    } else {
        UI_STRIP_HEIGHT
    };
    let strip = wanted.min(height);
    ui.set_bounds(Some(&Rect {
        x: 0,
        y: 0,
        width,
        height: strip,
    }));
    let content = Rect {
        x: 0,
        y: strip,
        width,
        height: (height - strip).max(0),
    };
    for view in views {
        view.set_bounds(Some(&content));
    }
}

/// Re-runs the manual layout with the current window size, now and once more after CEF's own
/// layout pass (adding a child view triggers the window's default fill layout, which would
/// otherwise overwrite our bounds).
fn relayout() {
    relayout_now();
    let mut task = RelayoutTask::new();
    post_task(ThreadId::UI, Some(&mut task));
}

fn relayout_now() {
    if let Some(window) = with_shell(|s| s.window.clone()) {
        let b = window.client_area_bounds_in_screen();
        apply_layout(b.width, b.height);
    }
    debug_bounds();
}

wrap_task! {
    struct RelayoutTask;

    impl Task {
        fn execute(&self) {
            relayout_now();
        }
    }
}

/// How long an inactive tab may stay loaded before it is discarded (`KB_DISCARD_SECS` overrides).
fn discard_after() -> std::time::Duration {
    let secs = std::env::var("KB_DISCARD_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30 * 60);
    std::time::Duration::from_secs(secs)
}

/// Pure rule: may this tab be discarded now?
pub fn should_discard(
    active: bool,
    loading: bool,
    url: &str,
    already: bool,
    idle: std::time::Duration,
    limit: std::time::Duration,
) -> bool {
    !active && !loading && !already && idle >= limit && kb_privacy::is_filterable(url)
}

/// Discards idle background tabs (called by a periodic task).
pub fn discard_idle_tabs() {
    let power = kb_platform::power::current();
    let mut limit = discard_after();
    if power.on_battery {
        limit = limit.min(std::time::Duration::from_secs(5 * 60)); // battery mode (§5.4)
    }
    if power.low_memory() {
        limit = limit.min(std::time::Duration::from_secs(60)); // memory pressure (§5.2)
    }
    let views: Vec<(BrowserView, String)> = with_shell(|s| {
        let active = s.tabs.active();
        s.tabs
            .iter()
            .filter(|(id, t)| {
                should_discard(
                    Some(*id) == active,
                    t.loading,
                    &t.url,
                    t.discarded.is_some(),
                    t.last_active.elapsed(),
                    limit,
                )
            })
            .map(|(_, t)| (t.view.clone(), t.url.clone()))
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
    for (view, url) in views {
        with_shell(|s| {
            let bid = view.browser().map(|b| b.identifier());
            if let Some(id) = s
                .tabs
                .find(|t| t.view.browser().map(|b| b.identifier()) == bid)
                && let Some(t) = s.tabs.get_mut(id)
            {
                t.discarded = Some(url.clone());
            }
        });
        if let Some(frame) = view.browser().and_then(|b| b.main_frame()) {
            frame.load_url(Some(&CefString::from("about:blank")));
        }
        debug_log(&format!("discarded tab {url}"));
    }
    refresh();
}

wrap_task! {
    struct LifecycleTask;

    impl Task {
        fn execute(&self) {
            discard_idle_tabs();
            schedule_lifecycle();
        }
    }
}

/// Runs `discard_idle_tabs` every 30 s (first call starts the loop).
pub fn schedule_lifecycle() {
    let mut task = LifecycleTask::new();
    post_delayed_task(ThreadId::UI, Some(&mut task), 30_000);
}

/// Shows only the active tab, focuses it and updates the window title.
pub fn refresh() {
    let snapshot = with_shell(|s| {
        let active = s.tabs.active();
        let views: Vec<(bool, BrowserView)> = s
            .tabs
            .iter()
            .map(|(id, t)| (Some(id) == active, t.view.clone()))
            .collect();
        (s.window.clone(), views)
    });
    let Some((window, views)) = snapshot else {
        return;
    };
    // Activating a discarded tab restores its page; the active tab is never "idle".
    let restore = with_shell(|s| {
        let id = s.tabs.active()?;
        let t = s.tabs.get_mut(id)?;
        t.last_active = std::time::Instant::now();
        t.discarded.take().map(|u| (t.view.clone(), u))
    })
    .flatten();
    if let Some((view, url)) = restore
        && let Some(frame) = view.browser().and_then(|b| b.main_frame())
    {
        frame.load_url(Some(&CefString::from(url.as_str())));
    }
    let mut title = String::from("Kendime-Brow");
    for (is_active, view) in &views {
        view.set_visible(*is_active as i32);
        if *is_active {
            if !PALETTE_OPEN.with(|p| *p.borrow()) {
                view.request_focus();
            }
            if let Some(entry) = view
                .browser()
                .and_then(|b| b.host())
                .and_then(|h| h.visible_navigation_entry())
            {
                let t = CefString::from(&entry.title()).to_string();
                if !t.is_empty() {
                    title = format!("{t} — Kendime-Brow");
                }
            }
        }
    }
    relayout();
    window.set_title(Some(&CefString::from(title.as_str())));
    crate::bridge::broadcast(&state());
    save_session();
    debug_bounds();
}

/// Snapshot of all tabs for the UI strip.
pub fn state() -> protocol::State {
    with_shell(|s| {
        let active = s.tabs.active();
        protocol::State {
            tabs: s
                .tabs
                .iter()
                .map(|(id, t)| protocol::TabState {
                    id: id.0,
                    title: t.title.clone(),
                    url: t.url.clone(),
                    loading: t.loading,
                    active: Some(id) == active,
                    can_go_back: t.can_go_back,
                    can_go_forward: t.can_go_forward,
                    bookmarked: with_store(|st| st.is_bookmarked(&t.url)).unwrap_or(false),
                    blocked: t.view.browser().map_or(0, |b| {
                        crate::content_blocking::blocked_count(b.identifier())
                    }),
                    shield: crate::content_blocking::shield_enabled_for(&t.url),
                    container: t.container.to_string(),
                })
                .collect(),
            palette: PALETTE_OPEN.with(|p| *p.borrow()),
            downloads: crate::downloads::list(),
        }
    })
    .unwrap_or_default()
}

/// Opens or closes the command palette: resizes the strip and tells the UI.
pub fn set_palette(open: bool) {
    PALETTE_OPEN.with(|p| *p.borrow_mut() = open);
    if open && let Some(ui) = with_shell(|s| s.ui.clone()) {
        ui.request_focus();
    }
    refresh();
    if !open && let Some(view) = active_view() {
        view.request_focus();
    }
}

/// Runs a command received from a `kb://` page.
pub fn run_bridge_command(cmd: Command) {
    match cmd {
        Command::NewTab { url, container } => {
            let url = url.map_or_else(|| NEW_TAB_URL.to_string(), |u| to_url(&u));
            match container {
                Some(c) => new_tab_in(&url, &c),
                None => new_tab(&url),
            }
        }
        Command::CloseTab { id } => close_tab(TabId(id)),
        Command::ActivateTab { id } => {
            with_shell(|s| s.tabs.activate(TabId(id)));
            refresh();
        }
        Command::Navigate { text } => {
            let url = to_url(&text);
            if let Some(frame) = active_view()
                .and_then(|v| v.browser())
                .and_then(|b| b.main_frame())
            {
                frame.load_url(Some(&CefString::from(url.as_str())));
            }
        }
        Command::Back => {
            run_command(command::BACK);
        }
        Command::Forward => {
            run_command(command::FORWARD);
        }
        Command::Reload => {
            run_command(command::RELOAD);
        }
        Command::Stop => {
            if let Some(b) = active_view().and_then(|v| v.browser()) {
                b.stop_load();
            }
        }
        Command::Autofill => {
            if let Some(frame) = active_view()
                .and_then(|v| v.browser())
                .and_then(|b| b.main_frame())
            {
                let url = CefString::from(&frame.url()).to_string();
                let js = crate::autofill::script_for(&url);
                frame.execute_java_script(
                    Some(&CefString::from(js.as_str())),
                    Some(&CefString::from("kb://autofill")),
                    0,
                );
            }
        }
        Command::CloseActiveTab => close_active_tab(),
        Command::CycleTab { forward } => {
            with_shell(|s| s.tabs.cycle(forward));
            refresh();
        }
        Command::RunAction { id } => {
            set_palette(false);
            if let Some(cmd) = crate::actions::command_for(&id) {
                run_bridge_command(cmd);
            }
        }
        Command::SetPalette { open } => set_palette(open),
        Command::ToggleBookmark => {
            let tab = with_shell(|s| {
                s.tabs
                    .active_item()
                    .map(|t| (t.url.clone(), t.title.clone()))
            })
            .flatten();
            if let Some((url, title)) = tab
                && !crate::store::is_internal(&url)
            {
                with_store(|s| s.toggle_bookmark(&url, &title));
                refresh();
            }
        }
        Command::SetSetting { key, value } => {
            if key.starts_with("proxy_") && !crate::net_privacy::valid_proxy(&value) {
                return;
            }
            with_store(|s| s.set_setting(&key, &value));
            if key == "doh" || key == "https_only" || key.starts_with("proxy_") {
                apply_network_settings();
            }
            if key == "fingerprint_protection" {
                // Applies to renderer processes started from now on (new tabs / reloads).
                crate::privacy_flags::set_fingerprint_level(kb_privacy::fingerprint::Level::parse(
                    &value,
                ));
            }
        }
        Command::DeleteBookmark { id } => {
            with_store(|s| s.delete_bookmark(id));
            refresh();
        }
        Command::ClearHistory => {
            with_store(|s| s.clear_history());
        }
        Command::CancelDownload { id } => crate::downloads::cancel(id),
        Command::PauseDownload { id } => crate::downloads::pause(id),
        Command::ResumeDownload { id } => crate::downloads::resume(id),
        Command::RevealDownload { id } => crate::downloads::reveal(id),
        Command::OpenDownload { id } => crate::downloads::open(id),
        Command::SetFilterList { id, enabled } => {
            if let Some(profile) = profile_dir() {
                let dir = crate::content_blocking::filters_dir(&profile);
                let mut state = kb_privacy::lists::ListsFile::load(&dir);
                if state.set_enabled(&id, enabled) {
                    let _ = state.save(&dir);
                    crate::content_blocking::reload(&profile);
                    if enabled {
                        crate::content_blocking::start_update(profile, false);
                    }
                }
            }
        }
        Command::ToggleShield => {
            let url = with_shell(|s| s.tabs.active_item().map(|t| t.url.clone())).flatten();
            if let Some(host) = url.as_deref().and_then(kb_privacy::url_clean::host_of) {
                let enable =
                    !crate::content_blocking::shield_enabled_for(url.as_deref().unwrap_or(""));
                let hosts = with_store(|s| {
                    let _ = s.set_shield(&host, enable);
                    s.shield_off_hosts().unwrap_or_default()
                })
                .unwrap_or_default();
                crate::content_blocking::set_shield_off_hosts(hosts);
                run_command(command::RELOAD);
                refresh();
            }
        }
        Command::UpdateFilters => {
            if let Some(profile) = profile_dir() {
                crate::content_blocking::start_update(profile, true);
            }
        }
        Command::ClearDownloads => {
            crate::downloads::clear_finished();
            refresh();
        }
        Command::KillProcess { pid } => {
            crate::tasks::kill(pid);
        }
        Command::State
        | Command::Subscribe
        | Command::Actions { .. }
        | Command::Profile { .. }
        | Command::Tasks
        | Command::PdfText
        | Command::PdfTables
        | Command::PdfFields
        | Command::SheetPreview { .. }
        | Command::Vault { .. }
        | Command::Tunnels { .. }
        | Command::Files { .. }
        | Command::Connections { .. }
        | Command::Net { .. }
        | Command::SessionLog { .. }
        | Command::Userscripts { .. }
        | Command::TermOpen { .. }
        | Command::TermWrite { .. }
        | Command::TermResize { .. }
        | Command::TermClose { .. }
        | Command::ImageMeta { .. }
        | Command::ImageStrip { .. }
        | Command::ImageConvert { .. }
        | Command::ImageData { .. }
        | Command::PdfFill { .. }
        | Command::PdfExtract { .. }
        | Command::PdfRotate { .. }
        | Command::PdfMerge { .. }
        | Command::PdfOpen { .. }
        | Command::PdfRender { .. } => {}
    }
}

#[derive(Default)]
pub struct TabUpdate {
    pub title: Option<String>,
    pub url: Option<String>,
    /// (loading, can_go_back, can_go_forward)
    pub loading: Option<(bool, bool, bool)>,
}

/// Updates the stored state of the tab showing `browser` and notifies the UI.
pub fn update_tab(browser: &mut Browser, upd: TabUpdate) {
    let changed = with_shell(|s| {
        let id = s.tabs.find(|t| t.is(browser))?;
        let tab = s.tabs.get_mut(id)?;
        if tab.discarded.is_some() {
            return None; // placeholder page: keep the saved title/URL
        }
        if let Some(t) = upd.title {
            if crate::containers::get(tab.container).is_some_and(|c| c.persistent) {
                with_store(|st| st.update_title(&tab.url, &t));
            }
            tab.title = t;
        }
        let private = crate::containers::get(tab.container).is_some_and(|c| !c.persistent);
        if let Some(u) = upd.url {
            if u != tab.url && !private {
                with_store(|st| st.record_visit(&u, ""));
            }
            tab.url = u;
        }
        if let Some((l, b, fw)) = upd.loading {
            tab.loading = l;
            tab.can_go_back = b;
            tab.can_go_forward = fw;
        }
        Some(())
    })
    .flatten();
    if changed.is_some() {
        refresh();
    }
}

fn active_view() -> Option<BrowserView> {
    with_shell(|s| s.tabs.active_item().map(|t| t.view.clone())).flatten()
}

fn tab_of(browser: &mut Browser) -> Option<TabId> {
    with_shell(|s| s.tabs.find(|t| t.is(browser))).flatten()
}

/// Maps a key-down (Windows virtual key code + modifiers) to a command.
pub fn command_for_key(key: i32, shift: bool, ctrl: bool, alt: bool) -> Option<i32> {
    ACCELERATORS
        .iter()
        .find(|(_, k, s, c, a)| (*k, *s, *c, *a) == (key, shift, ctrl, alt))
        .map(|(cmd, ..)| *cmd)
}

/// Runs a shell command (from a shortcut or, later, the UI). Returns true if handled.
pub fn run_command(cmd: i32) -> bool {
    if std::env::var_os("KB_DEBUG").is_some() {
        eprintln!("[kb] command {cmd}");
    }
    match cmd {
        command::NEW_TAB => new_tab(NEW_TAB_URL),
        command::PALETTE => set_palette(!PALETTE_OPEN.with(|p| *p.borrow())),
        command::CLOSE_TAB => close_active_tab(),
        command::NEXT_TAB | command::PREV_TAB => {
            with_shell(|s| s.tabs.cycle(cmd == command::NEXT_TAB));
            refresh();
        }
        command::BACK | command::FORWARD | command::RELOAD => {
            if let Some(browser) = active_view().and_then(|v| v.browser()) {
                match cmd {
                    command::BACK => browser.go_back(),
                    command::FORWARD => browser.go_forward(),
                    _ => browser.reload(),
                }
            }
        }
        _ => return false,
    }
    true
}

/// Closes the active tab; closing the last tab closes the window.
pub fn close_active_tab() {
    if let Some(id) = with_shell(|s| s.tabs.active()).flatten() {
        close_tab(id);
    }
}

/// Closes a tab; closing the last tab closes the window.
pub fn close_tab(id: TabId) {
    let Some(view) = with_shell(|s| s.tabs.get(id).map(|t| t.view.clone())).flatten() else {
        return;
    };
    let last = with_shell(|s| s.tabs.len() == 1).unwrap_or(true);
    if last {
        if let Some(window) = with_shell(|s| s.window.clone()) {
            window.close();
        }
        return;
    }
    if let Some(host) = view.browser().and_then(|b| b.host()) {
        // The tab is removed from the list in `on_before_close`.
        host.close_browser(0);
    }
}

/// Title of a browser changed.
pub fn on_title_change(browser: &mut Browser, title: String) {
    update_tab(
        browser,
        TabUpdate {
            title: Some(title),
            ..Default::default()
        },
    );
}

pub fn on_after_created() {
    let n = BROWSER_COUNT.with(|c| {
        *c.borrow_mut() += 1;
        *c.borrow()
    });
    debug_log(&format!("browser created, count={n}"));
}

/// Logs view bounds (KB_DEBUG) — layout diagnostics.
pub fn debug_bounds() {
    if std::env::var_os("KB_DEBUG").is_none() {
        return;
    }
    with_shell(|s| {
        let w = s.window.bounds();
        eprintln!(
            "[kb] window {}x{} children={}",
            w.width,
            w.height,
            s.window.child_view_count()
        );
        let u = s.ui.bounds();
        eprintln!(
            "[kb] ui y={} {}x{} visible={}",
            u.y,
            u.width,
            u.height,
            s.ui.is_visible()
        );
        for (id, t) in s.tabs.iter() {
            let b = t.view.bounds();
            eprintln!(
                "[kb] tab {} y={} {}x{} visible={}",
                id.0,
                b.y,
                b.width,
                b.height,
                t.view.is_visible()
            );
        }
    });
}

fn debug_log(msg: &str) {
    if std::env::var_os("KB_DEBUG").is_some() {
        eprintln!("[kb] {msg}");
    }
}

/// `LifeSpanHandler::do_close`. Returns true to cancel CEF's default behaviour, which would
/// close the whole window: a single tab is closed by removing its BrowserView from the
/// hierarchy (dropping our references destroys the browser).
pub fn do_close(browser: &mut Browser) -> bool {
    if WINDOW_CLOSING.with(|c| *c.borrow()) {
        return false;
    }
    let Some(id) = tab_of(browser) else {
        return false; // kb://ui strip or unknown browser
    };
    let removed = with_shell(|s| {
        if s.tabs.len() <= 1 {
            return None; // last tab: let the window close
        }
        s.tabs.close(id).map(|t| {
            let gone = t.container;
            if !s.tabs.iter().any(|(_, o)| o.container == gone) {
                crate::containers::discard(gone);
            }
            (t.view, s.window.clone())
        })
    })
    .flatten();
    let Some((view, window)) = removed else {
        return false;
    };
    let mut v = View::from(&view);
    window.remove_child_view(Some(&mut v));
    drop(v);
    drop(view);
    refresh();
    true
}

/// A browser is gone: drop any remaining tab entry and quit when nothing is left.
pub fn on_before_close(browser: &mut Browser) {
    if let Some(id) = tab_of(browser) {
        with_shell(|s| s.tabs.close(id));
    }
    let left = BROWSER_COUNT.with(|c| {
        let mut c = c.borrow_mut();
        *c = c.saturating_sub(1);
        *c
    });
    debug_log(&format!("browser closed, count={left}"));
    if left == 0 {
        SHELL.with(|s| s.borrow_mut().take());
        quit_message_loop();
    }
}

/// Window close requested: ask every browser to close; allow once all agree.
pub fn can_close_window(ui_view: Option<&BrowserView>) -> bool {
    WINDOW_CLOSING.with(|c| *c.borrow_mut() = true);
    let mut views: Vec<BrowserView> =
        with_shell(|s| s.tabs.iter().map(|(_, t)| t.view.clone()).collect()).unwrap_or_default();
    views.extend(ui_view.cloned());
    let mut all = true;
    for view in views {
        if let Some(host) = view.browser().and_then(|b| b.host()) {
            all &= host.try_close_browser() == 1;
        }
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_mapping() {
        assert_eq!(
            command_for_key(VK_T, false, true, false),
            Some(command::NEW_TAB)
        );
        assert_eq!(
            command_for_key(VK_TAB, true, true, false),
            Some(command::PREV_TAB)
        );
        assert_eq!(
            command_for_key(VK_TAB, false, true, false),
            Some(command::NEXT_TAB)
        );
        assert_eq!(
            command_for_key(VK_LEFT, false, false, true),
            Some(command::BACK)
        );
        assert_eq!(command_for_key(VK_T, false, false, false), None);
        assert_eq!(command_for_key(VK_T, true, true, false), None);
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn discard_rule() {
        let lim = Duration::from_secs(60);
        let idle = Duration::from_secs(120);
        assert!(should_discard(
            false,
            false,
            "https://a.org/",
            false,
            idle,
            lim
        ));
        assert!(!should_discard(
            true,
            false,
            "https://a.org/",
            false,
            idle,
            lim
        ));
        assert!(!should_discard(
            false,
            true,
            "https://a.org/",
            false,
            idle,
            lim
        ));
        assert!(!should_discard(
            false,
            false,
            "https://a.org/",
            true,
            idle,
            lim
        ));
        assert!(!should_discard(
            false,
            false,
            "kb://settings/",
            false,
            idle,
            lim
        ));
        assert!(!should_discard(
            false,
            false,
            "https://a.org/",
            false,
            Duration::from_secs(5),
            lim
        ));
    }
}
