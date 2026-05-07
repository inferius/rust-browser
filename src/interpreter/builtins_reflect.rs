//! Reflect (ES2015) - mirror object API.
//!
//! Extracted z builtins.rs pro velikost. Self-contained (zadne captured params).

use std::rc::Rc;
use std::cell::RefCell;
use super::{JsValue, JsObject, Environment};
use super::helpers::native;

pub fn setup_reflect(e: &mut Environment) {
    let mut refl = JsObject::new();
    refl.set("get".into(), native("Reflect.get", |a| {
        let mut iter = a.into_iter();
        let target = iter.next().unwrap_or(JsValue::Undefined);
        let key = iter.next().map(|v| v.to_string()).unwrap_or_default();
        match target {
            JsValue::Object(o) => Ok(o.borrow().get(&key)),
            JsValue::Array(arr) => {
                if let Ok(i) = key.parse::<usize>() {
                    Ok(arr.borrow().get(i).cloned().unwrap_or(JsValue::Undefined))
                } else if key == "length" {
                    Ok(JsValue::Number(arr.borrow().len() as f64))
                } else {
                    Ok(JsValue::Undefined)
                }
            }
            _ => Ok(JsValue::Undefined),
        }
    }));
    refl.set("set".into(), native("Reflect.set", |a| {
        let mut iter = a.into_iter();
        let target = iter.next().unwrap_or(JsValue::Undefined);
        let key = iter.next().map(|v| v.to_string()).unwrap_or_default();
        let val = iter.next().unwrap_or(JsValue::Undefined);
        match target {
            JsValue::Object(o) => {
                if !o.borrow().frozen {
                    o.borrow_mut().set(key, val);
                    return Ok(JsValue::Bool(true));
                }
                Ok(JsValue::Bool(false))
            }
            JsValue::Array(arr) => {
                if let Ok(i) = key.parse::<usize>() {
                    let mut a = arr.borrow_mut();
                    while a.len() <= i { a.push(JsValue::Undefined); }
                    a[i] = val;
                    return Ok(JsValue::Bool(true));
                }
                Ok(JsValue::Bool(false))
            }
            _ => Ok(JsValue::Bool(false)),
        }
    }));
    refl.set("has".into(), native("Reflect.has", |a| {
        let mut iter = a.into_iter();
        let target = iter.next().unwrap_or(JsValue::Undefined);
        let key = iter.next().map(|v| v.to_string()).unwrap_or_default();
        match target {
            JsValue::Object(o) => Ok(JsValue::Bool(o.borrow().has_own(&key)
                || matches!(o.borrow().get(&key), v if !matches!(v, JsValue::Undefined)))),
            _ => Ok(JsValue::Bool(false)),
        }
    }));
    refl.set("deleteProperty".into(), native("Reflect.deleteProperty", |a| {
        let mut iter = a.into_iter();
        let target = iter.next().unwrap_or(JsValue::Undefined);
        let key = iter.next().map(|v| v.to_string()).unwrap_or_default();
        match target {
            JsValue::Object(o) => {
                let removed = o.borrow_mut().props.remove(&key).is_some();
                Ok(JsValue::Bool(removed))
            }
            _ => Ok(JsValue::Bool(false)),
        }
    }));
    refl.set("ownKeys".into(), native("Reflect.ownKeys", |a| {
        match a.into_iter().next() {
            Some(JsValue::Object(o)) => {
                let keys: Vec<JsValue> = o.borrow().own_keys()
                    .into_iter().map(JsValue::Str).collect();
                Ok(JsValue::Array(Rc::new(RefCell::new(keys))))
            }
            _ => Ok(JsValue::Array(Rc::new(RefCell::new(vec![])))),
        }
    }));
    refl.set("getPrototypeOf".into(), native("Reflect.getPrototypeOf", |a| {
        match a.into_iter().next() {
            Some(JsValue::Object(o)) => Ok(match o.borrow().proto.clone() {
                Some(p) => JsValue::Object(p),
                None => JsValue::Null,
            }),
            _ => Ok(JsValue::Null),
        }
    }));
    refl.set("setPrototypeOf".into(), native("Reflect.setPrototypeOf", |a| {
        let mut iter = a.into_iter();
        let target = iter.next().unwrap_or(JsValue::Undefined);
        let proto = iter.next().unwrap_or(JsValue::Null);
        if let JsValue::Object(obj_rc) = &target {
            match proto {
                JsValue::Object(p) => { obj_rc.borrow_mut().proto = Some(Rc::clone(&p)); }
                JsValue::Null      => { obj_rc.borrow_mut().proto = None; }
                _ => return Ok(JsValue::Bool(false)),
            }
            return Ok(JsValue::Bool(true));
        }
        Ok(JsValue::Bool(false))
    }));
    refl.set("isExtensible".into(), native("Reflect.isExtensible", |a| {
        match a.into_iter().next() {
            Some(JsValue::Object(o)) => Ok(JsValue::Bool(!o.borrow().frozen)),
            _ => Ok(JsValue::Bool(false)),
        }
    }));
    refl.set("preventExtensions".into(), native("Reflect.preventExtensions", |a| {
        let obj = a.into_iter().next().unwrap_or(JsValue::Undefined);
        if let JsValue::Object(o) = &obj {
            o.borrow_mut().frozen = true;
            return Ok(JsValue::Bool(true));
        }
        Ok(JsValue::Bool(false))
    }));
    e.define("Reflect", JsValue::Object(Rc::new(RefCell::new(refl))));
}
