//! DOM element property objects: style, classList, dataset.
//! Extrahovano z mod.rs (Iter 267 refactor).

use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::collections::HashMap;
use super::{JsValue, JsObject};
use super::helpers::native;

/// Vrati style objekt s persistentnim ulozenim - cache pres Weak v interp.
/// Pri opakovanem volani vrati stejnou Rc instanci, takze setter v JS:
///   el.style.display = 'none'
/// updatne primo props v cached objektu. Pri setteru (eval_expr.rs assign_to)
/// se navic syncuje zpet do n.set_attr("style", ...) protoze objekt ma
/// internal prop "__style_node__" = JsValue::DomNode.
pub(crate) fn get_or_create_style_object(
    cache: &Rc<RefCell<HashMap<usize, Weak<RefCell<JsObject>>>>>,
    node: Rc<crate::browser::dom::NodeData>,
) -> JsValue {
    let key = Rc::as_ptr(&node) as usize;
    // Try lookup
    if let Some(weak) = cache.borrow().get(&key) {
        if let Some(strong) = weak.upgrade() {
            // Refresh from attribute (mohlo byt zmeneno z DOM strany - setAttribute)
            refresh_style_from_attr(&strong, &node);
            return JsValue::Object(strong);
        }
    }
    // Create new + insert
    let obj_rc = build_style_object(Rc::clone(&node));
    cache.borrow_mut().insert(key, Rc::downgrade(&obj_rc));
    // Cleanup stale entries (Weak::strong_count==0) - lazy GC
    cache.borrow_mut().retain(|_, w| w.strong_count() > 0);
    JsValue::Object(obj_rc)
}

/// Pomocna - refresh CSS props z atributu (kdyz dilo DOM strany).
fn refresh_style_from_attr(obj: &Rc<RefCell<JsObject>>, node: &Rc<crate::browser::dom::NodeData>) {
    let style_str = node.attr("style").unwrap_or_default();
    // Smaz vsechny CSS props (krome internich __key__ a metod)
    let mut o = obj.borrow_mut();
    let to_remove: Vec<String> = o.props.keys()
        .filter(|k| !k.starts_with("__") && !matches!(k.as_str(),
            "setProperty" | "getPropertyValue" | "removeProperty" | "cssText"))
        .cloned().collect();
    for k in to_remove { o.props.remove(&k); }
    drop(o);
    // Re-naplnit
    for pair in style_str.split(';') {
        if let Some(idx) = pair.find(':') {
            let prop = pair[..idx].trim().to_string();
            let val = pair[idx+1..].trim().to_string();
            if !prop.is_empty() {
                let camel = kebab_to_camel(&prop);
                obj.borrow_mut().set(camel, JsValue::Str(val.clone()));
                obj.borrow_mut().set(prop, JsValue::Str(val));
            }
        }
    }
    // Update cssText
    obj.borrow_mut().set("cssText".into(), JsValue::Str(style_str));
}

fn build_style_object(node: Rc<crate::browser::dom::NodeData>) -> Rc<RefCell<JsObject>> {
    let obj_rc = Rc::new(RefCell::new(JsObject::new()));
    // Internal: drzime Rc na node pro setter sync (eval_expr.rs)
    obj_rc.borrow_mut().set("__style_node__".into(), JsValue::DomNode(Rc::clone(&node)));
    // Pre-naplnit z atributu
    refresh_style_from_attr(&obj_rc, &node);
    // setProperty(name, value)
    {
        let n = Rc::clone(&node);
        let o_weak = Rc::downgrade(&obj_rc);
        obj_rc.borrow_mut().set("setProperty".into(), native("style.setProperty", move |args| {
            let mut it = args.into_iter();
            let prop = it.next().map(|v| v.to_string()).unwrap_or_default();
            let val  = it.next().map(|v| v.to_string()).unwrap_or_default();
            update_style_attr(&n, &prop, &val);
            if let Some(o) = o_weak.upgrade() { refresh_style_from_attr(&o, &n); }
            Ok(JsValue::Undefined)
        }));
    }
    // getPropertyValue(name)
    {
        let n = Rc::clone(&node);
        obj_rc.borrow_mut().set("getPropertyValue".into(), native("style.getPropertyValue", move |args| {
            let prop = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let style = n.attr("style").unwrap_or_default();
            for pair in style.split(';') {
                if let Some(idx) = pair.find(':') {
                    let p = pair[..idx].trim();
                    if p == prop {
                        return Ok(JsValue::Str(pair[idx+1..].trim().to_string()));
                    }
                }
            }
            Ok(JsValue::Str(String::new()))
        }));
    }
    // removeProperty(name)
    {
        let n = Rc::clone(&node);
        let o_weak = Rc::downgrade(&obj_rc);
        obj_rc.borrow_mut().set("removeProperty".into(), native("style.removeProperty", move |args| {
            let prop = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let style = n.attr("style").unwrap_or_default();
            let mut removed = String::new();
            let new_pairs: Vec<String> = style.split(';').filter_map(|pair| {
                if let Some(idx) = pair.find(':') {
                    let p = pair[..idx].trim();
                    if p == prop {
                        removed = pair[idx+1..].trim().to_string();
                        return None;
                    }
                    if !p.is_empty() { return Some(pair.trim().to_string()); }
                }
                None
            }).collect();
            n.set_attr("style", &new_pairs.join("; "));
            if let Some(o) = o_weak.upgrade() { refresh_style_from_attr(&o, &n); }
            Ok(JsValue::Str(removed))
        }));
    }
    // item(i) - vraci nazev i-te property (CSS L2 spec).
    {
        let n = Rc::clone(&node);
        obj_rc.borrow_mut().set("item".into(),
            native("style.item", move |args| {
                let idx = args.into_iter().next()
                    .map(|v| v.to_number() as usize).unwrap_or(0);
                let style = n.attr("style").unwrap_or_default();
                let props: Vec<String> = style.split(';')
                    .filter_map(|p| {
                        if let Some(c) = p.find(':') {
                            let name = p[..c].trim();
                            if !name.is_empty() { Some(name.to_string()) } else { None }
                        } else { None }
                    }).collect();
                Ok(JsValue::Str(props.get(idx).cloned().unwrap_or_default()))
            }));
    }
    // length getter - pres __get_length__.
    {
        let n = Rc::clone(&node);
        obj_rc.borrow_mut().set("__get_length__".into(),
            native("style.length getter", move |_| {
                let style = n.attr("style").unwrap_or_default();
                let cnt = style.split(';').filter(|p| p.contains(':')
                    && !p.split(':').next().unwrap_or("").trim().is_empty()).count();
                Ok(JsValue::Number(cnt as f64))
            }));
    }
    obj_rc
}

/// Update nebo pridat prop do node "style" atributu (kebab nebo camel - oba
/// se pak smapuji pri reparse). Volame z setteru `el.style.display = ...`.
pub(crate) fn update_style_attr(node: &Rc<crate::browser::dom::NodeData>, prop: &str, val: &str) {
    // Convert camelCase -> kebab-case pri pridani do attr
    let kebab = camel_to_kebab(prop);
    let style = node.attr("style").unwrap_or_default();
    let mut found = false;
    let mut new_pairs: Vec<String> = Vec::new();
    for pair in style.split(';') {
        if let Some(idx) = pair.find(':') {
            let p = pair[..idx].trim();
            if p == kebab {
                if !val.is_empty() {
                    new_pairs.push(format!("{kebab}: {val}"));
                }
                found = true;
            } else if !p.is_empty() {
                new_pairs.push(pair.trim().to_string());
            }
        }
    }
    if !found && !val.is_empty() {
        new_pairs.push(format!("{kebab}: {val}"));
    }
    node.set_attr("style", &new_pairs.join("; "));
}

pub(crate) fn camel_to_kebab(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}


/// classList JS object pro Element - DOMTokenList (DOM spec).
/// Methods: add/remove/toggle/contains/replace/item, forEach, value, length,
/// indexed access ([0], [1], ...), Symbol.iterator.
///
/// dom_version Rc shared with Interpreter - mutations bump counter aby cascade
/// cache invalidate. Bez tohoto classList.add nezpusobil re-render (style_map
/// caché vrátí stale entry pro .selected/.active class swaps).
pub(crate) fn create_class_list(
    node: Rc<crate::browser::dom::NodeData>,
    dom_version: Rc<std::cell::Cell<u64>>,
) -> JsValue {
    let obj_rc = Rc::new(RefCell::new(JsObject::new()));
    // Marker pro identifikaci tokenu listu
    obj_rc.borrow_mut().set("__dom_token_list__".into(), JsValue::Bool(true));
    obj_rc.borrow_mut().set("__token_list_node__".into(),
        JsValue::DomNode(Rc::clone(&node)));
    let bump = move |dv: &Rc<std::cell::Cell<u64>>| dv.set(dv.get().wrapping_add(1));
    {
        let n = Rc::clone(&node);
        let dv = Rc::clone(&dom_version);
        let bump = bump.clone();
        obj_rc.borrow_mut().set("add".into(), native("classList.add", move |args| {
            let class = n.attr("class").unwrap_or_default();
            let mut classes: Vec<String> = class.split_whitespace().map(String::from).collect();
            let mut changed = false;
            for arg in args {
                let name = arg.to_string();
                if !classes.contains(&name) { classes.push(name); changed = true; }
            }
            if changed {
                n.set_attr("class", &classes.join(" "));
                bump(&dv);
            }
            Ok(JsValue::Undefined)
        }));
    }
    {
        let n = Rc::clone(&node);
        let dv = Rc::clone(&dom_version);
        let bump = bump.clone();
        obj_rc.borrow_mut().set("remove".into(), native("classList.remove", move |args| {
            let class = n.attr("class").unwrap_or_default();
            let mut classes: Vec<String> = class.split_whitespace().map(String::from).collect();
            let pre_len = classes.len();
            for arg in args {
                let name = arg.to_string();
                classes.retain(|c| c != &name);
            }
            if classes.len() != pre_len {
                n.set_attr("class", &classes.join(" "));
                bump(&dv);
            }
            Ok(JsValue::Undefined)
        }));
    }
    {
        let n = Rc::clone(&node);
        let dv = Rc::clone(&dom_version);
        let bump = bump.clone();
        obj_rc.borrow_mut().set("toggle".into(), native("classList.toggle", move |args| {
            let mut it = args.into_iter();
            let name = it.next().map(|v| v.to_string()).unwrap_or_default();
            // DOMTokenList.toggle(token, force) - force argument NASTAVI stav
            // misto prepnuti. Drive ignorovan -> `toggle('visible',
            // e.isIntersecting)` v IO callbacku PREPINAL pri kazdem fire =
            // highlight rozsynchronizovany ("IO funguje obracene").
            let force = it.next().filter(|v| !matches!(v, JsValue::Undefined));
            let class = n.attr("class").unwrap_or_default();
            let mut classes: Vec<String> = class.split_whitespace().map(String::from).collect();
            let has = classes.contains(&name);
            let want = match &force {
                Some(v) => v.is_truthy(),
                None => !has,
            };
            if want && !has {
                classes.push(name);
            } else if !want && has {
                classes.retain(|c| c != &name);
            }
            if want != has {
                n.set_attr("class", &classes.join(" "));
                bump(&dv);
            }
            Ok(JsValue::Bool(want))
        }));
    }
    {
        let n = Rc::clone(&node);
        obj_rc.borrow_mut().set("contains".into(), native("classList.contains", move |args| {
            let name = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let class = n.attr("class").unwrap_or_default();
            let has = class.split_whitespace().any(|c| c == name);
            Ok(JsValue::Bool(has))
        }));
    }
    // DOMTokenList.replace(oldToken, newToken) - per spec: pokud oldToken
    // existuje, nahradi ho newToken (preserve poradi); vrati Bool zmena.
    {
        let n = Rc::clone(&node);
        obj_rc.borrow_mut().set("replace".into(), native("classList.replace", move |args| {
            let mut it = args.into_iter();
            let old = it.next().map(|v| v.to_string()).unwrap_or_default();
            let new = it.next().map(|v| v.to_string()).unwrap_or_default();
            let class = n.attr("class").unwrap_or_default();
            let mut classes: Vec<String> = class.split_whitespace().map(String::from).collect();
            let mut replaced = false;
            for c in &mut classes {
                if *c == old {
                    *c = new.clone();
                    replaced = true;
                    break;
                }
            }
            if replaced {
                n.set_attr("class", &classes.join(" "));
            }
            Ok(JsValue::Bool(replaced))
        }));
    }
    // DOMTokenList.item(index) - vraci token na danem indexu nebo null.
    {
        let n = Rc::clone(&node);
        obj_rc.borrow_mut().set("item".into(), native("classList.item", move |args| {
            let idx = args.into_iter().next().map(|v| v.to_number() as i64).unwrap_or(-1);
            if idx < 0 { return Ok(JsValue::Null); }
            let class = n.attr("class").unwrap_or_default();
            let classes: Vec<&str> = class.split_whitespace().collect();
            Ok(classes.get(idx as usize)
                .map(|s| JsValue::Str(s.to_string()))
                .unwrap_or(JsValue::Null))
        }));
    }
    // POZNAMKA: DOMTokenList.forEach() neni implementovany pres native fn
    // (native callback dispatch je out-of-scope pro native helpers - nema
    // pristup k interpreteru). Bezne JS pouziva Array.from(classList).forEach()
    // ktere funguje pres Symbol.iterator nize.
    // Symbol.iterator - iterate tokenu pro for-of a Array.from.
    {
        let n = Rc::clone(&node);
        obj_rc.borrow_mut().set("Symbol.iterator".into(),
            native("classList[Symbol.iterator]", move |_| {
                let class = n.attr("class").unwrap_or_default();
                let tokens: Vec<JsValue> = class.split_whitespace()
                    .map(|t| JsValue::Str(t.to_string())).collect();
                Ok(super::helpers::make_array_iterator(tokens))
            }));
    }
    // value getter/setter pres __dom_token_list__ marker resi eval_member.rs +
    // eval_expr.rs assign_to. Initial setup: prazdny string (rebuild dynamic).
    obj_rc.borrow_mut().set("value".into(),
        JsValue::Str(node.attr("class").unwrap_or_default()));
    JsValue::Object(obj_rc)
}

/// dataset JS object - vsechny data-* atributy node-u (kebab -> camel keys).
pub(crate) fn create_dataset(node: &Rc<crate::browser::dom::NodeData>) -> JsValue {
    let obj_rc = Rc::new(RefCell::new(JsObject::new()));
    let attrs = node.attributes.borrow();
    for (k, v) in attrs.iter() {
        if let Some(rest) = k.strip_prefix("data-") {
            // kebab-case -> camelCase: data-foo-bar -> fooBar
            let camel = kebab_to_camel(rest);
            obj_rc.borrow_mut().set(camel, JsValue::Str(v.clone()));
        }
    }
    JsValue::Object(obj_rc)
}

pub(crate) fn kebab_to_camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper = false;
    for c in s.chars() {
        if c == '-' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// URL parts (pro HTMLAnchorElement.protocol/host/...).
pub(crate) struct UrlParts {
    pub protocol: String, pub host: String, pub hostname: String, pub port: String,
    pub pathname: String, pub search: String, pub hash: String, pub origin: String,
}

pub(crate) fn parse_url_parts(url: &str) -> UrlParts {
    let (proto, rest) = if let Some(idx) = url.find("://") {
        (format!("{}:", &url[..idx]), url[idx + 3..].to_string())
    } else {
        ("https:".to_string(), url.to_string())
    };
    let (host_path, hash) = match rest.split_once('#') {
        Some((a, b)) => (a.to_string(), format!("#{b}")),
        None => (rest, String::new()),
    };
    let (host_path, search) = match host_path.split_once('?') {
        Some((a, b)) => (a.to_string(), format!("?{b}")),
        None => (host_path, String::new()),
    };
    let (host, pathname) = match host_path.find('/') {
        Some(i) => (host_path[..i].to_string(), host_path[i..].to_string()),
        None => (host_path, "/".to_string()),
    };
    let (hostname, port) = match host.split_once(':') {
        Some((h, p)) => (h.to_string(), p.to_string()),
        None => (host.clone(), String::new()),
    };
    let origin = format!("{proto}//{host}");
    UrlParts { protocol: proto, host, hostname, port, pathname, search, hash, origin }
}

/// application/x-www-form-urlencoded encoder (RFC 3986 unreserved chars).
pub(crate) fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
