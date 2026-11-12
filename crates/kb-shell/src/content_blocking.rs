//! Request blocking glue (architecture §4.2, task 2.2): asks `kb_privacy::Blocker` about every
//! sub-resource request and counts blocked requests per browser for the UI.

use std::collections::HashMap;
use std::sync::Mutex;

use cef::*;
use kb_privacy::{Blocker, ResourceType as Kind};

pub static BLOCKER: Blocker = Blocker::new();

/// Blocked request count per browser id (reset on main-frame navigation).
static BLOCKED: Mutex<Option<HashMap<i32, u64>>> = Mutex::new(None);

/// Loads `<profile>/filters/*.txt`. Returns the number of lists.
pub fn load_filters(profile: &std::path::Path) -> usize {
    BLOCKER.load_dir(&profile.join("filters")).unwrap_or(0)
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

fn kind(t: ResourceType) -> Option<Kind> {
    Some(match t {
        // Main-frame navigations are never blocked (decision in 2.2).
        ResourceType::MAIN_FRAME => return None,
        ResourceType::SUB_FRAME => Kind::SubFrame,
        ResourceType::STYLESHEET => Kind::Stylesheet,
        ResourceType::SCRIPT | ResourceType::WORKER | ResourceType::SHARED_WORKER => Kind::Script,
        ResourceType::IMAGE | ResourceType::FAVICON => Kind::Image,
        ResourceType::FONT_RESOURCE => Kind::Font,
        ResourceType::MEDIA => Kind::Media,
        ResourceType::XHR => Kind::Xhr,
        ResourceType::PING | ResourceType::CSP_REPORT => Kind::Ping,
        _ => Kind::Other,
    })
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
            let Some(k) = kind(request.resource_type()) else {
                return ReturnValue::CONTINUE;
            };
            let url = CefString::from(&request.url()).to_string();
            if !kb_privacy::is_filterable(&url) {
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
