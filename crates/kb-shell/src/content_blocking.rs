//! Request blocking glue (architecture §4.2, task 2.2): asks `kb_privacy::Blocker` about every
//! sub-resource request and counts blocked requests per browser for the UI.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, RwLock};

use cef::*;
use kb_privacy::{Blocker, ResourceType as Kind};

pub static BLOCKER: Blocker = Blocker::new();

/// Hosts with the shield switched off (mirror of the `shield_off` table for the IO thread).
static SHIELD_OFF: RwLock<Option<HashSet<String>>> = RwLock::new(None);

pub fn set_shield_off_hosts(hosts: impl IntoIterator<Item = String>) {
    if let Ok(mut s) = SHIELD_OFF.write() {
        *s = Some(hosts.into_iter().collect());
    }
}

pub fn shield_enabled_for(url: &str) -> bool {
    let Some(host) = kb_privacy::url_clean::host_of(url) else {
        return true;
    };
    SHIELD_OFF
        .read()
        .ok()
        .and_then(|s| s.as_ref().map(|s| !s.contains(&host)))
        .unwrap_or(true)
}

/// Blocked request count per browser id (reset on main-frame navigation).
static BLOCKED: Mutex<Option<HashMap<i32, u64>>> = Mutex::new(None);

use kb_privacy::lists::{self, ListsFile};

pub fn filters_dir(profile: &std::path::Path) -> std::path::PathBuf {
    profile.join("filters")
}

/// (Re)loads the enabled lists from `<profile>/filters`. Returns the number of lists.
pub fn reload(profile: &std::path::Path) -> usize {
    let dir = filters_dir(profile);
    let state = ListsFile::load(&dir);
    BLOCKER.load_files(&lists::enabled_files(&dir, &state))
}

/// Downloads due (or, with `force`, all enabled) lists on a background thread, swaps the
/// engine and asks the UI thread to refresh.
pub fn start_update(profile: std::path::PathBuf, force: bool) {
    std::thread::spawn(move || {
        let dir = filters_dir(&profile);
        let mut state = ListsFile::load(&dir);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let due = state.due(now, force);
        if due.is_empty() {
            return;
        }
        let changed = lists::update_lists(&dir, &mut state, &due, now, &lists::download);
        let _ = state.save(&dir);
        if changed {
            reload(&profile);
        }
        let mut task = crate::shell::RefreshTask::new();
        post_task(ThreadId::UI, Some(&mut task));
    });
}

/// JavaScript that applies the cosmetic filters of `url` (empty if there are none). Uses a
/// constructable stylesheet, which the page's own CSP does not restrict.
pub fn cosmetic_script(url: &str) -> String {
    if !kb_privacy::is_filterable(url) {
        return String::new();
    }
    let c = BLOCKER.cosmetic(url);
    let mut js = String::new();
    if !c.css.is_empty() {
        let css = serde_json::to_string(&c.css).unwrap_or_default();
        js.push_str(&format!(
            "(()=>{{try{{const s=new CSSStyleSheet();s.replaceSync({css});\
             const d=document;const add=()=>{{d.adoptedStyleSheets=[...d.adoptedStyleSheets,s]}};\
             add();}}catch(e){{}}}})();"
        ));
    }
    js.push_str(&c.script);
    js
}

pub fn blocked_count(browser_id: i32) -> u64 {
    BLOCKED
        .lock()
        .ok()
        .and_then(|m| m.as_ref().and_then(|m| m.get(&browser_id).copied()))
        .unwrap_or(0)
}

pub fn reset_count(browser_id: i32) {
    if let Ok(mut m) = BLOCKED.lock() {
        m.get_or_insert_with(HashMap::new).remove(&browser_id);
    }
}

fn count(browser_id: i32) {
    if let Ok(mut m) = BLOCKED.lock() {
        *m.get_or_insert_with(HashMap::new)
            .entry(browser_id)
            .or_default() += 1;
    }
}

fn kind(t: ResourceType) -> Kind {
    match t {
        ResourceType::MAIN_FRAME => Kind::Document,
        ResourceType::SUB_FRAME => Kind::SubFrame,
        ResourceType::STYLESHEET => Kind::Stylesheet,
        ResourceType::SCRIPT | ResourceType::WORKER | ResourceType::SHARED_WORKER => Kind::Script,
        ResourceType::IMAGE | ResourceType::FAVICON => Kind::Image,
        ResourceType::FONT_RESOURCE => Kind::Font,
        ResourceType::MEDIA => Kind::Media,
        ResourceType::XHR => Kind::Xhr,
        ResourceType::PING | ResourceType::CSP_REPORT => Kind::Ping,
        _ => Kind::Other,
    }
}

wrap_resource_request_handler! {
    pub struct KbResourceRequestHandler;

    impl ResourceRequestHandler {
        fn on_before_resource_load(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            let Some(request) = request else {
                return ReturnValue::CONTINUE;
            };
            let k = kind(request.resource_type());
            let url = CefString::from(&request.url()).to_string();
            if !kb_privacy::is_filterable(&url) {
                return ReturnValue::CONTINUE;
            }
            // The page the request belongs to decides the shield (main frame URL).
            let page = match k {
                Kind::Document => url.clone(),
                _ => browser
                    .as_deref()
                    .and_then(|b| b.main_frame())
                    .map(|f| CefString::from(&f.url()).to_string())
                    .unwrap_or_default(),
            };
            if !shield_enabled_for(&page) {
                return ReturnValue::CONTINUE;
            }
            // Documents: strip tracking parameters / unwrap redirect wrappers, never block
            // (a typed address always loads — decision in 2.2).
            if matches!(k, Kind::Document | Kind::SubFrame)
                && let Some(clean) = kb_privacy::url_clean::clean_url(&url)
            {
                request.set_url(Some(&CefString::from(clean.as_str())));
                return ReturnValue::CONTINUE;
            }
            if k == Kind::Document {
                return ReturnValue::CONTINUE;
            }
            let source = frame
                .map(|f| CefString::from(&f.url()).to_string())
                .unwrap_or_default();
            let decision = BLOCKER.check(&url, &source, k);
            if decision.block {
                if let Some(b) = browser {
                    count(b.identifier());
                }
                return ReturnValue::CANCEL;
            }
            if let Some(clean) = decision.rewritten_url {
                request.set_url(Some(&CefString::from(clean.as_str())));
            }
            ReturnValue::CONTINUE
        }
    }
}
