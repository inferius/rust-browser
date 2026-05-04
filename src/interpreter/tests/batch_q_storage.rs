/// Batch Q - Storage (localStorage, sessionStorage, indexedDB stub).

use super::helpers::*;
use crate::interpreter::JsValue;

#[test]
fn local_storage_set_get() {
    let v = run(r#"
        localStorage.setItem("name", "Alice");
        return localStorage.getItem("name");
    "#);
    assert_eq!(as_str(v), "Alice");
}

#[test]
fn local_storage_get_missing_returns_null() {
    assert!(matches!(run(r#"return localStorage.getItem("nonexistent");"#), JsValue::Null));
}

#[test]
fn local_storage_remove_item() {
    let v = run(r#"
        localStorage.setItem("x", "1");
        localStorage.removeItem("x");
        return localStorage.getItem("x");
    "#);
    assert!(matches!(v, JsValue::Null));
}

#[test]
fn local_storage_clear() {
    let v = run(r#"
        localStorage.setItem("a", "1");
        localStorage.setItem("b", "2");
        localStorage.clear();
        return localStorage.getItem("a");
    "#);
    assert!(matches!(v, JsValue::Null));
}

#[test]
fn local_storage_length_updates() {
    let v = run(r#"
        localStorage.clear();
        localStorage.setItem("a", "1");
        localStorage.setItem("b", "2");
        return localStorage.length;
    "#);
    assert_eq!(as_num(v), 2.0);
}

#[test]
fn session_storage_separate_from_local() {
    let v = run(r#"
        localStorage.clear();
        sessionStorage.clear();
        localStorage.setItem("k", "L");
        sessionStorage.setItem("k", "S");
        return localStorage.getItem("k") + sessionStorage.getItem("k");
    "#);
    assert_eq!(as_str(v), "LS");
}

#[test]
fn local_storage_key_by_index() {
    let v = run(r#"
        localStorage.clear();
        localStorage.setItem("first", "1");
        return localStorage.key(0);
    "#);
    assert_eq!(as_str(v), "first");
}

#[test]
fn indexed_db_open_stub() {
    let v = run(r#"
        const req = indexedDB.open("mydb");
        return req.name;
    "#);
    assert_eq!(as_str(v), "mydb");
}
