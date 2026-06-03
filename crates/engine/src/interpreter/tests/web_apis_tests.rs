/// Testy pro Web API stuby a runtime (TextEncoder/Decoder, URL, FormData,
/// crypto, navigator, performance, atd.)

use super::helpers::*;
use crate::interpreter::JsValue;

#[test]
fn url_object_protocol_host_pathname() {
    let r = run(r#"
        const u = new URL("https://example.com:8080/path?q=1");
        return u.protocol + "|" + u.host + "|" + u.pathname + "|" + u.search;
    "#);
    if let JsValue::Str(s) = r {
        assert!(s.contains("https:"));
        assert!(s.contains("example.com:8080"));
        assert!(s.contains("/path"));
        assert!(s.contains("?q=1"));
    } else { panic!("expected string, got {:?}", r); }
}

#[test]
fn url_search_params_get() {
    let r = run(r#"
        const u = new URL("https://x.com/?a=1&b=2");
        return u.searchParams.get("b");
    "#);
    assert_eq!(r.to_string(), "2");
}

#[test]
fn url_search_params_has() {
    let r = run(r#"
        const u = new URL("https://x.com/?key=val");
        return u.searchParams.has("key");
    "#);
    assert_eq!(r.to_string(), "true");
}

#[test]
fn url_no_protocol_defaults_https() {
    let r = run(r#"
        const u = new URL("example.com/x");
        return u.protocol;
    "#);
    assert_eq!(r.to_string(), "https:");
}

#[test]
fn url_search_params_constructor_string() {
    let r = run(r#"
        const p = new URLSearchParams("a=1&b=hello");
        return p.get("b");
    "#);
    assert_eq!(r.to_string(), "hello");
}

#[test]
fn url_search_params_set_overwrites() {
    let r = run(r#"
        const p = new URLSearchParams("a=1");
        p.set("a", "2");
        return p.get("a");
    "#);
    assert_eq!(r.to_string(), "2");
}

#[test]
fn url_search_params_append_then_get() {
    // append + get
    let r = run(r#"
        const p = new URLSearchParams();
        p.append("kk", "vv");
        return p.get("kk");
    "#);
    assert_eq!(r.to_string(), "vv");
}

#[test]
fn url_search_params_delete() {
    let r = run(r#"
        const p = new URLSearchParams("a=1&b=2");
        p.delete("a");
        return p.has("a");
    "#);
    assert_eq!(r.to_string(), "false");
}

#[test]
fn text_encoder_basic_ascii() {
    let r = run(r#"
        const e = new TextEncoder();
        const arr = e.encode("ABC");
        return arr.length + ":" + arr[0] + "," + arr[1] + "," + arr[2];
    "#);
    assert_eq!(r.to_string(), "3:65,66,67");
}

#[test]
fn text_decoder_decodes_array() {
    let r = run(r#"
        const d = new TextDecoder();
        return d.decode([72, 105]);
    "#);
    assert_eq!(r.to_string(), "Hi");
}

#[test]
fn crypto_random_uuid_format() {
    let r = run(r#"return crypto.randomUUID();"#);
    let s = r.to_string();
    // UUID format: 8-4-4-4-12 hex chars (36 chars total)
    assert_eq!(s.len(), 36);
    assert_eq!(s.chars().filter(|c| *c == '-').count(), 4);
}

#[test]
fn crypto_random_uuid_different_each_call() {
    let r = run(r#"
        const a = crypto.randomUUID();
        const b = crypto.randomUUID();
        return a + "|" + b;
    "#);
    let s = r.to_string();
    let parts: Vec<&str> = s.split('|').collect();
    assert_ne!(parts[0], parts[1]);
}

#[test]
fn crypto_get_random_values_fills_array() {
    let r = run(r#"
        const arr = [0, 0, 0, 0];
        crypto.getRandomValues(arr);
        // Aspon jedna hodnota nenulova
        return arr.some(v => v !== 0);
    "#);
    assert_eq!(r.to_string(), "true");
}

#[test]
fn navigator_user_agent_string() {
    let r = run(r#"return typeof navigator.userAgent;"#);
    assert_eq!(r.to_string(), "string");
}

#[test]
fn performance_now_returns_number() {
    let r = run(r#"return typeof performance.now();"#);
    assert_eq!(r.to_string(), "number");
}

#[test]
fn performance_now_increases() {
    let r = run(r#"
        const a = performance.now();
        // dela hodne praci
        let s = 0; for (let i = 0; i < 1000; i++) s += i;
        const b = performance.now();
        return b >= a;
    "#);
    assert_eq!(r.to_string(), "true");
}

#[test]
fn local_storage_set_get() {
    let r = run(r#"
        localStorage.setItem("key", "value123");
        return localStorage.getItem("key");
    "#);
    assert_eq!(r.to_string(), "value123");
}

#[test]
fn local_storage_remove() {
    let r = run(r#"
        localStorage.setItem("x", "1");
        localStorage.removeItem("x");
        return localStorage.getItem("x");
    "#);
    let s = r.to_string();
    assert!(s == "null" || s == "undefined");
}

#[test]
fn local_storage_length_updates() {
    let r = run(r#"
        localStorage.clear();
        localStorage.setItem("a", "1");
        localStorage.setItem("b", "2");
        return localStorage.length;
    "#);
    assert_eq!(r.to_string(), "2");
}

#[test]
fn local_storage_clear_resets() {
    let r = run(r#"
        localStorage.setItem("a", "1");
        localStorage.setItem("b", "2");
        localStorage.clear();
        return localStorage.length;
    "#);
    assert_eq!(r.to_string(), "0");
}

#[test]
fn session_storage_separate_from_local() {
    let r = run(r#"
        localStorage.clear();
        sessionStorage.clear();
        localStorage.setItem("k", "L");
        sessionStorage.setItem("k", "S");
        return localStorage.getItem("k") + "|" + sessionStorage.getItem("k");
    "#);
    assert_eq!(r.to_string(), "L|S");
}

#[test]
fn form_data_append_get() {
    let r = run(r#"
        const f = new FormData();
        f.append("name", "Alice");
        f.append("age", "30");
        return f.get("name");
    "#);
    assert_eq!(r.to_string(), "Alice");
}

#[test]
fn form_data_has() {
    let r = run(r#"
        const f = new FormData();
        f.append("k", "v");
        return f.has("k");
    "#);
    assert_eq!(r.to_string(), "true");
}

#[test]
fn blob_size_reported() {
    let r = run(r#"
        const b = new Blob(["hello"]);
        return b.size;
    "#);
    assert_eq!(r.to_string(), "5");
}

#[test]
fn blob_type_default() {
    let r = run(r#"
        const b = new Blob(["x"]);
        return typeof b.type;
    "#);
    assert_eq!(r.to_string(), "string");
}

#[test]
fn abort_controller_signal_initial() {
    let r = run(r#"
        const c = new AbortController();
        return c.signal.aborted;
    "#);
    assert_eq!(r.to_string(), "false");
}

#[test]
fn abort_controller_abort_sets_signal() {
    let r = run(r#"
        const c = new AbortController();
        c.abort();
        return c.signal.aborted;
    "#);
    assert_eq!(r.to_string(), "true");
}

#[test]
fn history_state_initial() {
    let r = run(r#"return typeof history.state;"#);
    let s = r.to_string();
    // typeof null je "object" v JS, undefined je "undefined"
    assert!(s == "object" || s == "undefined");
}

#[test]
fn request_animation_frame_returns_id() {
    let r = run(r#"
        const id = requestAnimationFrame(() => {});
        return typeof id;
    "#);
    assert_eq!(r.to_string(), "number");
}

#[test]
fn queue_microtask_callable() {
    let r = run(r#"
        let x = 0;
        queueMicrotask(() => { x = 1; });
        return typeof queueMicrotask;
    "#);
    assert_eq!(r.to_string(), "function");
}
