/// Batch P - Web APIs (URL, atob/btoa, crypto.randomUUID, fetch stub).

use super::helpers::*;

// ─── atob / btoa ─────────────────────────────────────────────────────────

#[test]
fn btoa_basic() {
    let v = run(r#"return btoa("hello");"#);
    assert_eq!(as_str(v), "aGVsbG8=");
}

#[test]
fn atob_basic() {
    let v = run(r#"return atob("aGVsbG8=");"#);
    assert_eq!(as_str(v), "hello");
}

#[test]
fn btoa_atob_roundtrip() {
    let v = run(r#"return atob(btoa("hello world"));"#);
    assert_eq!(as_str(v), "hello world");
}

// ─── URL ─────────────────────────────────────────────────────────────────

#[test]
fn url_basic_parsing() {
    let v = run(r#"
        const u = new URL("https://example.com:8080/path?x=1#hash");
        return u.protocol + "|" + u.hostname + "|" + u.port + "|" + u.pathname + "|" + u.search + "|" + u.hash;
    "#);
    assert_eq!(as_str(v), "https:|example.com|8080|/path|?x=1|#hash");
}

#[test]
fn url_origin() {
    let v = run(r#"
        return new URL("https://example.com/path").origin;
    "#);
    assert_eq!(as_str(v), "https://example.com");
}

// ─── URLSearchParams ─────────────────────────────────────────────────────

#[test]
fn url_search_params_from_string() {
    let v = run(r#"
        const p = new URLSearchParams("a=1&b=2");
        return typeof p;
    "#);
    assert_eq!(as_str(v), "object");
}

// ─── crypto ──────────────────────────────────────────────────────────────

#[test]
fn crypto_random_uuid_format() {
    let v = run(r#"return crypto.randomUUID();"#);
    let s = as_str(v);
    assert_eq!(s.len(), 36); // 32 hex + 4 dashes
    assert_eq!(s.chars().nth(8), Some('-'));
    assert_eq!(s.chars().nth(13), Some('-'));
    assert_eq!(s.chars().nth(14), Some('4')); // v4 version
    assert_eq!(s.chars().nth(18), Some('-'));
    assert_eq!(s.chars().nth(23), Some('-'));
}

#[test]
fn crypto_random_uuid_unique() {
    let v = run(r#"
        const a = crypto.randomUUID();
        const b = crypto.randomUUID();
        return a !== b;
    "#);
    assert_eq!(as_bool(v), true);
}

// ─── TextEncoder / TextDecoder ───────────────────────────────────────────

#[test]
fn text_encoder_construct() {
    let v = run(r#"
        const enc = new TextEncoder();
        return enc.encoding;
    "#);
    assert_eq!(as_str(v), "utf-8");
}

#[test]
fn text_decoder_default() {
    let v = run(r#"
        const dec = new TextDecoder();
        return dec.encoding;
    "#);
    assert_eq!(as_str(v), "utf-8");
}

// ─── fetch stub ──────────────────────────────────────────────────────────

#[test]
fn fetch_returns_promise() {
    let v = run(r#"
        let result = "no";
        fetch("https://example.com").then(r => { result = r.status; });
        return result;
    "#);
    assert_eq!(as_num(v), 200.0);
}

#[test]
fn fetch_response_url() {
    let v = run(r#"
        let url = "";
        fetch("https://example.com/api").then(r => { url = r.url; });
        return url;
    "#);
    assert_eq!(as_str(v), "https://example.com/api");
}
