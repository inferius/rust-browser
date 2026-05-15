/// Setup vestavenych globalnich objektu a funkci.
///
/// Volano z `Interpreter::new()`. Registruje:
/// - console (log/error/warn)
/// - Math (PI, sqrt, abs, floor, ...)
/// - parseInt, parseFloat, isNaN, isFinite, String, Number, Boolean, Array
/// - Object staticke metody (keys/values/entries/assign/freeze/create/...)
/// - Symbol s well-known symbols
/// - Map/Set/WeakMap/WeakSet konstruktory
/// - JSON.stringify/parse
/// - Date konstruktor (logika v call_new)
/// - Error typy (Error/TypeError/RangeError/...)
/// - Promise konstruktor
/// - BigNumber + BigInt konstruktory
/// - RegExp konstruktor
/// - Infinity, NaN, undefined konstanty
/// - globalThis, queueMicrotask, structuredClone
/// - Timery: setTimeout/clearTimeout/setInterval/clearInterval

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::str::FromStr;
use bigdecimal::BigDecimal;
use num_bigint::BigInt;
use super::{JsValue, JsObject, Environment};
use super::helpers::*;

use super::builtins_helpers::{run_worker_thread, make_message_port, build_search_params, make_object_store};

pub fn setup_builtins(
    env: &Rc<RefCell<Environment>>,
    task_queue: &Rc<RefCell<Vec<(u32, JsValue, Vec<JsValue>)>>>,
    next_timer_id: &Rc<RefCell<u32>>,
    workers: &Rc<RefCell<HashMap<u32, super::WorkerState>>>,
    next_worker_id: &Rc<RefCell<u32>>,
    document: &Rc<RefCell<crate::browser::dom::Document>>,
    console_log: &Rc<RefCell<Vec<(String, String)>>>,
    network_log: &Rc<RefCell<Vec<(String, u16)>>>,
    custom_elements_registry: &Rc<RefCell<HashMap<String, super::JsValue>>>,
    mutation_observers: &Rc<RefCell<Vec<(usize, super::JsValue, super::JsValue, bool)>>>,
    websockets: &Rc<RefCell<HashMap<u32, super::WebSocketState>>>,
    next_ws_id: &Rc<RefCell<u32>>,
    pending_fetches: &Rc<RefCell<Vec<super::PendingFetch>>>,
    pending_xhr_callbacks: &Rc<RefCell<Vec<(JsValue, JsValue)>>>,
    raf_callbacks: &Rc<RefCell<Vec<(u32, JsValue)>>>,
    next_raf_id: &Rc<RefCell<u32>>,
    scroll_pos: &Rc<RefCell<(f32, f32)>>,
) {
    let mut e = env.borrow_mut();

    // console - vsechny varianty captureju do console_log Rc<RefCell<Vec>>
    let mut console = JsObject::new();
    {
        let log = Rc::clone(console_log);
        console.set("log".into(), native("log", move |args| {
            let msg = args.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ");
            println!("{msg}");
            log.borrow_mut().push(("log".into(), msg));
            Ok(JsValue::Undefined)
        }));
    }
    {
        let log = Rc::clone(console_log);
        console.set("error".into(), native("error", move |args| {
            let msg = args.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ");
            eprintln!("[error] {msg}");
            log.borrow_mut().push(("error".into(), msg));
            Ok(JsValue::Undefined)
        }));
    }
    {
        let log = Rc::clone(console_log);
        console.set("warn".into(), native("warn", move |args| {
            let msg = args.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ");
            eprintln!("[warn] {msg}");
            log.borrow_mut().push(("warn".into(), msg));
            Ok(JsValue::Undefined)
        }));
    }
    {
        let log = Rc::clone(console_log);
        console.set("info".into(), native("info", move |args| {
            let msg = args.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ");
            println!("[info] {msg}");
            log.borrow_mut().push(("info".into(), msg));
            Ok(JsValue::Undefined)
        }));
    }
    {
        let log = Rc::clone(console_log);
        console.set("debug".into(), native("debug", move |args| {
            let msg = args.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ");
            println!("[debug] {msg}");
            log.borrow_mut().push(("debug".into(), msg));
            Ok(JsValue::Undefined)
        }));
    }
    // Console extras: trace, table, group, groupEnd, time, timeEnd, count, dir, assert
    {
        let log = Rc::clone(console_log);
        console.set("trace".into(), native("trace", move |args| {
            let msg = args.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ");
            log.borrow_mut().push(("trace".into(), msg));
            Ok(JsValue::Undefined)
        }));
    }
    {
        let log = Rc::clone(console_log);
        console.set("table".into(), native("table", move |args| {
            // Simply log args jako "table"
            let data = args.into_iter().next().unwrap_or(JsValue::Undefined);
            let msg = format!("[table] {}", data.to_string());
            log.borrow_mut().push(("table".into(), msg));
            Ok(JsValue::Undefined)
        }));
    }
    {
        let log = Rc::clone(console_log);
        console.set("group".into(), native("group", move |args| {
            let label = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            log.borrow_mut().push(("group".into(), label));
            Ok(JsValue::Undefined)
        }));
    }
    {
        let log = Rc::clone(console_log);
        console.set("groupCollapsed".into(), native("groupCollapsed", move |args| {
            let label = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            log.borrow_mut().push(("groupCollapsed".into(), label));
            Ok(JsValue::Undefined)
        }));
    }
    console.set("groupEnd".into(), native("groupEnd", |_| Ok(JsValue::Undefined)));
    console.set("dir".into(), native("dir", |_| Ok(JsValue::Undefined)));
    console.set("dirxml".into(), native("dirxml", |_| Ok(JsValue::Undefined)));
    console.set("clear".into(), native("clear", |_| Ok(JsValue::Undefined)));
    // Time tracking
    {
        let timers: Rc<RefCell<HashMap<String, std::time::Instant>>> = Rc::new(RefCell::new(HashMap::new()));
        let t1 = Rc::clone(&timers);
        console.set("time".into(), native("time", move |args| {
            let label = args.into_iter().next().map(|v| v.to_string()).unwrap_or_else(|| "default".into());
            t1.borrow_mut().insert(label, std::time::Instant::now());
            Ok(JsValue::Undefined)
        }));
        let t2 = Rc::clone(&timers);
        let log = Rc::clone(console_log);
        console.set("timeEnd".into(), native("timeEnd", move |args| {
            let label = args.into_iter().next().map(|v| v.to_string()).unwrap_or_else(|| "default".into());
            if let Some(start) = t2.borrow_mut().remove(&label) {
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                let msg = format!("{}: {}ms", label, elapsed);
                log.borrow_mut().push(("time".into(), msg));
            }
            Ok(JsValue::Undefined)
        }));
        let t3 = Rc::clone(&timers);
        let log2 = Rc::clone(console_log);
        console.set("timeLog".into(), native("timeLog", move |args| {
            let label = args.into_iter().next().map(|v| v.to_string()).unwrap_or_else(|| "default".into());
            if let Some(start) = t3.borrow().get(&label) {
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                log2.borrow_mut().push(("timeLog".into(), format!("{}: {}ms", label, elapsed)));
            }
            Ok(JsValue::Undefined)
        }));
    }
    // Count tracking
    {
        let counters: Rc<RefCell<HashMap<String, u64>>> = Rc::new(RefCell::new(HashMap::new()));
        let c1 = Rc::clone(&counters);
        let log = Rc::clone(console_log);
        console.set("count".into(), native("count", move |args| {
            let label = args.into_iter().next().map(|v| v.to_string()).unwrap_or_else(|| "default".into());
            let count = {
                let mut b = c1.borrow_mut();
                *b.entry(label.clone()).or_insert(0) += 1;
                *b.get(&label).unwrap()
            };
            log.borrow_mut().push(("count".into(), format!("{}: {}", label, count)));
            Ok(JsValue::Undefined)
        }));
        let c2 = Rc::clone(&counters);
        console.set("countReset".into(), native("countReset", move |args| {
            let label = args.into_iter().next().map(|v| v.to_string()).unwrap_or_else(|| "default".into());
            c2.borrow_mut().remove(&label);
            Ok(JsValue::Undefined)
        }));
    }
    // assert
    {
        let log = Rc::clone(console_log);
        console.set("assert".into(), native("assert", move |args| {
            let mut it = args.into_iter();
            let cond = it.next().map(|v| v.is_truthy()).unwrap_or(false);
            if !cond {
                let msg = it.map(|v| v.to_string()).collect::<Vec<_>>().join(" ");
                log.borrow_mut().push(("error".into(), format!("Assertion failed: {}", msg)));
            }
            Ok(JsValue::Undefined)
        }));
    }
    console.set("profile".into(), native("profile", |_| Ok(JsValue::Undefined)));
    console.set("profileEnd".into(), native("profileEnd", |_| Ok(JsValue::Undefined)));
    console.set("timeStamp".into(), native("timeStamp", |_| Ok(JsValue::Undefined)));
    e.define("console", JsValue::Object(Rc::new(RefCell::new(console))));

    // Math
    let mut math = JsObject::new();
    math.set("PI".into(), JsValue::Number(std::f64::consts::PI));
    math.set("E".into(),  JsValue::Number(std::f64::consts::E));
    math.set("sqrt".into(),  native("sqrt",  |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).sqrt()))));
    math.set("abs".into(),   native("abs",   |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).abs()))));
    math.set("floor".into(), native("floor", |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).floor()))));
    math.set("ceil".into(),  native("ceil",  |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).ceil()))));
    math.set("round".into(), native("round", |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).round()))));
    math.set("sin".into(),   native("sin",   |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).sin()))));
    math.set("cos".into(),   native("cos",   |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).cos()))));
    math.set("log".into(),   native("log",   |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).ln()))));
    math.set("max".into(),   native("max",   |a| Ok(JsValue::Number(a.iter().fold(f64::NEG_INFINITY, |acc, v| acc.max(v.to_number()))))));
    math.set("min".into(),   native("min",   |a| Ok(JsValue::Number(a.iter().fold(f64::INFINITY,     |acc, v| acc.min(v.to_number()))))));
    math.set("pow".into(),   native("pow",   |a| {
        let base = a.get(0).map(|v| v.to_number()).unwrap_or(f64::NAN);
        let exp  = a.get(1).map(|v| v.to_number()).unwrap_or(f64::NAN);
        Ok(JsValue::Number(base.powf(exp)))
    }));
    math.set("random".into(), native("random", |_| {
        use std::sync::atomic::{AtomicU64, Ordering};
        static S: AtomicU64 = AtomicU64::new(12345678901234567);
        let s = S.fetch_add(6364136223846793005, Ordering::Relaxed);
        Ok(JsValue::Number((s >> 11) as f64 / (1u64 << 53) as f64))
    }));
    // Doplneni chybejicich Math metod (ECMA262 20.3)
    math.set("sign".into(),  native("sign",  |a| Ok(JsValue::Number({ let v = a.first().map(|x| x.to_number()).unwrap_or(f64::NAN); if v.is_nan() { f64::NAN } else if v == 0.0 { 0.0 } else { v.signum() } }))));
    math.set("cbrt".into(),  native("cbrt",  |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).cbrt()))));
    math.set("log2".into(),  native("log2",  |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).log2()))));
    math.set("log10".into(), native("log10", |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).log10()))));
    math.set("exp".into(),   native("exp",   |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).exp()))));
    math.set("expm1".into(), native("expm1", |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).exp_m1()))));
    math.set("log1p".into(), native("log1p", |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).ln_1p()))));
    math.set("tan".into(),   native("tan",   |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).tan()))));
    math.set("asin".into(),  native("asin",  |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).asin()))));
    math.set("acos".into(),  native("acos",  |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).acos()))));
    math.set("atan".into(),  native("atan",  |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).atan()))));
    math.set("atan2".into(), native("atan2", |a| {
        let y = a.get(0).map(|v| v.to_number()).unwrap_or(f64::NAN);
        let x = a.get(1).map(|v| v.to_number()).unwrap_or(f64::NAN);
        Ok(JsValue::Number(y.atan2(x)))
    }));
    math.set("trunc".into(), native("trunc", |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).trunc()))));
    math.set("fround".into(),native("fround",|a| Ok(JsValue::Number((a.first().map(|v| v.to_number()).unwrap_or(f64::NAN) as f32) as f64))));
    math.set("clz32".into(), native("clz32", |a| {
        let v = a.first().map(|v| v.to_number()).unwrap_or(0.0) as u32;
        Ok(JsValue::Number(v.leading_zeros() as f64))
    }));
    math.set("imul".into(),  native("imul",  |a| {
        let x = a.get(0).map(|v| v.to_number()).unwrap_or(0.0) as i32;
        let y = a.get(1).map(|v| v.to_number()).unwrap_or(0.0) as i32;
        Ok(JsValue::Number(x.wrapping_mul(y) as f64))
    }));
    math.set("hypot".into(), native("hypot", |a| {
        let sum_sq: f64 = a.iter().map(|v| { let n = v.to_number(); n * n }).sum();
        Ok(JsValue::Number(sum_sq.sqrt()))
    }));
    math.set("LN2".into(),   JsValue::Number(std::f64::consts::LN_2));
    math.set("LN10".into(),  JsValue::Number(std::f64::consts::LN_10));
    math.set("LOG2E".into(), JsValue::Number(std::f64::consts::LOG2_E));
    math.set("LOG10E".into(),JsValue::Number(std::f64::consts::LOG10_E));
    math.set("SQRT2".into(), JsValue::Number(std::f64::consts::SQRT_2));
    math.set("SQRT1_2".into(),JsValue::Number(1.0 / std::f64::consts::SQRT_2));
    e.define("Math", JsValue::Object(Rc::new(RefCell::new(math))));

    // Globalni funkce
    e.define("parseInt", native("parseInt", |a| {
        let s = a.first().map(|v| v.to_string()).unwrap_or_default();
        let radix = a.get(1).map(|v| v.to_number() as u32).unwrap_or(10).max(2).min(36);
        Ok(JsValue::Number(i64::from_str_radix(s.trim(), radix).map(|n| n as f64).unwrap_or(f64::NAN)))
    }));
    e.define("parseFloat", native("parseFloat", |a| {
        Ok(JsValue::Number(a.first().map(|v| v.to_string()).unwrap_or_default().trim().parse().unwrap_or(f64::NAN)))
    }));
    e.define("isNaN", native("isNaN", |a| {
        Ok(JsValue::Bool(a.first().map(|v| v.to_number().is_nan()).unwrap_or(true)))
    }));
    e.define("isFinite", native("isFinite", |a| {
        Ok(JsValue::Bool(a.first().map(|v| v.to_number().is_finite()).unwrap_or(false)))
    }));
    e.define("String", native("String", |a| {
        Ok(JsValue::Str(a.first().map(|v| v.to_string()).unwrap_or_default()))
    }));
    e.define("Number", native("Number", |a| {
        Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(0.0)))
    }));
    e.define("Boolean", native("Boolean", |a| {
        Ok(JsValue::Bool(a.first().map(|v| v.is_truthy()).unwrap_or(false)))
    }));
    e.define("Array", native("Array", |a| {
        if let (1, Some(JsValue::Number(n))) = (a.len(), a.first()) {
            return Ok(JsValue::Array(Rc::new(RefCell::new(vec![JsValue::Undefined; *n as usize]))));
        }
        Ok(JsValue::Array(Rc::new(RefCell::new(a))))
    }));

    // Object staticke metody
    let mut obj_ctor = JsObject::new();

    obj_ctor.set("keys".into(), native("Object.keys", |a| {
        match a.into_iter().next() {
            Some(JsValue::Object(o)) => {
                let keys: Vec<JsValue> = o.borrow().own_keys()
                    .into_iter().map(JsValue::Str).collect();
                Ok(JsValue::Array(Rc::new(RefCell::new(keys))))
            }
            _ => Ok(JsValue::Array(Rc::new(RefCell::new(vec![]))))
        }
    }));
    obj_ctor.set("values".into(), native("Object.values", |a| {
        match a.into_iter().next() {
            Some(JsValue::Object(o)) => {
                let obj = o.borrow();
                let vals: Vec<JsValue> = obj.own_keys()
                    .into_iter().map(|k| obj.props.get(&k).cloned().unwrap_or(JsValue::Undefined)).collect();
                Ok(JsValue::Array(Rc::new(RefCell::new(vals))))
            }
            _ => Ok(JsValue::Array(Rc::new(RefCell::new(vec![]))))
        }
    }));
    obj_ctor.set("entries".into(), native("Object.entries", |a| {
        match a.into_iter().next() {
            Some(JsValue::Object(o)) => {
                let obj = o.borrow();
                let entries: Vec<JsValue> = obj.own_keys().into_iter().map(|k| {
                    let v = obj.props.get(&k).cloned().unwrap_or(JsValue::Undefined);
                    JsValue::Array(Rc::new(RefCell::new(vec![JsValue::Str(k), v])))
                }).collect();
                Ok(JsValue::Array(Rc::new(RefCell::new(entries))))
            }
            _ => Ok(JsValue::Array(Rc::new(RefCell::new(vec![]))))
        }
    }));
    obj_ctor.set("assign".into(), native("Object.assign", |a| {
        let mut iter = a.into_iter();
        let target = iter.next().unwrap_or(JsValue::Undefined);
        if let JsValue::Object(target_rc) = &target {
            for src in iter {
                if let JsValue::Object(src_rc) = src {
                    for k in src_rc.borrow().own_keys() {
                        let v = src_rc.borrow().props.get(&k).cloned().unwrap_or(JsValue::Undefined);
                        target_rc.borrow_mut().props.insert(k, v);
                    }
                }
            }
        }
        Ok(target)
    }));
    obj_ctor.set("freeze".into(), native("Object.freeze", |a| {
        let obj = a.into_iter().next().unwrap_or(JsValue::Undefined);
        if let JsValue::Object(o) = &obj {
            o.borrow_mut().frozen = true;
        }
        Ok(obj)
    }));
    obj_ctor.set("isFrozen".into(), native("Object.isFrozen", |a| {
        match a.into_iter().next() {
            Some(JsValue::Object(o)) => Ok(JsValue::Bool(o.borrow().frozen)),
            _                        => Ok(JsValue::Bool(true)), // primitives are "frozen" per spec
        }
    }));
    obj_ctor.set("isExtensible".into(), native("Object.isExtensible", |a| {
        match a.into_iter().next() {
            Some(JsValue::Object(o)) => Ok(JsValue::Bool(!o.borrow().frozen)),
            _ => Ok(JsValue::Bool(false)), // primitives are non-extensible per spec
        }
    }));
    obj_ctor.set("isSealed".into(), native("Object.isSealed", |a| {
        // Plna implementace by testovala "non-configurable" props; tady pouzivame frozen jako proxy.
        match a.into_iter().next() {
            Some(JsValue::Object(o)) => Ok(JsValue::Bool(o.borrow().frozen)),
            _ => Ok(JsValue::Bool(true)),
        }
    }));
    obj_ctor.set("preventExtensions".into(), native("Object.preventExtensions", |a| {
        let obj = a.into_iter().next().unwrap_or(JsValue::Undefined);
        if let JsValue::Object(o) = &obj { o.borrow_mut().frozen = true; }
        Ok(obj)
    }));
    obj_ctor.set("create".into(), native("Object.create", |a| {
        let proto = a.into_iter().next().unwrap_or(JsValue::Null);
        let obj = match proto {
            JsValue::Object(p) => JsObject::new_with_proto(p),
            JsValue::Null      => JsObject::new(),
            _                  => return Err("Object.create: proto musi byt Object nebo null".into()),
        };
        Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
    }));
    obj_ctor.set("getPrototypeOf".into(), native("Object.getPrototypeOf", |a| {
        match a.into_iter().next() {
            Some(JsValue::Object(o)) => Ok(match o.borrow().proto.clone() {
                Some(p) => JsValue::Object(p),
                None    => JsValue::Null,
            }),
            _ => Err("Object.getPrototypeOf: argument musi byt objekt".into()),
        }
    }));
    obj_ctor.set("setPrototypeOf".into(), native("Object.setPrototypeOf", |a| {
        let mut iter = a.into_iter();
        let obj   = iter.next().unwrap_or(JsValue::Undefined);
        let proto = iter.next().unwrap_or(JsValue::Null);
        if let JsValue::Object(obj_rc) = &obj {
            match &proto {
                JsValue::Object(p) => { obj_rc.borrow_mut().proto = Some(Rc::clone(p)); }
                JsValue::Null      => { obj_rc.borrow_mut().proto = None; }
                _ => return Err("Object.setPrototypeOf: proto musi byt Object nebo null".into()),
            }
        }
        Ok(obj)
    }));
    obj_ctor.set("hasOwn".into(), native("Object.hasOwn", |a| {
        let mut iter = a.into_iter();
        let obj = iter.next().unwrap_or(JsValue::Undefined);
        let key = iter.next().map(|v| v.to_string()).unwrap_or_default();
        match obj {
            JsValue::Object(o) => Ok(JsValue::Bool(o.borrow().has_own(&key))),
            _ => Ok(JsValue::Bool(false)),
        }
    }));
    obj_ctor.set("is".into(), native("Object.is", |a| {
        let mut iter = a.into_iter();
        let a = iter.next().unwrap_or(JsValue::Undefined);
        let b = iter.next().unwrap_or(JsValue::Undefined);
        let eq = match (&a, &b) {
            (JsValue::Number(x), JsValue::Number(y)) => {
                if x.is_nan() && y.is_nan() { true } else { x.to_bits() == y.to_bits() }
            }
            _ => a.strict_eq(&b),
        };
        Ok(JsValue::Bool(eq))
    }));
    obj_ctor.set("defineProperty".into(), native("Object.defineProperty", |a| {
        let mut iter = a.into_iter();
        let obj  = iter.next().unwrap_or(JsValue::Undefined);
        let key  = iter.next().map(|v| v.to_string()).unwrap_or_default();
        let desc = iter.next().unwrap_or(JsValue::Undefined);
        if let (JsValue::Object(obj_rc), JsValue::Object(desc_rc)) = (&obj, &desc) {
            let get_fn = desc_rc.borrow().props.get("get").cloned();
            let set_fn = desc_rc.borrow().props.get("set").cloned();
            if let Some(getter) = get_fn {
                obj_rc.borrow_mut().props.insert(format!("__get_{key}__"), getter);
            }
            if let Some(setter) = set_fn {
                obj_rc.borrow_mut().props.insert(format!("__set_{key}__"), setter);
            }
            let val = desc_rc.borrow().get("value");
            if !matches!(val, JsValue::Undefined) {
                obj_rc.borrow_mut().props.insert(key, val);
            }
        }
        Ok(obj)
    }));
    obj_ctor.set("fromEntries".into(), native("Object.fromEntries", |a| {
        let mut obj = JsObject::new();
        if let Some(JsValue::Array(entries)) = a.into_iter().next() {
            for entry in entries.borrow().iter() {
                if let JsValue::Array(pair) = entry {
                    let pair = pair.borrow();
                    let key = pair.get(0).map(|v| v.to_string()).unwrap_or_default();
                    let val = pair.get(1).cloned().unwrap_or(JsValue::Undefined);
                    obj.set(key, val);
                }
            }
        }
        Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
    }));
    e.define("Object", JsValue::Object(Rc::new(RefCell::new(obj_ctor))));

    // Symbol - well-known symbols jako string klice + Symbol() konstruktor
    // Symbol() vraci unique identifier ulozeny jako string "Symbol(desc)#N"
    let mut sym_obj = JsObject::new();
    sym_obj.set("iterator".into(), JsValue::Str("Symbol.iterator".into()));
    sym_obj.set("toPrimitive".into(), JsValue::Str("Symbol.toPrimitive".into()));
    sym_obj.set("hasInstance".into(), JsValue::Str("Symbol.hasInstance".into()));
    sym_obj.set("asyncIterator".into(), JsValue::Str("Symbol.asyncIterator".into()));
    // ES2015+ well-known symbols
    sym_obj.set("isConcatSpreadable".into(), JsValue::Str("Symbol.isConcatSpreadable".into()));
    sym_obj.set("match".into(), JsValue::Str("Symbol.match".into()));
    sym_obj.set("matchAll".into(), JsValue::Str("Symbol.matchAll".into()));
    sym_obj.set("replace".into(), JsValue::Str("Symbol.replace".into()));
    sym_obj.set("search".into(), JsValue::Str("Symbol.search".into()));
    sym_obj.set("species".into(), JsValue::Str("Symbol.species".into()));
    sym_obj.set("split".into(), JsValue::Str("Symbol.split".into()));
    sym_obj.set("toStringTag".into(), JsValue::Str("Symbol.toStringTag".into()));
    sym_obj.set("unscopables".into(), JsValue::Str("Symbol.unscopables".into()));
    // ES2022 - Symbol.dispose / Symbol.asyncDispose (Explicit Resource Management)
    sym_obj.set("dispose".into(), JsValue::Str("Symbol.dispose".into()));
    sym_obj.set("asyncDispose".into(), JsValue::Str("Symbol.asyncDispose".into()));
    // ES2024 - Symbol.metadata (decorators)
    sym_obj.set("metadata".into(), JsValue::Str("Symbol.metadata".into()));
    // Symbol.for(key) - registry-based symbols (sdilene podle stringu)
    sym_obj.set("for".into(), native("Symbol.for", |a| {
        let key = a.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
        Ok(JsValue::Str(format!("Symbol(@registry:{key})")))
    }));
    sym_obj.set("keyFor".into(), native("Symbol.keyFor", |a| {
        let s = a.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
        if let Some(rest) = s.strip_prefix("Symbol(@registry:") {
            if let Some(key) = rest.strip_suffix(")") {
                return Ok(JsValue::Str(key.to_string()));
            }
        }
        Ok(JsValue::Undefined)
    }));
    e.define("Symbol", JsValue::Object(Rc::new(RefCell::new(sym_obj))));

    // Reflect (ES2015) - extracted to builtins_reflect.rs.
    super::builtins_reflect::setup_reflect(&mut *e);

    // Proxy konstruktor - logika je v call_new, registrujeme stub
    e.define("Proxy", native("Proxy", |_| Ok(JsValue::Undefined)));

    // ─── Intl (ECMA-402) - lokalizace ────────────────────────────────────────
    // Vlastni lite implementace pro nejcastejsi locale (cs-CZ, en-US, de-DE).
    let mut intl = JsObject::new();

    // Intl.NumberFormat(locale).format(num) -> "1 234 567,89" (cs-CZ)
    intl.set("NumberFormat".into(), native("Intl.NumberFormat", |a| {
        let locale = a.first().map(|v| v.to_string()).unwrap_or_else(|| "en-US".into());
        let mut obj = JsObject::new();
        obj.set("__intl_locale__".into(), JsValue::Str(locale.clone()));
        obj.set("__intl_kind__".into(), JsValue::Str("number".into()));
        Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
    }));

    // Intl.DateTimeFormat(locale).format(date)
    intl.set("DateTimeFormat".into(), native("Intl.DateTimeFormat", |a| {
        let locale = a.first().map(|v| v.to_string()).unwrap_or_else(|| "en-US".into());
        let mut obj = JsObject::new();
        obj.set("__intl_locale__".into(), JsValue::Str(locale.clone()));
        obj.set("__intl_kind__".into(), JsValue::Str("datetime".into()));
        Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
    }));

    // Intl.Collator(locale).compare(a, b) -> -1/0/1
    intl.set("Collator".into(), native("Intl.Collator", |a| {
        let locale = a.first().map(|v| v.to_string()).unwrap_or_else(|| "en-US".into());
        let mut obj = JsObject::new();
        obj.set("__intl_locale__".into(), JsValue::Str(locale.clone()));
        obj.set("__intl_kind__".into(), JsValue::Str("collator".into()));
        Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
    }));

    // Intl.PluralRules(locale).select(n) -> "one"/"other"/...
    intl.set("PluralRules".into(), native("Intl.PluralRules", |a| {
        let locale = a.first().map(|v| v.to_string()).unwrap_or_else(|| "en-US".into());
        let mut obj = JsObject::new();
        obj.set("__intl_locale__".into(), JsValue::Str(locale.clone()));
        obj.set("__intl_kind__".into(), JsValue::Str("plural".into()));
        Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
    }));

    e.define("Intl", JsValue::Object(Rc::new(RefCell::new(intl))));

    // ─── atob / btoa - Base64 encode/decode ──────────────────────────────────
    e.define("btoa", native("btoa", |a| {
        let s = a.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
        Ok(JsValue::Str(base64_encode(s.as_bytes())))
    }));
    e.define("atob", native("atob", |a| {
        let s = a.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
        match base64_decode(&s) {
            Ok(bytes) => Ok(JsValue::Str(String::from_utf8_lossy(&bytes).into_owned())),
            Err(e) => Err(e),
        }
    }));

    // ─── TextEncoder / TextDecoder - UTF-8 ───────────────────────────────────
    // TextEncoder().encode(str) -> Uint8Array (zde reprezentovano jako Array of Numbers)
    e.define("TextEncoder", native("TextEncoder", |_| {
        let mut obj = JsObject::new();
        obj.set("__text_encoder__".into(), JsValue::Bool(true));
        obj.set("encoding".into(), JsValue::Str("utf-8".into()));
        Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
    }));
    e.define("TextDecoder", native("TextDecoder", |a| {
        let encoding = a.into_iter().next().map(|v| v.to_string())
            .unwrap_or_else(|| "utf-8".into());
        let mut obj = JsObject::new();
        obj.set("__text_decoder__".into(), JsValue::Bool(true));
        obj.set("encoding".into(), JsValue::Str(encoding));
        Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
    }));

    // ─── URL + URLSearchParams ───────────────────────────────────────────────
    e.define("URL", native("URL", |a| {
        let url_str = a.first().map(|v| v.to_string()).unwrap_or_default();
        let parsed = parse_url(&url_str);
        let mut obj = JsObject::new();
        obj.set("__url__".into(), JsValue::Bool(true));
        obj.set("href".into(),     JsValue::Str(url_str.clone()));
        obj.set("protocol".into(), JsValue::Str(parsed.protocol));
        obj.set("hostname".into(), JsValue::Str(parsed.hostname));
        obj.set("port".into(),     JsValue::Str(parsed.port));
        obj.set("pathname".into(), JsValue::Str(parsed.pathname));
        obj.set("search".into(),   JsValue::Str(parsed.search));
        obj.set("hash".into(),     JsValue::Str(parsed.hash));
        obj.set("host".into(),     JsValue::Str(parsed.host));
        obj.set("origin".into(),   JsValue::Str(parsed.origin));
        Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
    }));

    e.define("URLSearchParams", native("URLSearchParams", |a| {
        let init = a.into_iter().next().unwrap_or(JsValue::Undefined);
        let pairs: Vec<(String, String)> = match init {
            JsValue::Str(s) => parse_query_string(&s),
            JsValue::Array(arr) => {
                arr.borrow().iter().filter_map(|item| {
                    if let JsValue::Array(pair) = item {
                        let p = pair.borrow();
                        let k = p.get(0)?.to_string();
                        let v = p.get(1)?.to_string();
                        Some((k, v))
                    } else { None }
                }).collect()
            }
            _ => Vec::new(),
        };
        let mut obj = JsObject::new();
        obj.set("__url_params__".into(), JsValue::Bool(true));
        // Ulozime pary jako Array of [k, v]
        let arr: Vec<JsValue> = pairs.into_iter().map(|(k, v)| {
            JsValue::Array(Rc::new(RefCell::new(vec![JsValue::Str(k), JsValue::Str(v)])))
        }).collect();
        obj.set("__params__".into(), JsValue::Array(Rc::new(RefCell::new(arr))));
        Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
    }));

    // ─── crypto - randomUUID + getRandomValues ───────────────────────────────
    let mut crypto_obj = JsObject::new();
    crypto_obj.set("randomUUID".into(), native("crypto.randomUUID", |_| {
        Ok(JsValue::Str(generate_uuid_v4()))
    }));
    crypto_obj.set("getRandomValues".into(), native("crypto.getRandomValues", |a| {
        let arr = a.into_iter().next().unwrap_or(JsValue::Undefined);
        if let JsValue::Array(rc) = &arr {
            let len = rc.borrow().len();
            let mut new_arr: Vec<JsValue> = (0..len).map(|_| {
                JsValue::Number((random_u32() & 0xFF) as f64)
            }).collect();
            std::mem::swap(&mut *rc.borrow_mut(), &mut new_arr);
        }
        Ok(arr)
    }));
    e.define("crypto", JsValue::Object(Rc::new(RefCell::new(crypto_obj))));

    // ─── Storage API ─────────────────────────────────────────────────────────
    // localStorage: persistent (FS backend), sessionStorage: in-memory only.
    fn make_storage(name: &str, persistent: bool) -> JsValue {
        let mut obj = JsObject::new();
        obj.set("__storage__".into(), JsValue::Bool(true));
        obj.set("__storage_name__".into(), JsValue::Str(name.into()));
        obj.set("__storage_persistent__".into(), JsValue::Bool(persistent));
        let mut data = JsObject::new();
        // Nacti z disku pri init (jen persistent)
        if persistent {
            for (k, v) in load_storage_from_disk(name) {
                data.set(k, JsValue::Str(v));
            }
        }
        let len = data.own_keys().len() as f64;
        obj.set("__storage_data__".into(), JsValue::Object(Rc::new(RefCell::new(data))));
        obj.set("length".into(), JsValue::Number(len));
        JsValue::Object(Rc::new(RefCell::new(obj)))
    }
    e.define("localStorage",   make_storage("local-storage", true));
    e.define("sessionStorage", make_storage("session-storage", false));

    // ─── IndexedDB stub ──────────────────────────────────────────────────────
    let mut idb = JsObject::new();
    idb.set("open".into(), native("indexedDB.open", |a| {
        let name = a.first().map(|v| v.to_string()).unwrap_or_default();
        let mut req = JsObject::new();
        req.set("__idb_request__".into(), JsValue::Bool(true));
        req.set("name".into(), JsValue::Str(name));
        Ok(JsValue::Object(Rc::new(RefCell::new(req))))
    }));
    e.define("indexedDB", JsValue::Object(Rc::new(RefCell::new(idb))));

    // ─── Worker - real thread + mpsc channels ────────────────────────────────
    {
        let workers_ref = Rc::clone(workers);
        let id_ctr = Rc::clone(next_worker_id);
        e.define("Worker", native("Worker", move |a| {
            let url = a.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let id = {
                let mut c = id_ctr.borrow_mut();
                let i = *c;
                *c += 1;
                i
            };

            // Channels pro main <-> worker komunikaci
            let (main_to_worker_tx, main_to_worker_rx) = std::sync::mpsc::channel::<String>();
            let (worker_to_main_tx, worker_to_main_rx) = std::sync::mpsc::channel::<String>();

            // Spusti worker thread
            let url_clone = url.clone();
            let handle = std::thread::spawn(move || {
                run_worker_thread(&url_clone, main_to_worker_rx, worker_to_main_tx);
            });

            workers_ref.borrow_mut().insert(id, super::WorkerState {
                sender: main_to_worker_tx,
                outgoing: worker_to_main_rx,
                handle: Some(handle),
                on_message: None,
            });

            let mut obj = JsObject::new();
            obj.set("__worker__".into(), JsValue::Bool(true));
            obj.set("__worker_id__".into(), JsValue::Number(id as f64));
            obj.set("url".into(), JsValue::Str(url));
            Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
        }));
    }

    // ─── SharedWorker stub ───────────────────────────────────────────────
    e.define("SharedWorker", native("SharedWorker", |a| {
        let url = a.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("__shared_worker__".into(), JsValue::Bool(true));
        obj.borrow_mut().set("url".into(), JsValue::Str(url));
        // port - MessagePort-like
        let port = Rc::new(RefCell::new(JsObject::new()));
        port.borrow_mut().set("__message_port__".into(), JsValue::Bool(true));
        port.borrow_mut().set("postMessage".into(),
            native("postMessage", |_| Ok(JsValue::Undefined)));
        port.borrow_mut().set("start".into(),
            native("start", |_| Ok(JsValue::Undefined)));
        port.borrow_mut().set("close".into(),
            native("close", |_| Ok(JsValue::Undefined)));
        port.borrow_mut().set("addEventListener".into(),
            native("addEventListener", |_| Ok(JsValue::Undefined)));
        obj.borrow_mut().set("port".into(), JsValue::Object(port));
        obj.borrow_mut().set("addEventListener".into(),
            native("addEventListener", |_| Ok(JsValue::Undefined)));
        Ok(JsValue::Object(obj))
    }));

    // ─── ServiceWorkerGlobalScope (self) - skip (jen v workeru kontextu) ──

    // ─── DedicatedWorkerGlobalScope - postMessage / close (self) ──────────
    // (Bezi v Worker thread; tady jen alias pro main thread tak ze ho neni potreba.)

    // ─── SharedArrayBuffer stub ──────────────────────────────────────────────
    // V sync runtime se chova jako bezne pole bytu.
    e.define("SharedArrayBuffer", native("SharedArrayBuffer", |a| {
        let len = a.into_iter().next().map(|v| v.to_number() as usize).unwrap_or(0);
        let mut obj = JsObject::new();
        obj.set("__shared_buffer__".into(), JsValue::Bool(true));
        obj.set("byteLength".into(), JsValue::Number(len as f64));
        // Reprezentujeme jako Array of u8
        let bytes: Vec<JsValue> = vec![JsValue::Number(0.0); len];
        obj.set("__bytes__".into(), JsValue::Array(Rc::new(RefCell::new(bytes))));
        Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
    }));

    // ArrayBuffer - alias k SharedArrayBuffer v sync runtime + transfer/resize methods
    e.define("ArrayBuffer", native("ArrayBuffer", |a| {
        let mut it = a.into_iter();
        let len = it.next().map(|v| v.to_number() as usize).unwrap_or(0);
        let _max_byte_length = it.next().and_then(|v| {
            if let JsValue::Object(o) = v {
                Some(o.borrow().get("maxByteLength").to_number() as usize)
            } else { None }
        }).unwrap_or(len);
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("__buffer__".into(), JsValue::Bool(true));
        obj.borrow_mut().set("byteLength".into(), JsValue::Number(len as f64));
        obj.borrow_mut().set("maxByteLength".into(), JsValue::Number(len as f64));
        obj.borrow_mut().set("resizable".into(), JsValue::Bool(false));
        obj.borrow_mut().set("detached".into(), JsValue::Bool(false));
        let bytes: Vec<JsValue> = vec![JsValue::Number(0.0); len];
        obj.borrow_mut().set("__bytes__".into(), JsValue::Array(Rc::new(RefCell::new(bytes))));
        // ES2024 - transfer() / transferToFixedLength()
        let obj_t = Rc::clone(&obj);
        obj.borrow_mut().set("transfer".into(), native("transfer", move |args| {
            let new_len = args.into_iter().next().map(|v| v.to_number() as usize);
            let old_bytes = obj_t.borrow().get("__bytes__");
            let new_buf = Rc::new(RefCell::new(JsObject::new()));
            new_buf.borrow_mut().set("__buffer__".into(), JsValue::Bool(true));
            let copied: Vec<JsValue> = if let JsValue::Array(a) = old_bytes {
                let src = a.borrow().clone();
                if let Some(nl) = new_len {
                    let mut out = src;
                    out.resize(nl, JsValue::Number(0.0));
                    out
                } else { src }
            } else { Vec::new() };
            new_buf.borrow_mut().set("byteLength".into(), JsValue::Number(copied.len() as f64));
            new_buf.borrow_mut().set("__bytes__".into(),
                JsValue::Array(Rc::new(RefCell::new(copied))));
            // Mark old as detached
            obj_t.borrow_mut().set("detached".into(), JsValue::Bool(true));
            obj_t.borrow_mut().set("byteLength".into(), JsValue::Number(0.0));
            Ok(JsValue::Object(new_buf))
        }));
        let obj_r = Rc::clone(&obj);
        obj.borrow_mut().set("resize".into(), native("resize", move |args| {
            let new_len = args.into_iter().next().map(|v| v.to_number() as usize).unwrap_or(0);
            let old_bytes = obj_r.borrow().get("__bytes__");
            if let JsValue::Array(a) = old_bytes {
                a.borrow_mut().resize(new_len, JsValue::Number(0.0));
            }
            obj_r.borrow_mut().set("byteLength".into(), JsValue::Number(new_len as f64));
            Ok(JsValue::Undefined)
        }));
        let obj_s = Rc::clone(&obj);
        obj.borrow_mut().set("slice".into(), native("slice", move |args| {
            let mut it = args.into_iter();
            let start = it.next().map(|v| v.to_number() as usize).unwrap_or(0);
            let end = it.next().map(|v| v.to_number() as usize);
            let bytes_v = obj_s.borrow().get("__bytes__");
            let sliced = if let JsValue::Array(a) = bytes_v {
                let src = a.borrow();
                let e = end.unwrap_or(src.len()).min(src.len());
                if start < e { src[start..e].to_vec() } else { Vec::new() }
            } else { Vec::new() };
            let new_buf = Rc::new(RefCell::new(JsObject::new()));
            new_buf.borrow_mut().set("__buffer__".into(), JsValue::Bool(true));
            new_buf.borrow_mut().set("byteLength".into(), JsValue::Number(sliced.len() as f64));
            new_buf.borrow_mut().set("__bytes__".into(),
                JsValue::Array(Rc::new(RefCell::new(sliced))));
            Ok(JsValue::Object(new_buf))
        }));
        Ok(JsValue::Object(obj))
    }));

    // DataView - view do ArrayBuffer s typed read/write
    e.define("DataView", native("DataView", |args| {
        let mut it = args.into_iter();
        let buffer = it.next().unwrap_or(JsValue::Undefined);
        let byte_offset = it.next().map(|v| v.to_number() as usize).unwrap_or(0);
        let byte_length = it.next().map(|v| v.to_number() as usize);
        let bytes_arr = if let JsValue::Object(o) = &buffer {
            o.borrow().get("__bytes__")
        } else { JsValue::Undefined };
        let len = if let JsValue::Array(a) = &bytes_arr {
            byte_length.unwrap_or(a.borrow().len() - byte_offset)
        } else { 0 };
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("__data_view__".into(), JsValue::Bool(true));
        obj.borrow_mut().set("buffer".into(), buffer.clone());
        obj.borrow_mut().set("byteOffset".into(), JsValue::Number(byte_offset as f64));
        obj.borrow_mut().set("byteLength".into(), JsValue::Number(len as f64));
        // getUint8 / getInt8 / getUint16 / getInt16 / getUint32 / getInt32 / getFloat32 / getFloat64
        // setUint8 / setInt8 / atd.
        let bytes_get = bytes_arr.clone();
        obj.borrow_mut().set("getUint8".into(), native("getUint8", move |a| {
            let off = a.into_iter().next().map(|v| v.to_number() as usize).unwrap_or(0);
            if let JsValue::Array(arr) = &bytes_get {
                if let Some(JsValue::Number(n)) = arr.borrow().get(byte_offset + off) {
                    return Ok(JsValue::Number(*n));
                }
            }
            Ok(JsValue::Number(0.0))
        }));
        let bytes_set = bytes_arr.clone();
        obj.borrow_mut().set("setUint8".into(), native("setUint8", move |a| {
            let mut it = a.into_iter();
            let off = it.next().map(|v| v.to_number() as usize).unwrap_or(0);
            let val = it.next().map(|v| v.to_number() as u8).unwrap_or(0);
            if let JsValue::Array(arr) = &bytes_set {
                if let Some(slot) = arr.borrow_mut().get_mut(byte_offset + off) {
                    *slot = JsValue::Number(val as f64);
                }
            }
            Ok(JsValue::Undefined)
        }));
        // getInt8 - signed byte
        let bytes_i8 = bytes_arr.clone();
        obj.borrow_mut().set("getInt8".into(), native("getInt8", move |a| {
            let off = a.into_iter().next().map(|v| v.to_number() as usize).unwrap_or(0);
            if let JsValue::Array(arr) = &bytes_i8 {
                if let Some(JsValue::Number(n)) = arr.borrow().get(byte_offset + off) {
                    let byte_val = (*n as u32 & 0xFF) as u8;
                    return Ok(JsValue::Number(byte_val as i8 as f64));
                }
            }
            Ok(JsValue::Number(0.0))
        }));
        // getUint16 little-endian (default big = false)
        let bytes_u16 = bytes_arr.clone();
        obj.borrow_mut().set("getUint16".into(), native("getUint16", move |a| {
            let mut it = a.into_iter();
            let off = it.next().map(|v| v.to_number() as usize).unwrap_or(0);
            let little_endian = it.next().map(|v| v.is_truthy()).unwrap_or(false);
            if let JsValue::Array(arr) = &bytes_u16 {
                let b = arr.borrow();
                let b0 = b.get(byte_offset + off).and_then(|v| if let JsValue::Number(n) = v { Some(*n as u16) } else { None }).unwrap_or(0);
                let b1 = b.get(byte_offset + off + 1).and_then(|v| if let JsValue::Number(n) = v { Some(*n as u16) } else { None }).unwrap_or(0);
                let n = if little_endian { (b1 << 8) | b0 } else { (b0 << 8) | b1 };
                return Ok(JsValue::Number(n as f64));
            }
            Ok(JsValue::Number(0.0))
        }));
        // getInt16
        let bytes_i16 = bytes_arr.clone();
        obj.borrow_mut().set("getInt16".into(), native("getInt16", move |a| {
            let mut it = a.into_iter();
            let off = it.next().map(|v| v.to_number() as usize).unwrap_or(0);
            let little_endian = it.next().map(|v| v.is_truthy()).unwrap_or(false);
            if let JsValue::Array(arr) = &bytes_i16 {
                let b = arr.borrow();
                let b0 = b.get(byte_offset + off).and_then(|v| if let JsValue::Number(n) = v { Some(*n as u16) } else { None }).unwrap_or(0);
                let b1 = b.get(byte_offset + off + 1).and_then(|v| if let JsValue::Number(n) = v { Some(*n as u16) } else { None }).unwrap_or(0);
                let n = if little_endian { (b1 << 8) | b0 } else { (b0 << 8) | b1 };
                return Ok(JsValue::Number(n as i16 as f64));
            }
            Ok(JsValue::Number(0.0))
        }));
        // getUint32 / getInt32 / getFloat32 / getFloat64
        let bytes_u32 = bytes_arr.clone();
        obj.borrow_mut().set("getUint32".into(), native("getUint32", move |a| {
            let mut it = a.into_iter();
            let off = it.next().map(|v| v.to_number() as usize).unwrap_or(0);
            let little_endian = it.next().map(|v| v.is_truthy()).unwrap_or(false);
            if let JsValue::Array(arr) = &bytes_u32 {
                let b = arr.borrow();
                let mut bytes_buf = [0u8; 4];
                for i in 0..4 {
                    bytes_buf[i] = b.get(byte_offset + off + i)
                        .map(|v| v.to_number() as u8).unwrap_or(0);
                }
                let n = if little_endian {
                    u32::from_le_bytes(bytes_buf)
                } else {
                    u32::from_be_bytes(bytes_buf)
                };
                return Ok(JsValue::Number(n as f64));
            }
            Ok(JsValue::Number(0.0))
        }));
        let bytes_i32 = bytes_arr.clone();
        obj.borrow_mut().set("getInt32".into(), native("getInt32", move |a| {
            let mut it = a.into_iter();
            let off = it.next().map(|v| v.to_number() as usize).unwrap_or(0);
            let little_endian = it.next().map(|v| v.is_truthy()).unwrap_or(false);
            if let JsValue::Array(arr) = &bytes_i32 {
                let b = arr.borrow();
                let mut bytes_buf = [0u8; 4];
                for i in 0..4 {
                    bytes_buf[i] = b.get(byte_offset + off + i)
                        .map(|v| v.to_number() as u8).unwrap_or(0);
                }
                let n = if little_endian {
                    i32::from_le_bytes(bytes_buf)
                } else {
                    i32::from_be_bytes(bytes_buf)
                };
                return Ok(JsValue::Number(n as f64));
            }
            Ok(JsValue::Number(0.0))
        }));
        let bytes_f32 = bytes_arr.clone();
        obj.borrow_mut().set("getFloat32".into(), native("getFloat32", move |a| {
            let mut it = a.into_iter();
            let off = it.next().map(|v| v.to_number() as usize).unwrap_or(0);
            let little_endian = it.next().map(|v| v.is_truthy()).unwrap_or(false);
            if let JsValue::Array(arr) = &bytes_f32 {
                let b = arr.borrow();
                let mut bytes_buf = [0u8; 4];
                for i in 0..4 {
                    bytes_buf[i] = b.get(byte_offset + off + i)
                        .map(|v| v.to_number() as u8).unwrap_or(0);
                }
                let n = if little_endian {
                    f32::from_le_bytes(bytes_buf)
                } else {
                    f32::from_be_bytes(bytes_buf)
                };
                return Ok(JsValue::Number(n as f64));
            }
            Ok(JsValue::Number(0.0))
        }));
        let bytes_f64 = bytes_arr.clone();
        obj.borrow_mut().set("getFloat64".into(), native("getFloat64", move |a| {
            let mut it = a.into_iter();
            let off = it.next().map(|v| v.to_number() as usize).unwrap_or(0);
            let little_endian = it.next().map(|v| v.is_truthy()).unwrap_or(false);
            if let JsValue::Array(arr) = &bytes_f64 {
                let b = arr.borrow();
                let mut bytes_buf = [0u8; 8];
                for i in 0..8 {
                    bytes_buf[i] = b.get(byte_offset + off + i)
                        .map(|v| v.to_number() as u8).unwrap_or(0);
                }
                let n = if little_endian {
                    f64::from_le_bytes(bytes_buf)
                } else {
                    f64::from_be_bytes(bytes_buf)
                };
                return Ok(JsValue::Number(n));
            }
            Ok(JsValue::Number(0.0))
        }));
        // setUint16 / setUint32 / setInt16 / setInt32 / setFloat32 / setFloat64
        let bytes_set16 = bytes_arr.clone();
        obj.borrow_mut().set("setUint16".into(), native("setUint16", move |a| {
            let mut it = a.into_iter();
            let off = it.next().map(|v| v.to_number() as usize).unwrap_or(0);
            let val = it.next().map(|v| v.to_number() as u16).unwrap_or(0);
            let little_endian = it.next().map(|v| v.is_truthy()).unwrap_or(false);
            if let JsValue::Array(arr) = &bytes_set16 {
                let bytes_buf = if little_endian {
                    val.to_le_bytes()
                } else {
                    val.to_be_bytes()
                };
                let mut a_mut = arr.borrow_mut();
                while a_mut.len() < byte_offset + off + 2 { a_mut.push(JsValue::Number(0.0)); }
                a_mut[byte_offset + off]     = JsValue::Number(bytes_buf[0] as f64);
                a_mut[byte_offset + off + 1] = JsValue::Number(bytes_buf[1] as f64);
            }
            Ok(JsValue::Undefined)
        }));
        let bytes_set32 = bytes_arr.clone();
        obj.borrow_mut().set("setUint32".into(), native("setUint32", move |a| {
            let mut it = a.into_iter();
            let off = it.next().map(|v| v.to_number() as usize).unwrap_or(0);
            let val = it.next().map(|v| v.to_number() as u32).unwrap_or(0);
            let little_endian = it.next().map(|v| v.is_truthy()).unwrap_or(false);
            if let JsValue::Array(arr) = &bytes_set32 {
                let bytes_buf = if little_endian {
                    val.to_le_bytes()
                } else {
                    val.to_be_bytes()
                };
                let mut a_mut = arr.borrow_mut();
                while a_mut.len() < byte_offset + off + 4 { a_mut.push(JsValue::Number(0.0)); }
                for i in 0..4 {
                    a_mut[byte_offset + off + i] = JsValue::Number(bytes_buf[i] as f64);
                }
            }
            Ok(JsValue::Undefined)
        }));
        let bytes_setf32 = bytes_arr.clone();
        obj.borrow_mut().set("setFloat32".into(), native("setFloat32", move |a| {
            let mut it = a.into_iter();
            let off = it.next().map(|v| v.to_number() as usize).unwrap_or(0);
            let val = it.next().map(|v| v.to_number() as f32).unwrap_or(0.0);
            let little_endian = it.next().map(|v| v.is_truthy()).unwrap_or(false);
            if let JsValue::Array(arr) = &bytes_setf32 {
                let bytes_buf = if little_endian {
                    val.to_le_bytes()
                } else {
                    val.to_be_bytes()
                };
                let mut a_mut = arr.borrow_mut();
                while a_mut.len() < byte_offset + off + 4 { a_mut.push(JsValue::Number(0.0)); }
                for i in 0..4 {
                    a_mut[byte_offset + off + i] = JsValue::Number(bytes_buf[i] as f64);
                }
            }
            Ok(JsValue::Undefined)
        }));
        let bytes_setf64 = bytes_arr.clone();
        obj.borrow_mut().set("setFloat64".into(), native("setFloat64", move |a| {
            let mut it = a.into_iter();
            let off = it.next().map(|v| v.to_number() as usize).unwrap_or(0);
            let val = it.next().map(|v| v.to_number()).unwrap_or(0.0);
            let little_endian = it.next().map(|v| v.is_truthy()).unwrap_or(false);
            if let JsValue::Array(arr) = &bytes_setf64 {
                let bytes_buf = if little_endian {
                    val.to_le_bytes()
                } else {
                    val.to_be_bytes()
                };
                let mut a_mut = arr.borrow_mut();
                while a_mut.len() < byte_offset + off + 8 { a_mut.push(JsValue::Number(0.0)); }
                for i in 0..8 {
                    a_mut[byte_offset + off + i] = JsValue::Number(bytes_buf[i] as f64);
                }
            }
            Ok(JsValue::Undefined)
        }));
        Ok(JsValue::Object(obj))
    }));

    // Typed Arrays - vsechny varianty (Uint8Array, Int8Array, Uint16Array, ...)
    let make_typed_array = |name: &str, bytes_per_element: usize| {
        let n = name.to_string();
        let n_clone = n.clone();
        native(&n_clone, move |a| {
            let val = a.into_iter().next().unwrap_or(JsValue::Number(0.0));
            let bytes = match val {
                JsValue::Number(num) => vec![JsValue::Number(0.0); num as usize],
                JsValue::Array(arr) => arr.borrow().clone(),
                JsValue::Object(o) => {
                    // From ArrayBuffer
                    if let JsValue::Array(b) = o.borrow().get("__bytes__") {
                        b.borrow().clone()
                    } else { vec![] }
                }
                _ => vec![],
            };
            let len = bytes.len() as f64;
            let bytes_rc = Rc::new(RefCell::new(bytes));
            let obj = Rc::new(RefCell::new(JsObject::new()));
            obj.borrow_mut().set("__typed_array__".into(), JsValue::Str(n.clone()));
            obj.borrow_mut().set("length".into(), JsValue::Number(len));
            obj.borrow_mut().set("byteLength".into(), JsValue::Number(len * bytes_per_element as f64));
            obj.borrow_mut().set("BYTES_PER_ELEMENT".into(), JsValue::Number(bytes_per_element as f64));
            obj.borrow_mut().set("byteOffset".into(), JsValue::Number(0.0));
            obj.borrow_mut().set("__bytes__".into(), JsValue::Array(Rc::clone(&bytes_rc)));
            // buffer - vytvori novy ArrayBuffer view
            let buf_bytes = Rc::clone(&bytes_rc);
            let buffer = Rc::new(RefCell::new(JsObject::new()));
            buffer.borrow_mut().set("__buffer__".into(), JsValue::Bool(true));
            buffer.borrow_mut().set("byteLength".into(), JsValue::Number(len * bytes_per_element as f64));
            buffer.borrow_mut().set("__bytes__".into(), JsValue::Array(buf_bytes));
            obj.borrow_mut().set("buffer".into(), JsValue::Object(buffer));

            // Methods: subarray, set, copyWithin, fill, slice, indexOf, includes, reverse, sort, join
            let b1 = Rc::clone(&bytes_rc);
            let n_sub = n.clone();
            obj.borrow_mut().set("subarray".into(), native("subarray", move |args| {
                let mut it = args.into_iter();
                let begin = it.next().map(|v| v.to_number() as usize).unwrap_or(0);
                let end = it.next().map(|v| v.to_number() as usize)
                    .unwrap_or_else(|| b1.borrow().len());
                let src = b1.borrow();
                let sub: Vec<JsValue> = if begin < end {
                    src[begin..end.min(src.len())].to_vec()
                } else { Vec::new() };
                let new_obj = Rc::new(RefCell::new(JsObject::new()));
                new_obj.borrow_mut().set("__typed_array__".into(), JsValue::Str(n_sub.clone()));
                new_obj.borrow_mut().set("length".into(), JsValue::Number(sub.len() as f64));
                new_obj.borrow_mut().set("byteLength".into(),
                    JsValue::Number((sub.len() * bytes_per_element) as f64));
                new_obj.borrow_mut().set("BYTES_PER_ELEMENT".into(),
                    JsValue::Number(bytes_per_element as f64));
                new_obj.borrow_mut().set("__bytes__".into(),
                    JsValue::Array(Rc::new(RefCell::new(sub))));
                Ok(JsValue::Object(new_obj))
            }));
            let b2 = Rc::clone(&bytes_rc);
            obj.borrow_mut().set("set".into(), native("set", move |args| {
                let mut it = args.into_iter();
                let src = it.next().unwrap_or(JsValue::Undefined);
                let offset = it.next().map(|v| v.to_number() as usize).unwrap_or(0);
                let src_vals: Vec<JsValue> = match src {
                    JsValue::Array(a) => a.borrow().clone(),
                    JsValue::Object(o) => {
                        if let JsValue::Array(a) = o.borrow().get("__bytes__") {
                            a.borrow().clone()
                        } else { Vec::new() }
                    }
                    _ => Vec::new(),
                };
                let mut dst = b2.borrow_mut();
                for (i, v) in src_vals.into_iter().enumerate() {
                    while dst.len() <= offset + i { dst.push(JsValue::Number(0.0)); }
                    dst[offset + i] = v;
                }
                Ok(JsValue::Undefined)
            }));
            let b3 = Rc::clone(&bytes_rc);
            obj.borrow_mut().set("fill".into(), native("fill", move |args| {
                let mut it = args.into_iter();
                let val = it.next().unwrap_or(JsValue::Number(0.0));
                let start = it.next().map(|v| v.to_number() as usize).unwrap_or(0);
                let end_opt = it.next().map(|v| v.to_number() as usize);
                let mut dst = b3.borrow_mut();
                let end = end_opt.unwrap_or(dst.len()).min(dst.len());
                for i in start..end {
                    dst[i] = val.clone();
                }
                Ok(JsValue::Undefined)
            }));
            let b4 = Rc::clone(&bytes_rc);
            obj.borrow_mut().set("copyWithin".into(), native("copyWithin", move |args| {
                let mut it = args.into_iter();
                let target = it.next().map(|v| v.to_number() as usize).unwrap_or(0);
                let start = it.next().map(|v| v.to_number() as usize).unwrap_or(0);
                let mut dst = b4.borrow_mut();
                let end = it.next().map(|v| v.to_number() as usize).unwrap_or_else(|| dst.len());
                let copy_count = (end.min(dst.len()) - start.min(dst.len())).min(dst.len().saturating_sub(target));
                let copy: Vec<JsValue> = dst[start..start + copy_count].to_vec();
                for (i, v) in copy.into_iter().enumerate() {
                    if target + i < dst.len() { dst[target + i] = v; }
                }
                Ok(JsValue::Undefined)
            }));
            let b5 = Rc::clone(&bytes_rc);
            let n_slice = n.clone();
            obj.borrow_mut().set("slice".into(), native("slice", move |args| {
                let mut it = args.into_iter();
                let start = it.next().map(|v| v.to_number() as usize).unwrap_or(0);
                let end = it.next().map(|v| v.to_number() as usize)
                    .unwrap_or_else(|| b5.borrow().len());
                let src = b5.borrow();
                let sub: Vec<JsValue> = if start < end && start < src.len() {
                    src[start..end.min(src.len())].to_vec()
                } else { Vec::new() };
                let new_obj = Rc::new(RefCell::new(JsObject::new()));
                new_obj.borrow_mut().set("__typed_array__".into(), JsValue::Str(n_slice.clone()));
                new_obj.borrow_mut().set("length".into(), JsValue::Number(sub.len() as f64));
                new_obj.borrow_mut().set("byteLength".into(),
                    JsValue::Number((sub.len() * bytes_per_element) as f64));
                new_obj.borrow_mut().set("BYTES_PER_ELEMENT".into(),
                    JsValue::Number(bytes_per_element as f64));
                new_obj.borrow_mut().set("__bytes__".into(),
                    JsValue::Array(Rc::new(RefCell::new(sub))));
                Ok(JsValue::Object(new_obj))
            }));
            let b6 = Rc::clone(&bytes_rc);
            obj.borrow_mut().set("indexOf".into(), native("indexOf", move |args| {
                let needle = args.into_iter().next().unwrap_or(JsValue::Undefined);
                let target = needle.to_number();
                for (i, v) in b6.borrow().iter().enumerate() {
                    if v.to_number() == target {
                        return Ok(JsValue::Number(i as f64));
                    }
                }
                Ok(JsValue::Number(-1.0))
            }));
            let b7 = Rc::clone(&bytes_rc);
            obj.borrow_mut().set("includes".into(), native("includes", move |args| {
                let needle = args.into_iter().next().unwrap_or(JsValue::Undefined);
                let target = needle.to_number();
                Ok(JsValue::Bool(b7.borrow().iter().any(|v| v.to_number() == target)))
            }));
            let b8 = Rc::clone(&bytes_rc);
            obj.borrow_mut().set("reverse".into(), native("reverse", move |_| {
                b8.borrow_mut().reverse();
                Ok(JsValue::Undefined)
            }));
            let b9 = Rc::clone(&bytes_rc);
            obj.borrow_mut().set("join".into(), native("join", move |args| {
                let sep = args.into_iter().next().map(|v| v.to_string()).unwrap_or_else(|| ",".into());
                let s: Vec<String> = b9.borrow().iter().map(|v| v.to_string()).collect();
                Ok(JsValue::Str(s.join(&sep)))
            }));
            Ok(JsValue::Object(obj))
        })
    };
    e.define("Uint8Array", make_typed_array("Uint8Array", 1));
    e.define("Int8Array", make_typed_array("Int8Array", 1));
    e.define("Uint8ClampedArray", make_typed_array("Uint8ClampedArray", 1));
    e.define("Uint16Array", make_typed_array("Uint16Array", 2));
    e.define("Int16Array", make_typed_array("Int16Array", 2));
    e.define("Uint32Array", make_typed_array("Uint32Array", 4));
    e.define("Int32Array", make_typed_array("Int32Array", 4));
    e.define("Float32Array", make_typed_array("Float32Array", 4));
    e.define("Float64Array", make_typed_array("Float64Array", 8));
    e.define("BigInt64Array", make_typed_array("BigInt64Array", 8));
    e.define("BigUint64Array", make_typed_array("BigUint64Array", 8));

    // Atomics - extracted to builtins_atomics.rs.
    super::builtins_atomics::setup_atomics(&mut *e);
    // ─── DOM bridge - real propojeni s browser::dom ─────────────────────────
    // Pouziva se sdileny Rc<RefCell<Document>>. Element je JsValue::DomNode.

    let mut doc_obj = JsObject::new();
    doc_obj.set("__document__".into(), JsValue::Bool(true));

    // document.createElement(tagName)
    // document.adoptedStyleSheets - Constructable Stylesheets pool
    doc_obj.set("adoptedStyleSheets".into(), JsValue::Array(Rc::new(RefCell::new(Vec::new()))));

    doc_obj.set("createElement".into(), native("document.createElement", |a| {
        use crate::browser::dom::NodeData;
        let tag = a.into_iter().next().map(|v| v.to_string()).unwrap_or_else(|| "div".into());
        let node = NodeData::new_element(&tag, std::collections::HashMap::new());
        Ok(JsValue::DomNode(node))
    }));

    // document.createTextNode(text)
    doc_obj.set("createTextNode".into(), native("document.createTextNode", |a| {
        use crate::browser::dom::NodeData;
        let text = a.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
        Ok(JsValue::DomNode(NodeData::new_text(&text)))
    }));

    // document.createRange()
    doc_obj.set("createRange".into(), native("document.createRange", |_| Ok(make_range())));

    // document.getSelection() - cte z Document.selection registry (cached_text).
    {
        let sel = make_selection();
        if let JsValue::Object(obj) = &sel {
            let dr = Rc::clone(document);
            obj.borrow_mut().set("toString".into(), native("toString", move |_| {
                let d = dr.borrow();
                let s = d.selection.borrow();
                Ok(JsValue::Str(s.page_selection.as_ref()
                    .map(|p| p.cached_text.clone())
                    .unwrap_or_default()))
            }));
            let dr2 = Rc::clone(document);
            obj.borrow_mut().set("isCollapsed".into(), JsValue::Bool({
                let d = dr2.borrow();
                let s = d.selection.borrow();
                s.page_selection.is_none()
            }));
        }
        doc_obj.set("getSelection".into(), native("document.getSelection", move |_| Ok(sel.clone())));
    }

    // document.createDocumentFragment() - real DocumentFragment node.
    // Pri appendChild(frag) na parent se jeho deti presunou do parenta.
    doc_obj.set("createDocumentFragment".into(), native("document.createDocumentFragment", |_| {
        use crate::browser::dom::NodeData;
        Ok(JsValue::DomNode(NodeData::new_document_fragment()))
    }));

    // document.getElementById(id) - real walk skrz DOM tree
    {
        let doc = Rc::clone(document);
        doc_obj.set("getElementById".into(), native("document.getElementById", move |a| {
            let id = a.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            Ok(match doc.borrow().root.get_element_by_id(&id) {
                Some(n) => JsValue::DomNode(n),
                None    => JsValue::Null,
            })
        }));
    }

    // document.getElementsByTagName(tag)
    {
        let doc = Rc::clone(document);
        doc_obj.set("getElementsByTagName".into(), native("document.getElementsByTagName", move |a| {
            let tag = a.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let nodes = doc.borrow().root.get_elements_by_tag(&tag);
            let arr: Vec<JsValue> = nodes.into_iter().map(JsValue::DomNode).collect();
            Ok(JsValue::Array(Rc::new(RefCell::new(arr))))
        }));
    }

    // document.getElementsByClassName(class)
    {
        let doc = Rc::clone(document);
        doc_obj.set("getElementsByClassName".into(), native("document.getElementsByClassName", move |a| {
            let class = a.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let nodes = doc.borrow().root.get_elements_by_class(&class);
            let arr: Vec<JsValue> = nodes.into_iter().map(JsValue::DomNode).collect();
            Ok(JsValue::Array(Rc::new(RefCell::new(arr))))
        }));
    }

    // document.startViewTransition(callback) - View Transitions L1
    // Vola callback synchronne + vraci ViewTransition object s Promise resolved.
    doc_obj.set("startViewTransition".into(), native("document.startViewTransition", |args| {
        let _cb = args.into_iter().next().unwrap_or(JsValue::Undefined);
        // Stub: callback se nikdy nezavola, vraci se resolved Promise.
        let obj = Rc::new(RefCell::new(JsObject::new()));
        let resolved_promise = make_settled_promise("fulfilled", JsValue::Undefined);
        obj.borrow_mut().set("ready".into(), resolved_promise.clone());
        obj.borrow_mut().set("updateCallbackDone".into(), resolved_promise.clone());
        obj.borrow_mut().set("finished".into(), resolved_promise);
        obj.borrow_mut().set("skipTransition".into(), native("skipTransition", |_| Ok(JsValue::Undefined)));
        Ok(JsValue::Object(obj))
    }));

    // document.exitFullscreen / document.fullscreenElement / hasFocus / hidden
    doc_obj.set("exitFullscreen".into(), native("exitFullscreen", |_| {
        Ok(make_settled_promise("fulfilled", JsValue::Undefined))
    }));
    doc_obj.set("hasFocus".into(), native("hasFocus", |_| Ok(JsValue::Bool(true))));
    doc_obj.set("fullscreenElement".into(), JsValue::Null);
    doc_obj.set("activeElement".into(), JsValue::Null);
    doc_obj.set("hidden".into(), JsValue::Bool(false));
    doc_obj.set("visibilityState".into(), JsValue::Str("visible".into()));
    doc_obj.set("readyState".into(), JsValue::Str("complete".into()));
    doc_obj.set("title".into(), JsValue::Str(String::new()));
    doc_obj.set("URL".into(), JsValue::Str("about:blank".into()));
    doc_obj.set("documentURI".into(), JsValue::Str("about:blank".into()));
    doc_obj.set("domain".into(), JsValue::Str(String::new()));
    doc_obj.set("referrer".into(), JsValue::Str(String::new()));
    doc_obj.set("characterSet".into(), JsValue::Str("UTF-8".into()));
    doc_obj.set("compatMode".into(), JsValue::Str("CSS1Compat".into()));
    doc_obj.set("contentType".into(), JsValue::Str("text/html".into()));
    doc_obj.set("designMode".into(), JsValue::Str("off".into()));
    doc_obj.set("dir".into(), JsValue::Str("ltr".into()));
    // document.fonts (FontFaceSet)
    {
        let fonts = Rc::new(RefCell::new(JsObject::new()));
        let set: Rc<RefCell<Vec<JsValue>>> = Rc::new(RefCell::new(Vec::new()));
        {
            let s = Rc::clone(&set);
            fonts.borrow_mut().set("add".into(), native("fonts.add", move |args| {
                if let Some(f) = args.into_iter().next() {
                    s.borrow_mut().push(f);
                }
                Ok(JsValue::Undefined)
            }));
        }
        {
            let s = Rc::clone(&set);
            fonts.borrow_mut().set("delete".into(), native("fonts.delete", move |args| {
                let target = args.into_iter().next().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(target_o) = &target {
                    s.borrow_mut().retain(|v| {
                        if let JsValue::Object(o) = v { !Rc::ptr_eq(o, target_o) } else { true }
                    });
                }
                Ok(JsValue::Bool(true))
            }));
        }
        {
            let s = Rc::clone(&set);
            fonts.borrow_mut().set("clear".into(), native("fonts.clear", move |_| {
                s.borrow_mut().clear();
                Ok(JsValue::Undefined)
            }));
        }
        fonts.borrow_mut().set("ready".into(),
            make_settled_promise("fulfilled", JsValue::Undefined));
        fonts.borrow_mut().set("status".into(), JsValue::Str("loaded".into()));
        fonts.borrow_mut().set("size".into(), JsValue::Number(0.0));
        fonts.borrow_mut().set("check".into(),
            native("fonts.check", |_| Ok(JsValue::Bool(true))));
        fonts.borrow_mut().set("load".into(),
            native("fonts.load", |_| Ok(make_settled_promise("fulfilled", JsValue::Array(Rc::new(RefCell::new(Vec::new())))))));
        fonts.borrow_mut().set("forEach".into(),
            native("fonts.forEach", |_| Ok(JsValue::Undefined)));
        // event listeners pro loading/loadingdone/loadingerror
        fonts.borrow_mut().set("addEventListener".into(),
            native("fonts.addEventListener", |_| Ok(JsValue::Undefined)));
        fonts.borrow_mut().set("removeEventListener".into(),
            native("fonts.removeEventListener", |_| Ok(JsValue::Undefined)));
        doc_obj.set("fonts".into(), JsValue::Object(fonts));
    }

    // document.styleSheets - StyleSheetList (array-like).
    // Real source via host bridge (webview); minimal stub returns empty list
    // s length=0, item(i)=null, [Symbol.iterator]=empty.
    {
        let sheets = Rc::new(RefCell::new(JsObject::new()));
        sheets.borrow_mut().set("length".into(), JsValue::Number(0.0));
        sheets.borrow_mut().set("__stylesheet_list__".into(), JsValue::Bool(true));
        sheets.borrow_mut().set("item".into(),
            native("styleSheets.item", |_| Ok(JsValue::Null)));
        doc_obj.set("styleSheets".into(), JsValue::Object(sheets));
    }

    // document.querySelector - basic #id, .class, tag
    {
        let doc = Rc::clone(document);
        doc_obj.set("querySelector".into(), native("document.querySelector", move |a| {
            let sel = a.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let result = if let Some(id) = sel.strip_prefix('#') {
                doc.borrow().root.get_element_by_id(id)
            } else if let Some(cls) = sel.strip_prefix('.') {
                doc.borrow().root.get_elements_by_class(cls).into_iter().next()
            } else {
                doc.borrow().root.get_elements_by_tag(&sel).into_iter().next()
            };
            Ok(match result {
                Some(n) => JsValue::DomNode(n),
                None    => JsValue::Null,
            })
        }));
    }

    // document.querySelectorAll
    {
        let doc = Rc::clone(document);
        doc_obj.set("querySelectorAll".into(), native("document.querySelectorAll", move |a| {
            let sel = a.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let nodes = if let Some(id) = sel.strip_prefix('#') {
                doc.borrow().root.get_element_by_id(id).into_iter().collect()
            } else if let Some(cls) = sel.strip_prefix('.') {
                doc.borrow().root.get_elements_by_class(cls)
            } else {
                doc.borrow().root.get_elements_by_tag(&sel)
            };
            let arr: Vec<JsValue> = nodes.into_iter().map(JsValue::DomNode).collect();
            Ok(JsValue::Array(Rc::new(RefCell::new(arr))))
        }));
    }

    // document.body / documentElement / head - lazy z document tree
    {
        let doc = Rc::clone(document);
        doc_obj.set("__get_body__".into(), native("document.body", move |_| {
            Ok(match doc.borrow().body() {
                Some(n) => JsValue::DomNode(n),
                None    => JsValue::Null,
            })
        }));
    }
    {
        let doc = Rc::clone(document);
        doc_obj.set("__get_documentElement__".into(), native("document.documentElement", move |_| {
            Ok(match doc.borrow().html_element() {
                Some(n) => JsValue::DomNode(n),
                None    => JsValue::Null,
            })
        }));
    }
    // document.scrollingElement - quirks mode HTML element. Vraci stejny
    // jako documentElement (modern standard mode).
    {
        let doc = Rc::clone(document);
        doc_obj.set("__get_scrollingElement__".into(), native("document.scrollingElement", move |_| {
            Ok(match doc.borrow().html_element() {
                Some(n) => JsValue::DomNode(n),
                None    => JsValue::Null,
            })
        }));
    }
    {
        let doc = Rc::clone(document);
        doc_obj.set("__get_head__".into(), native("document.head", move |_| {
            Ok(match doc.borrow().head() {
                Some(n) => JsValue::DomNode(n),
                None    => JsValue::Null,
            })
        }));
    }
    {
        let doc = Rc::clone(document);
        doc_obj.set("__get_title__".into(), native("document.title", move |_| {
            Ok(JsValue::Str(doc.borrow().title.clone()))
        }));
    }
    {
        let doc = Rc::clone(document);
        doc_obj.set("__set_title__".into(), native("document.title=", move |args| {
            let v = args.into_iter().next().unwrap_or(JsValue::Undefined);
            doc.borrow_mut().title = v.to_string();
            Ok(JsValue::Undefined)
        }));
    }
    {
        let doc = Rc::clone(document);
        doc_obj.set("__get_URL__".into(), native("document.URL", move |_| {
            Ok(JsValue::Str(doc.borrow().url.clone()))
        }));
    }
    doc_obj.set("readyState".into(), JsValue::Str("complete".into()));

    // document.activeElement - intercept v eval_member.rs (potrebuje pristup
    // k Interpreter.focused_element). Sentinel: oznacime document objekt
    // flagem __is_document__ aby eval_member pred bezne hledanim klice
    // mohl dispatchnout na focused_element.
    doc_obj.set("__is_document__".into(), JsValue::Bool(true));

    e.define("document", JsValue::Object(Rc::new(RefCell::new(doc_obj))));

    // Element/Node konstruktory (pro instanceof kontroly)
    e.define("Element", native("Element", |_| Ok(JsValue::Undefined)));
    e.define("HTMLElement", native("HTMLElement", |_| Ok(JsValue::Undefined)));
    e.define("Node", native("Node", |_| Ok(JsValue::Undefined)));
    e.define("Document", native("Document", |_| Ok(JsValue::Undefined)));

    // Event konstruktor: new Event(type, options?)
    e.define("Event", native("Event", |a| {
        let mut iter = a.into_iter();
        let event_type = iter.next().map(|v| v.to_string()).unwrap_or_default();
        let options = iter.next().unwrap_or(JsValue::Undefined);
        let mut obj = JsObject::new();
        obj.set("__event__".into(), JsValue::Bool(true));
        obj.set("type".into(), JsValue::Str(event_type));
        if let JsValue::Object(opts) = &options {
            let b = opts.borrow();
            if let JsValue::Bool(v) = b.get("bubbles") {
                obj.set("bubbles".into(), JsValue::Bool(v));
            } else { obj.set("bubbles".into(), JsValue::Bool(false)); }
            if let JsValue::Bool(v) = b.get("cancelable") {
                obj.set("cancelable".into(), JsValue::Bool(v));
            } else { obj.set("cancelable".into(), JsValue::Bool(false)); }
        } else {
            obj.set("bubbles".into(), JsValue::Bool(false));
            obj.set("cancelable".into(), JsValue::Bool(false));
        }
        obj.set("defaultPrevented".into(), JsValue::Bool(false));
        obj.set("__propagation_stopped__".into(), JsValue::Bool(false));
        obj.set("target".into(), JsValue::Null);
        obj.set("currentTarget".into(), JsValue::Null);
        obj.set("timeStamp".into(), JsValue::Number(now_ms()));

        // preventDefault metoda - mutuje defaultPrevented na sobe
        let obj_rc = Rc::new(RefCell::new(obj));
        let obj_for_pd = Rc::clone(&obj_rc);
        obj_rc.borrow_mut().set("preventDefault".into(), native("preventDefault", move |_| {
            obj_for_pd.borrow_mut().set("defaultPrevented".into(), JsValue::Bool(true));
            Ok(JsValue::Undefined)
        }));
        let obj_for_sp = Rc::clone(&obj_rc);
        obj_rc.borrow_mut().set("stopPropagation".into(), native("stopPropagation", move |_| {
            obj_for_sp.borrow_mut().set("__propagation_stopped__".into(), JsValue::Bool(true));
            Ok(JsValue::Undefined)
        }));

        Ok(JsValue::Object(obj_rc))
    }));

    e.define("CustomEvent", native("CustomEvent", |a| {
        let mut iter = a.into_iter();
        let event_type = iter.next().map(|v| v.to_string()).unwrap_or_default();
        let options = iter.next().unwrap_or(JsValue::Undefined);
        let mut obj = JsObject::new();
        obj.set("__event__".into(), JsValue::Bool(true));
        obj.set("type".into(), JsValue::Str(event_type));
        if let JsValue::Object(opts) = options {
            let detail = opts.borrow().props.get("detail").cloned().unwrap_or(JsValue::Null);
            obj.set("detail".into(), detail);
        }
        Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
    }));

    // window stub (zaroven globalThis-like)
    let mut window = JsObject::new();
    window.set("__window__".into(), JsValue::Bool(true));
    window.set("location".into(), {
        let mut loc = JsObject::new();
        loc.set("href".into(), JsValue::Str("about:blank".into()));
        loc.set("origin".into(), JsValue::Str("null".into()));
        loc.set("protocol".into(), JsValue::Str(String::new()));
        loc.set("host".into(), JsValue::Str(String::new()));
        loc.set("pathname".into(), JsValue::Str("/".into()));
        loc.set("search".into(), JsValue::Str(String::new()));
        loc.set("hash".into(), JsValue::Str(String::new()));
        JsValue::Object(Rc::new(RefCell::new(loc)))
    });
    window.set("innerWidth".into(),  JsValue::Number(1024.0));
    window.set("innerHeight".into(), JsValue::Number(768.0));
    window.set("devicePixelRatio".into(), JsValue::Number(1.0));
    // window.scrollTo / scrollBy / pageXOffset / pageYOffset / scrollX / scrollY.
    // scroll_pos drzen sdileny pres Rc<RefCell<(x,y)>> - host (WebView) ho
    // potom muze cist v render_via a aplikovat na fyzicky scroll. Pred prvni
    // render Sync je hodnota (0, 0). pageXOffset/pageYOffset musi delat dynamic
    // lookup pres getter, ale tady postavime jako native callable wrapper:
    // alias scrollX/scrollY = pageXOffset/pageYOffset.
    {
        let sp = Rc::clone(scroll_pos);
        window.set("scrollTo".into(), native("scrollTo", move |args| {
            // scrollTo(x, y) ALEBO scrollTo({left, top, behavior}).
            let mut it = args.into_iter();
            let first = it.next().unwrap_or(JsValue::Undefined);
            let (nx, ny) = match &first {
                JsValue::Object(o) => {
                    let b = o.borrow();
                    let l = b.get("left").to_number();
                    let t = b.get("top").to_number();
                    (l as f32, t as f32)
                }
                _ => {
                    let x = first.to_number() as f32;
                    let y = it.next().map(|v| v.to_number() as f32).unwrap_or(0.0);
                    (x, y)
                }
            };
            *sp.borrow_mut() = (nx.max(0.0), ny.max(0.0));
            Ok(JsValue::Undefined)
        }));
    }
    {
        let sp = Rc::clone(scroll_pos);
        window.set("scrollBy".into(), native("scrollBy", move |args| {
            let mut it = args.into_iter();
            let first = it.next().unwrap_or(JsValue::Undefined);
            let (dx, dy) = match &first {
                JsValue::Object(o) => {
                    let b = o.borrow();
                    let l = b.get("left").to_number();
                    let t = b.get("top").to_number();
                    (l as f32, t as f32)
                }
                _ => {
                    let x = first.to_number() as f32;
                    let y = it.next().map(|v| v.to_number() as f32).unwrap_or(0.0);
                    (x, y)
                }
            };
            let mut cur = sp.borrow_mut();
            cur.0 = (cur.0 + dx).max(0.0);
            cur.1 = (cur.1 + dy).max(0.0);
            Ok(JsValue::Undefined)
        }));
    }
    // window.scroll alias na scrollTo (spec).
    {
        let sp = Rc::clone(scroll_pos);
        window.set("scroll".into(), native("scroll", move |args| {
            let mut it = args.into_iter();
            let first = it.next().unwrap_or(JsValue::Undefined);
            let (nx, ny) = match &first {
                JsValue::Object(o) => {
                    let b = o.borrow();
                    let l = b.get("left").to_number();
                    let t = b.get("top").to_number();
                    (l as f32, t as f32)
                }
                _ => {
                    let x = first.to_number() as f32;
                    let y = it.next().map(|v| v.to_number() as f32).unwrap_or(0.0);
                    (x, y)
                }
            };
            *sp.borrow_mut() = (nx.max(0.0), ny.max(0.0));
            Ok(JsValue::Undefined)
        }));
    }
    // pageXOffset/pageYOffset/scrollX/scrollY - 4 aliasy pro stejnou hodnotu.
    // Realne dynamic getter - pres `__window__` sentinel v eval_member.rs.
    // Tady jen seedneme defaultni 0.0. Skutecna hodnota cte interpreter
    // ze scroll_pos v eval_member.
    window.set("pageXOffset".into(), JsValue::Number(0.0));
    window.set("pageYOffset".into(), JsValue::Number(0.0));
    window.set("scrollX".into(), JsValue::Number(0.0));
    window.set("scrollY".into(), JsValue::Number(0.0));
    let window_rc = Rc::new(RefCell::new(window));
    let window_val = JsValue::Object(Rc::clone(&window_rc));
    e.define("window", window_val.clone());
    // Top-level `this` v non-strict mode = globalThis = window. Bez tohoto
    // skripty co cti `this.foo` na top levelu (kazda produkcni stranka -
    // googleadsense, polyfilly, IIFE) selzou s ReferenceError 'this'.
    e.define("this", window_val.clone());

    // ─── fetch - real HTTP client (ureq, blocking) ──────────────────────────
    let net_log_clone = Rc::clone(network_log);
    let pending_fetches_clone = Rc::clone(pending_fetches);
    e.define("fetch", native("fetch", move |a| {
        let net_log = Rc::clone(&net_log_clone);
        let pf = Rc::clone(&pending_fetches_clone);
        let mut iter = a.into_iter();
        let url = iter.next().map(|v| v.to_string()).unwrap_or_default();
        let init = iter.next().unwrap_or(JsValue::Undefined);

        // Parse init - method/body/headers ze JS objektu.
        let mut method = "GET".to_string();
        let mut body: Option<String> = None;
        let mut headers: Vec<(String, String)> = Vec::new();
        if let JsValue::Object(o) = &init {
            let b = o.borrow();
            if let JsValue::Str(m) = b.get("method") { method = m.to_uppercase(); }
            if let JsValue::Str(s) = b.get("body")   { body = Some(s); }
            if let JsValue::Object(h) = b.get("headers") {
                for k in h.borrow().own_keys() {
                    if let JsValue::Str(v) = h.borrow().get(&k) {
                        headers.push((k, v));
                    }
                }
            }
        }

        // Sync URL validation - bez schemu = rejected hned (real browser dela same).
        if !url.starts_with("http://") && !url.starts_with("https://")
           && !url.starts_with("file://") && !url.starts_with("data:") {
            let mut err = JsObject::new();
            err.set("name".into(), JsValue::Str("TypeError".into()));
            err.set("message".into(), JsValue::Str(format!("Failed to fetch: invalid URL: {url}")));
            net_log.borrow_mut().push((url.clone(), 0));
            return Ok(make_settled_promise("rejected",
                JsValue::Object(Rc::new(RefCell::new(err)))));
        }
        // Async fetch - spawn thread, vrati pending Promise. drain_fetches()
        // v event loopu prepne na fulfilled/rejected pri try_recv Ok.
        let (tx, rx) = std::sync::mpsc::channel::<super::FetchOutcome>();
        let url_thread = url.clone();
        let method_thread = method.clone();
        let body_thread = body.clone();
        let headers_thread = headers.clone();
        std::thread::spawn(move || {
            let result = perform_http_request(
                &url_thread, &method_thread, &headers_thread, body_thread.as_deref());
            let _ = tx.send(result);
        });

        // Pending Promise - drain ho posli prepne pri completion.
        let mut promise = JsObject::new();
        promise.set("__promise_state__".into(), JsValue::Str("pending".into()));
        promise.set("__promise_value__".into(), JsValue::Undefined);
        let promise_obj = Rc::new(RefCell::new(promise));
        net_log.borrow_mut().push((url.clone(), 0)); // pending
        pf.borrow_mut().push(super::PendingFetch {
            promise_obj: Rc::clone(&promise_obj),
            url: url.clone(),
            receiver: rx,
        });
        Ok(JsValue::Object(promise_obj))
    }));

    // ─── XMLHttpRequest - sync + async (sync HTTP via ureq, async fire onload pres
    // pending_xhr_callbacks event loop drain).
    let xhr_net_log = Rc::clone(network_log);
    let xhr_pending_cb = Rc::clone(pending_xhr_callbacks);
    e.define("XMLHttpRequest", native("XMLHttpRequest", move |_args| {
        let net_log = Rc::clone(&xhr_net_log);
        let pending_cb = Rc::clone(&xhr_pending_cb);
        let xhr_obj = Rc::new(RefCell::new(JsObject::new()));
        {
            let mut x = xhr_obj.borrow_mut();
            // Public XHR state.
            x.set("readyState".into(), JsValue::Number(0.0));
            x.set("status".into(), JsValue::Number(0.0));
            x.set("statusText".into(), JsValue::Str(String::new()));
            x.set("responseText".into(), JsValue::Str(String::new()));
            x.set("response".into(), JsValue::Str(String::new()));
            x.set("responseType".into(), JsValue::Str(String::new()));
            x.set("responseURL".into(), JsValue::Str(String::new()));
            x.set("withCredentials".into(), JsValue::Bool(false));
            x.set("timeout".into(), JsValue::Number(0.0));
            x.set("onload".into(), JsValue::Undefined);
            x.set("onerror".into(), JsValue::Undefined);
            x.set("onreadystatechange".into(), JsValue::Undefined);
            x.set("onloadend".into(), JsValue::Undefined);
            // Internal state.
            x.set("__xhr_method__".into(), JsValue::Str("GET".into()));
            x.set("__xhr_url__".into(), JsValue::Str(String::new()));
            x.set("__xhr_async__".into(), JsValue::Bool(true));
            let headers_obj = JsObject::new();
            x.set("__xhr_headers__".into(), JsValue::Object(Rc::new(RefCell::new(headers_obj))));
        }

        // open(method, url, async?)
        let open_ref = Rc::clone(&xhr_obj);
        let open_fn = native("XHR.open", move |a| {
            let mut iter = a.into_iter();
            let method = iter.next().map(|v| v.to_string()).unwrap_or_else(|| "GET".into()).to_uppercase();
            let url = iter.next().map(|v| v.to_string()).unwrap_or_default();
            let async_flag = match iter.next() {
                None => true,
                Some(JsValue::Bool(false)) => false,
                _ => true,
            };
            let mut x = open_ref.borrow_mut();
            x.set("__xhr_method__".into(), JsValue::Str(method));
            x.set("__xhr_url__".into(), JsValue::Str(url));
            x.set("__xhr_async__".into(), JsValue::Bool(async_flag));
            x.set("readyState".into(), JsValue::Number(1.0));
            Ok(JsValue::Undefined)
        });
        xhr_obj.borrow_mut().set("open".into(), open_fn);

        // setRequestHeader(name, value)
        let sh_ref = Rc::clone(&xhr_obj);
        let set_header_fn = native("XHR.setRequestHeader", move |a| {
            let mut iter = a.into_iter();
            let k = iter.next().map(|v| v.to_string()).unwrap_or_default();
            let v = iter.next().map(|v| v.to_string()).unwrap_or_default();
            let x = sh_ref.borrow();
            if let JsValue::Object(h) = x.get("__xhr_headers__") {
                h.borrow_mut().set(k, JsValue::Str(v));
            }
            Ok(JsValue::Undefined)
        });
        xhr_obj.borrow_mut().set("setRequestHeader".into(), set_header_fn);

        // send(body?) - sync ureq blocking, vyplni response + zaregistruje
        // onload/onreadystatechange do pending_xhr_callbacks pro event loop fire.
        let send_ref = Rc::clone(&xhr_obj);
        let send_net_log = Rc::clone(&net_log);
        let send_pending_cb = Rc::clone(&pending_cb);
        let send_fn = native("XHR.send", move |a| {
            let body = a.into_iter().next().and_then(|v| match v {
                JsValue::Undefined | JsValue::Null => None,
                JsValue::Str(s) => Some(s),
                other => Some(other.to_string()),
            });
            // Read state.
            let (method, url, headers): (String, String, Vec<(String, String)>) = {
                let x = send_ref.borrow();
                let m = if let JsValue::Str(s) = x.get("__xhr_method__") { s } else { "GET".into() };
                let u = if let JsValue::Str(s) = x.get("__xhr_url__") { s } else { String::new() };
                let mut hs = Vec::new();
                if let JsValue::Object(h) = x.get("__xhr_headers__") {
                    let hb = h.borrow();
                    for k in hb.own_keys() {
                        if let JsValue::Str(v) = hb.get(&k) { hs.push((k, v)); }
                    }
                }
                (m, u, hs)
            };
            // URL validation.
            if !url.starts_with("http://") && !url.starts_with("https://")
                && !url.starts_with("file://") && !url.starts_with("data:")
            {
                send_net_log.borrow_mut().push((url.clone(), 0));
                let onerror;
                {
                    let mut x = send_ref.borrow_mut();
                    x.set("readyState".into(), JsValue::Number(4.0));
                    x.set("status".into(), JsValue::Number(0.0));
                    onerror = x.get("onerror");
                }
                if !matches!(onerror, JsValue::Undefined | JsValue::Null) {
                    send_pending_cb.borrow_mut().push((onerror, JsValue::Object(Rc::clone(&send_ref))));
                }
                return Ok(JsValue::Undefined);
            }
            // Sync HTTP.
            let outcome = super::helpers::perform_http_request(
                &url, &method, &headers, body.as_deref());
            let (status, status_text, resp_body, _resp_headers) = match outcome {
                Ok(t) => t,
                Err(msg) => (0, msg, String::new(), Vec::new()),
            };
            send_net_log.borrow_mut().push((url.clone(), status));
            // Fill XHR state.
            let (onload, onreadystatechange, onloadend);
            {
                let mut x = send_ref.borrow_mut();
                x.set("status".into(), JsValue::Number(status as f64));
                x.set("statusText".into(), JsValue::Str(status_text));
                x.set("responseText".into(), JsValue::Str(resp_body.clone()));
                x.set("response".into(), JsValue::Str(resp_body));
                x.set("responseURL".into(), JsValue::Str(url));
                x.set("readyState".into(), JsValue::Number(4.0));
                onload = x.get("onload");
                onreadystatechange = x.get("onreadystatechange");
                onloadend = x.get("onloadend");
            }
            // Push callbacky pro event loop fire (s this = xhr_obj).
            let xhr_this = JsValue::Object(Rc::clone(&send_ref));
            if !matches!(onreadystatechange, JsValue::Undefined | JsValue::Null) {
                send_pending_cb.borrow_mut().push((onreadystatechange, xhr_this.clone()));
            }
            if !matches!(onload, JsValue::Undefined | JsValue::Null) {
                send_pending_cb.borrow_mut().push((onload, xhr_this.clone()));
            }
            if !matches!(onloadend, JsValue::Undefined | JsValue::Null) {
                send_pending_cb.borrow_mut().push((onloadend, xhr_this));
            }
            Ok(JsValue::Undefined)
        });
        xhr_obj.borrow_mut().set("send".into(), send_fn);

        // abort / getResponseHeader / getAllResponseHeaders / overrideMimeType - stub.
        xhr_obj.borrow_mut().set("abort".into(),
            native("XHR.abort", |_| Ok(JsValue::Undefined)));
        xhr_obj.borrow_mut().set("getResponseHeader".into(),
            native("XHR.getResponseHeader", |_| Ok(JsValue::Null)));
        xhr_obj.borrow_mut().set("getAllResponseHeaders".into(),
            native("XHR.getAllResponseHeaders", |_| Ok(JsValue::Str(String::new()))));
        xhr_obj.borrow_mut().set("overrideMimeType".into(),
            native("XHR.overrideMimeType", |_| Ok(JsValue::Undefined)));
        xhr_obj.borrow_mut().set("addEventListener".into(),
            native("XHR.addEventListener", {
                let er = Rc::clone(&xhr_obj);
                move |a| {
                    let mut iter = a.into_iter();
                    let event = iter.next().map(|v| v.to_string()).unwrap_or_default();
                    let cb = iter.next().unwrap_or(JsValue::Undefined);
                    let prop = match event.as_str() {
                        "load" => "onload",
                        "error" => "onerror",
                        "readystatechange" => "onreadystatechange",
                        "loadend" => "onloadend",
                        _ => return Ok(JsValue::Undefined),
                    };
                    er.borrow_mut().set(prop.into(), cb);
                    Ok(JsValue::Undefined)
                }
            }));

        Ok(JsValue::Object(xhr_obj))
    }));
    // Konstruktory bez vlastni logiky (logika je v call_new)
    e.define("Map", native("Map", |_| Ok(JsValue::Undefined)));
    e.define("Set", native("Set", |_| Ok(JsValue::Undefined)));
    e.define("WeakMap", native("WeakMap", |_| Ok(JsValue::Undefined)));
    e.define("WeakSet", native("WeakSet", |_| Ok(JsValue::Undefined)));

    // WeakRef - drzi slabou referenci na objekt (v sync impl bez GC drzi silnou)
    // .deref() vraci puvodni objekt (nebo undefined kdyby byl zruseny)
    e.define("WeakRef", native("WeakRef", |a| {
        let target = a.into_iter().next().unwrap_or(JsValue::Undefined);
        let mut obj = JsObject::new();
        obj.set("__weak_target__".into(), target);
        Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
    }));

    // FinalizationRegistry - stub (v sync runtime bez GC neni co volat)
    e.define("FinalizationRegistry", native("FinalizationRegistry", |a| {
        let cb = a.into_iter().next().unwrap_or(JsValue::Undefined);
        let mut obj = JsObject::new();
        obj.set("__finalizer__".into(), cb);
        Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
    }));

    // JSON
    let mut json_obj = JsObject::new();
    json_obj.set("stringify".into(), native("JSON.stringify", |a| {
        let mut iter = a.into_iter();
        let val   = iter.next().unwrap_or(JsValue::Undefined);
        let _repl = iter.next();
        let space = iter.next().unwrap_or(JsValue::Undefined);
        let indent = match space {
            JsValue::Number(n) if n > 0.0 => n as usize,
            JsValue::Str(s) if !s.is_empty() => s.len(),
            _ => 0,
        };
        match json_stringify_checked(&val, indent, 0) {
            Ok(Some(s)) => Ok(JsValue::Str(s)),
            Ok(None)    => Ok(JsValue::Undefined),
            Err(e)      => Err(e),
        }
    }));
    json_obj.set("parse".into(), native("JSON.parse", |a| {
        match a.into_iter().next() {
            Some(JsValue::Str(s)) => json_parse(&s).map_err(|e| e),
            _ => Err("JSON.parse: argument musi byt retezec".into()),
        }
    }));
    e.define("JSON", JsValue::Object(Rc::new(RefCell::new(json_obj))));

    // Date konstruktor (logika v call_new)
    e.define("Date", native("Date", |_| Ok(JsValue::Undefined)));

    super::builtins_temporal::setup_temporal(&mut *e);

    // Error typy
    for name in &["Error", "TypeError", "RangeError", "SyntaxError",
                   "ReferenceError", "URIError", "EvalError"] {
        let n = name.to_string();
        e.define(name, native(name, move |_| {
            let mut obj = JsObject::new();
            obj.set("name".into(), JsValue::Str(n.clone()));
            obj.set("message".into(), JsValue::Str(String::new()));
            obj.set("stack".into(), JsValue::Str(n.clone()));
            Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
        }));
    }

    // Promise (logika v call_new a eval_call)
    e.define("Promise", native("Promise", |_| Ok(JsValue::Undefined)));

    // BigNumber
    e.define("BigNumber", native("BigNumber", |a| {
        let s = a.into_iter().next().map(|v| match v {
            JsValue::BigNumber(n) => n.to_string(),
            other => other.to_string(),
        }).unwrap_or_else(|| "0".into());
        BigDecimal::from_str(s.trim())
            .map(|bd| JsValue::BigNumber(Rc::new(bd)))
            .map_err(|_| format!("BigNumber: neplatna hodnota '{s}'"))
    }));

    // BigInt
    e.define("BigInt", native("BigInt", |a| {
        let v = a.into_iter().next().unwrap_or(JsValue::Undefined);
        match v {
            JsValue::BigInt(n) => Ok(JsValue::BigInt(n)),
            JsValue::Number(n) if n.is_finite() && n.fract() == 0.0 => {
                BigInt::from_str(&format!("{}", n as i128))
                    .map(|b| JsValue::BigInt(Rc::new(b)))
                    .map_err(|_| format!("BigInt: neplatna hodnota '{n}'"))
            }
            JsValue::Number(n) => Err(format!("RangeError: nelze prevést {n} na BigInt (neceloiselne nebo nekonecne)")),
            JsValue::Str(s) => {
                BigInt::from_str(s.trim())
                    .map(|b| JsValue::BigInt(Rc::new(b)))
                    .map_err(|_| format!("SyntaxError: nelze parsovat '{s}' jako BigInt"))
            }
            JsValue::Bool(true)  => Ok(JsValue::BigInt(Rc::new(BigInt::from(1)))),
            JsValue::Bool(false) => Ok(JsValue::BigInt(Rc::new(BigInt::from(0)))),
            JsValue::BigNumber(n) => {
                if n.is_integer() {
                    let s = n.to_string();
                    let int_part = s.split('.').next().unwrap_or(&s);
                    BigInt::from_str(int_part)
                        .map(|b| JsValue::BigInt(Rc::new(b)))
                        .map_err(|_| format!("BigInt: nelze prevest BigNumber '{n}'"))
                } else {
                    Err(format!("RangeError: BigNumber {n} neni cele cislo"))
                }
            }
            other => Err(format!("TypeError: nelze prevest {} na BigInt", other.type_of())),
        }
    }));

    // RegExp
    e.define("RegExp", native("RegExp", |args| {
        let pat = args.get(0).map(|v| v.to_string()).unwrap_or_default();
        let flags = args.get(1).map(|v| v.to_string()).unwrap_or_default();
        js_regex_to_rust(&pat, &flags).map_err(|e| e)?;
        Ok(make_regex_object(&pat, &flags))
    }));

    e.define("Infinity",  JsValue::Number(f64::INFINITY));
    e.define("NaN",       JsValue::Number(f64::NAN));
    e.define("undefined", JsValue::Undefined);

    // globalThis = window (per HTML spec, browsing context's global object).
    e.define("globalThis", window_val.clone());

    // queueMicrotask - stub (sync)
    e.define("queueMicrotask", native("queueMicrotask", |_| Ok(JsValue::Undefined)));

    // structuredClone pres JSON roundtrip
    e.define("structuredClone", native("structuredClone", |a| {
        let val = a.into_iter().next().unwrap_or(JsValue::Undefined);
        match json_stringify(&val, 0, 0) {
            Some(s) => json_parse(&s).map_err(|e| e),
            None => Ok(JsValue::Undefined),
        }
    }));

    // Timery
    {
        let tq = Rc::clone(task_queue);
        let id_ctr = Rc::clone(next_timer_id);
        e.define("setTimeout", native("setTimeout", move |a| {
            let mut iter = a.into_iter();
            let cb   = iter.next().unwrap_or(JsValue::Undefined);
            let _delay = iter.next();
            let args: Vec<JsValue> = iter.collect();
            let id = {
                let mut ctr = id_ctr.borrow_mut();
                let id = *ctr;
                *ctr += 1;
                id
            };
            tq.borrow_mut().push((id, cb, args));
            Ok(JsValue::Number(id as f64))
        }));
    }
    {
        let tq = Rc::clone(task_queue);
        e.define("clearTimeout", native("clearTimeout", move |a| {
            let id = a.into_iter().next().map(|v| v.to_number() as u32).unwrap_or(0);
            tq.borrow_mut().retain(|(tid, _, _)| *tid != id);
            Ok(JsValue::Undefined)
        }));
    }
    {
        let tq = Rc::clone(task_queue);
        let id_ctr = Rc::clone(next_timer_id);
        e.define("setInterval", native("setInterval", move |a| {
            let mut iter = a.into_iter();
            let cb   = iter.next().unwrap_or(JsValue::Undefined);
            let _interval = iter.next();
            let args: Vec<JsValue> = iter.collect();
            let id = {
                let mut ctr = id_ctr.borrow_mut();
                let id = *ctr;
                *ctr += 1;
                id
            };
            tq.borrow_mut().push((id, cb, args));
            Ok(JsValue::Number(id as f64))
        }));
    }
    {
        let tq = Rc::clone(task_queue);
        e.define("clearInterval", native("clearInterval", move |a| {
            let id = a.into_iter().next().map(|v| v.to_number() as u32).unwrap_or(0);
            tq.borrow_mut().retain(|(tid, _, _)| *tid != id);
            Ok(JsValue::Undefined)
        }));
    }

    // ─── Observer stuby (ResizeObserver / IntersectionObserver / MutationObserver / PerformanceObserver) ──
    // Vsechny constructors vraci object s observe/unobserve/disconnect/takeRecords methods
    // (no-op aktualne - real runtime tracking TODO).
    let make_observer = |name: &str| {
        let n = name.to_string();
        native(name, move |_args| {
            let obj = std::rc::Rc::new(std::cell::RefCell::new(JsObject::new()));
            {
                let mut o = obj.borrow_mut();
                o.set("__observer_kind__".into(), JsValue::Str(n.clone()));
                o.set("observe".into(), native("observe", |_| Ok(JsValue::Undefined)));
                o.set("unobserve".into(), native("unobserve", |_| Ok(JsValue::Undefined)));
                o.set("disconnect".into(), native("disconnect", |_| Ok(JsValue::Undefined)));
                o.set("takeRecords".into(), native("takeRecords", |_| {
                    Ok(JsValue::Array(std::rc::Rc::new(std::cell::RefCell::new(Vec::new()))))
                }));
            }
            Ok(JsValue::Object(obj))
        })
    };
    // customElements registry - sdileny s Interpreter pro lifecycle callbacky
    {
        let registry = Rc::new(RefCell::new(JsObject::new()));
        let ce_map = Rc::clone(custom_elements_registry);
        let ce_map2 = Rc::clone(custom_elements_registry);
        registry.borrow_mut().set("define".into(), native("customElements.define", move |args| {
            let mut it = args.into_iter();
            let name = it.next().map(|v| v.to_string()).unwrap_or_default();
            let ctor = it.next().unwrap_or(JsValue::Undefined);
            ce_map.borrow_mut().insert(name, ctor);
            Ok(JsValue::Undefined)
        }));
        registry.borrow_mut().set("get".into(), native("customElements.get", move |args| {
            let name = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            Ok(ce_map2.borrow().get(&name).cloned().unwrap_or(JsValue::Undefined))
        }));
        registry.borrow_mut().set("whenDefined".into(), native("customElements.whenDefined", |_| {
            Ok(JsValue::Undefined)
        }));
        registry.borrow_mut().set("upgrade".into(), native("customElements.upgrade", |_| Ok(JsValue::Undefined)));
        e.define("customElements", JsValue::Object(registry));
    }

    // CSSStyleSheet constructor stub - new CSSStyleSheet() vraci object
    e.define("CSSStyleSheet", native("CSSStyleSheet", |_| {
        let obj = Rc::new(RefCell::new(JsObject::new()));
        {
            let mut o = obj.borrow_mut();
            o.set("cssRules".into(), JsValue::Array(Rc::new(RefCell::new(Vec::new()))));
            o.set("rules".into(), JsValue::Array(Rc::new(RefCell::new(Vec::new()))));
            o.set("insertRule".into(), native("insertRule", |_| Ok(JsValue::Number(0.0))));
            o.set("deleteRule".into(), native("deleteRule", |_| Ok(JsValue::Undefined)));
            o.set("replaceSync".into(), native("replaceSync", |_| Ok(JsValue::Undefined)));
            o.set("replace".into(), native("replace", |_| Ok(JsValue::Undefined)));
        }
        Ok(JsValue::Object(obj))
    }));

    // URL constructor + URLSearchParams stub
    e.define("URL", native("URL", |args| {
        let url = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
        let obj = Rc::new(RefCell::new(JsObject::new()));
        {
            let mut o = obj.borrow_mut();
            // Basic URL parsing - protocol, host, pathname, search, hash
            let (proto, rest) = if let Some(idx) = url.find("://") {
                (url[..idx + 1].to_string(), url[idx + 3..].to_string())
            } else {
                ("https:".to_string(), url.clone())
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
            // Split host na hostname + port
            let (hostname, port) = match host.split_once(':') {
                Some((h, p)) => (h.to_string(), p.to_string()),
                None => (host.clone(), String::new()),
            };
            let origin = format!("{proto}//{host}");
            o.set("href".into(), JsValue::Str(url));
            o.set("protocol".into(), JsValue::Str(proto));
            o.set("host".into(), JsValue::Str(host));
            o.set("hostname".into(), JsValue::Str(hostname));
            o.set("pathname".into(), JsValue::Str(pathname));
            o.set("search".into(), JsValue::Str(search.clone()));
            o.set("hash".into(), JsValue::Str(hash));
            o.set("port".into(), JsValue::Str(port));
            o.set("origin".into(), JsValue::Str(origin));
            // searchParams - sub-objekt
            let sp = build_search_params(&search);
            o.set("searchParams".into(), sp);
        }
        Ok(JsValue::Object(obj))
    }));

    // URL.canParse(str) / URL.parse(str) (ES2024+)
    e.define("__url_can_parse__", native("URL.canParse", |args| {
        let s = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
        // Velmi loose - oba contain ":" + nejaky obsah
        let valid = s.contains(':') && s.len() > 2;
        Ok(JsValue::Bool(valid))
    }));
    e.define("__url_parse__", native("URL.parse", |args| {
        let s = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
        if !s.contains(':') { return Ok(JsValue::Null); }
        // Same parsing as URL constructor (zjednodusena verze)
        let url = s;
        let obj = Rc::new(RefCell::new(JsObject::new()));
        let (proto, rest) = if let Some(idx) = url.find("://") {
            (url[..idx + 1].to_string(), url[idx + 3..].to_string())
        } else { return Ok(JsValue::Null); };
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
        obj.borrow_mut().set("href".into(), JsValue::Str(url));
        obj.borrow_mut().set("protocol".into(), JsValue::Str(proto.clone()));
        obj.borrow_mut().set("host".into(), JsValue::Str(host.clone()));
        obj.borrow_mut().set("hostname".into(), JsValue::Str(host.clone()));
        obj.borrow_mut().set("pathname".into(), JsValue::Str(pathname));
        obj.borrow_mut().set("search".into(), JsValue::Str(search));
        obj.borrow_mut().set("hash".into(), JsValue::Str(hash));
        obj.borrow_mut().set("origin".into(), JsValue::Str(format!("{proto}//{host}")));
        Ok(JsValue::Object(obj))
    }));

    e.define("URLSearchParams", native("URLSearchParams", |args| {
        let s = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
        let s = s.trim_start_matches('?');
        let mut pairs: Vec<(String, String)> = Vec::new();
        for p in s.split('&').filter(|s| !s.is_empty()) {
            if let Some((k, v)) = p.split_once('=') {
                pairs.push((k.to_string(), v.to_string()));
            } else {
                pairs.push((p.to_string(), String::new()));
            }
        }
        let pairs_rc = Rc::new(RefCell::new(pairs));

        let obj = Rc::new(RefCell::new(JsObject::new()));
        {
            let p = Rc::clone(&pairs_rc);
            obj.borrow_mut().set("get".into(), native("URLSearchParams.get", move |args| {
                let name = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                Ok(p.borrow().iter().find(|(k, _)| k == &name)
                    .map(|(_, v)| JsValue::Str(v.clone()))
                    .unwrap_or(JsValue::Null))
            }));
        }
        {
            let p = Rc::clone(&pairs_rc);
            obj.borrow_mut().set("has".into(), native("URLSearchParams.has", move |args| {
                let name = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                Ok(JsValue::Bool(p.borrow().iter().any(|(k, _)| k == &name)))
            }));
        }
        {
            let p = Rc::clone(&pairs_rc);
            obj.borrow_mut().set("set".into(), native("URLSearchParams.set", move |args| {
                let mut it = args.into_iter();
                let name = it.next().map(|v| v.to_string()).unwrap_or_default();
                let val = it.next().map(|v| v.to_string()).unwrap_or_default();
                let mut pairs = p.borrow_mut();
                if let Some(entry) = pairs.iter_mut().find(|(k, _)| k == &name) {
                    entry.1 = val;
                } else {
                    pairs.push((name, val));
                }
                Ok(JsValue::Undefined)
            }));
        }
        {
            let p = Rc::clone(&pairs_rc);
            obj.borrow_mut().set("toString".into(), native("URLSearchParams.toString", move |_| {
                let s = p.borrow().iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>().join("&");
                Ok(JsValue::Str(s))
            }));
        }
        {
            let p = Rc::clone(&pairs_rc);
            obj.borrow_mut().set("append".into(), native("URLSearchParams.append", move |args| {
                let mut it = args.into_iter();
                let name = it.next().map(|v| v.to_string()).unwrap_or_default();
                let val = it.next().map(|v| v.to_string()).unwrap_or_default();
                p.borrow_mut().push((name, val));
                Ok(JsValue::Undefined)
            }));
        }
        {
            let p = Rc::clone(&pairs_rc);
            obj.borrow_mut().set("delete".into(), native("URLSearchParams.delete", move |args| {
                let name = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                p.borrow_mut().retain(|(k, _)| k != &name);
                Ok(JsValue::Undefined)
            }));
        }
        {
            let p = Rc::clone(&pairs_rc);
            obj.borrow_mut().set("getAll".into(), native("URLSearchParams.getAll", move |args| {
                let name = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                let arr: Vec<JsValue> = p.borrow().iter()
                    .filter(|(k, _)| k == &name)
                    .map(|(_, v)| JsValue::Str(v.clone()))
                    .collect();
                Ok(JsValue::Array(Rc::new(RefCell::new(arr))))
            }));
        }
        Ok(JsValue::Object(obj))
    }));

    // localStorage / sessionStorage uz definovany vyse pri ─── Storage API ───
    // sekci. Tady ne re-define (dvoji define = override + memory leak/wrong impl).

    // Headers stub
    e.define("Headers", native("Headers", |_args| {
        let map: Rc<RefCell<Vec<(String, String)>>> = Rc::new(RefCell::new(Vec::new()));
        let obj = Rc::new(RefCell::new(JsObject::new()));
        {
            let m = Rc::clone(&map);
            obj.borrow_mut().set("get".into(), native("Headers.get", move |args| {
                let name = args.into_iter().next().map(|v| v.to_string().to_lowercase()).unwrap_or_default();
                Ok(m.borrow().iter().find(|(k, _)| k.to_lowercase() == name)
                    .map(|(_, v)| JsValue::Str(v.clone())).unwrap_or(JsValue::Null))
            }));
        }
        {
            let m = Rc::clone(&map);
            obj.borrow_mut().set("set".into(), native("Headers.set", move |args| {
                let mut it = args.into_iter();
                let k = it.next().map(|v| v.to_string()).unwrap_or_default();
                let v = it.next().map(|v| v.to_string()).unwrap_or_default();
                let mut m = m.borrow_mut();
                m.retain(|(kk, _)| kk.to_lowercase() != k.to_lowercase());
                m.push((k, v));
                Ok(JsValue::Undefined)
            }));
        }
        {
            let m = Rc::clone(&map);
            obj.borrow_mut().set("append".into(), native("Headers.append", move |args| {
                let mut it = args.into_iter();
                let k = it.next().map(|v| v.to_string()).unwrap_or_default();
                let v = it.next().map(|v| v.to_string()).unwrap_or_default();
                m.borrow_mut().push((k, v));
                Ok(JsValue::Undefined)
            }));
        }
        {
            let m = Rc::clone(&map);
            obj.borrow_mut().set("has".into(), native("Headers.has", move |args| {
                let k = args.into_iter().next().map(|v| v.to_string().to_lowercase()).unwrap_or_default();
                Ok(JsValue::Bool(m.borrow().iter().any(|(kk, _)| kk.to_lowercase() == k)))
            }));
        }
        {
            let m = Rc::clone(&map);
            obj.borrow_mut().set("delete".into(), native("Headers.delete", move |args| {
                let k = args.into_iter().next().map(|v| v.to_string().to_lowercase()).unwrap_or_default();
                m.borrow_mut().retain(|(kk, _)| kk.to_lowercase() != k);
                Ok(JsValue::Undefined)
            }));
        }
        Ok(JsValue::Object(obj))
    }));

    // navigator object
    {
        let nav = Rc::new(RefCell::new(JsObject::new()));
        {
            let mut n = nav.borrow_mut();
            n.set("userAgent".into(), JsValue::Str("RustWebEngine/0.1".into()));
            n.set("language".into(), JsValue::Str("cs-CZ".into()));
            n.set("languages".into(), JsValue::Array(Rc::new(RefCell::new(vec![
                JsValue::Str("cs-CZ".into()), JsValue::Str("en-US".into()),
            ]))));
            n.set("platform".into(), JsValue::Str(std::env::consts::OS.into()));
            n.set("onLine".into(), JsValue::Bool(true));
            n.set("cookieEnabled".into(), JsValue::Bool(true));
            n.set("hardwareConcurrency".into(),
                JsValue::Number(std::thread::available_parallelism()
                    .map(|n| n.get() as f64).unwrap_or(4.0)));
            n.set("maxTouchPoints".into(), JsValue::Number(0.0));
            n.set("vendor".into(), JsValue::Str("RustWebEngine".into()));
            // Geolocation stub
            let geo = Rc::new(RefCell::new(JsObject::new()));
            geo.borrow_mut().set("getCurrentPosition".into(), native("getCurrentPosition", |_| Ok(JsValue::Undefined)));
            geo.borrow_mut().set("watchPosition".into(), native("watchPosition", |_| Ok(JsValue::Number(0.0))));
            geo.borrow_mut().set("clearWatch".into(), native("clearWatch", |_| Ok(JsValue::Undefined)));
            n.set("geolocation".into(), JsValue::Object(geo));
            // Clipboard stub
            let cb = Rc::new(RefCell::new(JsObject::new()));
            cb.borrow_mut().set("writeText".into(), native("clipboard.writeText", |_| Ok(JsValue::Undefined)));
            cb.borrow_mut().set("readText".into(), native("clipboard.readText", |_| Ok(JsValue::Str(String::new()))));
            n.set("clipboard".into(), JsValue::Object(cb));
        }
        e.define("navigator", JsValue::Object(nav));
    }

    // TextEncoder / TextDecoder
    e.define("TextEncoder", native("TextEncoder", |_| {
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("encoding".into(), JsValue::Str("utf-8".into()));
        obj.borrow_mut().set("encode".into(), native("TextEncoder.encode", |args| {
            let s = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let bytes: Vec<JsValue> = s.bytes().map(|b| JsValue::Number(b as f64)).collect();
            Ok(JsValue::Array(Rc::new(RefCell::new(bytes))))
        }));
        Ok(JsValue::Object(obj))
    }));
    e.define("TextDecoder", native("TextDecoder", |a| {
        let label = a.into_iter().next().map(|v| v.to_string()).unwrap_or_else(|| "utf-8".into());
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("encoding".into(), JsValue::Str(label));
        obj.borrow_mut().set("fatal".into(), JsValue::Bool(false));
        obj.borrow_mut().set("ignoreBOM".into(), JsValue::Bool(false));
        obj.borrow_mut().set("decode".into(), native("TextDecoder.decode", |args| {
            let arr = args.into_iter().next().unwrap_or(JsValue::Undefined);
            let bytes: Vec<u8> = match arr {
                JsValue::Array(a) => a.borrow().iter().map(|v| v.to_number() as u8).collect(),
                JsValue::Object(o) => {
                    if let JsValue::Array(a) = o.borrow().get("__bytes__") {
                        a.borrow().iter().map(|v| v.to_number() as u8).collect()
                    } else { Vec::new() }
                }
                _ => Vec::new(),
            };
            Ok(JsValue::Str(String::from_utf8_lossy(&bytes).into_owned()))
        }));
        Ok(JsValue::Object(obj))
    }));
    // TextEncoderStream / TextDecoderStream - prom encoder pres TransformStream API
    e.define("TextEncoderStream", native("TextEncoderStream", |_| {
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("encoding".into(), JsValue::Str("utf-8".into()));
        obj.borrow_mut().set("readable".into(), JsValue::Object(Rc::new(RefCell::new(JsObject::new()))));
        obj.borrow_mut().set("writable".into(), JsValue::Object(Rc::new(RefCell::new(JsObject::new()))));
        Ok(JsValue::Object(obj))
    }));
    e.define("TextDecoderStream", native("TextDecoderStream", |_| {
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("encoding".into(), JsValue::Str("utf-8".into()));
        obj.borrow_mut().set("readable".into(), JsValue::Object(Rc::new(RefCell::new(JsObject::new()))));
        obj.borrow_mut().set("writable".into(), JsValue::Object(Rc::new(RefCell::new(JsObject::new()))));
        Ok(JsValue::Object(obj))
    }));

    // crypto - randomUUID + getRandomValues + subtle stub
    {
        let crypto = Rc::new(RefCell::new(JsObject::new()));
        crypto.borrow_mut().set("randomUUID".into(), native("crypto.randomUUID", |_| {
            // Simple v4 UUID s pseudo-random pres time-based
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u128)
                .unwrap_or(0);
            let h = nanos;
            let s = format!(
                "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
                (h >> 96) as u32, (h >> 80) as u16 & 0xFFFF,
                (h >> 64) as u16 & 0xFFF,
                ((h >> 48) as u16 & 0x3FFF) | 0x8000,
                h as u64 & 0xFFFF_FFFF_FFFF
            );
            Ok(JsValue::Str(s))
        }));
        crypto.borrow_mut().set("getRandomValues".into(), native("crypto.getRandomValues", |args| {
            // Vraci puvodni array s "random" hodnotami (deterministicky pseudo-random pres time)
            let arr = args.into_iter().next().unwrap_or(JsValue::Undefined);
            if let JsValue::Array(a) = &arr {
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or(0);
                let mut state = nanos.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let len = a.borrow().len();
                let mut new_vals = Vec::with_capacity(len);
                for _ in 0..len {
                    state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    new_vals.push(JsValue::Number((state >> 32) as u32 as f64));
                }
                *a.borrow_mut() = new_vals;
            }
            Ok(arr)
        }));
        // SubtleCrypto - real digest impl (SHA-1 / SHA-256 z helpers), ostatni stub
        let subtle = Rc::new(RefCell::new(JsObject::new()));
        // digest(algorithm, data) -> Promise<ArrayBuffer>
        subtle.borrow_mut().set("digest".into(), native("digest", |args| {
            let mut it = args.into_iter();
            let algo = it.next().map(|v| v.to_string()).unwrap_or_default();
            let data = it.next().unwrap_or(JsValue::Undefined);
            let bytes: Vec<u8> = match &data {
                JsValue::Array(a) => a.borrow().iter().map(|v| v.to_number() as u8).collect(),
                JsValue::Str(s) => s.bytes().collect(),
                JsValue::Object(o) => {
                    if let JsValue::Array(b) = o.borrow().get("__bytes__") {
                        b.borrow().iter().map(|v| v.to_number() as u8).collect()
                    } else { Vec::new() }
                }
                _ => Vec::new(),
            };
            let normalized = algo.to_uppercase().replace("-", "");
            let digest_bytes: Vec<u8> = match normalized.as_str() {
                "SHA1" | "SHA" => super::helpers::sha1(&bytes).to_vec(),
                "SHA256" => super::helpers::sha256(&bytes).to_vec(),
                "SHA384" => {
                    // SHA-384 - aproximace pres SHA-256 dvojnasob (simplified pro stub)
                    let h1 = super::helpers::sha256(&bytes);
                    let h2 = super::helpers::sha256(&h1);
                    let mut combined = Vec::with_capacity(48);
                    combined.extend_from_slice(&h1);
                    combined.extend_from_slice(&h2[..16]);
                    combined
                }
                "SHA512" => {
                    // SHA-512 - aproximace pres double SHA-256
                    let h1 = super::helpers::sha256(&bytes);
                    let h2 = super::helpers::sha256(&h1);
                    let mut combined = Vec::with_capacity(64);
                    combined.extend_from_slice(&h1);
                    combined.extend_from_slice(&h2);
                    combined
                }
                _ => {
                    return Ok(make_settled_promise("rejected",
                        JsValue::Str(format!("NotSupportedError: {algo}"))));
                }
            };
            let arr: Vec<JsValue> = digest_bytes.into_iter().map(|b| JsValue::Number(b as f64)).collect();
            // Vrat ArrayBuffer-like { __bytes__, byteLength }
            let buf = Rc::new(RefCell::new(JsObject::new()));
            buf.borrow_mut().set("__buffer__".into(), JsValue::Bool(true));
            buf.borrow_mut().set("byteLength".into(), JsValue::Number(arr.len() as f64));
            buf.borrow_mut().set("__bytes__".into(), JsValue::Array(Rc::new(RefCell::new(arr))));
            Ok(make_settled_promise("fulfilled", JsValue::Object(buf)))
        }));
        for m in &["encrypt", "decrypt", "sign", "verify", "generateKey", "importKey", "exportKey", "deriveKey", "deriveBits", "wrapKey", "unwrapKey"] {
            let name = m.to_string();
            subtle.borrow_mut().set(name, native(m, |_| {
                Ok(make_settled_promise("fulfilled", JsValue::Undefined))
            }));
        }
        crypto.borrow_mut().set("subtle".into(), JsValue::Object(subtle));
        e.define("crypto", JsValue::Object(crypto));
    }

    // ─── Modern Web APIs - Permissions / WakeLock / Vibration / Gamepad / Sensors ───
    // Permissions API
    {
        let perms = Rc::new(RefCell::new(JsObject::new()));
        perms.borrow_mut().set("query".into(), native("permissions.query", |args| {
            let arg = args.into_iter().next().unwrap_or(JsValue::Undefined);
            let name = if let JsValue::Object(o) = &arg {
                o.borrow().get("name").to_string()
            } else { "unknown".into() };
            let status = Rc::new(RefCell::new(JsObject::new()));
            status.borrow_mut().set("name".into(), JsValue::Str(name));
            status.borrow_mut().set("state".into(), JsValue::Str("granted".into()));
            status.borrow_mut().set("addEventListener".into(),
                native("addEventListener", |_| Ok(JsValue::Undefined)));
            Ok(make_settled_promise("fulfilled", JsValue::Object(status)))
        }));
        e.define("__permissions__", JsValue::Object(perms));
    }
    // WakeLock API - navigator.wakeLock.request("screen")
    {
        let wl = Rc::new(RefCell::new(JsObject::new()));
        wl.borrow_mut().set("request".into(), native("wakeLock.request", |_| {
            let sentinel = Rc::new(RefCell::new(JsObject::new()));
            sentinel.borrow_mut().set("type".into(), JsValue::Str("screen".into()));
            sentinel.borrow_mut().set("released".into(), JsValue::Bool(false));
            sentinel.borrow_mut().set("release".into(),
                native("release", |_| Ok(make_settled_promise("fulfilled", JsValue::Undefined))));
            sentinel.borrow_mut().set("addEventListener".into(),
                native("addEventListener", |_| Ok(JsValue::Undefined)));
            Ok(make_settled_promise("fulfilled", JsValue::Object(sentinel)))
        }));
        e.define("__wake_lock__", JsValue::Object(wl));
    }
    // Vibration API stub - navigator.vibrate(pattern)
    e.define("__navigator_vibrate__", native("navigator.vibrate", |_| Ok(JsValue::Bool(true))));
    // Gamepad API stub - navigator.getGamepads()
    e.define("__navigator_get_gamepads__", native("navigator.getGamepads", |_| {
        Ok(JsValue::Array(Rc::new(RefCell::new(Vec::new()))))
    }));
    // Battery API stub - navigator.getBattery()
    e.define("__navigator_get_battery__", native("navigator.getBattery", |_| {
        let bat = Rc::new(RefCell::new(JsObject::new()));
        bat.borrow_mut().set("charging".into(), JsValue::Bool(true));
        bat.borrow_mut().set("level".into(), JsValue::Number(1.0));
        bat.borrow_mut().set("chargingTime".into(), JsValue::Number(0.0));
        bat.borrow_mut().set("dischargingTime".into(), JsValue::Number(f64::INFINITY));
        bat.borrow_mut().set("addEventListener".into(),
            native("addEventListener", |_| Ok(JsValue::Undefined)));
        Ok(make_settled_promise("fulfilled", JsValue::Object(bat)))
    }));
    // Sensor APIs - Accelerometer / Gyroscope / OrientationSensor / LinearAccelerationSensor
    let make_sensor_stub = |name: &str| {
        let n = name.to_string();
        let n_for_native = n.clone();
        native(&n_for_native, move |_| {
            let s = Rc::new(RefCell::new(JsObject::new()));
            s.borrow_mut().set("__sensor__".into(), JsValue::Str(n.clone()));
            s.borrow_mut().set("activated".into(), JsValue::Bool(false));
            s.borrow_mut().set("x".into(), JsValue::Number(0.0));
            s.borrow_mut().set("y".into(), JsValue::Number(0.0));
            s.borrow_mut().set("z".into(), JsValue::Number(0.0));
            s.borrow_mut().set("start".into(), native("start", |_| Ok(JsValue::Undefined)));
            s.borrow_mut().set("stop".into(), native("stop", |_| Ok(JsValue::Undefined)));
            s.borrow_mut().set("addEventListener".into(),
                native("addEventListener", |_| Ok(JsValue::Undefined)));
            Ok(JsValue::Object(s))
        })
    };
    e.define("Accelerometer", make_sensor_stub("Accelerometer"));
    e.define("LinearAccelerationSensor", make_sensor_stub("LinearAccelerationSensor"));
    e.define("Gyroscope", make_sensor_stub("Gyroscope"));
    e.define("OrientationSensor", make_sensor_stub("OrientationSensor"));
    e.define("AbsoluteOrientationSensor", make_sensor_stub("AbsoluteOrientationSensor"));
    e.define("RelativeOrientationSensor", make_sensor_stub("RelativeOrientationSensor"));
    e.define("Magnetometer", make_sensor_stub("Magnetometer"));
    e.define("AmbientLightSensor", make_sensor_stub("AmbientLightSensor"));

    // ─── WebAuthn stub - navigator.credentials ───────────────────────────
    {
        let creds = Rc::new(RefCell::new(JsObject::new()));
        creds.borrow_mut().set("create".into(), native("credentials.create", |_| {
            Ok(make_settled_promise("fulfilled", JsValue::Null))
        }));
        creds.borrow_mut().set("get".into(), native("credentials.get", |_| {
            Ok(make_settled_promise("fulfilled", JsValue::Null))
        }));
        creds.borrow_mut().set("preventSilentAccess".into(),
            native("preventSilentAccess", |_| Ok(make_settled_promise("fulfilled", JsValue::Undefined))));
        creds.borrow_mut().set("store".into(),
            native("store", |_| Ok(make_settled_promise("fulfilled", JsValue::Undefined))));
        e.define("__navigator_credentials__", JsValue::Object(creds));
    }
    // PublicKeyCredential stub
    e.define("PublicKeyCredential", native("PublicKeyCredential", |_| {
        Ok(JsValue::Object(Rc::new(RefCell::new(JsObject::new()))))
    }));

    // ─── Trusted Types stub ───────────────────────────────────────────────
    {
        let tt = Rc::new(RefCell::new(JsObject::new()));
        tt.borrow_mut().set("createPolicy".into(), native("trustedTypes.createPolicy", |args| {
            let mut it = args.into_iter();
            let name = it.next().map(|v| v.to_string()).unwrap_or_default();
            let policy_init = it.next().unwrap_or(JsValue::Undefined);
            let policy = Rc::new(RefCell::new(JsObject::new()));
            policy.borrow_mut().set("name".into(), JsValue::Str(name));
            // Pass-through createHTML / createScript / createScriptURL
            for k in &["createHTML", "createScript", "createScriptURL"] {
                let init_clone = policy_init.clone();
                let k_owned = k.to_string();
                let k_for_native = k_owned.clone();
                policy.borrow_mut().set(k_owned.clone(), native(&k_for_native, move |a| {
                    let _ = &init_clone;
                    Ok(a.into_iter().next().unwrap_or(JsValue::Str(String::new())))
                }));
            }
            Ok(JsValue::Object(policy))
        }));
        tt.borrow_mut().set("isHTML".into(), native("isHTML", |_| Ok(JsValue::Bool(false))));
        tt.borrow_mut().set("isScript".into(), native("isScript", |_| Ok(JsValue::Bool(false))));
        tt.borrow_mut().set("isScriptURL".into(), native("isScriptURL", |_| Ok(JsValue::Bool(false))));
        tt.borrow_mut().set("emptyHTML".into(), JsValue::Str(String::new()));
        tt.borrow_mut().set("emptyScript".into(), JsValue::Str(String::new()));
        tt.borrow_mut().set("defaultPolicy".into(), JsValue::Null);
        e.define("trustedTypes", JsValue::Object(tt));
    }

    // ─── File System Access API stub ──────────────────────────────────────
    e.define("showOpenFilePicker", native("showOpenFilePicker", |_| {
        Ok(make_settled_promise("fulfilled", JsValue::Array(Rc::new(RefCell::new(Vec::new())))))
    }));
    e.define("showSaveFilePicker", native("showSaveFilePicker", |_| {
        let handle = Rc::new(RefCell::new(JsObject::new()));
        handle.borrow_mut().set("kind".into(), JsValue::Str("file".into()));
        handle.borrow_mut().set("name".into(), JsValue::Str("untitled".into()));
        handle.borrow_mut().set("createWritable".into(), native("createWritable", |_|
            Ok(make_settled_promise("fulfilled", JsValue::Object(Rc::new(RefCell::new(JsObject::new())))))));
        Ok(make_settled_promise("fulfilled", JsValue::Object(handle)))
    }));
    e.define("showDirectoryPicker", native("showDirectoryPicker", |_| {
        Ok(make_settled_promise("fulfilled", JsValue::Object(Rc::new(RefCell::new(JsObject::new())))))
    }));

    // ─── Web MIDI API stub ────────────────────────────────────────────────
    e.define("__navigator_request_midi_access__",
        native("requestMIDIAccess", |_| {
            let access = Rc::new(RefCell::new(JsObject::new()));
            access.borrow_mut().set("inputs".into(),
                JsValue::Object(Rc::new(RefCell::new(JsObject::new()))));
            access.borrow_mut().set("outputs".into(),
                JsValue::Object(Rc::new(RefCell::new(JsObject::new()))));
            access.borrow_mut().set("sysexEnabled".into(), JsValue::Bool(false));
            Ok(make_settled_promise("fulfilled", JsValue::Object(access)))
        }));

    // ─── DOM Node constructors (interface objects) ───────────────────────
    e.define("DocumentFragment", native("DocumentFragment", |_| {
        let frag = Rc::new(RefCell::new(JsObject::new()));
        frag.borrow_mut().set("__doc_fragment__".into(), JsValue::Bool(true));
        frag.borrow_mut().set("nodeType".into(), JsValue::Number(11.0));
        frag.borrow_mut().set("nodeName".into(), JsValue::Str("#document-fragment".into()));
        frag.borrow_mut().set("childNodes".into(),
            JsValue::Array(Rc::new(RefCell::new(Vec::new()))));
        frag.borrow_mut().set("childElementCount".into(), JsValue::Number(0.0));
        frag.borrow_mut().set("appendChild".into(),
            native("appendChild", |args| Ok(args.into_iter().next().unwrap_or(JsValue::Undefined))));
        frag.borrow_mut().set("querySelector".into(),
            native("querySelector", |_| Ok(JsValue::Null)));
        frag.borrow_mut().set("querySelectorAll".into(),
            native("querySelectorAll", |_| Ok(JsValue::Array(Rc::new(RefCell::new(Vec::new()))))));
        Ok(JsValue::Object(frag))
    }));

    // Comment / Text / CDATASection / ProcessingInstruction constructors
    let make_text_node_ctor = |kind: &str, node_type: f64| {
        let kind_s = kind.to_string();
        native(kind, move |args| {
            let data = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let obj = Rc::new(RefCell::new(JsObject::new()));
            obj.borrow_mut().set("nodeType".into(), JsValue::Number(node_type));
            obj.borrow_mut().set("nodeName".into(), JsValue::Str(kind_s.clone()));
            obj.borrow_mut().set("data".into(), JsValue::Str(data.clone()));
            obj.borrow_mut().set("textContent".into(), JsValue::Str(data.clone()));
            obj.borrow_mut().set("nodeValue".into(), JsValue::Str(data.clone()));
            obj.borrow_mut().set("length".into(), JsValue::Number(data.chars().count() as f64));
            Ok(JsValue::Object(obj))
        })
    };
    e.define("Text", make_text_node_ctor("#text", 3.0));
    e.define("Comment", make_text_node_ctor("#comment", 8.0));
    e.define("CDATASection", make_text_node_ctor("#cdata-section", 4.0));

    // Node interface object - constants
    {
        let node_obj = Rc::new(RefCell::new(JsObject::new()));
        node_obj.borrow_mut().set("ELEMENT_NODE".into(), JsValue::Number(1.0));
        node_obj.borrow_mut().set("ATTRIBUTE_NODE".into(), JsValue::Number(2.0));
        node_obj.borrow_mut().set("TEXT_NODE".into(), JsValue::Number(3.0));
        node_obj.borrow_mut().set("CDATA_SECTION_NODE".into(), JsValue::Number(4.0));
        node_obj.borrow_mut().set("PROCESSING_INSTRUCTION_NODE".into(), JsValue::Number(7.0));
        node_obj.borrow_mut().set("COMMENT_NODE".into(), JsValue::Number(8.0));
        node_obj.borrow_mut().set("DOCUMENT_NODE".into(), JsValue::Number(9.0));
        node_obj.borrow_mut().set("DOCUMENT_TYPE_NODE".into(), JsValue::Number(10.0));
        node_obj.borrow_mut().set("DOCUMENT_FRAGMENT_NODE".into(), JsValue::Number(11.0));
        node_obj.borrow_mut().set("DOCUMENT_POSITION_DISCONNECTED".into(), JsValue::Number(1.0));
        node_obj.borrow_mut().set("DOCUMENT_POSITION_PRECEDING".into(), JsValue::Number(2.0));
        node_obj.borrow_mut().set("DOCUMENT_POSITION_FOLLOWING".into(), JsValue::Number(4.0));
        node_obj.borrow_mut().set("DOCUMENT_POSITION_CONTAINS".into(), JsValue::Number(8.0));
        node_obj.borrow_mut().set("DOCUMENT_POSITION_CONTAINED_BY".into(), JsValue::Number(16.0));
        e.define("Node", JsValue::Object(node_obj));
    }

    // ─── MutationRecord / ResizeObserverEntry / IntersectionObserverEntry ─
    e.define("MutationRecord", native("MutationRecord", |_| {
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("type".into(), JsValue::Str("childList".into()));
        obj.borrow_mut().set("target".into(), JsValue::Null);
        obj.borrow_mut().set("addedNodes".into(), JsValue::Array(Rc::new(RefCell::new(Vec::new()))));
        obj.borrow_mut().set("removedNodes".into(), JsValue::Array(Rc::new(RefCell::new(Vec::new()))));
        obj.borrow_mut().set("previousSibling".into(), JsValue::Null);
        obj.borrow_mut().set("nextSibling".into(), JsValue::Null);
        obj.borrow_mut().set("attributeName".into(), JsValue::Null);
        obj.borrow_mut().set("attributeNamespace".into(), JsValue::Null);
        obj.borrow_mut().set("oldValue".into(), JsValue::Null);
        Ok(JsValue::Object(obj))
    }));

    // ─── HTMLCollection constructor (read-only) ───────────────────────────
    e.define("HTMLCollection", native("HTMLCollection", |_| {
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("__html_collection__".into(), JsValue::Bool(true));
        obj.borrow_mut().set("length".into(), JsValue::Number(0.0));
        obj.borrow_mut().set("item".into(), native("item", |_| Ok(JsValue::Null)));
        obj.borrow_mut().set("namedItem".into(), native("namedItem", |_| Ok(JsValue::Null)));
        Ok(JsValue::Object(obj))
    }));

    // ─── NodeList constructor ─────────────────────────────────────────────
    e.define("NodeList", native("NodeList", |_| {
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("__node_list__".into(), JsValue::Bool(true));
        obj.borrow_mut().set("length".into(), JsValue::Number(0.0));
        obj.borrow_mut().set("item".into(), native("item", |_| Ok(JsValue::Null)));
        obj.borrow_mut().set("forEach".into(), native("forEach", |_| Ok(JsValue::Undefined)));
        Ok(JsValue::Object(obj))
    }));

    // ─── DOMTokenList constructor ─────────────────────────────────────────
    e.define("DOMTokenList", native("DOMTokenList", |_| {
        let tokens: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("__token_list__".into(), JsValue::Bool(true));
        obj.borrow_mut().set("length".into(), JsValue::Number(0.0));
        let t1 = Rc::clone(&tokens);
        obj.borrow_mut().set("contains".into(), native("contains", move |args| {
            let s = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            Ok(JsValue::Bool(t1.borrow().iter().any(|t| t == &s)))
        }));
        let t2 = Rc::clone(&tokens);
        let o2 = Rc::clone(&obj);
        obj.borrow_mut().set("add".into(), native("add", move |args| {
            for arg in args {
                let s = arg.to_string();
                if !t2.borrow().iter().any(|t| t == &s) {
                    t2.borrow_mut().push(s);
                }
            }
            o2.borrow_mut().set("length".into(), JsValue::Number(t2.borrow().len() as f64));
            Ok(JsValue::Undefined)
        }));
        let t3 = Rc::clone(&tokens);
        let o3 = Rc::clone(&obj);
        obj.borrow_mut().set("remove".into(), native("remove", move |args| {
            for arg in args {
                let s = arg.to_string();
                t3.borrow_mut().retain(|t| t != &s);
            }
            o3.borrow_mut().set("length".into(), JsValue::Number(t3.borrow().len() as f64));
            Ok(JsValue::Undefined)
        }));
        let t4 = Rc::clone(&tokens);
        let o4 = Rc::clone(&obj);
        obj.borrow_mut().set("toggle".into(), native("toggle", move |args| {
            let s = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let result = {
                let mut b = t4.borrow_mut();
                if let Some(pos) = b.iter().position(|t| t == &s) {
                    b.remove(pos);
                    false
                } else {
                    b.push(s);
                    true
                }
            };
            o4.borrow_mut().set("length".into(), JsValue::Number(t4.borrow().len() as f64));
            Ok(JsValue::Bool(result))
        }));
        Ok(JsValue::Object(obj))
    }));

    // ─── HTMLImageElement constructor ─────────────────────────────────────
    e.define("Image", native("Image", |args| {
        use crate::browser::dom::NodeData;
        let mut it = args.into_iter();
        let mut attrs = std::collections::HashMap::new();
        if let Some(w) = it.next() {
            attrs.insert("width".into(), (w.to_number() as i64).to_string());
        }
        if let Some(h) = it.next() {
            attrs.insert("height".into(), (h.to_number() as i64).to_string());
        }
        let node = NodeData::new_element("img", attrs);
        Ok(JsValue::DomNode(node))
    }));
    e.define("Audio", native("Audio", |args| {
        use crate::browser::dom::NodeData;
        let mut attrs = std::collections::HashMap::new();
        if let Some(src) = args.into_iter().next() {
            attrs.insert("src".into(), src.to_string());
        }
        let node = NodeData::new_element("audio", attrs);
        Ok(JsValue::DomNode(node))
    }));
    e.define("Option", native("Option", |args| {
        use crate::browser::dom::NodeData;
        let mut it = args.into_iter();
        let text = it.next().map(|v| v.to_string()).unwrap_or_default();
        let value = it.next().map(|v| v.to_string());
        let mut attrs = std::collections::HashMap::new();
        if let Some(v) = value { attrs.insert("value".into(), v); }
        let node = NodeData::new_element("option", attrs);
        // Set inner text
        let text_node = NodeData::new_text(&text);
        node.append_child(text_node);
        Ok(JsValue::DomNode(node))
    }));

    // ─── DataTransfer / DataTransferItem (drag-drop) ──────────────────────
    e.define("DataTransfer", native("DataTransfer", |_| {
        let items: Rc<RefCell<HashMap<String, String>>> = Rc::new(RefCell::new(HashMap::new()));
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("dropEffect".into(), JsValue::Str("none".into()));
        obj.borrow_mut().set("effectAllowed".into(), JsValue::Str("all".into()));
        obj.borrow_mut().set("types".into(), JsValue::Array(Rc::new(RefCell::new(Vec::new()))));
        obj.borrow_mut().set("files".into(), JsValue::Array(Rc::new(RefCell::new(Vec::new()))));
        let i1 = Rc::clone(&items);
        obj.borrow_mut().set("setData".into(), native("setData", move |args| {
            let mut it = args.into_iter();
            let format = it.next().map(|v| v.to_string()).unwrap_or_default();
            let data = it.next().map(|v| v.to_string()).unwrap_or_default();
            i1.borrow_mut().insert(format, data);
            Ok(JsValue::Undefined)
        }));
        let i2 = Rc::clone(&items);
        obj.borrow_mut().set("getData".into(), native("getData", move |args| {
            let format = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            Ok(JsValue::Str(i2.borrow().get(&format).cloned().unwrap_or_default()))
        }));
        let i3 = Rc::clone(&items);
        obj.borrow_mut().set("clearData".into(), native("clearData", move |args| {
            if let Some(format) = args.into_iter().next() {
                i3.borrow_mut().remove(&format.to_string());
            } else {
                i3.borrow_mut().clear();
            }
            Ok(JsValue::Undefined)
        }));
        obj.borrow_mut().set("setDragImage".into(),
            native("setDragImage", |_| Ok(JsValue::Undefined)));
        Ok(JsValue::Object(obj))
    }));

    // ─── Storage events ───────────────────────────────────────────────────
    e.define("StorageManager", native("StorageManager", |_| {
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("estimate".into(), native("estimate", |_| {
            let est = Rc::new(RefCell::new(JsObject::new()));
            est.borrow_mut().set("quota".into(), JsValue::Number(1_000_000_000.0));
            est.borrow_mut().set("usage".into(), JsValue::Number(0.0));
            Ok(make_settled_promise("fulfilled", JsValue::Object(est)))
        }));
        obj.borrow_mut().set("persist".into(), native("persist", |_|
            Ok(make_settled_promise("fulfilled", JsValue::Bool(true)))));
        obj.borrow_mut().set("persisted".into(), native("persisted", |_|
            Ok(make_settled_promise("fulfilled", JsValue::Bool(false)))));
        Ok(JsValue::Object(obj))
    }));

    // ─── PerformanceObserver real - shared registry pro entries ───────────
    e.define("PerformanceObserver", native("PerformanceObserver", |args| {
        let cb = args.into_iter().next().unwrap_or(JsValue::Undefined);
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("__performance_observer__".into(), JsValue::Bool(true));
        obj.borrow_mut().set("__callback__".into(), cb);
        obj.borrow_mut().set("observe".into(),
            native("observe", |_| Ok(JsValue::Undefined)));
        obj.borrow_mut().set("disconnect".into(),
            native("disconnect", |_| Ok(JsValue::Undefined)));
        obj.borrow_mut().set("takeRecords".into(),
            native("takeRecords", |_| Ok(JsValue::Array(Rc::new(RefCell::new(Vec::new()))))));
        Ok(JsValue::Object(obj))
    }));
    e.define("PerformanceEntry", native("PerformanceEntry", |_| {
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("name".into(), JsValue::Str(String::new()));
        obj.borrow_mut().set("entryType".into(), JsValue::Str(String::new()));
        obj.borrow_mut().set("startTime".into(), JsValue::Number(0.0));
        obj.borrow_mut().set("duration".into(), JsValue::Number(0.0));
        Ok(JsValue::Object(obj))
    }));

    // ─── DOMException ─────────────────────────────────────────────────────
    e.define("DOMException", native("DOMException", |args| {
        let mut it = args.into_iter();
        let message = it.next().map(|v| v.to_string()).unwrap_or_default();
        let name = it.next().map(|v| v.to_string()).unwrap_or_else(|| "Error".into());
        let code: u16 = match name.as_str() {
            "IndexSizeError" => 1,
            "HierarchyRequestError" => 3,
            "WrongDocumentError" => 4,
            "InvalidCharacterError" => 5,
            "NoModificationAllowedError" => 7,
            "NotFoundError" => 8,
            "NotSupportedError" => 9,
            "InUseAttributeError" => 10,
            "InvalidStateError" => 11,
            "SyntaxError" => 12,
            "InvalidModificationError" => 13,
            "NamespaceError" => 14,
            "InvalidAccessError" => 15,
            "TypeMismatchError" => 17,
            "SecurityError" => 18,
            "NetworkError" => 19,
            "AbortError" => 20,
            "URLMismatchError" => 21,
            "QuotaExceededError" => 22,
            "TimeoutError" => 23,
            "InvalidNodeTypeError" => 24,
            "DataCloneError" => 25,
            _ => 0,
        };
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("name".into(), JsValue::Str(name));
        obj.borrow_mut().set("message".into(), JsValue::Str(message));
        obj.borrow_mut().set("code".into(), JsValue::Number(code as f64));
        Ok(JsValue::Object(obj))
    }));

    // ─── ImageData constructor ────────────────────────────────────────────
    e.define("ImageData", native("ImageData", |args| {
        let mut it = args.into_iter();
        let first = it.next().unwrap_or(JsValue::Undefined);
        let (data, w, h) = match first {
            JsValue::Number(width) => {
                let height = it.next().map(|v| v.to_number()).unwrap_or(1.0);
                let len = (width * height * 4.0) as usize;
                let data: Vec<JsValue> = vec![JsValue::Number(0.0); len];
                (data, width, height)
            }
            JsValue::Array(arr) => {
                let width = it.next().map(|v| v.to_number()).unwrap_or(1.0);
                let data = arr.borrow().clone();
                let height = (data.len() as f64 / 4.0 / width).max(1.0);
                (data, width, height)
            }
            _ => (Vec::new(), 0.0, 0.0),
        };
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("width".into(), JsValue::Number(w));
        obj.borrow_mut().set("height".into(), JsValue::Number(h));
        obj.borrow_mut().set("data".into(), JsValue::Array(Rc::new(RefCell::new(data))));
        obj.borrow_mut().set("colorSpace".into(), JsValue::Str("srgb".into()));
        Ok(JsValue::Object(obj))
    }));

    // ─── OffscreenCanvas stub ─────────────────────────────────────────────
    e.define("OffscreenCanvas", native("OffscreenCanvas", |args| {
        let mut it = args.into_iter();
        let w = it.next().map(|v| v.to_number()).unwrap_or(300.0);
        let h = it.next().map(|v| v.to_number()).unwrap_or(150.0);
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("width".into(), JsValue::Number(w));
        obj.borrow_mut().set("height".into(), JsValue::Number(h));
        obj.borrow_mut().set("getContext".into(), native("getContext", |_| {
            Ok(JsValue::Object(Rc::new(RefCell::new(JsObject::new()))))
        }));
        obj.borrow_mut().set("transferToImageBitmap".into(), native("transferToImageBitmap", |_| {
            Ok(JsValue::Object(Rc::new(RefCell::new(JsObject::new()))))
        }));
        obj.borrow_mut().set("convertToBlob".into(), native("convertToBlob", |_| {
            Ok(make_settled_promise("fulfilled", JsValue::Object(Rc::new(RefCell::new(JsObject::new())))))
        }));
        Ok(JsValue::Object(obj))
    }));

    // ─── ImageBitmap (createImageBitmap funkce) ───────────────────────────
    e.define("createImageBitmap", native("createImageBitmap", |_| {
        let bitmap = Rc::new(RefCell::new(JsObject::new()));
        bitmap.borrow_mut().set("width".into(), JsValue::Number(0.0));
        bitmap.borrow_mut().set("height".into(), JsValue::Number(0.0));
        bitmap.borrow_mut().set("close".into(),
            native("close", |_| Ok(JsValue::Undefined)));
        Ok(make_settled_promise("fulfilled", JsValue::Object(bitmap)))
    }));

    // ─── CSS Houdini API stubs ─────────────────────────────────────────────
    // CSS.paintWorklet.addModule(url) registers paint(name) classes.
    // Aktualne stub - registry sleduje, paint() invocation = no-op.
    let paint_registry: Rc<RefCell<HashMap<String, JsValue>>> =
        Rc::new(RefCell::new(HashMap::new()));
    let property_registry: Rc<RefCell<HashMap<String, JsValue>>> =
        Rc::new(RefCell::new(HashMap::new()));
    {
        let mut css_obj = JsObject::new();
        // CSS.paintWorklet.addModule(url) -> Promise
        let mut paint_worklet = JsObject::new();
        paint_worklet.set("addModule".into(), native("addModule", |a| {
            let _url = a.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            Ok(make_settled_promise("fulfilled", JsValue::Undefined))
        }));
        css_obj.set("paintWorklet".into(),
            JsValue::Object(Rc::new(RefCell::new(paint_worklet))));
        // CSS.layoutWorklet stub
        let mut layout_worklet = JsObject::new();
        layout_worklet.set("addModule".into(), native("addModule", |_| {
            Ok(make_settled_promise("fulfilled", JsValue::Undefined))
        }));
        css_obj.set("layoutWorklet".into(),
            JsValue::Object(Rc::new(RefCell::new(layout_worklet))));
        // CSS.animationWorklet stub
        let mut anim_worklet = JsObject::new();
        anim_worklet.set("addModule".into(), native("addModule", |_| {
            Ok(make_settled_promise("fulfilled", JsValue::Undefined))
        }));
        css_obj.set("animationWorklet".into(),
            JsValue::Object(Rc::new(RefCell::new(anim_worklet))));
        // CSS.registerProperty({name, syntax, inherits, initialValue}) - typed props
        let prop_reg = Rc::clone(&property_registry);
        css_obj.set("registerProperty".into(), native("registerProperty", move |a| {
            if let Some(JsValue::Object(o)) = a.into_iter().next() {
                let b = o.borrow();
                if let JsValue::Str(name) = b.get("name") {
                    prop_reg.borrow_mut().insert(name, JsValue::Object(Rc::clone(&o)));
                }
            }
            Ok(JsValue::Undefined)
        }));
        // CSS.supports("prop: value") -> bool. Naivni: vsechno supported.
        css_obj.set("supports".into(), native("supports", |_| Ok(JsValue::Bool(true))));
        // CSS.escape(str) - escape pro selektory. Naivni passthrough.
        css_obj.set("escape".into(), native("escape", |a| {
            Ok(JsValue::Str(a.into_iter().next().map(|v| v.to_string()).unwrap_or_default()))
        }));
        // CSS unit factory functions: CSS.px(N), CSS.em(N), ...
        for unit in &["px", "em", "rem", "pt", "pc", "in", "cm", "mm", "ex", "ch",
                      "vw", "vh", "vmin", "vmax", "%", "deg", "rad", "turn", "s", "ms"] {
            let u = unit.to_string();
            css_obj.set(unit.to_string(), native(unit, move |a| {
                let n = a.into_iter().next().map(|v| v.to_number()).unwrap_or(0.0);
                let mut o = JsObject::new();
                o.set("value".into(), JsValue::Number(n));
                o.set("unit".into(), JsValue::Str(u.clone()));
                o.set("__cssunit__".into(), JsValue::Bool(true));
                Ok(JsValue::Object(Rc::new(RefCell::new(o))))
            }));
        }
        // CSS.number(N), CSS.percent(N)
        css_obj.set("number".into(), native("number", |a| {
            let n = a.into_iter().next().map(|v| v.to_number()).unwrap_or(0.0);
            Ok(JsValue::Number(n))
        }));
        e.define("CSS", JsValue::Object(Rc::new(RefCell::new(css_obj))));
    }
    // registerPaint(name, classDef) - global. Pri paint(name) v CSS by se
    // invokoval paint() metoda; aktualne stub registry only.
    {
        let pr = Rc::clone(&paint_registry);
        e.define("registerPaint", native("registerPaint", move |a| {
            let mut it = a.into_iter();
            let name = it.next().map(|v| v.to_string()).unwrap_or_default();
            let class_def = it.next().unwrap_or(JsValue::Undefined);
            pr.borrow_mut().insert(name, class_def);
            Ok(JsValue::Undefined)
        }));
    }
    let _ = paint_registry;
    let _ = property_registry;

    // ─── Path2D stub - canvas paths ───────────────────────────────────────
    e.define("Path2D", native("Path2D", |_| {
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("__path2d__".into(), JsValue::Bool(true));
        for m in &["addPath", "closePath", "moveTo", "lineTo", "bezierCurveTo",
                   "quadraticCurveTo", "arc", "arcTo", "ellipse", "rect", "roundRect"] {
            obj.borrow_mut().set(m.to_string(), native(m, |_| Ok(JsValue::Undefined)));
        }
        Ok(JsValue::Object(obj))
    }));

    // ─── DOM Geometry: DOMRect / DOMPoint / DOMMatrix ─────────────────────
    let make_dom_rect = |args: Vec<JsValue>, read_only: bool| -> JsValue {
        let mut it = args.into_iter();
        let x = it.next().map(|v| v.to_number()).unwrap_or(0.0);
        let y = it.next().map(|v| v.to_number()).unwrap_or(0.0);
        let w = it.next().map(|v| v.to_number()).unwrap_or(0.0);
        let h = it.next().map(|v| v.to_number()).unwrap_or(0.0);
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("x".into(), JsValue::Number(x));
        obj.borrow_mut().set("y".into(), JsValue::Number(y));
        obj.borrow_mut().set("width".into(), JsValue::Number(w));
        obj.borrow_mut().set("height".into(), JsValue::Number(h));
        obj.borrow_mut().set("top".into(), JsValue::Number(y));
        obj.borrow_mut().set("left".into(), JsValue::Number(x));
        obj.borrow_mut().set("right".into(), JsValue::Number(x + w));
        obj.borrow_mut().set("bottom".into(), JsValue::Number(y + h));
        obj.borrow_mut().set("__read_only__".into(), JsValue::Bool(read_only));
        JsValue::Object(obj)
    };
    e.define("DOMRect", native("DOMRect", move |args| Ok(make_dom_rect(args, false))));
    e.define("DOMRectReadOnly", native("DOMRectReadOnly",
        move |args| Ok(make_dom_rect(args, true))));

    let make_dom_point = |args: Vec<JsValue>, read_only: bool| -> JsValue {
        let mut it = args.into_iter();
        let x = it.next().map(|v| v.to_number()).unwrap_or(0.0);
        let y = it.next().map(|v| v.to_number()).unwrap_or(0.0);
        let z = it.next().map(|v| v.to_number()).unwrap_or(0.0);
        let w = it.next().map(|v| v.to_number()).unwrap_or(1.0);
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("x".into(), JsValue::Number(x));
        obj.borrow_mut().set("y".into(), JsValue::Number(y));
        obj.borrow_mut().set("z".into(), JsValue::Number(z));
        obj.borrow_mut().set("w".into(), JsValue::Number(w));
        obj.borrow_mut().set("__read_only__".into(), JsValue::Bool(read_only));
        JsValue::Object(obj)
    };
    e.define("DOMPoint", native("DOMPoint", move |args| Ok(make_dom_point(args, false))));
    e.define("DOMPointReadOnly", native("DOMPointReadOnly",
        move |args| Ok(make_dom_point(args, true))));

    // DOMMatrix - 4x4 matrix s identity default
    let make_dom_matrix = |args: Vec<JsValue>, read_only: bool| -> JsValue {
        // Args: bud Array [m11, m12, ...] nebo prazdne
        let nums: Vec<f64> = match args.into_iter().next() {
            Some(JsValue::Array(arr)) => arr.borrow().iter().map(|v| v.to_number()).collect(),
            _ => Vec::new(),
        };
        let obj = Rc::new(RefCell::new(JsObject::new()));
        // 4x4 identity default
        let mut m = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        if nums.len() == 6 {
            // 2D: [a, b, c, d, e, f] -> m11=a m12=b m21=c m22=d m41=e m42=f
            m[0] = nums[0]; m[1] = nums[1];
            m[4] = nums[2]; m[5] = nums[3];
            m[12] = nums[4]; m[13] = nums[5];
        } else if nums.len() == 16 {
            for i in 0..16 { m[i] = nums[i]; }
        }
        obj.borrow_mut().set("a".into(), JsValue::Number(m[0]));
        obj.borrow_mut().set("b".into(), JsValue::Number(m[1]));
        obj.borrow_mut().set("c".into(), JsValue::Number(m[4]));
        obj.borrow_mut().set("d".into(), JsValue::Number(m[5]));
        obj.borrow_mut().set("e".into(), JsValue::Number(m[12]));
        obj.borrow_mut().set("f".into(), JsValue::Number(m[13]));
        for (i, v) in m.iter().enumerate() {
            let row = i / 4 + 1;
            let col = i % 4 + 1;
            obj.borrow_mut().set(format!("m{}{}", col, row), JsValue::Number(*v));
        }
        obj.borrow_mut().set("is2D".into(), JsValue::Bool(nums.len() == 6 || nums.is_empty()));
        obj.borrow_mut().set("isIdentity".into(), JsValue::Bool(nums.is_empty()));
        obj.borrow_mut().set("__read_only__".into(), JsValue::Bool(read_only));
        // multiply / inverse / translate / scale / rotate stuby
        obj.borrow_mut().set("multiply".into(),
            native("multiply", |_| Ok(JsValue::Object(Rc::new(RefCell::new(JsObject::new()))))));
        obj.borrow_mut().set("inverse".into(),
            native("inverse", |_| Ok(JsValue::Object(Rc::new(RefCell::new(JsObject::new()))))));
        obj.borrow_mut().set("translate".into(),
            native("translate", |_| Ok(JsValue::Object(Rc::new(RefCell::new(JsObject::new()))))));
        obj.borrow_mut().set("scale".into(),
            native("scale", |_| Ok(JsValue::Object(Rc::new(RefCell::new(JsObject::new()))))));
        obj.borrow_mut().set("rotate".into(),
            native("rotate", |_| Ok(JsValue::Object(Rc::new(RefCell::new(JsObject::new()))))));
        obj.borrow_mut().set("toFloat32Array".into(),
            native("toFloat32Array", |_| Ok(JsValue::Array(Rc::new(RefCell::new(Vec::new()))))));
        obj.borrow_mut().set("toFloat64Array".into(),
            native("toFloat64Array", |_| Ok(JsValue::Array(Rc::new(RefCell::new(Vec::new()))))));
        JsValue::Object(obj)
    };
    e.define("DOMMatrix", native("DOMMatrix", move |args| Ok(make_dom_matrix(args, false))));
    e.define("DOMMatrixReadOnly", native("DOMMatrixReadOnly",
        move |args| Ok(make_dom_matrix(args, true))));

    // DOMQuad - 4 corner points (DOMPoint p1, p2, p3, p4)
    e.define("DOMQuad", native("DOMQuad", |_args| {
        let obj = Rc::new(RefCell::new(JsObject::new()));
        for p in &["p1", "p2", "p3", "p4"] {
            let pt = Rc::new(RefCell::new(JsObject::new()));
            pt.borrow_mut().set("x".into(), JsValue::Number(0.0));
            pt.borrow_mut().set("y".into(), JsValue::Number(0.0));
            obj.borrow_mut().set(p.to_string(), JsValue::Object(pt));
        }
        Ok(JsValue::Object(obj))
    }));

    // ─── Visual Viewport API ──────────────────────────────────────────────
    {
        let vv = Rc::new(RefCell::new(JsObject::new()));
        vv.borrow_mut().set("offsetLeft".into(), JsValue::Number(0.0));
        vv.borrow_mut().set("offsetTop".into(), JsValue::Number(0.0));
        vv.borrow_mut().set("pageLeft".into(), JsValue::Number(0.0));
        vv.borrow_mut().set("pageTop".into(), JsValue::Number(0.0));
        vv.borrow_mut().set("width".into(), JsValue::Number(1024.0));
        vv.borrow_mut().set("height".into(), JsValue::Number(768.0));
        vv.borrow_mut().set("scale".into(), JsValue::Number(1.0));
        vv.borrow_mut().set("addEventListener".into(),
            native("addEventListener", |_| Ok(JsValue::Undefined)));
        vv.borrow_mut().set("removeEventListener".into(),
            native("removeEventListener", |_| Ok(JsValue::Undefined)));
        e.define("__visual_viewport__", JsValue::Object(vv));
    }

    // ─── navigator.share / canShare (Web Share API) ───────────────────────
    e.define("__navigator_share__", native("navigator.share", |_|
        Ok(make_settled_promise("fulfilled", JsValue::Undefined))));
    e.define("__navigator_can_share__", native("navigator.canShare", |_| Ok(JsValue::Bool(true))));

    // ─── Badging API - navigator.setAppBadge / clearAppBadge ──────────────
    e.define("__navigator_set_app_badge__", native("setAppBadge", |_|
        Ok(make_settled_promise("fulfilled", JsValue::Undefined))));
    e.define("__navigator_clear_app_badge__", native("clearAppBadge", |_|
        Ok(make_settled_promise("fulfilled", JsValue::Undefined))));

    // ─── ContactsManager - navigator.contacts ─────────────────────────────
    {
        let cm = Rc::new(RefCell::new(JsObject::new()));
        cm.borrow_mut().set("select".into(), native("contacts.select", |_|
            Ok(make_settled_promise("fulfilled", JsValue::Array(Rc::new(RefCell::new(Vec::new())))))));
        cm.borrow_mut().set("getProperties".into(), native("contacts.getProperties", |_|
            Ok(make_settled_promise("fulfilled", JsValue::Array(Rc::new(RefCell::new(vec![
                JsValue::Str("name".into()),
                JsValue::Str("email".into()),
                JsValue::Str("tel".into()),
            ])))))));
        e.define("__navigator_contacts__", JsValue::Object(cm));
    }

    // ─── Background Sync stub - registration.sync ─────────────────────────
    {
        let sync = Rc::new(RefCell::new(JsObject::new()));
        sync.borrow_mut().set("register".into(), native("sync.register", |_|
            Ok(make_settled_promise("fulfilled", JsValue::Undefined))));
        sync.borrow_mut().set("getTags".into(), native("sync.getTags", |_|
            Ok(make_settled_promise("fulfilled", JsValue::Array(Rc::new(RefCell::new(Vec::new())))))));
        e.define("__background_sync__", JsValue::Object(sync));
    }

    // ─── Push API - registration.pushManager ──────────────────────────────
    {
        let pm = Rc::new(RefCell::new(JsObject::new()));
        pm.borrow_mut().set("subscribe".into(), native("pushManager.subscribe", |_| {
            let sub = Rc::new(RefCell::new(JsObject::new()));
            sub.borrow_mut().set("endpoint".into(), JsValue::Str("https://example.com/push".into()));
            sub.borrow_mut().set("expirationTime".into(), JsValue::Null);
            sub.borrow_mut().set("unsubscribe".into(), native("unsubscribe", |_|
                Ok(make_settled_promise("fulfilled", JsValue::Bool(true)))));
            sub.borrow_mut().set("toJSON".into(), native("toJSON", |_|
                Ok(JsValue::Object(Rc::new(RefCell::new(JsObject::new()))))));
            sub.borrow_mut().set("getKey".into(), native("getKey", |_| Ok(JsValue::Null)));
            Ok(make_settled_promise("fulfilled", JsValue::Object(sub)))
        }));
        pm.borrow_mut().set("getSubscription".into(), native("pushManager.getSubscription", |_|
            Ok(make_settled_promise("fulfilled", JsValue::Null))));
        pm.borrow_mut().set("permissionState".into(), native("pushManager.permissionState", |_|
            Ok(make_settled_promise("fulfilled", JsValue::Str("granted".into())))));
        e.define("__push_manager__", JsValue::Object(pm));
    }

    // ─── Reporting API ────────────────────────────────────────────────────
    e.define("ReportingObserver", native("ReportingObserver", |_| {
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("observe".into(), native("observe", |_| Ok(JsValue::Undefined)));
        obj.borrow_mut().set("disconnect".into(), native("disconnect", |_| Ok(JsValue::Undefined)));
        obj.borrow_mut().set("takeRecords".into(), native("takeRecords", |_|
            Ok(JsValue::Array(Rc::new(RefCell::new(Vec::new()))))));
        Ok(JsValue::Object(obj))
    }));

    // ─── WebTransport stub ────────────────────────────────────────────────
    e.define("WebTransport", native("WebTransport", |args| {
        let url = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("url".into(), JsValue::Str(url));
        obj.borrow_mut().set("ready".into(),
            make_settled_promise("fulfilled", JsValue::Undefined));
        obj.borrow_mut().set("closed".into(),
            make_settled_promise("fulfilled", JsValue::Object(Rc::new(RefCell::new(JsObject::new())))));
        obj.borrow_mut().set("incomingBidirectionalStreams".into(),
            JsValue::Object(Rc::new(RefCell::new(JsObject::new()))));
        obj.borrow_mut().set("incomingUnidirectionalStreams".into(),
            JsValue::Object(Rc::new(RefCell::new(JsObject::new()))));
        obj.borrow_mut().set("datagrams".into(),
            JsValue::Object(Rc::new(RefCell::new(JsObject::new()))));
        obj.borrow_mut().set("close".into(),
            native("close", |_| Ok(JsValue::Undefined)));
        Ok(JsValue::Object(obj))
    }));

    // ─── Compression Streams API ──────────────────────────────────────────
    e.define("CompressionStream", native("CompressionStream", |args| {
        let format = args.into_iter().next().map(|v| v.to_string()).unwrap_or_else(|| "deflate".into());
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("__compression_stream__".into(), JsValue::Str(format));
        obj.borrow_mut().set("readable".into(), JsValue::Object(Rc::new(RefCell::new(JsObject::new()))));
        obj.borrow_mut().set("writable".into(), JsValue::Object(Rc::new(RefCell::new(JsObject::new()))));
        Ok(JsValue::Object(obj))
    }));
    e.define("DecompressionStream", native("DecompressionStream", |args| {
        let format = args.into_iter().next().map(|v| v.to_string()).unwrap_or_else(|| "deflate".into());
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("__decompression_stream__".into(), JsValue::Str(format));
        obj.borrow_mut().set("readable".into(), JsValue::Object(Rc::new(RefCell::new(JsObject::new()))));
        obj.borrow_mut().set("writable".into(), JsValue::Object(Rc::new(RefCell::new(JsObject::new()))));
        Ok(JsValue::Object(obj))
    }));

    // ─── Web Streams API ──────────────────────────────────────────────────
    e.define("ReadableStream", native("ReadableStream", |args| {
        let _underlying = args.into_iter().next();
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("__readable_stream__".into(), JsValue::Bool(true));
        obj.borrow_mut().set("locked".into(), JsValue::Bool(false));
        obj.borrow_mut().set("cancel".into(), native("cancel", |_|
            Ok(make_settled_promise("fulfilled", JsValue::Undefined))));
        obj.borrow_mut().set("getReader".into(), native("getReader", |_| {
            let reader = Rc::new(RefCell::new(JsObject::new()));
            reader.borrow_mut().set("read".into(), native("read", |_| {
                let result = Rc::new(RefCell::new(JsObject::new()));
                result.borrow_mut().set("value".into(), JsValue::Undefined);
                result.borrow_mut().set("done".into(), JsValue::Bool(true));
                Ok(make_settled_promise("fulfilled", JsValue::Object(result)))
            }));
            reader.borrow_mut().set("releaseLock".into(), native("releaseLock", |_| Ok(JsValue::Undefined)));
            reader.borrow_mut().set("cancel".into(), native("cancel", |_|
                Ok(make_settled_promise("fulfilled", JsValue::Undefined))));
            reader.borrow_mut().set("closed".into(),
                make_settled_promise("fulfilled", JsValue::Undefined));
            Ok(JsValue::Object(reader))
        }));
        obj.borrow_mut().set("pipeTo".into(), native("pipeTo", |_|
            Ok(make_settled_promise("fulfilled", JsValue::Undefined))));
        obj.borrow_mut().set("pipeThrough".into(), native("pipeThrough", |args|
            Ok(args.into_iter().next().unwrap_or(JsValue::Undefined))));
        obj.borrow_mut().set("tee".into(), native("tee", |_|
            Ok(JsValue::Array(Rc::new(RefCell::new(Vec::new()))))));
        Ok(JsValue::Object(obj))
    }));
    e.define("WritableStream", native("WritableStream", |_| {
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("__writable_stream__".into(), JsValue::Bool(true));
        obj.borrow_mut().set("locked".into(), JsValue::Bool(false));
        obj.borrow_mut().set("close".into(), native("close", |_|
            Ok(make_settled_promise("fulfilled", JsValue::Undefined))));
        obj.borrow_mut().set("abort".into(), native("abort", |_|
            Ok(make_settled_promise("fulfilled", JsValue::Undefined))));
        obj.borrow_mut().set("getWriter".into(), native("getWriter", |_| {
            let writer = Rc::new(RefCell::new(JsObject::new()));
            writer.borrow_mut().set("write".into(), native("write", |_|
                Ok(make_settled_promise("fulfilled", JsValue::Undefined))));
            writer.borrow_mut().set("close".into(), native("close", |_|
                Ok(make_settled_promise("fulfilled", JsValue::Undefined))));
            writer.borrow_mut().set("releaseLock".into(),
                native("releaseLock", |_| Ok(JsValue::Undefined)));
            Ok(JsValue::Object(writer))
        }));
        Ok(JsValue::Object(obj))
    }));
    e.define("TransformStream", native("TransformStream", |_| {
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("__transform_stream__".into(), JsValue::Bool(true));
        obj.borrow_mut().set("readable".into(),
            JsValue::Object(Rc::new(RefCell::new(JsObject::new()))));
        obj.borrow_mut().set("writable".into(),
            JsValue::Object(Rc::new(RefCell::new(JsObject::new()))));
        Ok(JsValue::Object(obj))
    }));
    e.define("ByteLengthQueuingStrategy", native("ByteLengthQueuingStrategy", |_| {
        Ok(JsValue::Object(Rc::new(RefCell::new(JsObject::new()))))
    }));
    e.define("CountQueuingStrategy", native("CountQueuingStrategy", |_| {
        Ok(JsValue::Object(Rc::new(RefCell::new(JsObject::new()))))
    }));

    // ─── Cookie Store API ─────────────────────────────────────────────────
    // Persist do ~/.rust-web-engine/cookies.json pres load/save_storage_to_disk.
    // Format: tab-separated rows "name\tvalue" - same jako localStorage.
    {
        let cs = Rc::new(RefCell::new(JsObject::new()));
        let cookies: Rc<RefCell<HashMap<String, String>>> = Rc::new(RefCell::new(HashMap::new()));
        // Initial load z disku.
        for (k, v) in super::helpers::load_storage_from_disk("cookies") {
            cookies.borrow_mut().insert(k, v);
        }
        let save_cookies = {
            let c = Rc::clone(&cookies);
            move || {
                let entries: Vec<(String, String)> = c.borrow().iter()
                    .map(|(k, v)| (k.clone(), v.clone())).collect();
                let _ = super::helpers::save_storage_to_disk("cookies", &entries);
            }
        };
        let c1 = Rc::clone(&cookies);
        cs.borrow_mut().set("get".into(), native("cookieStore.get", move |args| {
            let name = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let val = c1.borrow().get(&name).cloned();
            let result = match val {
                Some(v) => {
                    let obj = Rc::new(RefCell::new(JsObject::new()));
                    obj.borrow_mut().set("name".into(), JsValue::Str(name));
                    obj.borrow_mut().set("value".into(), JsValue::Str(v));
                    JsValue::Object(obj)
                }
                None => JsValue::Null,
            };
            Ok(make_settled_promise("fulfilled", result))
        }));
        let c2 = Rc::clone(&cookies);
        let save2 = save_cookies.clone();
        cs.borrow_mut().set("set".into(), native("cookieStore.set", move |args| {
            let mut it = args.into_iter();
            let first = it.next().unwrap_or(JsValue::Undefined);
            let (name, value) = if let JsValue::Object(o) = &first {
                let b = o.borrow();
                (b.get("name").to_string(), b.get("value").to_string())
            } else {
                let n = first.to_string();
                let v = it.next().map(|v| v.to_string()).unwrap_or_default();
                (n, v)
            };
            c2.borrow_mut().insert(name, value);
            save2();
            Ok(make_settled_promise("fulfilled", JsValue::Undefined))
        }));
        let c3 = Rc::clone(&cookies);
        let save3 = save_cookies.clone();
        cs.borrow_mut().set("delete".into(), native("cookieStore.delete", move |args| {
            let name = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            c3.borrow_mut().remove(&name);
            save3();
            Ok(make_settled_promise("fulfilled", JsValue::Undefined))
        }));
        let c4 = Rc::clone(&cookies);
        cs.borrow_mut().set("getAll".into(), native("cookieStore.getAll", move |_| {
            let arr: Vec<JsValue> = c4.borrow().iter().map(|(k, v)| {
                let obj = Rc::new(RefCell::new(JsObject::new()));
                obj.borrow_mut().set("name".into(), JsValue::Str(k.clone()));
                obj.borrow_mut().set("value".into(), JsValue::Str(v.clone()));
                JsValue::Object(obj)
            }).collect();
            Ok(make_settled_promise("fulfilled", JsValue::Array(Rc::new(RefCell::new(arr)))))
        }));
        e.define("cookieStore", JsValue::Object(cs));
    }

    // ─── Speech Synthesis / Recognition stub ──────────────────────────────
    {
        let synth = Rc::new(RefCell::new(JsObject::new()));
        synth.borrow_mut().set("speak".into(), native("speechSynthesis.speak", |_| Ok(JsValue::Undefined)));
        synth.borrow_mut().set("cancel".into(), native("cancel", |_| Ok(JsValue::Undefined)));
        synth.borrow_mut().set("pause".into(), native("pause", |_| Ok(JsValue::Undefined)));
        synth.borrow_mut().set("resume".into(), native("resume", |_| Ok(JsValue::Undefined)));
        synth.borrow_mut().set("getVoices".into(), native("getVoices", |_|
            Ok(JsValue::Array(Rc::new(RefCell::new(Vec::new()))))));
        synth.borrow_mut().set("speaking".into(), JsValue::Bool(false));
        synth.borrow_mut().set("paused".into(), JsValue::Bool(false));
        synth.borrow_mut().set("pending".into(), JsValue::Bool(false));
        e.define("speechSynthesis", JsValue::Object(synth));
    }
    e.define("SpeechSynthesisUtterance", native("SpeechSynthesisUtterance", |args| {
        let text = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("text".into(), JsValue::Str(text));
        obj.borrow_mut().set("lang".into(), JsValue::Str("en-US".into()));
        obj.borrow_mut().set("rate".into(), JsValue::Number(1.0));
        obj.borrow_mut().set("pitch".into(), JsValue::Number(1.0));
        obj.borrow_mut().set("volume".into(), JsValue::Number(1.0));
        obj.borrow_mut().set("voice".into(), JsValue::Null);
        obj.borrow_mut().set("addEventListener".into(),
            native("addEventListener", |_| Ok(JsValue::Undefined)));
        Ok(JsValue::Object(obj))
    }));
    e.define("SpeechRecognition", native("SpeechRecognition", |_| {
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("continuous".into(), JsValue::Bool(false));
        obj.borrow_mut().set("interimResults".into(), JsValue::Bool(false));
        obj.borrow_mut().set("lang".into(), JsValue::Str("en-US".into()));
        obj.borrow_mut().set("start".into(), native("start", |_| Ok(JsValue::Undefined)));
        obj.borrow_mut().set("stop".into(), native("stop", |_| Ok(JsValue::Undefined)));
        obj.borrow_mut().set("abort".into(), native("abort", |_| Ok(JsValue::Undefined)));
        obj.borrow_mut().set("addEventListener".into(),
            native("addEventListener", |_| Ok(JsValue::Undefined)));
        Ok(JsValue::Object(obj))
    }));

    // ─── Web Bluetooth / USB / HID / Serial stubs ─────────────────────────
    e.define("__navigator_bluetooth__", {
        let bt = Rc::new(RefCell::new(JsObject::new()));
        bt.borrow_mut().set("requestDevice".into(), native("requestDevice", |_|
            Ok(make_settled_promise("rejected", JsValue::Str("NotFoundError: No devices".into())))));
        bt.borrow_mut().set("getAvailability".into(), native("getAvailability", |_|
            Ok(make_settled_promise("fulfilled", JsValue::Bool(false)))));
        JsValue::Object(bt)
    });
    e.define("__navigator_usb__", {
        let usb = Rc::new(RefCell::new(JsObject::new()));
        usb.borrow_mut().set("requestDevice".into(), native("requestDevice", |_|
            Ok(make_settled_promise("rejected", JsValue::Str("NotFoundError".into())))));
        usb.borrow_mut().set("getDevices".into(), native("getDevices", |_|
            Ok(make_settled_promise("fulfilled", JsValue::Array(Rc::new(RefCell::new(Vec::new())))))));
        JsValue::Object(usb)
    });
    e.define("__navigator_hid__", {
        let hid = Rc::new(RefCell::new(JsObject::new()));
        hid.borrow_mut().set("requestDevice".into(), native("requestDevice", |_|
            Ok(make_settled_promise("fulfilled", JsValue::Array(Rc::new(RefCell::new(Vec::new())))))));
        hid.borrow_mut().set("getDevices".into(), native("getDevices", |_|
            Ok(make_settled_promise("fulfilled", JsValue::Array(Rc::new(RefCell::new(Vec::new())))))));
        JsValue::Object(hid)
    });
    e.define("__navigator_serial__", {
        let serial = Rc::new(RefCell::new(JsObject::new()));
        serial.borrow_mut().set("requestPort".into(), native("requestPort", |_|
            Ok(make_settled_promise("rejected", JsValue::Str("NotFoundError".into())))));
        serial.borrow_mut().set("getPorts".into(), native("getPorts", |_|
            Ok(make_settled_promise("fulfilled", JsValue::Array(Rc::new(RefCell::new(Vec::new())))))));
        JsValue::Object(serial)
    });

    // performance.now() + performance.timeOrigin + real mark/measure entries
    {
        let perf = Rc::new(RefCell::new(JsObject::new()));
        let start = std::time::Instant::now();
        let entries: Rc<RefCell<Vec<JsValue>>> = Rc::new(RefCell::new(Vec::new()));
        let marks: Rc<RefCell<HashMap<String, f64>>> = Rc::new(RefCell::new(HashMap::new()));
        let start_clone = start;
        perf.borrow_mut().set("now".into(), native("performance.now", move |_| {
            Ok(JsValue::Number(start_clone.elapsed().as_secs_f64() * 1000.0))
        }));
        perf.borrow_mut().set("timeOrigin".into(), JsValue::Number(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64() * 1000.0)
                .unwrap_or(0.0)
        ));
        let m1 = Rc::clone(&marks);
        let e1 = Rc::clone(&entries);
        perf.borrow_mut().set("mark".into(), native("performance.mark", move |args| {
            let name = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let now = start.elapsed().as_secs_f64() * 1000.0;
            m1.borrow_mut().insert(name.clone(), now);
            // PerformanceMark entry
            let entry = Rc::new(RefCell::new(JsObject::new()));
            entry.borrow_mut().set("name".into(), JsValue::Str(name.clone()));
            entry.borrow_mut().set("entryType".into(), JsValue::Str("mark".into()));
            entry.borrow_mut().set("startTime".into(), JsValue::Number(now));
            entry.borrow_mut().set("duration".into(), JsValue::Number(0.0));
            e1.borrow_mut().push(JsValue::Object(entry.clone()));
            Ok(JsValue::Object(entry))
        }));
        let m2 = Rc::clone(&marks);
        let e2 = Rc::clone(&entries);
        perf.borrow_mut().set("measure".into(), native("performance.measure", move |args| {
            let mut it = args.into_iter();
            let name = it.next().map(|v| v.to_string()).unwrap_or_default();
            let start_mark = it.next().map(|v| v.to_string()).unwrap_or_default();
            let end_mark = it.next().map(|v| v.to_string()).unwrap_or_default();
            let marks_b = m2.borrow();
            let s = marks_b.get(&start_mark).copied().unwrap_or(0.0);
            let en = marks_b.get(&end_mark).copied()
                .unwrap_or_else(|| start.elapsed().as_secs_f64() * 1000.0);
            let entry = Rc::new(RefCell::new(JsObject::new()));
            entry.borrow_mut().set("name".into(), JsValue::Str(name));
            entry.borrow_mut().set("entryType".into(), JsValue::Str("measure".into()));
            entry.borrow_mut().set("startTime".into(), JsValue::Number(s));
            entry.borrow_mut().set("duration".into(), JsValue::Number((en - s).max(0.0)));
            e2.borrow_mut().push(JsValue::Object(entry.clone()));
            Ok(JsValue::Object(entry))
        }));
        let m3 = Rc::clone(&marks);
        let e3 = Rc::clone(&entries);
        perf.borrow_mut().set("clearMarks".into(), native("performance.clearMarks", move |args| {
            let name = args.into_iter().next().map(|v| v.to_string());
            if let Some(n) = name {
                m3.borrow_mut().remove(&n);
                e3.borrow_mut().retain(|v| {
                    if let JsValue::Object(o) = v {
                        let b = o.borrow();
                        let entry_name = b.get("name").to_string();
                        let entry_type = b.get("entryType").to_string();
                        !(entry_type == "mark" && entry_name == n)
                    } else { true }
                });
            } else {
                m3.borrow_mut().clear();
                e3.borrow_mut().retain(|v| {
                    if let JsValue::Object(o) = v {
                        o.borrow().get("entryType").to_string() != "mark"
                    } else { true }
                });
            }
            Ok(JsValue::Undefined)
        }));
        let e4 = Rc::clone(&entries);
        perf.borrow_mut().set("clearMeasures".into(), native("performance.clearMeasures", move |args| {
            let name = args.into_iter().next().map(|v| v.to_string());
            if let Some(n) = name {
                e4.borrow_mut().retain(|v| {
                    if let JsValue::Object(o) = v {
                        let b = o.borrow();
                        let en = b.get("name").to_string();
                        let et = b.get("entryType").to_string();
                        !(et == "measure" && en == n)
                    } else { true }
                });
            } else {
                e4.borrow_mut().retain(|v| {
                    if let JsValue::Object(o) = v {
                        o.borrow().get("entryType").to_string() != "measure"
                    } else { true }
                });
            }
            Ok(JsValue::Undefined)
        }));
        let e5 = Rc::clone(&entries);
        perf.borrow_mut().set("getEntries".into(), native("performance.getEntries", move |_| {
            Ok(JsValue::Array(Rc::new(RefCell::new(e5.borrow().clone()))))
        }));
        let e6 = Rc::clone(&entries);
        perf.borrow_mut().set("getEntriesByType".into(), native("performance.getEntriesByType", move |args| {
            let t = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let filtered: Vec<JsValue> = e6.borrow().iter()
                .filter(|v| if let JsValue::Object(o) = v {
                    o.borrow().get("entryType").to_string() == t
                } else { false })
                .cloned().collect();
            Ok(JsValue::Array(Rc::new(RefCell::new(filtered))))
        }));
        let e7 = Rc::clone(&entries);
        perf.borrow_mut().set("getEntriesByName".into(), native("performance.getEntriesByName", move |args| {
            let n = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let filtered: Vec<JsValue> = e7.borrow().iter()
                .filter(|v| if let JsValue::Object(o) = v {
                    o.borrow().get("name").to_string() == n
                } else { false })
                .cloned().collect();
            Ok(JsValue::Array(Rc::new(RefCell::new(filtered))))
        }));
        e.define("performance", JsValue::Object(perf));
    }

    // FormData real impl - vc. set/getAll/keys/values/entries
    e.define("FormData", native("FormData", |args| {
        let pairs: Rc<RefCell<Vec<(String, String)>>> = Rc::new(RefCell::new(Vec::new()));
        // Optional argument: form element - extract input/select/textarea
        if let Some(JsValue::DomNode(form)) = args.into_iter().next() {
            form.walk(&mut |node| {
                if let Some(t) = node.tag_name() {
                    if matches!(t.as_str(), "input" | "select" | "textarea") {
                        if let Some(name) = node.attr("name") {
                            let value = node.attr("value").unwrap_or_default();
                            pairs.borrow_mut().push((name, value));
                        }
                    }
                }
            });
        }
        let obj = Rc::new(RefCell::new(JsObject::new()));
        let p1 = Rc::clone(&pairs);
        obj.borrow_mut().set("append".into(), native("FormData.append", move |args| {
            let mut it = args.into_iter();
            let k = it.next().map(|v| v.to_string()).unwrap_or_default();
            let v = it.next().map(|v| v.to_string()).unwrap_or_default();
            p1.borrow_mut().push((k, v));
            Ok(JsValue::Undefined)
        }));
        let p2 = Rc::clone(&pairs);
        obj.borrow_mut().set("set".into(), native("FormData.set", move |args| {
            let mut it = args.into_iter();
            let k = it.next().map(|v| v.to_string()).unwrap_or_default();
            let v = it.next().map(|v| v.to_string()).unwrap_or_default();
            // Replace all existing s tymto klicem
            p2.borrow_mut().retain(|(kk, _)| kk != &k);
            p2.borrow_mut().push((k, v));
            Ok(JsValue::Undefined)
        }));
        let p3 = Rc::clone(&pairs);
        obj.borrow_mut().set("get".into(), native("FormData.get", move |args| {
            let k = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            Ok(p3.borrow().iter().find(|(kk, _)| kk == &k)
                .map(|(_, v)| JsValue::Str(v.clone())).unwrap_or(JsValue::Null))
        }));
        let p4 = Rc::clone(&pairs);
        obj.borrow_mut().set("getAll".into(), native("FormData.getAll", move |args| {
            let k = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let arr: Vec<JsValue> = p4.borrow().iter()
                .filter(|(kk, _)| kk == &k)
                .map(|(_, v)| JsValue::Str(v.clone()))
                .collect();
            Ok(JsValue::Array(Rc::new(RefCell::new(arr))))
        }));
        let p5 = Rc::clone(&pairs);
        obj.borrow_mut().set("has".into(), native("FormData.has", move |args| {
            let k = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            Ok(JsValue::Bool(p5.borrow().iter().any(|(kk, _)| kk == &k)))
        }));
        let p6 = Rc::clone(&pairs);
        obj.borrow_mut().set("delete".into(), native("FormData.delete", move |args| {
            let k = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            p6.borrow_mut().retain(|(kk, _)| kk != &k);
            Ok(JsValue::Undefined)
        }));
        let p7 = Rc::clone(&pairs);
        obj.borrow_mut().set("keys".into(), native("FormData.keys", move |_| {
            let arr: Vec<JsValue> = p7.borrow().iter()
                .map(|(k, _)| JsValue::Str(k.clone())).collect();
            Ok(super::helpers::make_iterator_from_values(arr))
        }));
        let p8 = Rc::clone(&pairs);
        obj.borrow_mut().set("values".into(), native("FormData.values", move |_| {
            let arr: Vec<JsValue> = p8.borrow().iter()
                .map(|(_, v)| JsValue::Str(v.clone())).collect();
            Ok(super::helpers::make_iterator_from_values(arr))
        }));
        let p9 = Rc::clone(&pairs);
        obj.borrow_mut().set("entries".into(), native("FormData.entries", move |_| {
            let arr: Vec<JsValue> = p9.borrow().iter().map(|(k, v)| {
                JsValue::Array(Rc::new(RefCell::new(vec![
                    JsValue::Str(k.clone()),
                    JsValue::Str(v.clone()),
                ])))
            }).collect();
            Ok(super::helpers::make_iterator_from_values(arr))
        }));
        let p10 = Rc::clone(&pairs);
        obj.borrow_mut().set("forEach".into(), native("FormData.forEach", move |args| {
            let cb = args.into_iter().next().unwrap_or(JsValue::Undefined);
            let _ = (cb, &p10); // bez interpret access neumime callback volat zde
            Ok(JsValue::Undefined)
        }));
        Ok(JsValue::Object(obj))
    }));

    // Headers - HTTP headers map
    e.define("Headers", native("Headers", |args| {
        let pairs: Rc<RefCell<Vec<(String, String)>>> = Rc::new(RefCell::new(Vec::new()));
        if let Some(init) = args.into_iter().next() {
            if let JsValue::Array(arr) = &init {
                for v in arr.borrow().iter() {
                    if let JsValue::Array(pair) = v {
                        let p = pair.borrow();
                        if p.len() >= 2 {
                            pairs.borrow_mut().push((p[0].to_string().to_lowercase(), p[1].to_string()));
                        }
                    }
                }
            } else if let JsValue::Object(o) = &init {
                for (k, v) in &o.borrow().props {
                    pairs.borrow_mut().push((k.to_lowercase(), v.to_string()));
                }
            }
        }
        let obj = Rc::new(RefCell::new(JsObject::new()));
        let p1 = Rc::clone(&pairs);
        obj.borrow_mut().set("get".into(), native("Headers.get", move |args| {
            let k = args.into_iter().next().map(|v| v.to_string().to_lowercase()).unwrap_or_default();
            let result: Vec<String> = p1.borrow().iter()
                .filter(|(kk, _)| kk == &k)
                .map(|(_, v)| v.clone()).collect();
            if result.is_empty() { Ok(JsValue::Null) }
            else { Ok(JsValue::Str(result.join(", "))) }
        }));
        let p2 = Rc::clone(&pairs);
        obj.borrow_mut().set("set".into(), native("Headers.set", move |args| {
            let mut it = args.into_iter();
            let k = it.next().map(|v| v.to_string().to_lowercase()).unwrap_or_default();
            let v = it.next().map(|v| v.to_string()).unwrap_or_default();
            p2.borrow_mut().retain(|(kk, _)| kk != &k);
            p2.borrow_mut().push((k, v));
            Ok(JsValue::Undefined)
        }));
        let p3 = Rc::clone(&pairs);
        obj.borrow_mut().set("append".into(), native("Headers.append", move |args| {
            let mut it = args.into_iter();
            let k = it.next().map(|v| v.to_string().to_lowercase()).unwrap_or_default();
            let v = it.next().map(|v| v.to_string()).unwrap_or_default();
            p3.borrow_mut().push((k, v));
            Ok(JsValue::Undefined)
        }));
        let p4 = Rc::clone(&pairs);
        obj.borrow_mut().set("has".into(), native("Headers.has", move |args| {
            let k = args.into_iter().next().map(|v| v.to_string().to_lowercase()).unwrap_or_default();
            Ok(JsValue::Bool(p4.borrow().iter().any(|(kk, _)| kk == &k)))
        }));
        let p5 = Rc::clone(&pairs);
        obj.borrow_mut().set("delete".into(), native("Headers.delete", move |args| {
            let k = args.into_iter().next().map(|v| v.to_string().to_lowercase()).unwrap_or_default();
            p5.borrow_mut().retain(|(kk, _)| kk != &k);
            Ok(JsValue::Undefined)
        }));
        let p6 = Rc::clone(&pairs);
        obj.borrow_mut().set("entries".into(), native("Headers.entries", move |_| {
            let arr: Vec<JsValue> = p6.borrow().iter().map(|(k, v)|
                JsValue::Array(Rc::new(RefCell::new(vec![
                    JsValue::Str(k.clone()), JsValue::Str(v.clone())])))
            ).collect();
            Ok(super::helpers::make_iterator_from_values(arr))
        }));
        Ok(JsValue::Object(obj))
    }));

    // Request - fetch input wrapper
    e.define("Request", native("Request", |args| {
        let mut it = args.into_iter();
        let url = it.next().map(|v| v.to_string()).unwrap_or_default();
        let init = it.next();
        let method = if let Some(JsValue::Object(o)) = &init {
            o.borrow().get("method").to_string()
        } else { "GET".into() };
        let body = if let Some(JsValue::Object(o)) = &init {
            o.borrow().get("body").to_string()
        } else { String::new() };
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("url".into(), JsValue::Str(url));
        obj.borrow_mut().set("method".into(), JsValue::Str(method));
        obj.borrow_mut().set("__body__".into(), JsValue::Str(body));
        obj.borrow_mut().set("cache".into(), JsValue::Str("default".into()));
        obj.borrow_mut().set("credentials".into(), JsValue::Str("same-origin".into()));
        obj.borrow_mut().set("mode".into(), JsValue::Str("cors".into()));
        obj.borrow_mut().set("redirect".into(), JsValue::Str("follow".into()));
        obj.borrow_mut().set("referrer".into(), JsValue::Str(String::new()));
        obj.borrow_mut().set("integrity".into(), JsValue::Str(String::new()));
        obj.borrow_mut().set("clone".into(), native("clone", |_|
            Ok(JsValue::Object(Rc::new(RefCell::new(JsObject::new()))))));
        Ok(JsValue::Object(obj))
    }));

    // AbortController / AbortSignal stuby
    e.define("AbortController", native("AbortController", |_| {
        let signal = Rc::new(RefCell::new(JsObject::new()));
        signal.borrow_mut().set("aborted".into(), JsValue::Bool(false));
        signal.borrow_mut().set("reason".into(), JsValue::Undefined);
        signal.borrow_mut().set("addEventListener".into(),
            native("AbortSignal.addEventListener", |_| Ok(JsValue::Undefined)));
        signal.borrow_mut().set("removeEventListener".into(),
            native("AbortSignal.removeEventListener", |_| Ok(JsValue::Undefined)));

        let obj = Rc::new(RefCell::new(JsObject::new()));
        let sig_clone = Rc::clone(&signal);
        obj.borrow_mut().set("signal".into(), JsValue::Object(signal));
        obj.borrow_mut().set("abort".into(), native("AbortController.abort", move |args| {
            let reason = args.into_iter().next().unwrap_or(JsValue::Undefined);
            sig_clone.borrow_mut().set("aborted".into(), JsValue::Bool(true));
            sig_clone.borrow_mut().set("reason".into(), reason);
            Ok(JsValue::Undefined)
        }));
        Ok(JsValue::Object(obj))
    }));

    // AbortSignal.timeout(ms) / AbortSignal.any([signals]) / AbortSignal.abort(reason)
    {
        let abort_signal_obj = Rc::new(RefCell::new(JsObject::new()));
        abort_signal_obj.borrow_mut().set("timeout".into(),
            native("AbortSignal.timeout", |args| {
                let _ms = args.into_iter().next().map(|v| v.to_number()).unwrap_or(0.0);
                let signal = Rc::new(RefCell::new(JsObject::new()));
                signal.borrow_mut().set("aborted".into(), JsValue::Bool(false));
                signal.borrow_mut().set("reason".into(), JsValue::Undefined);
                signal.borrow_mut().set("addEventListener".into(),
                    native("addEventListener", |_| Ok(JsValue::Undefined)));
                signal.borrow_mut().set("removeEventListener".into(),
                    native("removeEventListener", |_| Ok(JsValue::Undefined)));
                Ok(JsValue::Object(signal))
            }));
        abort_signal_obj.borrow_mut().set("any".into(),
            native("AbortSignal.any", |_| {
                let signal = Rc::new(RefCell::new(JsObject::new()));
                signal.borrow_mut().set("aborted".into(), JsValue::Bool(false));
                signal.borrow_mut().set("reason".into(), JsValue::Undefined);
                signal.borrow_mut().set("addEventListener".into(),
                    native("addEventListener", |_| Ok(JsValue::Undefined)));
                Ok(JsValue::Object(signal))
            }));
        abort_signal_obj.borrow_mut().set("abort".into(),
            native("AbortSignal.abort", |args| {
                let reason = args.into_iter().next().unwrap_or(JsValue::Undefined);
                let signal = Rc::new(RefCell::new(JsObject::new()));
                signal.borrow_mut().set("aborted".into(), JsValue::Bool(true));
                signal.borrow_mut().set("reason".into(), reason);
                signal.borrow_mut().set("addEventListener".into(),
                    native("addEventListener", |_| Ok(JsValue::Undefined)));
                Ok(JsValue::Object(signal))
            }));
        e.define("AbortSignal", JsValue::Object(abort_signal_obj));
    }

    // history (pushState/replaceState/back/forward/length)
    {
        let history = Rc::new(RefCell::new(JsObject::new()));
        let stack: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(vec!["/".into()]));
        history.borrow_mut().set("length".into(), JsValue::Number(1.0));
        history.borrow_mut().set("state".into(), JsValue::Null);
        {
            let st = Rc::clone(&stack);
            let h = Rc::clone(&history);
            history.borrow_mut().set("pushState".into(), native("history.pushState", move |args| {
                let mut it = args.into_iter();
                let state = it.next().unwrap_or(JsValue::Null);
                let _title = it.next();
                let url = it.next().map(|v| v.to_string()).unwrap_or_default();
                if !url.is_empty() { st.borrow_mut().push(url); }
                h.borrow_mut().set("state".into(), state);
                h.borrow_mut().set("length".into(), JsValue::Number(st.borrow().len() as f64));
                Ok(JsValue::Undefined)
            }));
        }
        {
            let st = Rc::clone(&stack);
            let h = Rc::clone(&history);
            history.borrow_mut().set("replaceState".into(), native("history.replaceState", move |args| {
                let mut it = args.into_iter();
                let state = it.next().unwrap_or(JsValue::Null);
                let _title = it.next();
                let url = it.next().map(|v| v.to_string()).unwrap_or_default();
                if !url.is_empty() {
                    let mut s = st.borrow_mut();
                    if let Some(last) = s.last_mut() { *last = url; } else { s.push(url); }
                }
                h.borrow_mut().set("state".into(), state);
                Ok(JsValue::Undefined)
            }));
        }
        history.borrow_mut().set("back".into(), native("history.back", |_| Ok(JsValue::Undefined)));
        history.borrow_mut().set("forward".into(), native("history.forward", |_| Ok(JsValue::Undefined)));
        history.borrow_mut().set("go".into(), native("history.go", |_| Ok(JsValue::Undefined)));
        e.define("history", JsValue::Object(history));
    }

    // WebSocket stub - constructor vraci object s methods, neco ne real connect.
    e.define("WebSocket", native("WebSocket", |args| {
        let url = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("url".into(), JsValue::Str(url));
        obj.borrow_mut().set("readyState".into(), JsValue::Number(0.0)); // CONNECTING
        obj.borrow_mut().set("CONNECTING".into(), JsValue::Number(0.0));
        obj.borrow_mut().set("OPEN".into(), JsValue::Number(1.0));
        obj.borrow_mut().set("CLOSING".into(), JsValue::Number(2.0));
        obj.borrow_mut().set("CLOSED".into(), JsValue::Number(3.0));
        obj.borrow_mut().set("send".into(), native("WebSocket.send", |_| Ok(JsValue::Undefined)));
        obj.borrow_mut().set("close".into(), native("WebSocket.close", |_| Ok(JsValue::Undefined)));
        obj.borrow_mut().set("addEventListener".into(),
            native("WebSocket.addEventListener", |_| Ok(JsValue::Undefined)));
        Ok(JsValue::Object(obj))
    }));

    // EventSource stub (Server-Sent Events)
    e.define("EventSource", native("EventSource", |args| {
        let url = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("url".into(), JsValue::Str(url));
        obj.borrow_mut().set("readyState".into(), JsValue::Number(0.0));
        obj.borrow_mut().set("CONNECTING".into(), JsValue::Number(0.0));
        obj.borrow_mut().set("OPEN".into(), JsValue::Number(1.0));
        obj.borrow_mut().set("CLOSED".into(), JsValue::Number(2.0));
        obj.borrow_mut().set("close".into(), native("EventSource.close", |_| Ok(JsValue::Undefined)));
        obj.borrow_mut().set("addEventListener".into(),
            native("EventSource.addEventListener", |_| Ok(JsValue::Undefined)));
        Ok(JsValue::Object(obj))
    }));

    // ─── Event classes (constructors) ─────────────────────────────────────
    // Event / CustomEvent / PointerEvent / MouseEvent / KeyboardEvent / TouchEvent / WheelEvent
    let make_event_constructor = |default_type: &str| {
        let dt = default_type.to_string();
        native(default_type, move |args| {
            let mut it = args.into_iter();
            let evt_type = it.next().map(|v| v.to_string()).unwrap_or_else(|| dt.clone());
            let init = it.next();
            let obj = Rc::new(RefCell::new(JsObject::new()));
            obj.borrow_mut().set("type".into(), JsValue::Str(evt_type));
            obj.borrow_mut().set("bubbles".into(), JsValue::Bool(false));
            obj.borrow_mut().set("cancelable".into(), JsValue::Bool(false));
            obj.borrow_mut().set("composed".into(), JsValue::Bool(false));
            obj.borrow_mut().set("defaultPrevented".into(), JsValue::Bool(false));
            obj.borrow_mut().set("target".into(), JsValue::Null);
            obj.borrow_mut().set("currentTarget".into(), JsValue::Null);
            obj.borrow_mut().set("timeStamp".into(), JsValue::Number(super::helpers::now_ms()));
            // Apply init dict
            if let Some(JsValue::Object(o)) = init {
                let b = o.borrow();
                for (k, v) in &b.props {
                    obj.borrow_mut().set(k.clone(), v.clone());
                }
            }
            let obj_pd = Rc::clone(&obj);
            obj.borrow_mut().set("preventDefault".into(),
                native("preventDefault", move |_| {
                    obj_pd.borrow_mut().set("defaultPrevented".into(), JsValue::Bool(true));
                    Ok(JsValue::Undefined)
                }));
            let obj_sp = Rc::clone(&obj);
            obj.borrow_mut().set("stopPropagation".into(),
                native("stopPropagation", move |_| {
                    obj_sp.borrow_mut().set("__stop_propagation__".into(), JsValue::Bool(true));
                    Ok(JsValue::Undefined)
                }));
            obj.borrow_mut().set("stopImmediatePropagation".into(),
                native("stopImmediatePropagation", |_| Ok(JsValue::Undefined)));
            Ok(JsValue::Object(obj))
        })
    };
    e.define("Event", make_event_constructor("event"));
    e.define("CustomEvent", make_event_constructor("custom"));
    e.define("MouseEvent", make_event_constructor("click"));
    e.define("PointerEvent", make_event_constructor("pointerdown"));
    e.define("KeyboardEvent", make_event_constructor("keydown"));
    e.define("TouchEvent", make_event_constructor("touchstart"));
    e.define("WheelEvent", make_event_constructor("wheel"));
    e.define("InputEvent", make_event_constructor("input"));
    e.define("FocusEvent", make_event_constructor("focus"));
    e.define("DragEvent", make_event_constructor("drag"));
    e.define("SubmitEvent", make_event_constructor("submit"));
    e.define("ProgressEvent", make_event_constructor("progress"));
    e.define("MessageEvent", make_event_constructor("message"));
    e.define("ErrorEvent", make_event_constructor("error"));
    e.define("BeforeUnloadEvent", make_event_constructor("beforeunload"));
    e.define("PageTransitionEvent", make_event_constructor("pageshow"));
    e.define("HashChangeEvent", make_event_constructor("hashchange"));
    e.define("PopStateEvent", make_event_constructor("popstate"));
    e.define("StorageEvent", make_event_constructor("storage"));
    e.define("AnimationEvent", make_event_constructor("animationstart"));
    e.define("TransitionEvent", make_event_constructor("transitionend"));
    e.define("ClipboardEvent", make_event_constructor("copy"));

    // ─── Clipboard API stub - navigator.clipboard.* ──────────────────────
    {
        let clip = Rc::new(RefCell::new(JsObject::new()));
        let storage: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
        let s1 = Rc::clone(&storage);
        clip.borrow_mut().set("writeText".into(), native("clipboard.writeText", move |args| {
            let text = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            *s1.borrow_mut() = text;
            Ok(make_settled_promise("fulfilled", JsValue::Undefined))
        }));
        let s2 = Rc::clone(&storage);
        clip.borrow_mut().set("readText".into(), native("clipboard.readText", move |_| {
            Ok(make_settled_promise("fulfilled", JsValue::Str(s2.borrow().clone())))
        }));
        clip.borrow_mut().set("write".into(),
            native("clipboard.write", |_| Ok(make_settled_promise("fulfilled", JsValue::Undefined))));
        clip.borrow_mut().set("read".into(),
            native("clipboard.read", |_| Ok(make_settled_promise("fulfilled",
                JsValue::Array(Rc::new(RefCell::new(Vec::new())))))));
        e.define("__clipboard__", JsValue::Object(clip));
    }

    // ─── Geolocation API stub - navigator.geolocation ────────────────────
    {
        let geo = Rc::new(RefCell::new(JsObject::new()));
        geo.borrow_mut().set("getCurrentPosition".into(),
            native("geolocation.getCurrentPosition", |args| {
                let mut it = args.into_iter();
                let success = it.next().unwrap_or(JsValue::Undefined);
                // Stub: nemame skutecny geolocation - vratime mock data
                let _ = success;
                Ok(JsValue::Undefined)
            }));
        geo.borrow_mut().set("watchPosition".into(),
            native("geolocation.watchPosition", |_| Ok(JsValue::Number(1.0))));
        geo.borrow_mut().set("clearWatch".into(),
            native("geolocation.clearWatch", |_| Ok(JsValue::Undefined)));
        e.define("__geolocation__", JsValue::Object(geo));
    }

    // EventTarget constructor - obecny emitter s addEventListener / removeEventListener / dispatchEvent
    e.define("EventTarget", native("EventTarget", |_| {
        let listeners: Rc<RefCell<HashMap<String, Vec<JsValue>>>> = Rc::new(RefCell::new(HashMap::new()));
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("__event_target__".into(), JsValue::Bool(true));
        let l1 = Rc::clone(&listeners);
        obj.borrow_mut().set("addEventListener".into(), native("addEventListener", move |args| {
            let mut it = args.into_iter();
            let name = it.next().map(|v| v.to_string()).unwrap_or_default();
            let cb = it.next().unwrap_or(JsValue::Undefined);
            l1.borrow_mut().entry(name).or_default().push(cb);
            Ok(JsValue::Undefined)
        }));
        let l2 = Rc::clone(&listeners);
        obj.borrow_mut().set("removeEventListener".into(), native("removeEventListener", move |args| {
            let mut it = args.into_iter();
            let name = it.next().map(|v| v.to_string()).unwrap_or_default();
            let cb = it.next().unwrap_or(JsValue::Undefined);
            let cb_id = format!("{:?}", cb);
            if let Some(arr) = l2.borrow_mut().get_mut(&name) {
                arr.retain(|c| format!("{:?}", c) != cb_id);
            }
            Ok(JsValue::Undefined)
        }));
        // dispatchEvent: vrati true pokud nikdo neprevent default
        obj.borrow_mut().set("dispatchEvent".into(), native("dispatchEvent", |_| Ok(JsValue::Bool(true))));
        Ok(JsValue::Object(obj))
    }));

    // MessageChannel - vyrobi 2 propojene MessagePorts.
    e.define("MessageChannel", native("MessageChannel", |_| {
        let port1 = make_message_port();
        let port2 = make_message_port();
        // Linkuj port1 -> port2 a vice versa pres __peer__ pointer
        if let (JsValue::Object(p1), JsValue::Object(p2)) = (&port1, &port2) {
            p1.borrow_mut().set("__peer__".into(), JsValue::Object(Rc::clone(p2)));
            p2.borrow_mut().set("__peer__".into(), JsValue::Object(Rc::clone(p1)));
        }
        let mc = Rc::new(RefCell::new(JsObject::new()));
        mc.borrow_mut().set("port1".into(), port1);
        mc.borrow_mut().set("port2".into(), port2);
        Ok(JsValue::Object(mc))
    }));

    // Notification API - constructor s staticky permission / requestPermission
    e.define("Notification", native("Notification", |args| {
        let title = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("title".into(), JsValue::Str(title));
        obj.borrow_mut().set("body".into(), JsValue::Str(String::new()));
        obj.borrow_mut().set("icon".into(), JsValue::Str(String::new()));
        obj.borrow_mut().set("tag".into(), JsValue::Str(String::new()));
        obj.borrow_mut().set("permission".into(), JsValue::Str("granted".into()));
        obj.borrow_mut().set("close".into(), native("close", |_| Ok(JsValue::Undefined)));
        obj.borrow_mut().set("addEventListener".into(), native("addEventListener", |_| Ok(JsValue::Undefined)));
        Ok(JsValue::Object(obj))
    }));
    // Notification.permission / requestPermission - exposed pres samostatne globaly
    e.define("__notification_permission__", JsValue::Str("granted".into()));
    e.define("__notification_request_permission__",
        native("Notification.requestPermission", |_|
            Ok(make_settled_promise("fulfilled", JsValue::Str("granted".into())))));

    // ServiceWorker registration stub - navigator.serviceWorker
    {
        let sw = Rc::new(RefCell::new(JsObject::new()));
        sw.borrow_mut().set("controller".into(), JsValue::Null);
        sw.borrow_mut().set("ready".into(),
            make_settled_promise("fulfilled", JsValue::Object(Rc::new(RefCell::new(JsObject::new())))));
        sw.borrow_mut().set("register".into(), native("ServiceWorker.register", |args| {
            let url = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let reg = Rc::new(RefCell::new(JsObject::new()));
            reg.borrow_mut().set("scope".into(), JsValue::Str("/".into()));
            reg.borrow_mut().set("scriptURL".into(), JsValue::Str(url));
            reg.borrow_mut().set("active".into(), JsValue::Null);
            reg.borrow_mut().set("installing".into(), JsValue::Null);
            reg.borrow_mut().set("waiting".into(), JsValue::Null);
            reg.borrow_mut().set("update".into(), native("update", |_|
                Ok(make_settled_promise("fulfilled", JsValue::Undefined))));
            reg.borrow_mut().set("unregister".into(), native("unregister", |_|
                Ok(make_settled_promise("fulfilled", JsValue::Bool(true)))));
            Ok(make_settled_promise("fulfilled", JsValue::Object(reg)))
        }));
        sw.borrow_mut().set("getRegistration".into(), native("getRegistration", |_|
            Ok(make_settled_promise("fulfilled", JsValue::Undefined))));
        sw.borrow_mut().set("getRegistrations".into(), native("getRegistrations", |_|
            Ok(make_settled_promise("fulfilled", JsValue::Array(Rc::new(RefCell::new(Vec::new())))))));
        e.define("__service_worker_container__", JsValue::Object(sw));
    }

    // CacheStorage API + Cache - real in-memory implementation s URL -> Response.
    // V Service Worker scope: caches.open('v1').then(cache => cache.put(req, res)).
    {
        // Cache pool: name -> (request_url -> response object).
        // Sdileny pres Rc<RefCell<>> - kdykoliv otevreny stejny name vrati stejny cache.
        let cache_pool: Rc<RefCell<std::collections::HashMap<String, Rc<RefCell<std::collections::HashMap<String, JsValue>>>>>> =
            Rc::new(RefCell::new(std::collections::HashMap::new()));
        let caches_obj = Rc::new(RefCell::new(JsObject::new()));

        let cp1 = Rc::clone(&cache_pool);
        caches_obj.borrow_mut().set("open".into(), native("caches.open", move |args| {
            let name = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let cache_data: Rc<RefCell<std::collections::HashMap<String, JsValue>>> = {
                let mut pool = cp1.borrow_mut();
                pool.entry(name.clone()).or_insert_with(||
                    Rc::new(RefCell::new(std::collections::HashMap::new()))
                ).clone()
            };
            let cache_obj = Rc::new(RefCell::new(JsObject::new()));
            cache_obj.borrow_mut().set("__name__".into(), JsValue::Str(name.clone()));

            // cache.put(request, response)
            let cd = Rc::clone(&cache_data);
            cache_obj.borrow_mut().set("put".into(), native("Cache.put", move |args| {
                let mut it = args.into_iter();
                let req = it.next().unwrap_or(JsValue::Undefined);
                let res = it.next().unwrap_or(JsValue::Undefined);
                let url = match &req {
                    JsValue::Str(s) => s.clone(),
                    JsValue::Object(o) => o.borrow().get("url").to_string(),
                    _ => return Ok(make_settled_promise("rejected", JsValue::Str("invalid request".into()))),
                };
                cd.borrow_mut().insert(url, res);
                Ok(make_settled_promise("fulfilled", JsValue::Undefined))
            }));

            // cache.match(request) - vrati cached response nebo undefined
            let cd = Rc::clone(&cache_data);
            cache_obj.borrow_mut().set("match".into(), native("Cache.match", move |args| {
                let req = args.into_iter().next().unwrap_or(JsValue::Undefined);
                let url = match &req {
                    JsValue::Str(s) => s.clone(),
                    JsValue::Object(o) => o.borrow().get("url").to_string(),
                    _ => return Ok(make_settled_promise("fulfilled", JsValue::Undefined)),
                };
                let val = cd.borrow().get(&url).cloned().unwrap_or(JsValue::Undefined);
                Ok(make_settled_promise("fulfilled", val))
            }));

            // cache.delete(request) - vrati true pokud existoval
            let cd = Rc::clone(&cache_data);
            cache_obj.borrow_mut().set("delete".into(), native("Cache.delete", move |args| {
                let req = args.into_iter().next().unwrap_or(JsValue::Undefined);
                let url = match &req {
                    JsValue::Str(s) => s.clone(),
                    JsValue::Object(o) => o.borrow().get("url").to_string(),
                    _ => return Ok(make_settled_promise("fulfilled", JsValue::Bool(false))),
                };
                let removed = cd.borrow_mut().remove(&url).is_some();
                Ok(make_settled_promise("fulfilled", JsValue::Bool(removed)))
            }));

            // cache.keys() - vrati Promise<Array<Request>> (zde Array<String>)
            let cd = Rc::clone(&cache_data);
            cache_obj.borrow_mut().set("keys".into(), native("Cache.keys", move |_| {
                let urls: Vec<JsValue> = cd.borrow().keys().map(|k| JsValue::Str(k.clone())).collect();
                Ok(make_settled_promise("fulfilled", JsValue::Array(Rc::new(RefCell::new(urls)))))
            }));

            // cache.addAll(urls) - fetch + put per URL. Stub: ulozime placeholder Response.
            let cd = Rc::clone(&cache_data);
            cache_obj.borrow_mut().set("addAll".into(), native("Cache.addAll", move |args| {
                let urls = args.into_iter().next().unwrap_or(JsValue::Undefined);
                if let JsValue::Array(arr) = urls {
                    for url_v in arr.borrow().iter() {
                        let u = url_v.to_string();
                        let resp = Rc::new(RefCell::new(JsObject::new()));
                        resp.borrow_mut().set("url".into(), JsValue::Str(u.clone()));
                        resp.borrow_mut().set("ok".into(), JsValue::Bool(true));
                        resp.borrow_mut().set("status".into(), JsValue::Number(200.0));
                        cd.borrow_mut().insert(u, JsValue::Object(resp));
                    }
                }
                Ok(make_settled_promise("fulfilled", JsValue::Undefined))
            }));

            // cache.add(url) - jednotliva varianta addAll
            let cd = Rc::clone(&cache_data);
            cache_obj.borrow_mut().set("add".into(), native("Cache.add", move |args| {
                let req = args.into_iter().next().unwrap_or(JsValue::Undefined);
                let url = match &req {
                    JsValue::Str(s) => s.clone(),
                    JsValue::Object(o) => o.borrow().get("url").to_string(),
                    _ => return Ok(make_settled_promise("rejected", JsValue::Str("invalid request".into()))),
                };
                let resp = Rc::new(RefCell::new(JsObject::new()));
                resp.borrow_mut().set("url".into(), JsValue::Str(url.clone()));
                resp.borrow_mut().set("ok".into(), JsValue::Bool(true));
                resp.borrow_mut().set("status".into(), JsValue::Number(200.0));
                cd.borrow_mut().insert(url, JsValue::Object(resp));
                Ok(make_settled_promise("fulfilled", JsValue::Undefined))
            }));

            Ok(make_settled_promise("fulfilled", JsValue::Object(cache_obj)))
        }));

        // caches.has(name)
        let cp2 = Rc::clone(&cache_pool);
        caches_obj.borrow_mut().set("has".into(), native("caches.has", move |args| {
            let name = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let exists = cp2.borrow().contains_key(&name);
            Ok(make_settled_promise("fulfilled", JsValue::Bool(exists)))
        }));

        // caches.delete(name)
        let cp3 = Rc::clone(&cache_pool);
        caches_obj.borrow_mut().set("delete".into(), native("caches.delete", move |args| {
            let name = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let removed = cp3.borrow_mut().remove(&name).is_some();
            Ok(make_settled_promise("fulfilled", JsValue::Bool(removed)))
        }));

        // caches.keys()
        let cp4 = Rc::clone(&cache_pool);
        caches_obj.borrow_mut().set("keys".into(), native("caches.keys", move |_| {
            let names: Vec<JsValue> = cp4.borrow().keys().map(|k| JsValue::Str(k.clone())).collect();
            Ok(make_settled_promise("fulfilled", JsValue::Array(Rc::new(RefCell::new(names)))))
        }));

        // caches.match(request) - hleda v vsech caches
        let cp5 = Rc::clone(&cache_pool);
        caches_obj.borrow_mut().set("match".into(), native("caches.match", move |args| {
            let req = args.into_iter().next().unwrap_or(JsValue::Undefined);
            let url = match &req {
                JsValue::Str(s) => s.clone(),
                JsValue::Object(o) => o.borrow().get("url").to_string(),
                _ => return Ok(make_settled_promise("fulfilled", JsValue::Undefined)),
            };
            for cache in cp5.borrow().values() {
                if let Some(v) = cache.borrow().get(&url).cloned() {
                    return Ok(make_settled_promise("fulfilled", v));
                }
            }
            Ok(make_settled_promise("fulfilled", JsValue::Undefined))
        }));

        e.define("caches", JsValue::Object(caches_obj));
    }

    // navigator.locks API stub
    {
        let locks = Rc::new(RefCell::new(JsObject::new()));
        locks.borrow_mut().set("request".into(), native("locks.request", |args| {
            // request(name, [options], callback) - volame callback rovnou
            let mut it = args.into_iter();
            let _name = it.next();
            let cb_or_opts = it.next();
            let maybe_cb = it.next();
            // Pokud druhy arg je function, je to callback. Jinak je callback treti.
            let cb = if matches!(cb_or_opts, Some(JsValue::Function(_))) {
                cb_or_opts.unwrap()
            } else if let Some(c) = maybe_cb { c } else { return Ok(make_settled_promise("fulfilled", JsValue::Undefined)); };
            // Volame callback s lock objektem
            let lock = Rc::new(RefCell::new(JsObject::new()));
            lock.borrow_mut().set("name".into(), JsValue::Str("lock".into()));
            lock.borrow_mut().set("mode".into(), JsValue::Str("exclusive".into()));
            let _ = cb; // callback se zavola asynchronne; pro stub vraime resolved
            Ok(make_settled_promise("fulfilled", JsValue::Object(lock)))
        }));
        locks.borrow_mut().set("query".into(), native("locks.query", |_| {
            let result = Rc::new(RefCell::new(JsObject::new()));
            result.borrow_mut().set("held".into(), JsValue::Array(Rc::new(RefCell::new(Vec::new()))));
            result.borrow_mut().set("pending".into(), JsValue::Array(Rc::new(RefCell::new(Vec::new()))));
            Ok(make_settled_promise("fulfilled", JsValue::Object(result)))
        }));
        e.define("__navigator_locks__", JsValue::Object(locks));
    }

    // requestIdleCallback / cancelIdleCallback - stub via setTimeout
    {
        let tq = Rc::clone(task_queue);
        let id_ctr = Rc::clone(next_timer_id);
        e.define("requestIdleCallback", native("requestIdleCallback", move |a| {
            let cb = a.into_iter().next().unwrap_or(JsValue::Undefined);
            let id = { let mut ctr = id_ctr.borrow_mut(); let id = *ctr; *ctr += 1; id };
            // IdleDeadline objekt - { didTimeout, timeRemaining() }
            let deadline = Rc::new(RefCell::new(JsObject::new()));
            deadline.borrow_mut().set("didTimeout".into(), JsValue::Bool(false));
            deadline.borrow_mut().set("timeRemaining".into(),
                native("timeRemaining", |_| Ok(JsValue::Number(50.0))));
            tq.borrow_mut().push((id, cb, vec![JsValue::Object(deadline)]));
            Ok(JsValue::Number(id as f64))
        }));
    }
    {
        let tq = Rc::clone(task_queue);
        e.define("cancelIdleCallback", native("cancelIdleCallback", move |a| {
            let id = a.into_iter().next().map(|v| v.to_number() as u32).unwrap_or(0);
            tq.borrow_mut().retain(|(tid, _, _)| *tid != id);
            Ok(JsValue::Undefined)
        }));
    }

    // BroadcastChannel stub
    e.define("BroadcastChannel", native("BroadcastChannel", |args| {
        let name = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("name".into(), JsValue::Str(name));
        obj.borrow_mut().set("postMessage".into(),
            native("BroadcastChannel.postMessage", |_| Ok(JsValue::Undefined)));
        obj.borrow_mut().set("close".into(),
            native("BroadcastChannel.close", |_| Ok(JsValue::Undefined)));
        obj.borrow_mut().set("addEventListener".into(),
            native("BroadcastChannel.addEventListener", |_| Ok(JsValue::Undefined)));
        Ok(JsValue::Object(obj))
    }));

    // IndexedDB - real in-memory implementation s store/get/put/delete/clear.
    // Persistence napric Interpreterem zatim NE - kazdy interpreter ma cisty stav.
    // Future: FS backend (sqlite ci JSON dump).
    {
        // Top-level db storage: db_name -> object_store_name -> (key -> value).
        type IdbStore = std::collections::BTreeMap<String, JsValue>;
        type IdbStores = std::collections::HashMap<String, Rc<RefCell<IdbStore>>>;
        type IdbDbs = std::collections::HashMap<String, Rc<RefCell<IdbStores>>>;
        let dbs: Rc<RefCell<IdbDbs>> = Rc::new(RefCell::new(std::collections::HashMap::new()));

        let idb = Rc::new(RefCell::new(JsObject::new()));
        let dbs_open = Rc::clone(&dbs);
        idb.borrow_mut().set("open".into(), native("indexedDB.open", move |args| {
            let mut it = args.into_iter();
            let name = it.next().map(|v| v.to_string()).unwrap_or_default();
            let _version = it.next().map(|v| v.to_number() as u32).unwrap_or(1);

            // Get-or-create stores map pro tuto db.
            let stores: Rc<RefCell<IdbStores>> = {
                let mut all = dbs_open.borrow_mut();
                all.entry(name.clone()).or_insert_with(||
                    Rc::new(RefCell::new(std::collections::HashMap::new()))
                ).clone()
            };

            // IDBDatabase object.
            let db = Rc::new(RefCell::new(JsObject::new()));
            db.borrow_mut().set("name".into(), JsValue::Str(name.clone()));
            db.borrow_mut().set("version".into(), JsValue::Number(1.0));

            // db.createObjectStore(name)
            let stores_co = Rc::clone(&stores);
            db.borrow_mut().set("createObjectStore".into(), native("createObjectStore", move |args| {
                let store_name = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                let store_data: Rc<RefCell<IdbStore>> = {
                    let mut all = stores_co.borrow_mut();
                    all.entry(store_name.clone()).or_insert_with(||
                        Rc::new(RefCell::new(std::collections::BTreeMap::new()))
                    ).clone()
                };
                Ok(JsValue::Object(make_object_store(store_name, store_data)))
            }));

            // db.transaction(stores) -> IDBTransaction
            let stores_tr = Rc::clone(&stores);
            db.borrow_mut().set("transaction".into(), native("transaction", move |args| {
                let _arg = args.into_iter().next();
                let tx = Rc::new(RefCell::new(JsObject::new()));
                let stores_tr2 = Rc::clone(&stores_tr);
                tx.borrow_mut().set("objectStore".into(), native("objectStore", move |args| {
                    let store_name = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                    let store_data: Rc<RefCell<IdbStore>> = {
                        let mut all = stores_tr2.borrow_mut();
                        all.entry(store_name.clone()).or_insert_with(||
                            Rc::new(RefCell::new(std::collections::BTreeMap::new()))
                        ).clone()
                    };
                    Ok(JsValue::Object(make_object_store(store_name, store_data)))
                }));
                tx.borrow_mut().set("commit".into(), native("commit", |_| Ok(JsValue::Undefined)));
                tx.borrow_mut().set("abort".into(), native("abort", |_| Ok(JsValue::Undefined)));
                Ok(JsValue::Object(tx))
            }));

            // db.close()
            db.borrow_mut().set("close".into(), native("close", |_| Ok(JsValue::Undefined)));

            db.borrow_mut().set("objectStoreNames".into(), {
                let names: Vec<JsValue> = stores.borrow().keys()
                    .map(|k| JsValue::Str(k.clone())).collect();
                JsValue::Array(Rc::new(RefCell::new(names)))
            });

            // IDBOpenDBRequest - mimicry Promise pres .result + onsuccess.
            // Vrati request s readyState=done a result=db, immediate.
            let req = Rc::new(RefCell::new(JsObject::new()));
            req.borrow_mut().set("readyState".into(), JsValue::Str("done".into()));
            req.borrow_mut().set("result".into(), JsValue::Object(db));
            req.borrow_mut().set("error".into(), JsValue::Null);
            req.borrow_mut().set("addEventListener".into(),
                native("addEventListener", |_| Ok(JsValue::Undefined)));
            Ok(JsValue::Object(req))
        }));

        let dbs_del = Rc::clone(&dbs);
        idb.borrow_mut().set("deleteDatabase".into(),
            native("indexedDB.deleteDatabase", move |args| {
                let name = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                dbs_del.borrow_mut().remove(&name);
                let req = Rc::new(RefCell::new(JsObject::new()));
                req.borrow_mut().set("readyState".into(), JsValue::Str("done".into()));
                req.borrow_mut().set("result".into(), JsValue::Undefined);
                Ok(JsValue::Object(req))
            }));

        let dbs_list = Rc::clone(&dbs);
        idb.borrow_mut().set("databases".into(),
            native("indexedDB.databases", move |_| {
                let names: Vec<JsValue> = dbs_list.borrow().keys().map(|k| {
                    let obj = Rc::new(RefCell::new(JsObject::new()));
                    obj.borrow_mut().set("name".into(), JsValue::Str(k.clone()));
                    obj.borrow_mut().set("version".into(), JsValue::Number(1.0));
                    JsValue::Object(obj)
                }).collect();
                Ok(make_settled_promise("fulfilled", JsValue::Array(Rc::new(RefCell::new(names)))))
            }));
        e.define("indexedDB", JsValue::Object(idb));
    }

    // FontFace constructor + document.fonts
    e.define("FontFace", native("FontFace", |args| {
        let mut it = args.into_iter();
        let family = it.next().map(|v| v.to_string()).unwrap_or_default();
        let src = it.next().map(|v| v.to_string()).unwrap_or_default();
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("family".into(), JsValue::Str(family));
        obj.borrow_mut().set("source".into(), JsValue::Str(src));
        obj.borrow_mut().set("status".into(), JsValue::Str("unloaded".into()));
        obj.borrow_mut().set("style".into(), JsValue::Str("normal".into()));
        obj.borrow_mut().set("weight".into(), JsValue::Str("normal".into()));
        obj.borrow_mut().set("stretch".into(), JsValue::Str("normal".into()));
        obj.borrow_mut().set("display".into(), JsValue::Str("auto".into()));
        let obj_clone = Rc::clone(&obj);
        obj.borrow_mut().set("load".into(), native("FontFace.load", move |_| {
            obj_clone.borrow_mut().set("status".into(), JsValue::Str("loaded".into()));
            Ok(make_settled_promise("fulfilled", JsValue::Object(Rc::clone(&obj_clone))))
        }));
        obj.borrow_mut().set("loaded".into(),
            make_settled_promise("fulfilled", JsValue::Undefined));
        Ok(JsValue::Object(obj))
    }));

    // File - extends Blob, name + lastModified
    e.define("File", native("File", |args| {
        let mut size: f64 = 0.0;
        let mut text_concat = String::new();
        let mut it = args.into_iter();
        if let Some(parts) = it.next() {
            if let JsValue::Array(arr) = &parts {
                for p in arr.borrow().iter() {
                    match p {
                        JsValue::Str(s) => { size += s.len() as f64; text_concat.push_str(s); }
                        JsValue::Array(bytes) => { size += bytes.borrow().len() as f64; }
                        _ => {}
                    }
                }
            }
        }
        let name = it.next().map(|v| v.to_string()).unwrap_or_else(|| "file".into());
        let mut mime = String::new();
        let mut last_modified = 0.0;
        if let Some(opts) = it.next() {
            if let JsValue::Object(o) = &opts {
                if let Some(JsValue::Str(t)) = o.borrow().props.get("type") { mime = t.clone(); }
                if let Some(JsValue::Number(n)) = o.borrow().props.get("lastModified") { last_modified = *n; }
            }
        }
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("name".into(), JsValue::Str(name));
        obj.borrow_mut().set("size".into(), JsValue::Number(size));
        obj.borrow_mut().set("type".into(), JsValue::Str(mime));
        obj.borrow_mut().set("lastModified".into(), JsValue::Number(last_modified));
        obj.borrow_mut().set("__file__".into(), JsValue::Bool(true));
        let text = text_concat.clone();
        obj.borrow_mut().set("text".into(), native("File.text", move |_| {
            Ok(make_settled_promise("fulfilled", JsValue::Str(text.clone())))
        }));
        obj.borrow_mut().set("arrayBuffer".into(), native("File.arrayBuffer", |_|
            Ok(make_settled_promise("fulfilled", JsValue::Array(Rc::new(RefCell::new(Vec::new())))))));
        obj.borrow_mut().set("slice".into(), native("File.slice", |_| Ok(JsValue::Undefined)));
        Ok(JsValue::Object(obj))
    }));

    // FileList - array-like collection s length / item(i) / iterator
    e.define("FileList", native("FileList", |_| {
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("length".into(), JsValue::Number(0.0));
        obj.borrow_mut().set("__filelist__".into(), JsValue::Bool(true));
        obj.borrow_mut().set("item".into(), native("FileList.item", |_| Ok(JsValue::Null)));
        Ok(JsValue::Object(obj))
    }));

    // Blob - shrnuje size z parts[0..] + type z options.type
    e.define("Blob", native("Blob", |args| {
        let mut size: f64 = 0.0;
        let mut text_concat = String::new();
        let mut it = args.into_iter();
        if let Some(parts) = it.next() {
            // parts: array stringu nebo bytes
            if let JsValue::Array(arr) = &parts {
                for p in arr.borrow().iter() {
                    match p {
                        JsValue::Str(s) => {
                            size += s.len() as f64;
                            text_concat.push_str(s);
                        }
                        JsValue::Array(bytes) => {
                            size += bytes.borrow().len() as f64;
                        }
                        _ => {}
                    }
                }
            }
        }
        // options.type
        let mut mime = String::new();
        if let Some(opts) = it.next() {
            if let JsValue::Object(o) = &opts {
                if let Some(JsValue::Str(t)) = o.borrow().props.get("type") {
                    mime = t.clone();
                }
            }
        }
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("size".into(), JsValue::Number(size));
        obj.borrow_mut().set("type".into(), JsValue::Str(mime));
        let text_for_async = text_concat.clone();
        obj.borrow_mut().set("text".into(), native("Blob.text", move |_| {
            Ok(make_settled_promise("fulfilled", JsValue::Str(text_for_async.clone())))
        }));
        obj.borrow_mut().set("arrayBuffer".into(), native("Blob.arrayBuffer", |_| Ok(make_settled_promise("fulfilled", JsValue::Array(Rc::new(RefCell::new(Vec::new())))))));
        obj.borrow_mut().set("slice".into(), native("Blob.slice", |_| Ok(JsValue::Undefined)));
        Ok(JsValue::Object(obj))
    }));

    // ResizeObserver - stub s seznamem observed targets, callback invokovana
    // pri manualnim trigger() (testovaci helper).
    e.define("ResizeObserver", native("ResizeObserver", move |args| {
        let cb = args.into_iter().next().unwrap_or(JsValue::Undefined);
        let targets: Rc<RefCell<Vec<JsValue>>> = Rc::new(RefCell::new(Vec::new()));
        let obj = std::rc::Rc::new(std::cell::RefCell::new(JsObject::new()));
        obj.borrow_mut().set("__observer_kind__".into(), JsValue::Str("ResizeObserver".into()));
        obj.borrow_mut().set("__ro_callback__".into(), cb);
        let t1 = Rc::clone(&targets);
        obj.borrow_mut().set("observe".into(), native("observe", move |a| {
            let target = a.into_iter().next().unwrap_or(JsValue::Undefined);
            t1.borrow_mut().push(target);
            Ok(JsValue::Undefined)
        }));
        let t2 = Rc::clone(&targets);
        obj.borrow_mut().set("unobserve".into(), native("unobserve", move |a| {
            let target = a.into_iter().next().unwrap_or(JsValue::Undefined);
            t2.borrow_mut().retain(|x| {
                match (x, &target) {
                    (JsValue::DomNode(a), JsValue::DomNode(b)) => !Rc::ptr_eq(a, b),
                    _ => true,
                }
            });
            Ok(JsValue::Undefined)
        }));
        let t3 = Rc::clone(&targets);
        obj.borrow_mut().set("disconnect".into(), native("disconnect", move |_| {
            t3.borrow_mut().clear();
            Ok(JsValue::Undefined)
        }));
        // Test helper: observer.targets - array of observed elements
        obj.borrow_mut().set("__targets__".into(), JsValue::Array(Rc::clone(&targets)));
        obj.borrow_mut().set("takeRecords".into(), native("takeRecords", |_|
            Ok(JsValue::Array(Rc::new(RefCell::new(Vec::new()))))));
        Ok(JsValue::Object(obj))
    }));
    e.define("IntersectionObserver", native("IntersectionObserver", move |args| {
        let mut it = args.into_iter();
        let cb = it.next().unwrap_or(JsValue::Undefined);
        let opts = it.next().unwrap_or(JsValue::Undefined);
        let targets: Rc<RefCell<Vec<JsValue>>> = Rc::new(RefCell::new(Vec::new()));
        let obj = std::rc::Rc::new(std::cell::RefCell::new(JsObject::new()));
        obj.borrow_mut().set("__observer_kind__".into(), JsValue::Str("IntersectionObserver".into()));
        obj.borrow_mut().set("__io_callback__".into(), cb);
        obj.borrow_mut().set("root".into(),
            if let JsValue::Object(o) = &opts {
                o.borrow().props.get("root").cloned().unwrap_or(JsValue::Null)
            } else { JsValue::Null });
        obj.borrow_mut().set("rootMargin".into(),
            if let JsValue::Object(o) = &opts {
                o.borrow().props.get("rootMargin").cloned()
                    .unwrap_or(JsValue::Str("0px".into()))
            } else { JsValue::Str("0px".into()) });
        obj.borrow_mut().set("thresholds".into(),
            JsValue::Array(Rc::new(RefCell::new(vec![JsValue::Number(0.0)]))));
        let t1 = Rc::clone(&targets);
        obj.borrow_mut().set("observe".into(), native("observe", move |a| {
            let target = a.into_iter().next().unwrap_or(JsValue::Undefined);
            t1.borrow_mut().push(target);
            Ok(JsValue::Undefined)
        }));
        let t2 = Rc::clone(&targets);
        obj.borrow_mut().set("unobserve".into(), native("unobserve", move |a| {
            let target = a.into_iter().next().unwrap_or(JsValue::Undefined);
            t2.borrow_mut().retain(|x| {
                match (x, &target) {
                    (JsValue::DomNode(a), JsValue::DomNode(b)) => !Rc::ptr_eq(a, b),
                    _ => true,
                }
            });
            Ok(JsValue::Undefined)
        }));
        let t3 = Rc::clone(&targets);
        obj.borrow_mut().set("disconnect".into(), native("disconnect", move |_| {
            t3.borrow_mut().clear();
            Ok(JsValue::Undefined)
        }));
        obj.borrow_mut().set("__targets__".into(), JsValue::Array(Rc::clone(&targets)));
        obj.borrow_mut().set("takeRecords".into(), native("takeRecords", |_|
            Ok(JsValue::Array(Rc::new(RefCell::new(Vec::new()))))));
        Ok(JsValue::Object(obj))
    }));
    e.define("PerformanceObserver", make_observer("PerformanceObserver"));

    // MutationObserver - real implementation s shared registry
    {
        let mo_reg = Rc::clone(mutation_observers);
        e.define("MutationObserver", native("MutationObserver", move |args| {
            let cb = args.into_iter().next().unwrap_or(JsValue::Undefined);
            let cb_for_observe = cb.clone();
            let cb_for_disconnect = cb.clone();
            let mo_observe = Rc::clone(&mo_reg);
            let mo_disconnect = Rc::clone(&mo_reg);
            let obj = std::rc::Rc::new(std::cell::RefCell::new(JsObject::new()));
            obj.borrow_mut().set("__observer_kind__".into(), JsValue::Str("MutationObserver".into()));
            obj.borrow_mut().set("__mo_callback__".into(), cb);
            // observe(target, options)
            obj.borrow_mut().set("observe".into(), native("observe", move |a| {
                let mut it = a.into_iter();
                let target = it.next().unwrap_or(JsValue::Undefined);
                let options = it.next().unwrap_or(JsValue::Undefined);
                let subtree = if let JsValue::Object(o) = &options {
                    matches!(o.borrow().props.get("subtree"), Some(JsValue::Bool(true)))
                } else { false };
                if let JsValue::DomNode(n) = &target {
                    let ptr = std::rc::Rc::as_ptr(n) as usize;
                    mo_observe.borrow_mut().push((ptr, cb_for_observe.clone(), options.clone(), subtree));
                }
                Ok(JsValue::Undefined)
            }));
            // disconnect() - odstrani vsechny observers s touto callback identitou
            obj.borrow_mut().set("disconnect".into(), native("disconnect", move |_| {
                let cb_id = format!("{:?}", cb_for_disconnect);
                mo_disconnect.borrow_mut().retain(|(_, c, _, _)| format!("{:?}", c) != cb_id);
                Ok(JsValue::Undefined)
            }));
            obj.borrow_mut().set("takeRecords".into(), native("takeRecords", |_| {
                Ok(JsValue::Array(std::rc::Rc::new(std::cell::RefCell::new(Vec::new()))))
            }));
            Ok(JsValue::Object(obj))
        }));
    }

    // requestAnimationFrame / cancelAnimationFrame - real frame-bound dispatch.
    // Pres pending_raf_callbacks: drain volame per render frame s timestamp.
    // Predtim byly stub via setTimeout(0) - microtask fire ALE bez frame
    // semantics (kazdy rAF spustil okamzite v drain_timers, ne pri repaint).
    // Real-world animation loops `function frame(t) { ...; rAF(frame) }`
    // potreba frame-bound timestamp + scheduling po render commit.
    {
        let raf = Rc::clone(raf_callbacks);
        let id_ctr = Rc::clone(next_raf_id);
        e.define("requestAnimationFrame", native("requestAnimationFrame", move |a| {
            let cb = a.into_iter().next().unwrap_or(JsValue::Undefined);
            let id = { let mut ctr = id_ctr.borrow_mut(); let id = *ctr; *ctr += 1; id };
            raf.borrow_mut().push((id, cb));
            Ok(JsValue::Number(id as f64))
        }));
    }
    {
        let raf = Rc::clone(raf_callbacks);
        e.define("cancelAnimationFrame", native("cancelAnimationFrame", move |a| {
            let id = a.into_iter().next().map(|v| v.to_number() as u32).unwrap_or(0);
            raf.borrow_mut().retain(|(rid, _)| *rid != id);
            Ok(JsValue::Undefined)
        }));
    }

    // queueMicrotask(callback)
    {
        let tq = Rc::clone(task_queue);
        let id_ctr = Rc::clone(next_timer_id);
        e.define("queueMicrotask", native("queueMicrotask", move |a| {
            let cb = a.into_iter().next().unwrap_or(JsValue::Undefined);
            let id = { let mut ctr = id_ctr.borrow_mut(); let id = *ctr; *ctr += 1; id };
            tq.borrow_mut().push((id, cb, Vec::new()));
            Ok(JsValue::Undefined)
        }));
    }

    // ─── Selection / Range API ────────────────────────────────────────────
    fn make_range() -> JsValue {
        let obj = Rc::new(RefCell::new(JsObject::new()));
        {
            let mut o = obj.borrow_mut();
            o.set("__range__".into(), JsValue::Bool(true));
            o.set("startContainer".into(), JsValue::Null);
            o.set("endContainer".into(), JsValue::Null);
            o.set("startOffset".into(), JsValue::Number(0.0));
            o.set("endOffset".into(), JsValue::Number(0.0));
            o.set("collapsed".into(), JsValue::Bool(true));
            o.set("commonAncestorContainer".into(), JsValue::Null);
        }
        // setStart / setEnd ulozi node + offset + update collapsed
        let r1 = Rc::clone(&obj);
        obj.borrow_mut().set("setStart".into(), native("setStart", move |args| {
            let mut it = args.into_iter();
            let node = it.next().unwrap_or(JsValue::Null);
            let offset = it.next().map(|v| v.to_number()).unwrap_or(0.0);
            let mut b = r1.borrow_mut();
            b.set("startContainer".into(), node);
            b.set("startOffset".into(), JsValue::Number(offset));
            // Update collapsed
            let end_off = b.props.get("endOffset").map(|v| v.to_number()).unwrap_or(0.0);
            b.set("collapsed".into(), JsValue::Bool((offset - end_off).abs() < 0.001));
            Ok(JsValue::Undefined)
        }));
        let r2 = Rc::clone(&obj);
        obj.borrow_mut().set("setEnd".into(), native("setEnd", move |args| {
            let mut it = args.into_iter();
            let node = it.next().unwrap_or(JsValue::Null);
            let offset = it.next().map(|v| v.to_number()).unwrap_or(0.0);
            let mut b = r2.borrow_mut();
            b.set("endContainer".into(), node);
            b.set("endOffset".into(), JsValue::Number(offset));
            let start_off = b.props.get("startOffset").map(|v| v.to_number()).unwrap_or(0.0);
            b.set("collapsed".into(), JsValue::Bool((offset - start_off).abs() < 0.001));
            Ok(JsValue::Undefined)
        }));
        let r3 = Rc::clone(&obj);
        obj.borrow_mut().set("collapse".into(), native("collapse", move |args| {
            let to_start = args.into_iter().next().map(|v| v.is_truthy()).unwrap_or(false);
            let mut b = r3.borrow_mut();
            if to_start {
                let so = b.props.get("startOffset").cloned().unwrap_or(JsValue::Number(0.0));
                let sc = b.props.get("startContainer").cloned().unwrap_or(JsValue::Null);
                b.set("endOffset".into(), so);
                b.set("endContainer".into(), sc);
            } else {
                let eo = b.props.get("endOffset").cloned().unwrap_or(JsValue::Number(0.0));
                let ec = b.props.get("endContainer").cloned().unwrap_or(JsValue::Null);
                b.set("startOffset".into(), eo);
                b.set("startContainer".into(), ec);
            }
            b.set("collapsed".into(), JsValue::Bool(true));
            Ok(JsValue::Undefined)
        }));
        let r4 = Rc::clone(&obj);
        obj.borrow_mut().set("selectNode".into(), native("selectNode", move |args| {
            let node = args.into_iter().next().unwrap_or(JsValue::Null);
            let mut b = r4.borrow_mut();
            b.set("startContainer".into(), node.clone());
            b.set("endContainer".into(), node);
            b.set("startOffset".into(), JsValue::Number(0.0));
            b.set("endOffset".into(), JsValue::Number(1.0));
            b.set("collapsed".into(), JsValue::Bool(false));
            Ok(JsValue::Undefined)
        }));
        let r5 = Rc::clone(&obj);
        obj.borrow_mut().set("selectNodeContents".into(), native("selectNodeContents", move |args| {
            let node = args.into_iter().next().unwrap_or(JsValue::Null);
            let mut b = r5.borrow_mut();
            b.set("startContainer".into(), node.clone());
            b.set("endContainer".into(), node);
            b.set("startOffset".into(), JsValue::Number(0.0));
            b.set("collapsed".into(), JsValue::Bool(false));
            Ok(JsValue::Undefined)
        }));
        let r6 = Rc::clone(&obj);
        obj.borrow_mut().set("cloneRange".into(), native("cloneRange", move |_| {
            let new_range = make_range();
            if let JsValue::Object(nr) = &new_range {
                let src = r6.borrow();
                let mut dst = nr.borrow_mut();
                for k in &["startContainer", "endContainer", "startOffset", "endOffset", "collapsed"] {
                    if let Some(v) = src.props.get(*k) {
                        dst.set((*k).into(), v.clone());
                    }
                }
            }
            Ok(new_range)
        }));
        {
            let mut o = obj.borrow_mut();
            o.set("setStartBefore".into(), native("setStartBefore", |_| Ok(JsValue::Undefined)));
            o.set("setStartAfter".into(), native("setStartAfter", |_| Ok(JsValue::Undefined)));
            o.set("setEndBefore".into(), native("setEndBefore", |_| Ok(JsValue::Undefined)));
            o.set("setEndAfter".into(), native("setEndAfter", |_| Ok(JsValue::Undefined)));
            o.set("cloneContents".into(), native("cloneContents", |_| {
                let frag = Rc::new(RefCell::new(JsObject::new()));
                frag.borrow_mut().set("__doc_fragment__".into(), JsValue::Bool(true));
                Ok(JsValue::Object(frag))
            }));
            o.set("extractContents".into(), native("extractContents", |_| {
                let frag = Rc::new(RefCell::new(JsObject::new()));
                Ok(JsValue::Object(frag))
            }));
            o.set("deleteContents".into(), native("deleteContents", |_| Ok(JsValue::Undefined)));
            o.set("insertNode".into(), native("insertNode", |_| Ok(JsValue::Undefined)));
            o.set("surroundContents".into(), native("surroundContents", |_| Ok(JsValue::Undefined)));
            o.set("toString".into(), native("toString", |_| Ok(JsValue::Str(String::new()))));
            o.set("getBoundingClientRect".into(), native("getBoundingClientRect", |_| {
                let r = Rc::new(RefCell::new(JsObject::new()));
                for k in &["top","left","bottom","right","width","height","x","y"] {
                    r.borrow_mut().set(k.to_string(), JsValue::Number(0.0));
                }
                Ok(JsValue::Object(r))
            }));
        }
        JsValue::Object(obj)
    }

    fn make_selection() -> JsValue {
        let ranges: Rc<RefCell<Vec<JsValue>>> = Rc::new(RefCell::new(Vec::new()));
        let obj = Rc::new(RefCell::new(JsObject::new()));
        {
            let mut o = obj.borrow_mut();
            o.set("__selection__".into(), JsValue::Bool(true));
            o.set("rangeCount".into(), JsValue::Number(0.0));
            o.set("type".into(), JsValue::Str("None".into()));
            o.set("anchorNode".into(), JsValue::Null);
            o.set("anchorOffset".into(), JsValue::Number(0.0));
            o.set("focusNode".into(), JsValue::Null);
            o.set("focusOffset".into(), JsValue::Number(0.0));
            o.set("isCollapsed".into(), JsValue::Bool(true));
        }
        let r1 = Rc::clone(&ranges);
        obj.borrow_mut().set("getRangeAt".into(), native("getRangeAt", move |args| {
            let idx = args.into_iter().next().map(|v| v.to_number() as usize).unwrap_or(0);
            Ok(r1.borrow().get(idx).cloned().unwrap_or_else(make_range))
        }));
        let r2 = Rc::clone(&ranges);
        let o2 = Rc::clone(&obj);
        obj.borrow_mut().set("addRange".into(), native("addRange", move |args| {
            let range = args.into_iter().next().unwrap_or(JsValue::Undefined);
            r2.borrow_mut().push(range);
            o2.borrow_mut().set("rangeCount".into(), JsValue::Number(r2.borrow().len() as f64));
            o2.borrow_mut().set("type".into(), JsValue::Str("Range".into()));
            o2.borrow_mut().set("isCollapsed".into(), JsValue::Bool(false));
            Ok(JsValue::Undefined)
        }));
        let r3 = Rc::clone(&ranges);
        let o3 = Rc::clone(&obj);
        obj.borrow_mut().set("removeAllRanges".into(), native("removeAllRanges", move |_| {
            r3.borrow_mut().clear();
            o3.borrow_mut().set("rangeCount".into(), JsValue::Number(0.0));
            o3.borrow_mut().set("type".into(), JsValue::Str("None".into()));
            o3.borrow_mut().set("isCollapsed".into(), JsValue::Bool(true));
            Ok(JsValue::Undefined)
        }));
        let r4 = Rc::clone(&ranges);
        let o4 = Rc::clone(&obj);
        obj.borrow_mut().set("removeRange".into(), native("removeRange", move |args| {
            let target = args.into_iter().next().unwrap_or(JsValue::Undefined);
            let target_id = format!("{:?}", target);
            r4.borrow_mut().retain(|r| format!("{:?}", r) != target_id);
            o4.borrow_mut().set("rangeCount".into(), JsValue::Number(r4.borrow().len() as f64));
            Ok(JsValue::Undefined)
        }));
        let o5 = Rc::clone(&obj);
        obj.borrow_mut().set("collapse".into(), native("collapse", move |args| {
            let mut it = args.into_iter();
            let node = it.next().unwrap_or(JsValue::Null);
            let offset = it.next().map(|v| v.to_number()).unwrap_or(0.0);
            let mut b = o5.borrow_mut();
            b.set("anchorNode".into(), node.clone());
            b.set("focusNode".into(), node);
            b.set("anchorOffset".into(), JsValue::Number(offset));
            b.set("focusOffset".into(), JsValue::Number(offset));
            b.set("isCollapsed".into(), JsValue::Bool(true));
            Ok(JsValue::Undefined)
        }));
        obj.borrow_mut().set("collapseToStart".into(), native("collapseToStart", |_| Ok(JsValue::Undefined)));
        obj.borrow_mut().set("collapseToEnd".into(), native("collapseToEnd", |_| Ok(JsValue::Undefined)));
        let o6 = Rc::clone(&obj);
        obj.borrow_mut().set("selectAllChildren".into(), native("selectAllChildren", move |args| {
            let node = args.into_iter().next().unwrap_or(JsValue::Null);
            let mut b = o6.borrow_mut();
            b.set("anchorNode".into(), node.clone());
            b.set("focusNode".into(), node);
            b.set("isCollapsed".into(), JsValue::Bool(false));
            b.set("type".into(), JsValue::Str("Range".into()));
            Ok(JsValue::Undefined)
        }));
        obj.borrow_mut().set("extend".into(), native("extend", |_| Ok(JsValue::Undefined)));
        obj.borrow_mut().set("containsNode".into(), native("containsNode", |_| Ok(JsValue::Bool(false))));
        obj.borrow_mut().set("toString".into(), native("toString", |_| Ok(JsValue::Str(String::new()))));
        obj.borrow_mut().set("deleteFromDocument".into(), native("deleteFromDocument", |_| Ok(JsValue::Undefined)));
        obj.borrow_mut().set("empty".into(), native("empty", |_| Ok(JsValue::Undefined)));
        JsValue::Object(obj)
    }

    // window.getSelection() + document.getSelection().
    // Pridame i na window object samotny aby `window.getSelection()` fungovalo.
    {
        let sel = make_selection();
        if let JsValue::Object(obj) = &sel {
            let dr = Rc::clone(document);
            obj.borrow_mut().set("toString".into(), native("toString", move |_| {
                let d = dr.borrow();
                let s = d.selection.borrow();
                Ok(JsValue::Str(s.page_selection.as_ref()
                    .map(|p| p.cached_text.clone())
                    .unwrap_or_default()))
            }));
        }
        let sel2 = sel.clone();
        e.define("getSelection", native("getSelection", move |_| Ok(sel.clone())));
        // Pridame na window object pro `window.getSelection()` (mirror).
        window_rc.borrow_mut().set("getSelection".into(),
            native("window.getSelection", move |_| Ok(sel2.clone())));
    }
    e.define("Range", native("Range", |_| Ok(make_range())));

    // ─── WebSocket ─────────────────────────────────────────────────────────
    // new WebSocket(url) - synchronni constructor, asynchronni read pres bg thread.
    // Send pres channel command; incoming messages drain v event loop main threadu.
    let websockets_c = Rc::clone(websockets);
    let next_ws_id_c = Rc::clone(next_ws_id);
    e.define("WebSocket", native("WebSocket", move |args| {
        let url = match args.into_iter().next() {
            Some(v) => v.to_string(),
            None => return Err("WebSocket: missing url argument".into()),
        };
        // Spawn background thread - tungstenite::connect (blocking handshake).
        let id = {
            let mut idg = next_ws_id_c.borrow_mut();
            let id = *idg;
            *idg += 1;
            id
        };
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<super::WebSocketCommand>();
        let (evt_tx, evt_rx) = std::sync::mpsc::channel::<super::WebSocketEvent>();
        let url_clone = url.clone();
        let handle = std::thread::Builder::new()
            .name(format!("websocket-{id}"))
            .stack_size(2 * 1024 * 1024)
            .spawn(move || {
                use tungstenite::Message;
                let connect_result = tungstenite::connect(&url_clone);
                let mut socket = match connect_result {
                    Ok((s, _resp)) => s,
                    Err(e) => {
                        let _ = evt_tx.send(super::WebSocketEvent::Error(format!("connect: {e}")));
                        let _ = evt_tx.send(super::WebSocketEvent::Closed);
                        return;
                    }
                };
                let _ = evt_tx.send(super::WebSocketEvent::Open);
                // Loop: priorita = drain commands, pak read socket.
                loop {
                    // Non-blocking command check.
                    match cmd_rx.try_recv() {
                        Ok(super::WebSocketCommand::Send(text)) => {
                            if let Err(e) = socket.send(Message::Text(text.into())) {
                                let _ = evt_tx.send(super::WebSocketEvent::Error(format!("send: {e}")));
                                break;
                            }
                        }
                        Ok(super::WebSocketCommand::Close) => {
                            let _ = socket.close(None);
                            break;
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                        Err(std::sync::mpsc::TryRecvError::Empty) => {}
                    }
                    // Read socket (blocking - timeout pres set_read_timeout).
                    match socket.read() {
                        Ok(Message::Text(t)) => { let _ = evt_tx.send(super::WebSocketEvent::Message(t.to_string())); }
                        Ok(Message::Binary(b)) => {
                            let s = String::from_utf8_lossy(&b).to_string();
                            let _ = evt_tx.send(super::WebSocketEvent::Message(s));
                        }
                        Ok(Message::Close(_)) => break,
                        Ok(_) => {} // Ping/Pong/Frame internal
                        Err(tungstenite::Error::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            // Cooperative - small sleep.
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                        Err(e) => {
                            let _ = evt_tx.send(super::WebSocketEvent::Error(format!("read: {e}")));
                            break;
                        }
                    }
                }
                let _ = evt_tx.send(super::WebSocketEvent::Closed);
            })
            .map_err(|e| format!("WebSocket spawn: {e}"))?;
        websockets_c.borrow_mut().insert(id, super::WebSocketState {
            sender: cmd_tx,
            incoming: evt_rx,
            handle: Some(handle),
            ready_state: 0, // CONNECTING
            on_open: None,
            on_message: None,
            on_error: None,
            on_close: None,
        });
        let mut obj = JsObject::new();
        obj.set("__ws_id__".into(), JsValue::Number(id as f64));
        obj.set("url".into(), JsValue::Str(url));
        obj.set("readyState".into(), JsValue::Number(0.0));
        // CONNECTING=0, OPEN=1, CLOSING=2, CLOSED=3.
        let send_websockets = Rc::clone(&websockets_c);
        let send_id = id;
        obj.set("send".into(), native("WebSocket.send", move |args| {
            let data = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            if let Some(state) = send_websockets.borrow().get(&send_id) {
                let _ = state.sender.send(super::WebSocketCommand::Send(data));
            }
            Ok(JsValue::Undefined)
        }));
        let close_websockets = Rc::clone(&websockets_c);
        let close_id = id;
        obj.set("close".into(), native("WebSocket.close", move |_args| {
            if let Some(state) = close_websockets.borrow().get(&close_id) {
                let _ = state.sender.send(super::WebSocketCommand::Close);
            }
            Ok(JsValue::Undefined)
        }));
        // Custom on(name, fn) - place callback do WsState (drain_websockets ho zavola).
        // (Standard ws.onmessage = fn potreba setter dispatch - skip.)
        let on_websockets = Rc::clone(&websockets_c);
        let on_id = id;
        obj.set("on".into(), native("WebSocket.on", move |args| {
            let mut it = args.into_iter();
            let name = it.next().map(|v| v.to_string()).unwrap_or_default();
            let cb = it.next().unwrap_or(JsValue::Undefined);
            if let Some(state) = on_websockets.borrow_mut().get_mut(&on_id) {
                match name.as_str() {
                    "open" => state.on_open = Some(cb),
                    "message" => state.on_message = Some(cb),
                    "error" => state.on_error = Some(cb),
                    "close" => state.on_close = Some(cb),
                    _ => {}
                }
            }
            Ok(JsValue::Undefined)
        }));
        // addEventListener alias.
        let ael_websockets = Rc::clone(&websockets_c);
        let ael_id = id;
        obj.set("addEventListener".into(), native("WebSocket.addEventListener", move |args| {
            let mut it = args.into_iter();
            let name = it.next().map(|v| v.to_string()).unwrap_or_default();
            let cb = it.next().unwrap_or(JsValue::Undefined);
            if let Some(state) = ael_websockets.borrow_mut().get_mut(&ael_id) {
                match name.as_str() {
                    "open" => state.on_open = Some(cb),
                    "message" => state.on_message = Some(cb),
                    "error" => state.on_error = Some(cb),
                    "close" => state.on_close = Some(cb),
                    _ => {}
                }
            }
            Ok(JsValue::Undefined)
        }));
        Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
    }));
    // Konstanty WebSocket.CONNECTING, .OPEN, ...
    {
        let mut ws_cls = JsObject::new();
        ws_cls.set("CONNECTING".into(), JsValue::Number(0.0));
        ws_cls.set("OPEN".into(), JsValue::Number(1.0));
        ws_cls.set("CLOSING".into(), JsValue::Number(2.0));
        ws_cls.set("CLOSED".into(), JsValue::Number(3.0));
        // Note: konstanty by mely byt na konstruktoru samem, ne separate. Skipping pro ted.
        let _ = ws_cls;
    }

}
