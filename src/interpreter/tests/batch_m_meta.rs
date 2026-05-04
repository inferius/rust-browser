/// Batch M - Symbol + Proxy + Reflect.

use super::helpers::*;

// ─── Symbol ──────────────────────────────────────────────────────────────

#[test]
fn symbol_for_registry() {
    let v = run(r#"
        const a = Symbol.for("key");
        const b = Symbol.for("key");
        return a === b;
    "#);
    assert_eq!(as_bool(v), true);
}

#[test]
fn symbol_key_for() {
    let v = run(r#"
        const s = Symbol.for("test");
        return Symbol.keyFor(s);
    "#);
    assert_eq!(as_str(v), "test");
}

// ─── Reflect ─────────────────────────────────────────────────────────────

#[test]
fn reflect_get() {
    let v = run(r#"
        const obj = { x: 42 };
        return Reflect.get(obj, "x");
    "#);
    assert_eq!(as_num(v), 42.0);
}

#[test]
fn reflect_set() {
    let v = run(r#"
        const obj = {};
        Reflect.set(obj, "x", 99);
        return obj.x;
    "#);
    assert_eq!(as_num(v), 99.0);
}

#[test]
fn reflect_has() {
    assert_eq!(as_bool(run(r#"
        return Reflect.has({a: 1}, "a");
    "#)), true);
    assert_eq!(as_bool(run(r#"
        return Reflect.has({a: 1}, "b");
    "#)), false);
}

#[test]
fn reflect_delete_property() {
    let v = run(r#"
        const obj = { x: 1, y: 2 };
        Reflect.deleteProperty(obj, "x");
        return Reflect.has(obj, "x");
    "#);
    assert_eq!(as_bool(v), false);
}

#[test]
fn reflect_own_keys() {
    let v = run(r#"
        const obj = { a: 1, b: 2, c: 3 };
        return Reflect.ownKeys(obj).length;
    "#);
    assert_eq!(as_num(v), 3.0);
}

#[test]
fn reflect_get_prototype_of() {
    let v = run(r#"
        const proto = { x: 1 };
        const obj = Object.create(proto);
        return Reflect.getPrototypeOf(obj) === proto;
    "#);
    assert_eq!(as_bool(v), true);
}

// ─── Proxy ───────────────────────────────────────────────────────────────

#[test]
fn proxy_basic_pass_through() {
    // Bez handler trapu - Proxy deleguje na target
    let v = run(r#"
        const target = { x: 42 };
        const p = new Proxy(target, {});
        return p.x;
    "#);
    assert_eq!(as_num(v), 42.0);
}

#[test]
fn proxy_typeof_object() {
    let v = run(r#"
        const p = new Proxy({}, {});
        return typeof p;
    "#);
    assert_eq!(as_str(v), "object");
}
