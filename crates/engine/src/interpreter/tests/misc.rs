/// Misc: delete, void, instanceof pro built-in typy, structuredClone.

use super::helpers::*;
use crate::interpreter::JsValue;

#[test]
fn delete_object_property() {
    assert_eq!(as_bool(run(r#"
        const o = { a: 1, b: 2 };
        delete o.a;
        return !("a" in o) && o.b === 2;
    "#)), true);
}

#[test]
fn delete_returns_true() {
    assert_eq!(as_bool(run(r#"
        const o = { x: 1 };
        return delete o.x;
    "#)), true);
}

#[test]
fn void_returns_undefined() {
    assert!(matches!(run(r#"return void 42;"#), JsValue::Undefined));
    assert!(matches!(run(r#"return void "hello";"#), JsValue::Undefined));
}

#[test]
fn void_evaluates_expr() {
    assert_eq!(as_num(run(r#"
        let x = 0;
        void (x = 5);
        return x;
    "#)), 5.0);
}

#[test]
fn instanceof_error() {
    assert_eq!(as_bool(run(r#"
        const e = new Error("x");
        return e instanceof Error;
    "#)), true);
}

#[test]
fn instanceof_type_error() {
    assert_eq!(as_bool(run(r#"
        const e = new TypeError("x");
        return e instanceof TypeError;
    "#)), true);
}

#[test]
fn instanceof_map() {
    assert_eq!(as_bool(run(r#"
        const m = new Map();
        return m instanceof Map;
    "#)), true);
}

#[test]
fn instanceof_array() {
    assert_eq!(as_bool(run(r#"return [] instanceof Array;"#)), true);
    assert_eq!(as_bool(run(r#"return {} instanceof Array;"#)), false);
}

#[test]
fn structured_clone_object() {
    assert_eq!(as_num(run(r#"
        const obj = { x: 1, y: 2 };
        const clone = structuredClone(obj);
        clone.x = 99;
        return obj.x;
    "#)), 1.0);
}
