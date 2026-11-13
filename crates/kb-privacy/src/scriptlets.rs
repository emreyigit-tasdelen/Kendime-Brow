//! Built-in scriptlets (task 2.3), written for this project: the uBlock Origin scriptlets most
//! filter lists use (`aopr`, `aopw`, `set`, `nostif`, `nosiif`). The full uBO resource set is not
//! bundled; rules naming other scriptlets are ignored.

use adblock::resources::{Resource, ResourceType};
use base64::Engine as _;

/// (name, alias, JavaScript template with `{{1}}`/`{{2}}` placeholders)
const SCRIPTLETS: &[(&str, &str, &str)] = &[
    (
        "abort-on-property-read",
        "aopr",
        r####"(function(){function kbDefine(chain, mk){const parts=chain.split('.');let o=window;for(let i=0;i<parts.length-1;i++){if(!(parts[i] in o))return;o=o[parts[i]];if(o===null||(typeof o!=='object'&&typeof o!=='function'))return;}const k=parts[parts.length-1];mk(o,k);}const m='kb-'+Math.random().toString(36).slice(2);kbDefine('{{1}}',function(o,k){try{Object.defineProperty(o,k,{get:function(){throw new ReferenceError(m);},set:function(){},configurable:true});}catch(e){}});})();"####,
    ),
    (
        "abort-on-property-write",
        "aopw",
        r####"(function(){function kbDefine(chain, mk){const parts=chain.split('.');let o=window;for(let i=0;i<parts.length-1;i++){if(!(parts[i] in o))return;o=o[parts[i]];if(o===null||(typeof o!=='object'&&typeof o!=='function'))return;}const k=parts[parts.length-1];mk(o,k);}const m='kb-'+Math.random().toString(36).slice(2);kbDefine('{{1}}',function(o,k){try{Object.defineProperty(o,k,{get:function(){},set:function(){throw new ReferenceError(m);},configurable:true});}catch(e){}});})();"####,
    ),
    (
        "set-constant",
        "set",
        r####"(function(){function kbDefine(chain, mk){const parts=chain.split('.');let o=window;for(let i=0;i<parts.length-1;i++){if(!(parts[i] in o))return;o=o[parts[i]];if(o===null||(typeof o!=='object'&&typeof o!=='function'))return;}const k=parts[parts.length-1];mk(o,k);}const raw='{{2}}';const map={'true':true,'false':false,'null':null,'undefined':undefined,'noopFunc':function(){},'trueFunc':function(){return true},'falseFunc':function(){return false},'':undefined,'0':0,'1':1,'emptyStr':'','emptyArr':[],'emptyObj':{}};if(!(raw in map))return;const v=map[raw];kbDefine('{{1}}',function(o,k){try{Object.defineProperty(o,k,{get:function(){return v},set:function(){},configurable:true});}catch(e){}});})();"####,
    ),
    (
        "no-setTimeout-if",
        "nostif",
        r####"(function(){const pat='{{1}}';const re=pat?new RegExp(pat):null;const o=window.setTimeout;window.setTimeout=function(f,d){try{if(!re||re.test(String(f)))return 0;}catch(e){}return o.apply(this,arguments);};})();"####,
    ),
    (
        "no-setInterval-if",
        "nosiif",
        r####"(function(){const pat='{{1}}';const re=pat?new RegExp(pat):null;const o=window.setInterval;window.setInterval=function(f,d){try{if(!re||re.test(String(f)))return 0;}catch(e){}return o.apply(this,arguments);};})();"####,
    ),
];

pub fn builtin() -> Vec<Resource> {
    SCRIPTLETS
        .iter()
        .map(|(name, alias, body)| Resource {
            name: format!("{name}.js"),
            aliases: vec![
                alias.to_string(),
                format!("{alias}.js"),
                (*name).to_string(),
            ],
            kind: ResourceType::Template,
            content: base64::engine::general_purpose::STANDARD.encode(body),
            dependencies: vec![],
            permission: Default::default(),
        })
        .collect()
}
