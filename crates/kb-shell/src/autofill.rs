//! Password autofill (task 6.2). Runs only when the user asks (palette action
//! "Fill saved login"): the active tab's origin is matched against vault logins with
//! `kb_vault::origin::may_fill`, and the first match is written into the page's login form.
//! Nothing is filled automatically, in iframes, or on non-matching origins.

use serde_json::json;

/// JavaScript that fills the form (or shows a short in-page notice) for `page_url`.
pub fn script_for(page_url: &str) -> String {
    match crate::vault::logins_for(page_url) {
        Ok(found) if !found.is_empty() => fill_script(&found[0].0, &found[0].1),
        Ok(_) => notice("No saved login for this site."),
        Err(e) => notice(&e),
    }
}

fn notice(text: &str) -> String {
    let t = json!(format!("Kendime-Brow: {text}"));
    format!(
        "(function(){{const d=document.createElement('div');d.textContent={t};\
d.style.cssText='position:fixed;z-index:2147483647;top:12px;right:12px;padding:8px 12px;background:#222;color:#fff;font:14px sans-serif;border-radius:6px;box-shadow:0 2px 8px #0006';\
document.documentElement.appendChild(d);setTimeout(()=>d.remove(),3500)}})();"
    )
}

fn fill_script(username: &str, password: &str) -> String {
    let u = json!(username);
    let p = json!(password);
    format!(
        "(function(){{const U={u},P={p};\
const vis=e=>{{const r=e.getBoundingClientRect();return r.width>0&&r.height>0&&getComputedStyle(e).visibility!=='hidden'}};\
const pw=[...document.querySelectorAll('input[type=password]')].find(vis);\
if(!pw){{const d=document.createElement('div');d.textContent='Kendime-Brow: no password field on this page.';\
d.style.cssText='position:fixed;z-index:2147483647;top:12px;right:12px;padding:8px 12px;background:#222;color:#fff;font:14px sans-serif;border-radius:6px';\
document.documentElement.appendChild(d);setTimeout(()=>d.remove(),3500);return}}\
const set=(el,v)=>{{const s=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set;s.call(el,v);\
el.dispatchEvent(new Event('input',{{bubbles:true}}));el.dispatchEvent(new Event('change',{{bubbles:true}}))}};\
const scope=pw.form||document;\
const fields=[...scope.querySelectorAll('input')].filter(i=>vis(i)&&['text','email','tel',''].includes(i.type));\
const before=fields.filter(i=>pw.compareDocumentPosition(i)&Node.DOCUMENT_POSITION_PRECEDING);\
const user=before[before.length-1];\
if(user&&U)set(user,U);set(pw,P);pw.focus()}})();"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_are_json_escaped_in_the_script() {
        let s = fill_script("me\"; alert(1); //", "p\\w</script>");
        assert!(s.contains("U=\"me\\\"; alert(1); //\""));
        assert!(s.contains("P=\"p\\\\w</script>\""));
    }

    #[test]
    fn notices_escape_text() {
        assert!(notice("a\"b").contains("\\\"b"));
    }
}
