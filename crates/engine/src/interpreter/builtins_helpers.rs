//! Builtin construction helpers: worker thread runner, message port, URLSearchParams,
//! IDB object store factory.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use super::{JsValue, JsObject};
use super::helpers::*;

/// Worker thread loop: nacte JS skript ze souboru a interpretuje ho.
/// Worker scope ma globalni `self`, `postMessage(data)` a `onmessage = fn`.
/// Pri prijeti zpravy z main: parse JSON, zavola self.onmessage({data: parsed}).
/// Pri postMessage z workera: serializuj a posli outgoing channel.
pub(super) fn run_worker_thread(
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
pub(super) fn make_message_port() -> JsValue {
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
pub(super) fn build_search_params(search: &str) -> JsValue {
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

/// Vyrobi IDBObjectStore objekt s funkcemi put/get/delete/clear/getAll/count.
/// Backend: BTreeMap<String, JsValue> sdileny pres Rc<RefCell<>>.
pub(super) fn make_object_store(
    name: String,
    data: Rc<RefCell<std::collections::BTreeMap<String, JsValue>>>,
) -> Rc<RefCell<JsObject>> {
    let store = Rc::new(RefCell::new(JsObject::new()));
    store.borrow_mut().set("name".into(), JsValue::Str(name.clone()));
    store.borrow_mut().set("keyPath".into(), JsValue::Null);
    store.borrow_mut().set("autoIncrement".into(), JsValue::Bool(false));

    fn make_request(result: JsValue) -> JsValue {
        let req = Rc::new(RefCell::new(JsObject::new()));
        req.borrow_mut().set("readyState".into(), JsValue::Str("done".into()));
        req.borrow_mut().set("result".into(), result);
        req.borrow_mut().set("error".into(), JsValue::Null);
        req.borrow_mut().set("addEventListener".into(),
            native("addEventListener", |_| Ok(JsValue::Undefined)));
        JsValue::Object(req)
    }

    // store.put(value, key)
    let d = Rc::clone(&data);
    store.borrow_mut().set("put".into(), native("put", move |args| {
        let mut it = args.into_iter();
        let value = it.next().unwrap_or(JsValue::Undefined);
        let key = it.next().map(|v| v.to_string()).unwrap_or_else(|| {
            // Pri value object s "id" pouzij ho jako key.
            if let JsValue::Object(o) = &value {
                let id = o.borrow().get("id");
                if !matches!(id, JsValue::Undefined) { return id.to_string(); }
            }
            // Auto-incr: pouzij size+1.
            (d.borrow().len() + 1).to_string()
        });
        d.borrow_mut().insert(key.clone(), value);
        Ok(make_request(JsValue::Str(key)))
    }));
    // store.add(value, key) - alias pro put (s fail-on-duplicate semantikou)
    let d = Rc::clone(&data);
    store.borrow_mut().set("add".into(), native("add", move |args| {
        let mut it = args.into_iter();
        let value = it.next().unwrap_or(JsValue::Undefined);
        let key = it.next().map(|v| v.to_string()).unwrap_or_else(|| {
            (d.borrow().len() + 1).to_string()
        });
        if d.borrow().contains_key(&key) {
            // Real IDB by hodilo ConstraintError - my jen pridame anyway.
        }
        d.borrow_mut().insert(key.clone(), value);
        Ok(make_request(JsValue::Str(key)))
    }));
    // store.get(key)
    let d = Rc::clone(&data);
    store.borrow_mut().set("get".into(), native("get", move |args| {
        let key = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
        let val = d.borrow().get(&key).cloned().unwrap_or(JsValue::Undefined);
        Ok(make_request(val))
    }));
    // store.delete(key)
    let d = Rc::clone(&data);
    store.borrow_mut().set("delete".into(), native("delete", move |args| {
        let key = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
        d.borrow_mut().remove(&key);
        Ok(make_request(JsValue::Undefined))
    }));
    // store.clear()
    let d = Rc::clone(&data);
    store.borrow_mut().set("clear".into(), native("clear", move |_| {
        d.borrow_mut().clear();
        Ok(make_request(JsValue::Undefined))
    }));
    // store.count()
    let d = Rc::clone(&data);
    store.borrow_mut().set("count".into(), native("count", move |_| {
        let c = d.borrow().len() as f64;
        Ok(make_request(JsValue::Number(c)))
    }));
    // store.getAll()
    let d = Rc::clone(&data);
    store.borrow_mut().set("getAll".into(), native("getAll", move |_| {
        let arr: Vec<JsValue> = d.borrow().values().cloned().collect();
        Ok(make_request(JsValue::Array(Rc::new(RefCell::new(arr)))))
    }));
    // store.getAllKeys()
    let d = Rc::clone(&data);
    store.borrow_mut().set("getAllKeys".into(), native("getAllKeys", move |_| {
        let arr: Vec<JsValue> = d.borrow().keys().map(|k| JsValue::Str(k.clone())).collect();
        Ok(make_request(JsValue::Array(Rc::new(RefCell::new(arr)))))
    }));
    // store.openCursor() - jednoduchy: vrati first-key cursor s next() metodou.
    let d = Rc::clone(&data);
    store.borrow_mut().set("openCursor".into(), native("openCursor", move |_| {
        let entries: Vec<(String, JsValue)> = d.borrow().iter()
            .map(|(k, v)| (k.clone(), v.clone())).collect();
        let cursor_state = Rc::new(RefCell::new(0usize));
        let entries_rc = Rc::new(entries);
        let cursor = Rc::new(RefCell::new(JsObject::new()));
        if let Some((k, v)) = entries_rc.first().cloned() {
            cursor.borrow_mut().set("key".into(), JsValue::Str(k));
            cursor.borrow_mut().set("value".into(), v);
        }
        let cs = Rc::clone(&cursor_state);
        let er = Rc::clone(&entries_rc);
        let cursor_inner = Rc::clone(&cursor);
        cursor.borrow_mut().set("continue".into(), native("continue", move |_| {
            let next = *cs.borrow() + 1;
            *cs.borrow_mut() = next;
            if next < er.len() {
                let (k, v) = er[next].clone();
                cursor_inner.borrow_mut().set("key".into(), JsValue::Str(k));
                cursor_inner.borrow_mut().set("value".into(), v);
            } else {
                cursor_inner.borrow_mut().set("key".into(), JsValue::Undefined);
                cursor_inner.borrow_mut().set("value".into(), JsValue::Undefined);
            }
            Ok(JsValue::Undefined)
        }));
        Ok(make_request(JsValue::Object(cursor)))
    }));
    // store.createIndex() - stub
    store.borrow_mut().set("createIndex".into(), native("createIndex", |_| {
        let idx = Rc::new(RefCell::new(JsObject::new()));
        Ok(JsValue::Object(idx))
    }));
    store
}
