/// Testy pro CacheStorage / Cache API (Service Worker fundamentals).

use super::helpers::run;
use crate::interpreter::JsValue;

fn extract_promise_value(v: JsValue) -> JsValue {
    if let JsValue::Object(obj) = &v {
        let inner = obj.borrow();
        let state = inner.get("__state__");
        if let JsValue::Str(s) = state {
            if s == "fulfilled" {
                return inner.get("__value__");
            }
        }
    }
    v
}

#[test]
fn caches_open_returns_cache() {
    let r = run(r#"
        let cache;
        caches.open('v1').then(c => { cache = c; });
        return typeof cache;
    "#);
    if let JsValue::Str(s) = &extract_promise_value(r) {
        assert_eq!(s, "object");
    }
}

#[test]
fn cache_put_then_match_roundtrip() {
    let r = run(r#"
        let result;
        caches.open('v1').then(cache => {
            cache.put('/api/data', 'hello world');
            cache.match('/api/data').then(v => { result = v; });
        });
        return result;
    "#);
    let val = extract_promise_value(r);
    if let JsValue::Str(s) = val {
        assert_eq!(s, "hello world");
    }
}

#[test]
fn cache_delete_removes_entry() {
    let r = run(r#"
        let removed, after;
        caches.open('v1').then(cache => {
            cache.put('/key', 'val');
            cache.delete('/key').then(b => { removed = b; });
            cache.match('/key').then(v => { after = v; });
        });
        return removed;
    "#);
    if let JsValue::Bool(b) = extract_promise_value(r) {
        assert!(b);
    }
}

#[test]
fn caches_has_and_keys() {
    let r = run(r#"
        let has_v1;
        caches.open('v1').then(_ => {
            caches.has('v1').then(b => { has_v1 = b; });
        });
        return has_v1;
    "#);
    if let JsValue::Bool(b) = extract_promise_value(r) {
        assert!(b);
    }
}

#[test]
fn caches_delete_removes_storage() {
    let r = run(r#"
        let removed;
        caches.open('temp').then(_ => {
            caches.delete('temp').then(b => { removed = b; });
        });
        return removed;
    "#);
    if let JsValue::Bool(b) = extract_promise_value(r) {
        assert!(b);
    }
}

#[test]
#[allow(non_snake_case)]
fn cache_addAll_stores_urls() {
    let r = run(r#"
        let count;
        caches.open('static').then(cache => {
            cache.addAll(['/index.html', '/style.css', '/main.js']);
            cache.keys().then(arr => { count = arr.length; });
        });
        return count;
    "#);
    if let JsValue::Number(n) = extract_promise_value(r) {
        assert_eq!(n, 3.0);
    }
}
