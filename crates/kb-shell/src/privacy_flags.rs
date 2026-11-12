//! Chromium switches that turn off Google services and background networking
//! (architecture §4.1, task 2.1). Applied to the browser process command line in
//! `App::on_before_command_line_processing`; Chromium forwards what subprocesses need.
//!
//! Verified by `scripts/check-connections.mjs` (NetLog of a fresh profile must contain no
//! connections outside the allowlist).

/// Switches without a value.
pub const SWITCHES: &[&str] = &[
    "disable-background-networking", // update checks, field trials, safe browsing list updates…
    "disable-component-update",
    "disable-domain-reliability",
    "disable-sync",
    "disable-client-side-phishing-detection",
    "disable-default-apps",
    "disable-breakpad", // no crash uploads (local dumps only, §4.1)
    "no-pings",         // <a ping> auditing
    "no-default-browser-check",
    "no-first-run",
    "metrics-recording-only", // never upload metrics
    "disable-search-engine-choice-screen",
    "dns-prefetch-disable",
];

/// Features disabled through `--disable-features` (comma-joined).
pub const DISABLED_FEATURES: &[&str] = &[
    "OptimizationHints",
    "OptimizationGuideModelDownloading",
    "MediaRouter",
    "Translate",
    "AutofillServerCommunication",
    "CertificateTransparencyComponentUpdater",
    "InterestFeedContentSuggestions",
    "NetworkTimeServiceQuerying",
    "SafeBrowsingEnhancedProtection",
    "PrivacySandboxSettings4",
    "BrowsingTopics",
    "FledgeInterestGroups",
    "AttributionReportingCrossAppWeb",
    "NoStatePrefetch",
    "SpeculationRulesPrefetchFuture",
    // AI Mode eligibility service: fetches www.google.com/async/folae at startup
    // (components/omnibox/browser/aim_eligibility_service*.cc).
    "AimEnabled",
    "AimServerEligibilityEnabled",
    "AimServerRequestOnStartupEnabled",
];

/// Preference value.
#[derive(Debug, Clone, Copy)]
pub enum Pref {
    Bool(bool),
    Int(i32),
    Str(&'static str),
}

/// Local-state (global) preferences.
pub const GLOBAL_PREFS: &[(&str, Pref)] = &[
    (
        "component_updates.component_updates_enabled",
        Pref::Bool(false),
    ),
    ("background_mode.enabled", Pref::Bool(false)),
    // Automatic DoH upgrade probes the system resolver's DoH endpoint (e.g. Google DNS);
    // user-selected DoH comes in task 2.7.
    ("dns_over_https.mode", Pref::Str("off")),
];

/// Profile preferences (applied to the global request context).
pub const PROFILE_PREFS: &[(&str, Pref)] = &[
    ("search.suggest_enabled", Pref::Bool(false)),
    ("safebrowsing.enabled", Pref::Bool(false)),
    ("translate.enabled", Pref::Bool(false)),
    ("alternate_error_pages.enabled", Pref::Bool(false)),
    ("spellcheck.use_spelling_service", Pref::Bool(false)),
    (
        "url_keyed_anonymized_data_collection.enabled",
        Pref::Bool(false),
    ),
    ("enable_do_not_track", Pref::Bool(true)),
    // 2 = never predict / preconnect (architecture §4.5).
    ("net.network_prediction_options", Pref::Int(2)),
    // WebRTC must not reveal local IPs (architecture §4.4).
    (
        "webrtc.ip_handling_policy",
        Pref::Str("disable_non_proxied_udp"),
    ),
    // Block third-party cookies (architecture §4.3). 1 = block third-party.
    ("profile.cookie_controls_mode", Pref::Int(1)),
    ("profile.block_third_party_cookies", Pref::Bool(true)),
];

// ------------------------------------------------------------ fingerprinting (task 2.6)

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// Random per browser session; renderers derive per-site seeds from it.
static FP_SEED: AtomicU64 = AtomicU64::new(0);
static FP_LEVEL: Mutex<&'static str> = Mutex::new("standard");

pub const SWITCH_FP_SEED: &str = "kb-fp-seed";
pub const SWITCH_FP_LEVEL: &str = "kb-fp-level";

/// Call once in the browser process before child processes start.
pub fn init_fingerprint(level: kb_privacy::fingerprint::Level) {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    );
    FP_SEED.store(h.finish() | 1, Ordering::Relaxed);
    if let Ok(mut l) = FP_LEVEL.lock() {
        *l = level.as_str();
    }
}

pub fn set_fingerprint_level(level: kb_privacy::fingerprint::Level) {
    if let Ok(mut l) = FP_LEVEL.lock() {
        *l = level.as_str();
    }
}

/// Adds the fingerprinting switches to a child process command line.
pub fn add_child_switches(cl: &mut cef::CommandLine) {
    use cef::{CefString, ImplCommandLine};
    let level = FP_LEVEL.lock().map(|l| *l).unwrap_or("standard");
    cl.append_switch_with_value(
        Some(&CefString::from(SWITCH_FP_SEED)),
        Some(&CefString::from(
            FP_SEED.load(Ordering::Relaxed).to_string().as_str(),
        )),
    );
    cl.append_switch_with_value(
        Some(&CefString::from(SWITCH_FP_LEVEL)),
        Some(&CefString::from(level)),
    );
}

/// Applies `PROFILE_PREFS` to one request context (used for every container). Returns the
/// names that could not be set.
pub fn apply_context_prefs(ctx: &cef::RequestContext) -> Vec<String> {
    use cef::*;
    let mut failed = Vec::new();
    for (name, pref) in PROFILE_PREFS {
        let Some(mut v) = value_create() else {
            continue;
        };
        match pref {
            Pref::Bool(b) => v.set_bool(*b as i32),
            Pref::Int(i) => v.set_int(*i),
            Pref::Str(t) => v.set_string(Some(&CefString::from(*t))),
        };
        let n = CefString::from(*name);
        if ctx.can_set_preference(Some(&n)) == 0
            || ctx.set_preference(Some(&n), Some(&mut v), None) == 0
        {
            failed.push(name.to_string());
        }
    }
    failed
}

/// Applies `GLOBAL_PREFS` and `PROFILE_PREFS`. Unknown prefs are skipped. Returns the names
/// that could not be set (for logging).
pub fn apply_prefs() -> Vec<String> {
    use cef::*;
    let mut failed = Vec::new();
    let mut set = |mgr: &dyn Fn(&CefString, &mut Value) -> bool, name: &str, pref: Pref| {
        let Some(mut v) = value_create() else { return };
        match pref {
            Pref::Bool(b) => v.set_bool(b as i32),
            Pref::Int(i) => v.set_int(i),
            Pref::Str(t) => v.set_string(Some(&CefString::from(t))),
        };
        if !mgr(&CefString::from(name), &mut v) {
            failed.push(name.to_string());
        }
    };
    if let Some(global) = preference_manager_get_global() {
        let apply = |name: &CefString, v: &mut Value| {
            global.can_set_preference(Some(name)) != 0
                && global.set_preference(Some(name), Some(v), None) != 0
        };
        for (name, pref) in GLOBAL_PREFS {
            set(&apply, name, *pref);
        }
    }
    if let Some(ctx) = request_context_get_global_context() {
        let apply = |name: &CefString, v: &mut Value| {
            ctx.can_set_preference(Some(name)) != 0
                && ctx.set_preference(Some(name), Some(v), None) != 0
        };
        for (name, pref) in PROFILE_PREFS {
            set(&apply, name, *pref);
        }
    }
    failed
}

/// `(switch, value)` pairs.
/// Hosts made unresolvable for the whole network service. Needed because the component
/// updater runs in the browser process' system network context, ignores
/// `--disable-component-update` in CEF 156 (verified with NetLog) and is not visible to CEF's
/// request handlers.
pub const BLOCKED_HOSTS: &[&str] = &[
    "update.googleapis.com",
    "*.gvt1.com",
    "clients2.google.com",
    "clientservices.googleapis.com",
    "optimizationguide-pa.googleapis.com",
    "safebrowsing.googleapis.com",
];

pub fn host_resolver_rules() -> String {
    BLOCKED_HOSTS
        .iter()
        .map(|h| format!("MAP {h} ~NOTFOUND"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn valued_switches() -> Vec<(&'static str, String)> {
    vec![
        ("host-resolver-rules", host_resolver_rules()),
        ("disable-features", DISABLED_FEATURES.join(",")),
        // No Google API key is embedded; make it explicit for components that check.
        ("google-api-key", "no".into()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switch_lists_are_sane() {
        let mut all: Vec<&str> = SWITCHES.to_vec();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), SWITCHES.len(), "duplicate switch");
        assert!(SWITCHES.iter().all(|s| !s.starts_with('-')));
        assert!(!DISABLED_FEATURES.iter().any(|f| f.contains(',')));
        assert!(
            valued_switches()
                .iter()
                .any(|(_, v)| v.contains("Translate"))
        );
        assert_eq!(
            host_resolver_rules().split(", ").next(),
            Some("MAP update.googleapis.com ~NOTFOUND")
        );
    }
}
