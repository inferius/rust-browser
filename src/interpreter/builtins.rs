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

/// MessagePort instance - addEventListener('message', ...), postMessage, start, close.
/// Pri postMessage odesle na __peer__ port a triggers jeho 'message' listenery.
fn make_message_port() -> JsValue {
    let listeners: Rc<RefCell<HashMap<String, Vec<JsValue>>>> = Rc::new(RefCell::new(HashMap::new()));
    let queue: Rc<RefCell<Vec<JsValue>>> = Rc::new(RefCell::new(Vec::new()));
    let obj = Rc::new(RefCell::new(JsObject::new()));
    obj.borrow_mut().set("__message_port__".into(), JsValue::Bool(true));
    let l1 = Rc::clone(&listeners);
    obj.borrow_mut().set("addEventListener".into(), native("addEventListener", move |args| {
        let mut it = args.into_iter();
        let name = it.next().map(|v| v.to_string()).unwrap_or_default();
        let cb = it.next().unwrap_or(JsValue::Undefined);
        l1.borrow_mut().entry(name).or_default().push(cb);
        Ok(JsValue::Undefined)
    }));
    let q = Rc::clone(&queue);
    obj.borrow_mut().set("postMessage".into(), native("postMessage", move |args| {
        let msg = args.into_iter().next().unwrap_or(JsValue::Undefined);
        q.borrow_mut().push(msg);
        Ok(JsValue::Undefined)
    }));
    obj.borrow_mut().set("start".into(), native("start", |_| Ok(JsValue::Undefined)));
    obj.borrow_mut().set("close".into(), native("close", |_| Ok(JsValue::Undefined)));
    obj.borrow_mut().set("__queue__".into(), JsValue::Array(Rc::new(RefCell::new(Vec::new()))));
    JsValue::Object(obj)
}

/// Helper - vytvori URLSearchParams-like object z search retezce ("?a=1&b=2").
fn build_search_params(search: &str) -> JsValue {
    let s = search.trim_start_matches('?');
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
        obj.borrow_mut().set("getAll".into(), native("URLSearchParams.getAll", move |args| {
            let name = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let arr: Vec<JsValue> = p.borrow().iter()
                .filter(|(k, _)| k == &name)
                .map(|(_, v)| JsValue::Str(v.clone()))
                .collect();
            Ok(JsValue::Array(Rc::new(RefCell::new(arr))))
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
    JsValue::Object(obj)
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
    custom_elements_registry: &Rc<RefCell<HashMap<String, super::JsValue>>>,
    mutation_observers: &Rc<RefCell<Vec<(usize, super::JsValue, super::JsValue, bool)>>>,
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

    // document.getSelection()
    {
        let sel = make_selection();
        doc_obj.set("getSelection".into(), native("document.getSelection", move |_| Ok(sel.clone())));
    }

    // document.createDocumentFragment()
    doc_obj.set("createDocumentFragment".into(), native("document.createDocumentFragment", |_| {
        use crate::browser::dom::NodeData;
        let node = NodeData::new_element("fragment", std::collections::HashMap::new());
        Ok(JsValue::DomNode(node))
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
        // Real implementace by snapshotla pred-stav, pak po-stav, transitionovat.
        // Zde callback se zavola pri prvni next event (nemam direct access -
        // user code musi pri budoucim render volat). Prozatim no-op + stub object.
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
        doc_obj.set("fonts".into(), JsValue::Object(fonts));
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

    // ─── Temporal API stub (TC39 Stage 3) ────────────────────────────────
    // Temporal.Now / PlainDate / PlainTime / Duration / Instant
    {
        let temporal = Rc::new(RefCell::new(JsObject::new()));
        // Temporal.Now
        let now_obj = Rc::new(RefCell::new(JsObject::new()));
        now_obj.borrow_mut().set("instant".into(), native("Temporal.Now.instant", |_| {
            let inst = Rc::new(RefCell::new(JsObject::new()));
            let ms = super::helpers::now_ms();
            inst.borrow_mut().set("__instant_ms__".into(), JsValue::Number(ms));
            inst.borrow_mut().set("epochMilliseconds".into(), JsValue::Number(ms));
            inst.borrow_mut().set("epochNanoseconds".into(), JsValue::Number(ms * 1_000_000.0));
            Ok(JsValue::Object(inst))
        }));
        now_obj.borrow_mut().set("plainDateISO".into(), native("Temporal.Now.plainDateISO", |_| {
            let ms = super::helpers::now_ms();
            let (yr, mo, day, _, _, _, _) = super::helpers::ms_to_parts(ms);
            let pd = Rc::new(RefCell::new(JsObject::new()));
            pd.borrow_mut().set("year".into(), JsValue::Number(yr as f64));
            pd.borrow_mut().set("month".into(), JsValue::Number((mo + 1) as f64));
            pd.borrow_mut().set("day".into(), JsValue::Number(day as f64));
            pd.borrow_mut().set("__plain_date__".into(), JsValue::Bool(true));
            Ok(JsValue::Object(pd))
        }));
        now_obj.borrow_mut().set("plainTimeISO".into(), native("Temporal.Now.plainTimeISO", |_| {
            let ms = super::helpers::now_ms();
            let (_, _, _, hr, mi, sec, ms_p) = super::helpers::ms_to_parts(ms);
            let pt = Rc::new(RefCell::new(JsObject::new()));
            pt.borrow_mut().set("hour".into(), JsValue::Number(hr as f64));
            pt.borrow_mut().set("minute".into(), JsValue::Number(mi as f64));
            pt.borrow_mut().set("second".into(), JsValue::Number(sec as f64));
            pt.borrow_mut().set("millisecond".into(), JsValue::Number(ms_p as f64));
            Ok(JsValue::Object(pt))
        }));
        now_obj.borrow_mut().set("zonedDateTimeISO".into(), native("Temporal.Now.zonedDateTimeISO", |_| {
            let ms = super::helpers::now_ms();
            let zdt = Rc::new(RefCell::new(JsObject::new()));
            zdt.borrow_mut().set("epochMilliseconds".into(), JsValue::Number(ms));
            zdt.borrow_mut().set("timeZoneId".into(), JsValue::Str("UTC".into()));
            Ok(JsValue::Object(zdt))
        }));
        temporal.borrow_mut().set("Now".into(), JsValue::Object(now_obj));
        // Temporal.PlainDate (constructor)
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
                // ISO date string "2024-01-15"
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

    // localStorage / sessionStorage - in-memory storage
    let make_storage = || {
        let store: Rc<RefCell<std::collections::HashMap<String, String>>> = Rc::new(RefCell::new(std::collections::HashMap::new()));
        let obj = Rc::new(RefCell::new(JsObject::new()));
        {
            let s = Rc::clone(&store);
            obj.borrow_mut().set("getItem".into(), native("Storage.getItem", move |args| {
                let key = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                Ok(s.borrow().get(&key).cloned().map(JsValue::Str).unwrap_or(JsValue::Null))
            }));
        }
        {
            let s = Rc::clone(&store);
            let o = Rc::clone(&obj);
            obj.borrow_mut().set("setItem".into(), native("Storage.setItem", move |args| {
                let mut it = args.into_iter();
                let key = it.next().map(|v| v.to_string()).unwrap_or_default();
                let val = it.next().map(|v| v.to_string()).unwrap_or_default();
                s.borrow_mut().insert(key, val);
                let len = s.borrow().len();
                o.borrow_mut().set("length".into(), JsValue::Number(len as f64));
                Ok(JsValue::Undefined)
            }));
        }
        {
            let s = Rc::clone(&store);
            let o = Rc::clone(&obj);
            obj.borrow_mut().set("removeItem".into(), native("Storage.removeItem", move |args| {
                let key = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                s.borrow_mut().remove(&key);
                let len = s.borrow().len();
                o.borrow_mut().set("length".into(), JsValue::Number(len as f64));
                Ok(JsValue::Undefined)
            }));
        }
        {
            let s = Rc::clone(&store);
            let o = Rc::clone(&obj);
            obj.borrow_mut().set("clear".into(), native("Storage.clear", move |_| {
                s.borrow_mut().clear();
                o.borrow_mut().set("length".into(), JsValue::Number(0.0));
                Ok(JsValue::Undefined)
            }));
        }
        {
            let s = Rc::clone(&store);
            obj.borrow_mut().set("key".into(), native("Storage.key", move |args| {
                let n = args.into_iter().next().map(|v| v.to_number() as usize).unwrap_or(0);
                Ok(s.borrow().keys().nth(n).cloned().map(JsValue::Str).unwrap_or(JsValue::Null))
            }));
        }
        obj.borrow_mut().set("length".into(), JsValue::Number(0.0));
        JsValue::Object(obj)
    };
    e.define("localStorage", make_storage());
    e.define("sessionStorage", make_storage());

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
    e.define("TextDecoder", native("TextDecoder", |_| {
        let obj = Rc::new(RefCell::new(JsObject::new()));
        obj.borrow_mut().set("encoding".into(), JsValue::Str("utf-8".into()));
        obj.borrow_mut().set("decode".into(), native("TextDecoder.decode", |args| {
            let arr = args.into_iter().next().unwrap_or(JsValue::Undefined);
            if let JsValue::Array(a) = arr {
                let bytes: Vec<u8> = a.borrow().iter()
                    .map(|v| v.to_number() as u8).collect();
                Ok(JsValue::Str(String::from_utf8_lossy(&bytes).into_owned()))
            } else {
                Ok(JsValue::Str(String::new()))
            }
        }));
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
        // Subtle stub
        let subtle = Rc::new(RefCell::new(JsObject::new()));
        for m in &["digest", "encrypt", "decrypt", "sign", "verify", "generateKey", "importKey", "exportKey", "deriveKey", "deriveBits", "wrapKey", "unwrapKey"] {
            let name = m.to_string();
            subtle.borrow_mut().set(name, native(m, |_| {
                Ok(make_settled_promise("fulfilled", JsValue::Undefined))
            }));
        }
        crypto.borrow_mut().set("subtle".into(), JsValue::Object(subtle));
        e.define("crypto", JsValue::Object(crypto));
    }

    // performance.now() + performance.timeOrigin
    {
        let perf = Rc::new(RefCell::new(JsObject::new()));
        let start = std::time::Instant::now();
        perf.borrow_mut().set("now".into(), native("performance.now", move |_| {
            Ok(JsValue::Number(start.elapsed().as_secs_f64() * 1000.0))
        }));
        perf.borrow_mut().set("timeOrigin".into(), JsValue::Number(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64() * 1000.0)
                .unwrap_or(0.0)
        ));
        perf.borrow_mut().set("mark".into(), native("performance.mark", |_| Ok(JsValue::Undefined)));
        perf.borrow_mut().set("measure".into(), native("performance.measure", |_| Ok(JsValue::Undefined)));
        perf.borrow_mut().set("clearMarks".into(), native("performance.clearMarks", |_| Ok(JsValue::Undefined)));
        perf.borrow_mut().set("clearMeasures".into(), native("performance.clearMeasures", |_| Ok(JsValue::Undefined)));
        perf.borrow_mut().set("getEntries".into(), native("performance.getEntries", |_| {
            Ok(JsValue::Array(Rc::new(RefCell::new(Vec::new()))))
        }));
        e.define("performance", JsValue::Object(perf));
    }

    // FormData stub
    e.define("FormData", native("FormData", |_| {
        let pairs: Rc<RefCell<Vec<(String, String)>>> = Rc::new(RefCell::new(Vec::new()));
        let obj = Rc::new(RefCell::new(JsObject::new()));
        {
            let p = Rc::clone(&pairs);
            obj.borrow_mut().set("append".into(), native("FormData.append", move |args| {
                let mut it = args.into_iter();
                let k = it.next().map(|v| v.to_string()).unwrap_or_default();
                let v = it.next().map(|v| v.to_string()).unwrap_or_default();
                p.borrow_mut().push((k, v));
                Ok(JsValue::Undefined)
            }));
        }
        {
            let p = Rc::clone(&pairs);
            obj.borrow_mut().set("get".into(), native("FormData.get", move |args| {
                let k = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                Ok(p.borrow().iter().find(|(kk, _)| kk == &k)
                    .map(|(_, v)| JsValue::Str(v.clone())).unwrap_or(JsValue::Null))
            }));
        }
        {
            let p = Rc::clone(&pairs);
            obj.borrow_mut().set("has".into(), native("FormData.has", move |args| {
                let k = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                Ok(JsValue::Bool(p.borrow().iter().any(|(kk, _)| kk == &k)))
            }));
        }
        {
            let p = Rc::clone(&pairs);
            obj.borrow_mut().set("delete".into(), native("FormData.delete", move |args| {
                let k = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                p.borrow_mut().retain(|(kk, _)| kk != &k);
                Ok(JsValue::Undefined)
            }));
        }
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

    // IndexedDB stub
    {
        let idb = Rc::new(RefCell::new(JsObject::new()));
        idb.borrow_mut().set("open".into(), native("indexedDB.open", |args| {
            let name = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            // Vraci IDBOpenDBRequest - asynchronni Promise-like
            let req = Rc::new(RefCell::new(JsObject::new()));
            req.borrow_mut().set("name".into(), JsValue::Str(name));
            req.borrow_mut().set("readyState".into(), JsValue::Str("done".into()));
            req.borrow_mut().set("result".into(), JsValue::Null);
            req.borrow_mut().set("error".into(), JsValue::Null);
            req.borrow_mut().set("addEventListener".into(),
                native("addEventListener", |_| Ok(JsValue::Undefined)));
            Ok(JsValue::Object(req))
        }));
        idb.borrow_mut().set("deleteDatabase".into(),
            native("indexedDB.deleteDatabase", |_| Ok(JsValue::Undefined)));
        idb.borrow_mut().set("databases".into(),
            native("indexedDB.databases", |_| {
                Ok(make_settled_promise("fulfilled", JsValue::Array(Rc::new(RefCell::new(Vec::new())))))
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

    // requestAnimationFrame / cancelAnimationFrame - stub via setTimeout
    {
        let tq = Rc::clone(task_queue);
        let id_ctr = Rc::clone(next_timer_id);
        e.define("requestAnimationFrame", native("requestAnimationFrame", move |a| {
            let cb = a.into_iter().next().unwrap_or(JsValue::Undefined);
            let id = { let mut ctr = id_ctr.borrow_mut(); let id = *ctr; *ctr += 1; id };
            tq.borrow_mut().push((id, cb, vec![JsValue::Number(0.0)]));
            Ok(JsValue::Number(id as f64))
        }));
    }
    {
        let tq = Rc::clone(task_queue);
        e.define("cancelAnimationFrame", native("cancelAnimationFrame", move |a| {
            let id = a.into_iter().next().map(|v| v.to_number() as u32).unwrap_or(0);
            tq.borrow_mut().retain(|(tid, _, _)| *tid != id);
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
            o.set("startContainer".into(), JsValue::Null);
            o.set("endContainer".into(), JsValue::Null);
            o.set("startOffset".into(), JsValue::Number(0.0));
            o.set("endOffset".into(), JsValue::Number(0.0));
            o.set("collapsed".into(), JsValue::Bool(true));
            o.set("commonAncestorContainer".into(), JsValue::Null);
            o.set("setStart".into(), native("setStart", |_| Ok(JsValue::Undefined)));
            o.set("setEnd".into(), native("setEnd", |_| Ok(JsValue::Undefined)));
            o.set("setStartBefore".into(), native("setStartBefore", |_| Ok(JsValue::Undefined)));
            o.set("setStartAfter".into(), native("setStartAfter", |_| Ok(JsValue::Undefined)));
            o.set("setEndBefore".into(), native("setEndBefore", |_| Ok(JsValue::Undefined)));
            o.set("setEndAfter".into(), native("setEndAfter", |_| Ok(JsValue::Undefined)));
            o.set("collapse".into(), native("collapse", |_| Ok(JsValue::Undefined)));
            o.set("selectNode".into(), native("selectNode", |_| Ok(JsValue::Undefined)));
            o.set("selectNodeContents".into(), native("selectNodeContents", |_| Ok(JsValue::Undefined)));
            o.set("cloneRange".into(), native("cloneRange", |_| Ok(make_range())));
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
        let obj = Rc::new(RefCell::new(JsObject::new()));
        {
            let mut o = obj.borrow_mut();
            o.set("rangeCount".into(), JsValue::Number(0.0));
            o.set("type".into(), JsValue::Str("None".into()));
            o.set("anchorNode".into(), JsValue::Null);
            o.set("anchorOffset".into(), JsValue::Number(0.0));
            o.set("focusNode".into(), JsValue::Null);
            o.set("focusOffset".into(), JsValue::Number(0.0));
            o.set("isCollapsed".into(), JsValue::Bool(true));
            o.set("getRangeAt".into(), native("getRangeAt", |_| Ok(make_range())));
            o.set("addRange".into(), native("addRange", |_| Ok(JsValue::Undefined)));
            o.set("removeRange".into(), native("removeRange", |_| Ok(JsValue::Undefined)));
            o.set("removeAllRanges".into(), native("removeAllRanges", |_| Ok(JsValue::Undefined)));
            o.set("collapse".into(), native("collapse", |_| Ok(JsValue::Undefined)));
            o.set("collapseToStart".into(), native("collapseToStart", |_| Ok(JsValue::Undefined)));
            o.set("collapseToEnd".into(), native("collapseToEnd", |_| Ok(JsValue::Undefined)));
            o.set("selectAllChildren".into(), native("selectAllChildren", |_| Ok(JsValue::Undefined)));
            o.set("extend".into(), native("extend", |_| Ok(JsValue::Undefined)));
            o.set("containsNode".into(), native("containsNode", |_| Ok(JsValue::Bool(false))));
            o.set("toString".into(), native("toString", |_| Ok(JsValue::Str(String::new()))));
            o.set("deleteFromDocument".into(), native("deleteFromDocument", |_| Ok(JsValue::Undefined)));
        }
        JsValue::Object(obj)
    }

    let sel = make_selection();
    e.define("getSelection", native("getSelection", move |_| Ok(sel.clone())));
    e.define("Range", native("Range", |_| Ok(make_range())));

}
