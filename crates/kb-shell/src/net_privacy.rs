//! Network privacy settings (architecture §4.5, task 2.7): HTTPS-Only, DNS-over-HTTPS and
//! per-container proxies, applied through Chromium preferences.

use cef::*;

/// DoH providers offered in settings: (id, title, template).
pub const DOH_PROVIDERS: &[(&str, &str, &str)] = &[
    ("quad9", "Quad9", "https://dns.quad9.net/dns-query"),
    ("mullvad", "Mullvad", "https://dns.mullvad.net/dns-query"),
    (
        "cloudflare",
        "Cloudflare",
        "https://cloudflare-dns.com/dns-query",
    ),
];

/// DoH template for a setting value: `off`, a provider id, or a custom `https://` template.
pub fn doh_template(setting: &str) -> Option<String> {
    if setting.is_empty() || setting == "off" {
        return None;
    }
    if let Some((_, _, t)) = DOH_PROVIDERS.iter().find(|(id, _, _)| *id == setting) {
        return Some((*t).to_string());
    }
    setting.starts_with("https://").then(|| setting.to_string())
}

/// Validates a proxy setting: empty (direct) or `socks5://host:port` / `http://host:port`.
pub fn valid_proxy(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    let Some(rest) = s
        .strip_prefix("socks5://")
        .or_else(|| s.strip_prefix("http://"))
    else {
        return false;
    };
    match rest.rsplit_once(':') {
        Some((host, port)) => {
            !host.is_empty()
                && !host.contains(['/', ' '])
                && port.parse::<u16>().is_ok_and(|p| p > 0)
        }
        None => false,
    }
}

fn set(mgr: &impl ImplRequestContext, name: &str, v: &mut Value) -> bool {
    let n = CefString::from(name);
    mgr.can_set_preference(Some(&n)) != 0 && mgr.set_preference(Some(&n), Some(v), None) != 0
}

fn set_global(name: &str, v: &mut Value) -> bool {
    let Some(g) = preference_manager_get_global() else {
        return false;
    };
    let n = CefString::from(name);
    g.can_set_preference(Some(&n)) != 0 && g.set_preference(Some(&n), Some(v), None) != 0
}

fn string_value(s: &str) -> Option<Value> {
    let v = value_create()?;
    v.set_string(Some(&CefString::from(s)));
    Some(v)
}

/// DNS-over-HTTPS (global): `secure` mode with the template, or off.
pub fn apply_doh(setting: &str) -> bool {
    let (mode, template) = match doh_template(setting) {
        Some(t) => ("secure", t),
        None => ("off", String::new()),
    };
    let (Some(mut m), Some(mut t)) = (string_value(mode), string_value(&template)) else {
        return false;
    };
    set_global("dns_over_https.templates", &mut t) & set_global("dns_over_https.mode", &mut m)
}

/// HTTPS-Only mode for one context.
pub fn apply_https_only(ctx: &RequestContext, enabled: bool) -> bool {
    let Some(mut v) = value_create() else {
        return false;
    };
    v.set_bool(enabled as i32);
    set(ctx, "https_only_mode_enabled", &mut v)
}

/// Fixed proxy (or direct) for one context.
pub fn apply_proxy(ctx: &RequestContext, proxy: &str) -> bool {
    let Some(mut dict) = dictionary_value_create() else {
        return false;
    };
    if proxy.is_empty() {
        dict.set_string(
            Some(&CefString::from("mode")),
            Some(&CefString::from("direct")),
        );
    } else {
        dict.set_string(
            Some(&CefString::from("mode")),
            Some(&CefString::from("fixed_servers")),
        );
        dict.set_string(
            Some(&CefString::from("server")),
            Some(&CefString::from(proxy)),
        );
    }
    let Some(mut v) = value_create() else {
        return false;
    };
    v.set_dictionary(Some(&mut dict));
    set(ctx, "proxy", &mut v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doh_templates() {
        assert_eq!(doh_template("off"), None);
        assert_eq!(doh_template(""), None);
        assert_eq!(
            doh_template("quad9").as_deref(),
            Some("https://dns.quad9.net/dns-query")
        );
        assert_eq!(
            doh_template("https://my.dns/q").as_deref(),
            Some("https://my.dns/q")
        );
        assert_eq!(doh_template("http://insecure/q"), None);
    }

    #[test]
    fn proxies() {
        assert!(valid_proxy(""));
        assert!(valid_proxy("socks5://127.0.0.1:1080"));
        assert!(valid_proxy("http://proxy.lan:3128"));
        assert!(!valid_proxy("socks5://127.0.0.1"));
        assert!(!valid_proxy("socks5://:1080"));
        assert!(!valid_proxy("ftp://x:1"));
        assert!(!valid_proxy("socks5://a b:1"));
        assert!(!valid_proxy("socks5://x:99999"));
    }
}
