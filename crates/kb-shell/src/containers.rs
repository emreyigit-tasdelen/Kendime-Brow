//! Containers (architecture §4.3, task 2.5): each container is its own `CefRequestContext`
//! with separate cookies, cache and storage. `temporary` is in-memory (private tabs) and is
//! wiped when its last tab closes.

use std::cell::RefCell;
use std::collections::HashMap;

use cef::*;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Container {
    pub id: &'static str,
    pub title: &'static str,
    /// CSS colour used by the UI strip.
    pub color: &'static str,
    /// Persistent containers keep data on disk; non-persistent ones are in-memory.
    pub persistent: bool,
}

pub const DEFAULT: &str = "personal";
pub const PRIVATE: &str = "temporary";

pub const CONTAINERS: &[Container] = &[
    Container {
        id: "personal",
        title: "Personal",
        color: "",
        persistent: true,
    },
    Container {
        id: "work",
        title: "Work",
        color: "#e0913b",
        persistent: true,
    },
    Container {
        id: "temporary",
        title: "Private",
        color: "#8a63d2",
        persistent: false,
    },
];

pub fn get(id: &str) -> Option<&'static Container> {
    CONTAINERS.iter().find(|c| c.id == id)
}

/// Normalizes an unknown id to the default container.
pub fn normalize(id: &str) -> &'static str {
    get(id).map_or(DEFAULT, |c| c.id)
}

thread_local! {
    static CONTEXTS: RefCell<HashMap<&'static str, RequestContext>> = RefCell::new(HashMap::new());
}

/// Request context for a container (UI thread). `personal` uses the global context.
pub fn context(id: &str) -> Option<RequestContext> {
    let c = get(id)?;
    if c.id == DEFAULT {
        return None;
    }
    if let Some(ctx) = CONTEXTS.with(|m| m.borrow().get(c.id).cloned()) {
        return Some(ctx);
    }
    // Persistent containers live under <profile>/cef/containers/<id> (must be inside the
    // root cache path); an empty cache path makes the context in-memory ("incognito").
    let cache = if c.persistent {
        crate::shell::profile_dir()
            .map(|p| p.join("cef").join("containers").join(c.id))
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let settings = RequestContextSettings {
        cache_path: CefString::from(cache.as_str()),
        persist_session_cookies: c.persistent as i32,
        ..Default::default()
    };
    let ctx = request_context_create_context(Some(&settings), None)?;
    crate::privacy_flags::apply_context_prefs(&ctx);
    crate::shell::apply_context_network(&ctx, c.id);
    CONTEXTS.with(|m| m.borrow_mut().insert(c.id, ctx.clone()));
    Some(ctx)
}

/// The context of a container if it has been created already (no creation).
pub fn existing(id: &str) -> Option<RequestContext> {
    CONTEXTS.with(|m| m.borrow().get(id).cloned())
}

/// Drops a non-persistent container's context (its data disappears with it).
pub fn discard(id: &str) {
    if get(id).is_some_and(|c| !c.persistent) {
        CONTEXTS.with(|m| m.borrow_mut().remove(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog() {
        assert_eq!(normalize("work"), "work");
        assert_eq!(normalize("nope"), DEFAULT);
        assert!(!get(PRIVATE).unwrap().persistent);
        assert!(get(DEFAULT).unwrap().persistent);
    }
}
