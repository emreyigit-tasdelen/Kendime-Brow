//! Userscripts (task 6.5): scripts live in `<profile>/userscripts/*.user.js` (`.off` suffix =
//! disabled). Matching scripts are injected into the main frame of http(s) pages, at load start
//! for `@run-at document-start` and at load end otherwise. A small `GM_*` shim is provided.
//! Limitation: `GM_getValue`/`GM_setValue` use the page's localStorage, so values are per site.

use kb_userscript::{Meta, parse_meta, should_run};
use serde_json::{Value, json};

fn dir() -> Result<std::path::PathBuf, String> {
    let d = crate::shell::profile_dir()
        .ok_or("no profile directory")?
        .join("userscripts");
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    Ok(d)
}

fn safe_file(name: &str) -> String {
    let base: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(60)
        .collect();
    let base = if base.trim_matches('_').is_empty() {
        "script".to_string()
    } else {
        base
    };
    format!("{base}.user.js")
}

fn valid_file(f: &str) -> bool {
    !f.contains(['/', '\\']) && (f.ends_with(".user.js") || f.ends_with(".user.js.off"))
}

/// Wraps a script with its `GM_*` shim in a function scope.
fn wrap(meta: &Meta, source: &str) -> String {
    let name = serde_json::to_string(&meta.name).unwrap_or_else(|_| "\"\"".into());
    let info = json!({ "script": { "name": meta.name, "version": meta.version, "description": meta.description } });
    format!(
        "(function(){{const NAME={name};const P='kbgm:'+NAME+':';\
const GM_info={info};\
function GM_log(...a){{console.log('[userscript '+NAME+']',...a)}}\
function GM_getValue(k,d){{try{{const v=localStorage.getItem(P+k);return v===null?d:JSON.parse(v)}}catch(e){{return d}}}}\
function GM_setValue(k,v){{try{{localStorage.setItem(P+k,JSON.stringify(v))}}catch(e){{}}}}\
function GM_deleteValue(k){{try{{localStorage.removeItem(P+k)}}catch(e){{}}}}\
function GM_addStyle(css){{const s=document.createElement('style');s.textContent=css;(document.head||document.documentElement).appendChild(s);return s}}\
try{{\n{source}\n}}catch(e){{console.error('[userscript '+NAME+']',e)}}}})();"
    )
}

fn read_all() -> Vec<(String, bool, String)> {
    let Ok(d) = dir() else { return Vec::new() };
    let mut out: Vec<(String, bool, String)> = std::fs::read_dir(d)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let f = e.file_name().to_string_lossy().into_owned();
                    valid_file(&f)
                        .then(|| std::fs::read_to_string(e.path()).ok())
                        .flatten()
                        .map(|src| (f.clone(), !f.ends_with(".off"), src))
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// Script text to run in the main frame at `url`, or an empty string.
pub fn script_for(url: &str, at_start: bool) -> String {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return String::new();
    }
    read_all()
        .into_iter()
        .filter(|(_, enabled, _)| *enabled)
        .filter_map(|(_, _, src)| parse_meta(&src).map(|m| (m, src)))
        .filter(|(m, _)| (m.run_at == "document-start") == at_start && should_run(m, url))
        .map(|(m, src)| wrap(&m, &src))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn call(op: &str, args: &Value) -> Result<String, String> {
    let file = args.get("file").and_then(Value::as_str).unwrap_or_default();
    match op {
        "list" => {
            let items: Vec<Value> = read_all()
                .into_iter()
                .map(|(f, enabled, src)| {
                    let m = parse_meta(&src).unwrap_or_default();
                    let all = [m.matches.clone(), m.includes.clone()].concat();
                    json!({ "file": f, "enabled": enabled, "name": m.name, "version": m.version,
                            "run_at": m.run_at, "matches": all })
                })
                .collect();
            Ok(Value::Array(items).to_string())
        }
        "get" => {
            if !valid_file(file) {
                return Err("invalid script file".into());
            }
            std::fs::read_to_string(dir()?.join(file))
                .map(|s| json!({ "source": s }).to_string())
                .map_err(|e| e.to_string())
        }
        "save" => {
            let source = args
                .get("source")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let meta = parse_meta(source).ok_or("missing // ==UserScript== header")?;
            if meta.name.trim().is_empty() {
                return Err("the header needs an @name".into());
            }
            if meta.matches.is_empty() && meta.includes.is_empty() {
                return Err("the header needs at least one @match or @include".into());
            }
            let target = if valid_file(file) {
                file.to_string()
            } else {
                safe_file(&meta.name)
            };
            std::fs::write(dir()?.join(&target), source).map_err(|e| e.to_string())?;
            Ok(json!({ "file": target }).to_string())
        }
        "remove" => {
            if !valid_file(file) {
                return Err("invalid script file".into());
            }
            std::fs::remove_file(dir()?.join(file)).map_err(|e| e.to_string())?;
            Ok("{}".into())
        }
        "toggle" => {
            if !valid_file(file) {
                return Err("invalid script file".into());
            }
            let d = dir()?;
            let to = match file.strip_suffix(".off") {
                Some(on) => on.to_string(),
                None => format!("{file}.off"),
            };
            std::fs::rename(d.join(file), d.join(&to)).map_err(|e| e.to_string())?;
            Ok(json!({ "file": to }).to_string())
        }
        _ => Err(format!("unknown userscripts op: {op}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_safe() {
        assert_eq!(safe_file("Dark mode!"), "Dark_mode_.user.js");
        assert_eq!(safe_file("../../x"), "______x.user.js");
        assert_eq!(safe_file("???"), "script.user.js");
        assert!(valid_file("a.user.js") && valid_file("a.user.js.off"));
        assert!(!valid_file("../a.user.js") && !valid_file("a.txt"));
    }

    #[test]
    fn wrapper_scopes_the_script_and_escapes_the_name() {
        let m = Meta {
            name: "He said \"hi\"".into(),
            ..Meta::default()
        };
        let w = wrap(&m, "GM_log(1);");
        assert!(w.starts_with("(function(){"));
        assert!(w.contains("const NAME=\"He said \\\"hi\\\"\""));
        assert!(w.contains("GM_log(1);"));
        assert!(w.contains("function GM_getValue"));
    }

    #[test]
    fn non_web_pages_get_nothing() {
        assert_eq!(script_for("kb://newtab/", true), "");
        assert_eq!(script_for("file:///etc/hosts", false), "");
    }
}
