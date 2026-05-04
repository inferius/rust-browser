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

/// Worker thread loop: nacte JS skript ze souboru a interpretuje ho.
/// Worker scope ma globalni `self`, `postMessage(data)` a `onmessage = fn`.
/// Pri prijeti zpravy z main: parse JSON, zavola self.onmessage({data: parsed}).
/// Pri postMessage z workera: serializuj a posli outgoing channel.
fn run_worker_thread(
    script_url: &str,
    incoming: std::sync::mpsc::Receiver<String>,
    outgoing: std::sync::mpsc::Sender<String>,
) {
    use crate::lexer::base::Lexer;
    use crate::parser::Parser;
    use crate::tokens::TokenKind;

    // Worker ma vlastni Interpreter. Zaregistrujeme postMessage co posle pres outgoing.
    let mut interp = super::Interpreter::new();
    let outgoing_clone = outgoing.clone();

    // Pridam worker postMessage do scope
    let post_fn = JsValue::Function(super::JsFunc::Native(
        "postMessage".to_string(),
        std::rc::Rc::new(move |args: Vec<JsValue>| {
            let val = args.into_iter().next().unwrap_or(JsValue::Undefined);
            let serialized = json_stringify(&val, 0, 0)
                .unwrap_or_else(|| val.to_string());
            let _ = outgoing_clone.send(serialized);
            Ok(JsValue::Undefined)
        }),
    ));
    interp.global.borrow_mut().define("postMessage", post_fn);
    // Self reference - zatim prazdny objekt s onmessage placeholder
    let mut self_obj = JsObject::new();
    self_obj.set("onmessage".into(), JsValue::Undefined);
    let self_val = JsValue::Object(Rc::new(RefCell::new(self_obj)));
    interp.global.borrow_mut().define("self", self_val.clone());

    // Nacti script - zkus FS, fallback na inline test stub
    let script_src = match std::fs::read_to_string(script_url) {
        Ok(s) => s,
        Err(_) => {
            // Fallback: jednoduchy echo handler
            r#"self.onmessage = function(e) { postMessage("worker received: " + e.data); };"#.to_string()
        }
    };

    // Parse + run scriptu
    if let Ok(lex) = Lexer::parse_str(&script_src, script_url) {
        let tokens: Vec<_> = lex.tokens.into_iter()
            .filter(|t| !matches!(t.kind,
                TokenKind::Whitespace | TokenKind::Newline
                | TokenKind::CommentLine(_) | TokenKind::CommentBlock(_)))
            .collect();
        let mut parser = Parser::new(tokens);
        if let Ok(prog) = parser.parse() {
            let _ = interp.run(&prog);
        }
    }

    // Loop: cti zpravy, vola self.onmessage
    while let Ok(msg) = incoming.recv() {
        let parsed = json_parse(&msg).unwrap_or(JsValue::Str(msg.clone()));
        let mut event = JsObject::new();
        event.set("data".into(), parsed);
        let event_val = JsValue::Object(Rc::new(RefCell::new(event)));

        // self.onmessage(event)
        let onmessage = if let JsValue::Object(s) = &self_val {
            s.borrow().get("onmessage")
        } else { JsValue::Undefined };

        if !matches!(onmessage, JsValue::Undefined) {
            let _ = interp.call_function(onmessage, vec![event_val], None);
        }
    }
}

pub fn setup_builtins(
    env: &Rc<RefCell<Environment>>,
    task_queue: &Rc<RefCell<Vec<(u32, JsValue, Vec<JsValue>)>>>,
    next_timer_id: &Rc<RefCell<u32>>,
    workers: &Rc<RefCell<HashMap<u32, super::WorkerState>>>,
    next_worker_id: &Rc<RefCell<u32>>,
    document: &Rc<RefCell<crate::browser::dom::Document>>,
    console_log: &Rc<RefCell<Vec<(String, String)>>>,
    network_log: &Rc<RefCell<Vec<(String, u16)>>>,
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
            _                        => Ok(JsValue::Bool(false)),
        }
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

    // ─── Reflect (ES2015) - mirror objects API ───────────────────────────────
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

    // ArrayBuffer - alias k SharedArrayBuffer v sync runtime
    e.define("ArrayBuffer", native("ArrayBuffer", |a| {
        let len = a.into_iter().next().map(|v| v.to_number() as usize).unwrap_or(0);
        let mut obj = JsObject::new();
        obj.set("__buffer__".into(), JsValue::Bool(true));
        obj.set("byteLength".into(), JsValue::Number(len as f64));
        let bytes: Vec<JsValue> = vec![JsValue::Number(0.0); len];
        obj.set("__bytes__".into(), JsValue::Array(Rc::new(RefCell::new(bytes))));
        Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
    }));

    // Typed Arrays - jen Uint8Array stub
    e.define("Uint8Array", native("Uint8Array", |a| {
        let val = a.into_iter().next().unwrap_or(JsValue::Number(0.0));
        let bytes = match val {
            JsValue::Number(n) => vec![JsValue::Number(0.0); n as usize],
            JsValue::Array(arr) => arr.borrow().clone(),
            _ => vec![],
        };
        let len = bytes.len() as f64;
        let mut obj = JsObject::new();
        obj.set("__typed_array__".into(), JsValue::Str("Uint8Array".into()));
        obj.set("length".into(), JsValue::Number(len));
        obj.set("byteLength".into(), JsValue::Number(len));
        obj.set("__bytes__".into(), JsValue::Array(Rc::new(RefCell::new(bytes))));
        Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
    }));

    // ─── Atomics ─────────────────────────────────────────────────────────────
    // V sync runtime jsou to bezne operace (zadna konkurence).
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
    e.define("Atomics", JsValue::Object(Rc::new(RefCell::new(atomics))));

    // ─── DOM bridge - real propojeni s browser::dom ─────────────────────────
    // Pouziva se sdileny Rc<RefCell<Document>>. Element je JsValue::DomNode.

    let mut doc_obj = JsObject::new();
    doc_obj.set("__document__".into(), JsValue::Bool(true));

    // document.createElement(tagName)
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
        doc_obj.set("__get_URL__".into(), native("document.URL", move |_| {
            Ok(JsValue::Str(doc.borrow().url.clone()))
        }));
    }
    doc_obj.set("readyState".into(), JsValue::Str("complete".into()));

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
    e.define("window", JsValue::Object(Rc::new(RefCell::new(window))));


    // ─── fetch - real HTTP client (ureq, blocking) ──────────────────────────
    let net_log_clone = Rc::clone(network_log);
    e.define("fetch", native("fetch", move |a| {
        let net_log = Rc::clone(&net_log_clone);
        let mut iter = a.into_iter();
        let url = iter.next().map(|v| v.to_string()).unwrap_or_default();
        let init = iter.next().unwrap_or(JsValue::Undefined);

        // Parse init
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

        // Dispatch request
        let req_result = perform_http_request(&url, &method, &headers, body.as_deref());
        // Log network call
        let log_status = match &req_result {
            Ok((s, ..)) => *s,
            Err(_) => 0,
        };
        net_log.borrow_mut().push((url.clone(), log_status));
        match req_result {
            Ok((status, status_text, resp_body, resp_headers)) => {
                let mut response = JsObject::new();
                response.set("__response__".into(), JsValue::Bool(true));
                response.set("__body__".into(),    JsValue::Str(resp_body));
                response.set("url".into(),         JsValue::Str(url));
                response.set("status".into(),      JsValue::Number(status as f64));
                response.set("ok".into(),          JsValue::Bool(status >= 200 && status < 300));
                response.set("statusText".into(),  JsValue::Str(status_text));
                let mut hdr_obj = JsObject::new();
                for (k, v) in resp_headers {
                    hdr_obj.set(k.to_lowercase(), JsValue::Str(v));
                }
                hdr_obj.set("__headers__".into(), JsValue::Bool(true));
                response.set("headers".into(), JsValue::Object(Rc::new(RefCell::new(hdr_obj))));
                let response_val = JsValue::Object(Rc::new(RefCell::new(response)));
                Ok(make_settled_promise("fulfilled", response_val))
            }
            Err(msg) => {
                let mut err = JsObject::new();
                err.set("name".into(),    JsValue::Str("TypeError".into()));
                err.set("message".into(), JsValue::Str(format!("Failed to fetch: {msg}")));
                Ok(make_settled_promise("rejected", JsValue::Object(Rc::new(RefCell::new(err)))))
            }
        }
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
        match json_stringify(&val, indent, 0) {
            Some(s) => Ok(JsValue::Str(s)),
            None    => Ok(JsValue::Undefined),
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

    // globalThis - stub
    e.define("globalThis", JsValue::Object(Rc::new(RefCell::new(JsObject::new()))));

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
}
