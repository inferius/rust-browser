//! Strukturovany format pro console.log argumenty.
//!
//! Pred A3 reworkem byl console.log capture jen joinly-string (`args.join(" ")`),
//! coz ztracilo typovou informaci - DevTools nemohly rozlisit Object od stringu
//! a vykreslovat je s color-coded preview / inline expand.
//!
//! `ConsoleArg` zachovava (kind, repr, children) - dost pro 1-uroven expand
//! v DevTools console paint. Pro deep nested expand by sla strukturalni
//! rekurze, zatim drzime jen flat first level.

use std::rc::Rc;
use std::cell::RefCell;

use super::{JsValue, JsObject};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsoleArgKind {
    String,
    Number,
    Bool,
    Null,
    Undefined,
    Object,
    Array,
    Function,
    Error,
    BigInt,
    Date,
    RegExp,
    Dom,
    Map,
    Set,
    Promise,
}

#[derive(Debug, Clone)]
pub struct ConsoleArg {
    pub kind: ConsoleArgKind,
    /// Stringified preview / summary - "42", "\"hello\"", "Object { a: 1 }", ...
    pub repr: String,
    /// Pro Object/Array: prvni-uroven children (key, value_repr) pro inline expand.
    /// Bez children = primitive nebo unsupported kind.
    pub children: Vec<(String, String)>,
}

impl ConsoleArg {
    /// Postavi ConsoleArg z JsValue. Volane v console.log/error/warn native
    /// closures. Children jen pro Object/Array do 1 urovne (limit 16 polozek).
    pub fn from_jsvalue(v: &JsValue) -> Self {
        match v {
            JsValue::Str(s) => ConsoleArg {
                kind: ConsoleArgKind::String,
                // Pri direct console.log("...") zobrazujeme bez uvozovek (Chrome chovani);
                // uvozovky pridavame az pri expand objektu nested string.
                repr: s.clone(),
                children: Vec::new(),
            },
            JsValue::Number(n) => ConsoleArg {
                kind: ConsoleArgKind::Number,
                repr: format_number(*n),
                children: Vec::new(),
            },
            JsValue::Bool(b) => ConsoleArg {
                kind: ConsoleArgKind::Bool,
                repr: b.to_string(),
                children: Vec::new(),
            },
            JsValue::Null => ConsoleArg {
                kind: ConsoleArgKind::Null,
                repr: "null".into(),
                children: Vec::new(),
            },
            JsValue::Undefined => ConsoleArg {
                kind: ConsoleArgKind::Undefined,
                repr: "undefined".into(),
                children: Vec::new(),
            },
            JsValue::BigInt(b) => ConsoleArg {
                kind: ConsoleArgKind::BigInt,
                repr: format!("{}n", b),
                children: Vec::new(),
            },
            JsValue::Array(arr) => {
                let v = arr.borrow();
                let len = v.len();
                let children: Vec<(String, String)> = v.iter().take(16).enumerate()
                    .map(|(i, x)| (i.to_string(), nested_preview(x)))
                    .collect();
                let preview = if len == 0 {
                    "Array(0) []".to_string()
                } else {
                    let items: Vec<String> = v.iter().take(4)
                        .map(nested_preview).collect();
                    let suffix = if len > 4 { ", ..." } else { "" };
                    format!("Array({}) [{}{}]", len, items.join(", "), suffix)
                };
                ConsoleArg {
                    kind: ConsoleArgKind::Array,
                    repr: preview,
                    children,
                }
            }
            JsValue::Object(o) => build_object_preview(o),
            JsValue::Function(f) => ConsoleArg {
                kind: ConsoleArgKind::Function,
                repr: function_preview(f),
                children: Vec::new(),
            },
            JsValue::DomNode(n) => ConsoleArg {
                kind: ConsoleArgKind::Dom,
                repr: dom_preview(n),
                children: Vec::new(),
            },
            JsValue::Map(m) => {
                let mb = m.borrow();
                let children: Vec<(String, String)> = mb.entries.iter().take(16)
                    .map(|(k, val)| (nested_preview(k), nested_preview(val)))
                    .collect();
                ConsoleArg {
                    kind: ConsoleArgKind::Map,
                    repr: format!("Map({})", mb.entries.len()),
                    children,
                }
            }
            JsValue::Set(s) => {
                let sb = s.borrow();
                let children: Vec<(String, String)> = sb.values.iter().take(16).enumerate()
                    .map(|(i, x)| (i.to_string(), nested_preview(x)))
                    .collect();
                ConsoleArg {
                    kind: ConsoleArgKind::Set,
                    repr: format!("Set({})", sb.values.len()),
                    children,
                }
            }
            JsValue::BigNumber(b) => ConsoleArg {
                kind: ConsoleArgKind::Number,
                repr: b.to_string(),
                children: Vec::new(),
            },
        }
    }
}

fn format_number(n: f64) -> String {
    if n.is_nan() { return "NaN".into(); }
    if n.is_infinite() { return if n > 0.0 { "Infinity".into() } else { "-Infinity".into() }; }
    if n == n.trunc() && n.abs() < 1e16 {
        return format!("{}", n as i64);
    }
    format!("{}", n)
}

fn build_object_preview(o: &Rc<RefCell<JsObject>>) -> ConsoleArg {
    let ob = o.borrow();
    // Detekce specialnich object kinds
    if ob.props.contains_key("__date__") {
        if let Some(JsValue::Number(ts)) = ob.props.get("__date__") {
            return ConsoleArg {
                kind: ConsoleArgKind::Date,
                repr: format!("Date({}ms)", ts),
                children: Vec::new(),
            };
        }
    }
    if ob.props.contains_key("__regex_pattern__") {
        let pat = ob.props.get("__regex_pattern__").map(|v| v.to_string()).unwrap_or_default();
        let flags = ob.props.get("__regex_flags__").map(|v| v.to_string()).unwrap_or_default();
        return ConsoleArg {
            kind: ConsoleArgKind::RegExp,
            repr: format!("/{}/{}", pat, flags),
            children: Vec::new(),
        };
    }
    if ob.props.contains_key("__error_message__") {
        let msg = ob.props.get("__error_message__").map(|v| v.to_string()).unwrap_or_default();
        let name = ob.props.get("name").map(|v| v.to_string()).unwrap_or_else(|| "Error".into());
        return ConsoleArg {
            kind: ConsoleArgKind::Error,
            repr: format!("{}: {}", name, msg),
            children: Vec::new(),
        };
    }
    if ob.props.contains_key("__promise_state__") {
        let state = ob.props.get("__promise_state__").map(|v| v.to_string()).unwrap_or_default();
        return ConsoleArg {
            kind: ConsoleArgKind::Promise,
            repr: format!("Promise <{}>", state),
            children: Vec::new(),
        };
    }
    // Plain Object - children + summary preview.
    let visible_keys: Vec<&String> = ob.props.keys()
        .filter(|k| !k.starts_with("__"))
        .take(16)
        .collect();
    let children: Vec<(String, String)> = visible_keys.iter()
        .map(|k| (
            (*k).clone(),
            ob.props.get(*k).map(nested_preview).unwrap_or_else(|| "undefined".into()),
        ))
        .collect();
    let preview_items: Vec<String> = visible_keys.iter().take(4)
        .map(|k| format!("{}: {}", k,
            ob.props.get(*k).map(nested_preview).unwrap_or_else(|| "undefined".into())))
        .collect();
    let class_name = ob.props.get("__class_name__")
        .map(|v| v.to_string())
        .unwrap_or_else(|| "Object".into());
    let suffix = if visible_keys.len() > 4 { ", ..." } else { "" };
    let repr = if preview_items.is_empty() {
        format!("{} {{}}", class_name)
    } else {
        format!("{} {{ {}{} }}", class_name, preview_items.join(", "), suffix)
    };
    ConsoleArg {
        kind: ConsoleArgKind::Object,
        repr,
        children,
    }
}

fn nested_preview(v: &JsValue) -> String {
    match v {
        JsValue::Str(s) => format!("\"{}\"", s),
        JsValue::Number(n) => format_number(*n),
        JsValue::Bool(b) => b.to_string(),
        JsValue::Null => "null".into(),
        JsValue::Undefined => "undefined".into(),
        JsValue::BigInt(b) => format!("{}n", b),
        JsValue::BigNumber(b) => b.to_string(),
        JsValue::Array(a) => format!("Array({})", a.borrow().len()),
        JsValue::Object(o) => {
            let ob = o.borrow();
            if ob.props.contains_key("__date__") { return "Date".into(); }
            if ob.props.contains_key("__regex_pattern__") { return "RegExp".into(); }
            if ob.props.contains_key("__promise_state__") { return "Promise".into(); }
            ob.props.get("__class_name__").map(|v| v.to_string()).unwrap_or_else(|| "Object".into())
        }
        JsValue::DomNode(n) => format!("<{}>", n.tag_name_ref().unwrap_or("?")),
        JsValue::Function(_) => "fn".into(),
        JsValue::Map(m) => format!("Map({})", m.borrow().entries.len()),
        JsValue::Set(s) => format!("Set({})", s.borrow().values.len()),
    }
}

fn function_preview(f: &super::JsFunc) -> String {
    match f {
        super::JsFunc::User { name, .. } => format!("function {}", name.as_deref().unwrap_or("")),
        super::JsFunc::Native(name, _) => format!("function {} [native]", name),
        super::JsFunc::Generator { name, .. } => format!("function* {}", name.as_deref().unwrap_or("")),
        super::JsFunc::Async { name, .. } => format!("async function {}", name.as_deref().unwrap_or("")),
        _ => "function".into(),
    }
}

fn dom_preview(n: &Rc<crate::browser::dom::NodeData>) -> String {
    let tag = n.tag_name_ref().unwrap_or("");
    let id = n.attr("id");
    let class = n.attr("class");
    let mut s = format!("<{}", tag);
    if let Some(i) = id { s.push_str(&format!(" id=\"{}\"", i)); }
    if let Some(c) = class { s.push_str(&format!(" class=\"{}\"", c)); }
    s.push('>');
    s
}
