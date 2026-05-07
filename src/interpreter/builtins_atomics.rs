//! Atomics - extracted z builtins.rs.
//!
//! V sync runtime jsou to bezne operace (zadna konkurence). Pracuje na
//! ArrayBuffer/SharedArrayBuffer's __bytes__ Array internalu.

use std::rc::Rc;
use std::cell::RefCell;
use super::{JsValue, JsObject, Environment};
use super::helpers::native;

pub fn setup_atomics(e: &mut Environment) {
    let mut atomics = JsObject::new();
    atomics.set("load".into(), native("Atomics.load", |a| {
        let arr = a.first().cloned().unwrap_or(JsValue::Undefined);
        let i = a.get(1).map(|v| v.to_number() as usize).unwrap_or(0);
        if let JsValue::Object(o) = arr {
            if let Some(JsValue::Array(bytes)) = o.borrow().props.get("__bytes__") {
                return Ok(bytes.borrow().get(i).cloned().unwrap_or(JsValue::Number(0.0)));
            }
        }
        Ok(JsValue::Number(0.0))
    }));
    atomics.set("store".into(), native("Atomics.store", |a| {
        let arr = a.first().cloned().unwrap_or(JsValue::Undefined);
        let i = a.get(1).map(|v| v.to_number() as usize).unwrap_or(0);
        let val = a.get(2).cloned().unwrap_or(JsValue::Number(0.0));
        if let JsValue::Object(o) = arr {
            if let Some(JsValue::Array(bytes)) = o.borrow().props.get("__bytes__") {
                let mut b = bytes.borrow_mut();
                while b.len() <= i { b.push(JsValue::Number(0.0)); }
                b[i] = val.clone();
            }
        }
        Ok(val)
    }));
    atomics.set("add".into(), native("Atomics.add", |a| {
        let arr = a.first().cloned().unwrap_or(JsValue::Undefined);
        let i = a.get(1).map(|v| v.to_number() as usize).unwrap_or(0);
        let delta = a.get(2).map(|v| v.to_number()).unwrap_or(0.0);
        if let JsValue::Object(o) = arr {
            if let Some(JsValue::Array(bytes)) = o.borrow().props.get("__bytes__") {
                let mut b = bytes.borrow_mut();
                let old = b.get(i).map(|v| v.to_number()).unwrap_or(0.0);
                while b.len() <= i { b.push(JsValue::Number(0.0)); }
                b[i] = JsValue::Number(old + delta);
                return Ok(JsValue::Number(old));
            }
        }
        Ok(JsValue::Number(0.0))
    }));
    atomics.set("sub".into(), native("Atomics.sub", |a| {
        let arr = a.first().cloned().unwrap_or(JsValue::Undefined);
        let i = a.get(1).map(|v| v.to_number() as usize).unwrap_or(0);
        let delta = a.get(2).map(|v| v.to_number()).unwrap_or(0.0);
        if let JsValue::Object(o) = arr {
            if let Some(JsValue::Array(bytes)) = o.borrow().props.get("__bytes__") {
                let mut b = bytes.borrow_mut();
                let old = b.get(i).map(|v| v.to_number()).unwrap_or(0.0);
                while b.len() <= i { b.push(JsValue::Number(0.0)); }
                b[i] = JsValue::Number(old - delta);
                return Ok(JsValue::Number(old));
            }
        }
        Ok(JsValue::Number(0.0))
    }));
    atomics.set("compareExchange".into(), native("Atomics.compareExchange", |a| {
        let arr = a.first().cloned().unwrap_or(JsValue::Undefined);
        let i = a.get(1).map(|v| v.to_number() as usize).unwrap_or(0);
        let expected = a.get(2).map(|v| v.to_number()).unwrap_or(0.0);
        let new_val = a.get(3).map(|v| v.to_number()).unwrap_or(0.0);
        if let JsValue::Object(o) = arr {
            if let Some(JsValue::Array(bytes)) = o.borrow().props.get("__bytes__") {
                let mut b = bytes.borrow_mut();
                let current = b.get(i).map(|v| v.to_number()).unwrap_or(0.0);
                if current == expected {
                    while b.len() <= i { b.push(JsValue::Number(0.0)); }
                    b[i] = JsValue::Number(new_val);
                }
                return Ok(JsValue::Number(current));
            }
        }
        Ok(JsValue::Number(0.0))
    }));
    atomics.set("wait".into(), native("Atomics.wait", |_| {
        Ok(JsValue::Str("not-equal".into()))
    }));
    atomics.set("waitAsync".into(), native("Atomics.waitAsync", |_| {
        let result = Rc::new(RefCell::new(JsObject::new()));
        result.borrow_mut().set("async".into(), JsValue::Bool(false));
        result.borrow_mut().set("value".into(), JsValue::Str("not-equal".into()));
        Ok(JsValue::Object(result))
    }));
    atomics.set("notify".into(), native("Atomics.notify", |_| Ok(JsValue::Number(0.0))));
    atomics.set("isLockFree".into(), native("Atomics.isLockFree", |a| {
        let size = a.into_iter().next().map(|v| v.to_number() as i64).unwrap_or(0);
        Ok(JsValue::Bool(matches!(size, 1 | 2 | 4 | 8)))
    }));
    atomics.set("pause".into(), native("Atomics.pause", |_| Ok(JsValue::Undefined)));
    atomics.set("exchange".into(), native("Atomics.exchange", |a| {
        let arr = a.first().cloned().unwrap_or(JsValue::Undefined);
        let i = a.get(1).map(|v| v.to_number() as usize).unwrap_or(0);
        let new_val = a.get(2).map(|v| v.to_number()).unwrap_or(0.0);
        if let JsValue::Object(o) = arr {
            if let Some(JsValue::Array(bytes)) = o.borrow().props.get("__bytes__") {
                let mut b = bytes.borrow_mut();
                let old = b.get(i).map(|v| v.to_number()).unwrap_or(0.0);
                while b.len() <= i { b.push(JsValue::Number(0.0)); }
                b[i] = JsValue::Number(new_val);
                return Ok(JsValue::Number(old));
            }
        }
        Ok(JsValue::Number(0.0))
    }));
    atomics.set("and".into(), native("Atomics.and", |a| {
        let arr = a.first().cloned().unwrap_or(JsValue::Undefined);
        let i = a.get(1).map(|v| v.to_number() as usize).unwrap_or(0);
        let mask = a.get(2).map(|v| v.to_number() as i64).unwrap_or(0);
        if let JsValue::Object(o) = arr {
            if let Some(JsValue::Array(bytes)) = o.borrow().props.get("__bytes__") {
                let mut b = bytes.borrow_mut();
                let old = b.get(i).map(|v| v.to_number() as i64).unwrap_or(0);
                while b.len() <= i { b.push(JsValue::Number(0.0)); }
                b[i] = JsValue::Number((old & mask) as f64);
                return Ok(JsValue::Number(old as f64));
            }
        }
        Ok(JsValue::Number(0.0))
    }));
    atomics.set("or".into(), native("Atomics.or", |a| {
        let arr = a.first().cloned().unwrap_or(JsValue::Undefined);
        let i = a.get(1).map(|v| v.to_number() as usize).unwrap_or(0);
        let mask = a.get(2).map(|v| v.to_number() as i64).unwrap_or(0);
        if let JsValue::Object(o) = arr {
            if let Some(JsValue::Array(bytes)) = o.borrow().props.get("__bytes__") {
                let mut b = bytes.borrow_mut();
                let old = b.get(i).map(|v| v.to_number() as i64).unwrap_or(0);
                while b.len() <= i { b.push(JsValue::Number(0.0)); }
                b[i] = JsValue::Number((old | mask) as f64);
                return Ok(JsValue::Number(old as f64));
            }
        }
        Ok(JsValue::Number(0.0))
    }));
    atomics.set("xor".into(), native("Atomics.xor", |a| {
        let arr = a.first().cloned().unwrap_or(JsValue::Undefined);
        let i = a.get(1).map(|v| v.to_number() as usize).unwrap_or(0);
        let mask = a.get(2).map(|v| v.to_number() as i64).unwrap_or(0);
        if let JsValue::Object(o) = arr {
            if let Some(JsValue::Array(bytes)) = o.borrow().props.get("__bytes__") {
                let mut b = bytes.borrow_mut();
                let old = b.get(i).map(|v| v.to_number() as i64).unwrap_or(0);
                while b.len() <= i { b.push(JsValue::Number(0.0)); }
                b[i] = JsValue::Number((old ^ mask) as f64);
                return Ok(JsValue::Number(old as f64));
            }
        }
        Ok(JsValue::Number(0.0))
    }));
    e.define("Atomics", JsValue::Object(Rc::new(RefCell::new(atomics))));
}
