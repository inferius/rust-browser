//! Implementace metod na JsValue - extracted z mod.rs pro velikost.
//!
//! Obsahuje:
//! - Display pro JsValue (toString-like serialize)
//! - Debug pro JsFunc
//! - Methods: is_truthy, to_number, type_of, loose_eq, strict_eq, to_bigdecimal, to_bigint

use std::rc::Rc;
use std::str::FromStr;
use bigdecimal::BigDecimal;
use num_bigint::BigInt;
use num_traits::{ToPrimitive, Zero as NumZero};
use super::{JsValue, JsFunc};

impl std::fmt::Debug for JsFunc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsFunc::User { name, .. }      => write!(f, "[Function: {}]", name.as_deref().unwrap_or("anonymous")),
            JsFunc::Native(name, _)        => write!(f, "[NativeFunction: {name}]"),
            JsFunc::Class { name, .. }     => write!(f, "[class {}]", name.as_deref().unwrap_or("(anonymous)")),
            JsFunc::Generator { name, .. } => write!(f, "[GeneratorFunction: {}]", name.as_deref().unwrap_or("anonymous")),
            JsFunc::Async { name, .. }     => write!(f, "[AsyncFunction: {}]", name.as_deref().unwrap_or("anonymous")),
            JsFunc::Bound { .. }           => write!(f, "[BoundFunction]"),
        }
    }
}

impl std::fmt::Display for JsValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsValue::Undefined    => write!(f, "undefined"),
            JsValue::Null         => write!(f, "null"),
            JsValue::Bool(b)      => write!(f, "{b}"),
            JsValue::Number(n)    => {
                if n.is_nan()           { write!(f, "NaN") }
                else if n.is_infinite() { write!(f, "{}Infinity", if *n > 0.0 { "" } else { "-" }) }
                else if *n == n.trunc() && n.abs() < 1e15 { write!(f, "{}", *n as i64) }
                else                    { write!(f, "{n}") }
            }
            JsValue::Str(s)       => write!(f, "{s}"),
            JsValue::Object(o)    => {
                let pairs: Vec<String> = o.borrow().props.iter().map(|(k,v)| format!("{k}: {v}")).collect();
                write!(f, "{{ {} }}", pairs.join(", "))
            }
            JsValue::Array(a)     => {
                let items: Vec<String> = a.borrow().iter().map(|v| v.to_string()).collect();
                write!(f, "[{}]", items.join(", "))
            }
            JsValue::Function(fn_) => write!(f, "{fn_:?}"),
            JsValue::Map(m) => {
                let pairs: Vec<String> = m.borrow().entries.iter()
                    .map(|(k, v)| format!("{k} => {v}")).collect();
                write!(f, "Map {{ {} }}", pairs.join(", "))
            }
            JsValue::Set(s) => {
                let items: Vec<String> = s.borrow().values.iter().map(|v| v.to_string()).collect();
                write!(f, "Set {{ {} }}", items.join(", "))
            }
            JsValue::BigNumber(n) => write!(f, "{n}"),
            JsValue::BigInt(n) => write!(f, "{n}"),
            JsValue::DomNode(n) => {
                if let Some(tag) = n.tag_name() {
                    write!(f, "[DOM <{tag}>]")
                } else {
                    write!(f, "[DOM Node]")
                }
            }
        }
    }
}

impl JsValue {
    pub fn is_truthy(&self) -> bool {
        match self {
            JsValue::Undefined | JsValue::Null => false,
            JsValue::Bool(b)   => *b,
            JsValue::Number(n) => *n != 0.0 && !n.is_nan(),
            JsValue::Str(s)    => !s.is_empty(),
            JsValue::BigInt(n) => !NumZero::is_zero(n.as_ref()),
            JsValue::BigNumber(n) => !n.is_zero(),
            JsValue::DomNode(_)   => true,
            _                  => true,
        }
    }

    pub fn to_number(&self) -> f64 {
        match self {
            JsValue::Number(n)    => *n,
            JsValue::Bool(true)   => 1.0,
            JsValue::Bool(false)  => 0.0,
            JsValue::Null         => 0.0,
            JsValue::Undefined    => f64::NAN,
            JsValue::Str(s)       => {
                let t = s.trim();
                if t.is_empty() { return 0.0; }
                t.parse().unwrap_or(f64::NAN)
            }
            JsValue::BigNumber(n) => n.to_f64().unwrap_or(f64::NAN),
            JsValue::BigInt(n)    => n.to_f64().unwrap_or(f64::NAN),
            JsValue::Object(o) => {
                let b = o.borrow();
                if let Some(JsValue::Number(ms)) = b.props.get("__date_ms__") {
                    return *ms;
                }
                f64::NAN
            }
            JsValue::Array(a) => {
                let arr = a.borrow();
                if arr.is_empty() { return 0.0; }
                if arr.len() == 1 { return arr[0].to_number(); }
                f64::NAN
            }
            _                     => f64::NAN,
        }
    }

    pub fn type_of(&self) -> &'static str {
        match self {
            JsValue::Undefined    => "undefined",
            JsValue::Null         => "object",
            JsValue::Bool(_)      => "boolean",
            JsValue::Number(_)    => "number",
            JsValue::Str(_)       => "string",
            JsValue::Object(_)    => "object",
            JsValue::Array(_)     => "object",
            JsValue::Function(_)  => "function",
            JsValue::Map(_)       => "object",
            JsValue::Set(_)       => "object",
            JsValue::BigNumber(_) => "bignumber",
            JsValue::BigInt(_)    => "bigint",
            JsValue::DomNode(_)   => "object",
        }
    }

    pub(crate) fn loose_eq(&self, other: &JsValue) -> bool {
        match (self, other) {
            (JsValue::Null | JsValue::Undefined, JsValue::Null | JsValue::Undefined) => true,
            (JsValue::Number(a), JsValue::Number(b)) => a == b,
            (JsValue::Str(a), JsValue::Str(b))       => a == b,
            (JsValue::Bool(a), JsValue::Bool(b))     => a == b,
            (JsValue::Number(n), JsValue::Str(s)) | (JsValue::Str(s), JsValue::Number(n)) =>
                s.trim().parse::<f64>().ok().as_ref() == Some(n),
            _ => false,
        }
    }

    pub(crate) fn strict_eq(&self, other: &JsValue) -> bool {
        match (self, other) {
            (JsValue::Undefined, JsValue::Undefined) => true,
            (JsValue::Null, JsValue::Null)           => true,
            (JsValue::Bool(a), JsValue::Bool(b))     => a == b,
            (JsValue::Number(a), JsValue::Number(b)) => a == b,
            (JsValue::Str(a), JsValue::Str(b))       => a == b,
            (JsValue::Object(a), JsValue::Object(b)) => Rc::ptr_eq(a, b),
            (JsValue::Array(a),  JsValue::Array(b))  => Rc::ptr_eq(a, b),
            (JsValue::Map(a),    JsValue::Map(b))    => Rc::ptr_eq(a, b),
            (JsValue::Set(a),    JsValue::Set(b))    => Rc::ptr_eq(a, b),
            (JsValue::BigNumber(a), JsValue::BigNumber(b)) => *a == *b,
            (JsValue::BigInt(a),    JsValue::BigInt(b))    => *a == *b,
            (JsValue::DomNode(a),   JsValue::DomNode(b))   => Rc::ptr_eq(a, b),
            _ => false,
        }
    }

    /// Vrati JsValue jako BigDecimal (pro BigNumber operace).
    pub fn to_bigdecimal(&self) -> Option<BigDecimal> {
        match self {
            JsValue::BigNumber(n) => Some((**n).clone()),
            JsValue::BigInt(n)    => Some(BigDecimal::from(n.as_ref().clone())),
            JsValue::Number(n) if n.is_finite() => {
                BigDecimal::from_str(&n.to_string()).ok()
            }
            JsValue::Str(s) => BigDecimal::from_str(s.trim()).ok(),
            JsValue::Bool(true)  => Some(BigDecimal::from(1)),
            JsValue::Bool(false) => Some(BigDecimal::from(0)),
            _ => None,
        }
    }

    /// Vrati JsValue jako BigInt (truncate pro Number, parse pro Str).
    pub fn to_bigint(&self) -> Option<BigInt> {
        match self {
            JsValue::BigInt(n)    => Some((**n).clone()),
            JsValue::BigNumber(n) => {
                let s = n.with_scale(0).to_string();
                let int_str = s.split('.').next().unwrap_or("0");
                BigInt::from_str(int_str).ok()
            }
            JsValue::Number(n) if n.is_finite() => {
                BigInt::from_str(&format!("{}", *n as i128)).ok()
            }
            JsValue::Str(s) => BigInt::from_str(s.trim()).ok(),
            JsValue::Bool(true)  => Some(BigInt::from(1)),
            JsValue::Bool(false) => Some(BigInt::from(0)),
            _ => None,
        }
    }
}
