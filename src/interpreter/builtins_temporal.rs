//! Temporal API stub (TC39 Stage 3) - extracted z builtins.rs.
//!
//! Temporal.Now / PlainDate / PlainTime / Duration / Instant.

use std::rc::Rc;
use std::cell::RefCell;
use super::{JsValue, JsObject, Environment};
use super::helpers::{native, now_ms, ms_to_parts};

pub fn setup_temporal(e: &mut Environment) {
    let temporal = Rc::new(RefCell::new(JsObject::new()));
    // Temporal.Now
    let now_obj = Rc::new(RefCell::new(JsObject::new()));
    now_obj.borrow_mut().set("instant".into(), native("Temporal.Now.instant", |_| {
        let inst = Rc::new(RefCell::new(JsObject::new()));
        let ms = now_ms();
        inst.borrow_mut().set("__instant_ms__".into(), JsValue::Number(ms));
        inst.borrow_mut().set("epochMilliseconds".into(), JsValue::Number(ms));
        inst.borrow_mut().set("epochNanoseconds".into(), JsValue::Number(ms * 1_000_000.0));
        Ok(JsValue::Object(inst))
    }));
    now_obj.borrow_mut().set("plainDateISO".into(), native("Temporal.Now.plainDateISO", |_| {
        let ms = now_ms();
        let (yr, mo, day, _, _, _, _) = ms_to_parts(ms);
        let pd = Rc::new(RefCell::new(JsObject::new()));
        pd.borrow_mut().set("year".into(), JsValue::Number(yr as f64));
        pd.borrow_mut().set("month".into(), JsValue::Number((mo + 1) as f64));
        pd.borrow_mut().set("day".into(), JsValue::Number(day as f64));
        pd.borrow_mut().set("__plain_date__".into(), JsValue::Bool(true));
        Ok(JsValue::Object(pd))
    }));
    now_obj.borrow_mut().set("plainTimeISO".into(), native("Temporal.Now.plainTimeISO", |_| {
        let ms = now_ms();
        let (_, _, _, hr, mi, sec, ms_p) = ms_to_parts(ms);
        let pt = Rc::new(RefCell::new(JsObject::new()));
        pt.borrow_mut().set("hour".into(), JsValue::Number(hr as f64));
        pt.borrow_mut().set("minute".into(), JsValue::Number(mi as f64));
        pt.borrow_mut().set("second".into(), JsValue::Number(sec as f64));
        pt.borrow_mut().set("millisecond".into(), JsValue::Number(ms_p as f64));
        Ok(JsValue::Object(pt))
    }));
    now_obj.borrow_mut().set("zonedDateTimeISO".into(), native("Temporal.Now.zonedDateTimeISO", |_| {
        let ms = now_ms();
        let zdt = Rc::new(RefCell::new(JsObject::new()));
        zdt.borrow_mut().set("epochMilliseconds".into(), JsValue::Number(ms));
        zdt.borrow_mut().set("timeZoneId".into(), JsValue::Str("UTC".into()));
        Ok(JsValue::Object(zdt))
    }));
    temporal.borrow_mut().set("Now".into(), JsValue::Object(now_obj));
    // Temporal.PlainDate
    let plain_date = Rc::new(RefCell::new(JsObject::new()));
    plain_date.borrow_mut().set("from".into(), native("Temporal.PlainDate.from", |args| {
        let arg = args.into_iter().next().unwrap_or(JsValue::Undefined);
        let pd = Rc::new(RefCell::new(JsObject::new()));
        if let JsValue::Object(o) = &arg {
            let b = o.borrow();
            pd.borrow_mut().set("year".into(), b.props.get("year").cloned().unwrap_or(JsValue::Number(1970.0)));
            pd.borrow_mut().set("month".into(), b.props.get("month").cloned().unwrap_or(JsValue::Number(1.0)));
            pd.borrow_mut().set("day".into(), b.props.get("day").cloned().unwrap_or(JsValue::Number(1.0)));
        } else if let JsValue::Str(s) = &arg {
            let parts: Vec<&str> = s.split('-').collect();
            pd.borrow_mut().set("year".into(),
                JsValue::Number(parts.first().and_then(|p| p.parse().ok()).unwrap_or(1970.0)));
            pd.borrow_mut().set("month".into(),
                JsValue::Number(parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(1.0)));
            pd.borrow_mut().set("day".into(),
                JsValue::Number(parts.get(2).and_then(|p| p.parse().ok()).unwrap_or(1.0)));
        }
        pd.borrow_mut().set("__plain_date__".into(), JsValue::Bool(true));
        Ok(JsValue::Object(pd))
    }));
    temporal.borrow_mut().set("PlainDate".into(), JsValue::Object(plain_date));
    // Temporal.Duration
    let duration = Rc::new(RefCell::new(JsObject::new()));
    duration.borrow_mut().set("from".into(), native("Temporal.Duration.from", |args| {
        let arg = args.into_iter().next().unwrap_or(JsValue::Undefined);
        let dur = Rc::new(RefCell::new(JsObject::new()));
        if let JsValue::Object(o) = &arg {
            let b = o.borrow();
            for k in &["years", "months", "weeks", "days", "hours", "minutes", "seconds", "milliseconds"] {
                let v = b.props.get(*k).cloned().unwrap_or(JsValue::Number(0.0));
                dur.borrow_mut().set((*k).into(), v);
            }
        }
        dur.borrow_mut().set("__duration__".into(), JsValue::Bool(true));
        Ok(JsValue::Object(dur))
    }));
    temporal.borrow_mut().set("Duration".into(), JsValue::Object(duration));
    // Temporal.Instant
    let instant = Rc::new(RefCell::new(JsObject::new()));
    instant.borrow_mut().set("fromEpochMilliseconds".into(), native("Temporal.Instant.fromEpochMilliseconds", |args| {
        let ms = args.into_iter().next().map(|v| v.to_number()).unwrap_or(0.0);
        let inst = Rc::new(RefCell::new(JsObject::new()));
        inst.borrow_mut().set("epochMilliseconds".into(), JsValue::Number(ms));
        inst.borrow_mut().set("epochNanoseconds".into(), JsValue::Number(ms * 1_000_000.0));
        inst.borrow_mut().set("__instant_ms__".into(), JsValue::Number(ms));
        Ok(JsValue::Object(inst))
    }));
    temporal.borrow_mut().set("Instant".into(), JsValue::Object(instant));
    e.define("Temporal", JsValue::Object(temporal));
}
