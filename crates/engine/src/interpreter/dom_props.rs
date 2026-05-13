//! DOM element property objects: style, classList, dataset.
//! Extrahovano z mod.rs (Iter 267 refactor).

use std::rc::Rc;
use std::cell::RefCell;
use super::{JsValue, JsObject};
use super::helpers::native;


/// CSSStyleDeclaration object pro element.style.
/// Nese referenci na node + parsuje "style" attribute pro getter / setter.
pub(crate) fn create_style_object(node: Rc<crate::browser::dom::NodeData>) -> JsValue {
    let obj_rc = Rc::new(RefCell::new(JsObject::new()));
    // Pre-naplnim z aktualniho style attributu - kebab-case keys + camelCase
    if let Some(style_str) = node.attr("style") {
        for pair in style_str.split(';') {
            if let Some(idx) = pair.find(':') {
                let prop = pair[..idx].trim().to_string();
                let val = pair[idx+1..].trim().to_string();
                if !prop.is_empty() {
                    let camel = kebab_to_camel(&prop);
                    obj_rc.borrow_mut().set(camel, JsValue::Str(val.clone()));
                    obj_rc.borrow_mut().set(prop, JsValue::Str(val));
                }
            }
        }
    }
    // setProperty(name, value)
    {
        let n = Rc::clone(&node);
        obj_rc.borrow_mut().set("setProperty".into(), native("style.setProperty", move |args| {
            let mut it = args.into_iter();
            let prop = it.next().map(|v| v.to_string()).unwrap_or_default();
            let val  = it.next().map(|v| v.to_string()).unwrap_or_default();
            let mut style = n.attr("style").unwrap_or_default();
            // Replace nebo pridat
            let mut found = false;
            let mut new_pairs: Vec<String> = Vec::new();
            for pair in style.split(';') {
                if let Some(idx) = pair.find(':') {
                    let p = pair[..idx].trim();
                    if p == prop {
                        new_pairs.push(format!("{prop}: {val}"));
                        found = true;
                    } else if !p.is_empty() {
                        new_pairs.push(pair.trim().to_string());
                    }
                }
            }
            if !found {
                new_pairs.push(format!("{prop}: {val}"));
            }
            style = new_pairs.join("; ");
            n.set_attr("style", &style);
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
            Ok(JsValue::Str(removed))
        }));
    }
    JsValue::Object(obj_rc)
}

/// classList JS object pro Element - methods add/remove/toggle/contains.
pub(crate) fn create_class_list(node: Rc<crate::browser::dom::NodeData>) -> JsValue {
    let obj_rc = Rc::new(RefCell::new(JsObject::new()));
    {
        let n = Rc::clone(&node);
        obj_rc.borrow_mut().set("add".into(), native("classList.add", move |args| {
            let class = n.attr("class").unwrap_or_default();
            let mut classes: Vec<String> = class.split_whitespace().map(String::from).collect();
            for arg in args {
                let name = arg.to_string();
                if !classes.contains(&name) { classes.push(name); }
            }
            n.set_attr("class", &classes.join(" "));
            Ok(JsValue::Undefined)
        }));
    }
    {
        let n = Rc::clone(&node);
        obj_rc.borrow_mut().set("remove".into(), native("classList.remove", move |args| {
            let class = n.attr("class").unwrap_or_default();
            let mut classes: Vec<String> = class.split_whitespace().map(String::from).collect();
            for arg in args {
                let name = arg.to_string();
                classes.retain(|c| c != &name);
            }
            n.set_attr("class", &classes.join(" "));
            Ok(JsValue::Undefined)
        }));
    }
    {
        let n = Rc::clone(&node);
        obj_rc.borrow_mut().set("toggle".into(), native("classList.toggle", move |args| {
            let name = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let class = n.attr("class").unwrap_or_default();
            let mut classes: Vec<String> = class.split_whitespace().map(String::from).collect();
            let has = classes.contains(&name);
            if has {
                classes.retain(|c| c != &name);
            } else {
                classes.push(name);
            }
            n.set_attr("class", &classes.join(" "));
            Ok(JsValue::Bool(!has))
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
