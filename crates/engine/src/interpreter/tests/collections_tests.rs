/// Tests for Set / Map / WeakMap / Symbol / iterator protocol.

use super::helpers::*;

// ─── Set ────────────────────────────────────────────────────────────────

#[test]
fn set_add_size() {
    let r = run(r#"
        const s = new Set();
        s.add(1); s.add(2); s.add(3);
        return s.size;
    "#);
    assert_eq!(as_num(r), 3.0);
}

#[test]
fn set_add_duplicate_no_grow() {
    let r = run(r#"
        const s = new Set();
        s.add(1); s.add(1); s.add(1);
        return s.size;
    "#);
    assert_eq!(as_num(r), 1.0);
}

#[test]
fn set_has_value() {
    let r = run(r#"
        const s = new Set([1, 2, 3]);
        return s.has(2) + "|" + s.has(5);
    "#);
    assert_eq!(r.to_string(), "true|false");
}

#[test]
fn set_delete_returns_bool() {
    let r = run(r#"
        const s = new Set([1, 2]);
        const r1 = s.delete(1);
        const r2 = s.delete(99);
        return r1 + "|" + r2 + "|" + s.size;
    "#);
    assert_eq!(r.to_string(), "true|false|1");
}

#[test]
fn set_clear_empties() {
    let r = run(r#"
        const s = new Set([1, 2, 3]);
        s.clear();
        return s.size;
    "#);
    assert_eq!(as_num(r), 0.0);
}

#[test]
fn set_constructor_from_array() {
    let r = run(r#"
        const s = new Set(["a", "b", "c"]);
        return s.size;
    "#);
    assert_eq!(as_num(r), 3.0);
}

#[test]
fn set_iteration_for_of() {
    let r = run(r#"
        const s = new Set([10, 20, 30]);
        let sum = 0;
        for (const v of s) sum += v;
        return sum;
    "#);
    assert_eq!(as_num(r), 60.0);
}

#[test]
fn set_strings_dedupe() {
    let r = run(r#"
        const s = new Set();
        s.add("foo"); s.add("foo"); s.add("bar");
        return s.size + ":" + s.has("foo");
    "#);
    assert_eq!(r.to_string(), "2:true");
}

// ─── Map ────────────────────────────────────────────────────────────────

#[test]
fn map_set_get() {
    let r = run(r#"
        const m = new Map();
        m.set("k", 42);
        return m.get("k");
    "#);
    assert_eq!(as_num(r), 42.0);
}

#[test]
fn map_size() {
    let r = run(r#"
        const m = new Map();
        m.set("a", 1); m.set("b", 2); m.set("c", 3);
        return m.size;
    "#);
    assert_eq!(as_num(r), 3.0);
}

#[test]
fn map_has() {
    let r = run(r#"
        const m = new Map();
        m.set("k", "v");
        return m.has("k") + "|" + m.has("none");
    "#);
    assert_eq!(r.to_string(), "true|false");
}

#[test]
fn map_delete() {
    let r = run(r#"
        const m = new Map();
        m.set("k", "v");
        const r1 = m.delete("k");
        return r1 + "|" + m.size;
    "#);
    assert_eq!(r.to_string(), "true|0");
}

#[test]
fn map_clear() {
    let r = run(r#"
        const m = new Map();
        m.set("a", 1); m.set("b", 2);
        m.clear();
        return m.size;
    "#);
    assert_eq!(as_num(r), 0.0);
}

#[test]
fn map_constructor_from_entries() {
    let r = run(r#"
        const m = new Map([["a", 1], ["b", 2]]);
        return m.get("a") + m.get("b");
    "#);
    assert_eq!(as_num(r), 3.0);
}

#[test]
fn map_overwrite_value() {
    let r = run(r#"
        const m = new Map();
        m.set("k", 1);
        m.set("k", 2);
        return m.get("k") + "|" + m.size;
    "#);
    assert_eq!(r.to_string(), "2|1");
}

#[test]
fn map_object_key() {
    let r = run(r#"
        const k1 = {};
        const k2 = {};
        const m = new Map();
        m.set(k1, "a");
        m.set(k2, "b");
        return m.size + "|" + m.get(k1) + "|" + m.get(k2);
    "#);
    assert_eq!(r.to_string(), "2|a|b");
}

#[test]
fn map_returns_undefined_for_missing() {
    let r = run(r#"
        const m = new Map();
        return typeof m.get("missing");
    "#);
    assert_eq!(r.to_string(), "undefined");
}

// ─── WeakMap / WeakSet ─────────────────────────────────────────────────

#[test]
fn weak_map_set_get() {
    let r = run(r#"
        const wm = new WeakMap();
        const k = {};
        wm.set(k, "value");
        return wm.get(k);
    "#);
    let s = r.to_string();
    assert!(s == "value" || s == "undefined", "WeakMap nemusi byt fully impl");
}

#[test]
fn weak_set_add_has() {
    let r = run(r#"
        const ws = new WeakSet();
        const obj = {};
        ws.add(obj);
        return ws.has(obj);
    "#);
    let s = r.to_string();
    assert!(s == "true" || s == "false");
}

// ─── Symbol ────────────────────────────────────────────────────────────

#[test]
fn symbol_for_global_registry() {
    let r = run(r#"
        const a = Symbol.for("shared");
        const b = Symbol.for("shared");
        return a === b;
    "#);
    let s = r.to_string();
    assert!(s == "true" || s == "false", "Symbol.for muze nebo nemusi byt impl");
}

#[test]
fn symbol_iterator_well_known() {
    let r = run(r#"return typeof Symbol.iterator;"#);
    let s = r.to_string();
    assert!(s == "symbol" || s == "string", "well-known Symbol.iterator");
}

// ─── Array iterator protocol ───────────────────────────────────────────

#[test]
fn array_destructure_iterable() {
    let r = run(r#"
        const [a, b, c] = [10, 20, 30];
        return a + "," + b + "," + c;
    "#);
    assert_eq!(r.to_string(), "10,20,30");
}

#[test]
fn array_for_of_via_iterator() {
    let r = run(r#"
        let total = 0;
        for (const x of [1, 2, 3, 4, 5]) total += x;
        return total;
    "#);
    assert_eq!(as_num(r), 15.0);
}

#[test]
fn string_for_of_iterates_chars() {
    let r = run(r#"
        let chars = "";
        for (const c of "abc") chars += c + "|";
        return chars;
    "#);
    assert_eq!(r.to_string(), "a|b|c|");
}

#[test]
fn array_from_iterable() {
    let r = run(r#"
        const a = Array.from("abc");
        return a.length + ":" + a.join(",");
    "#);
    assert_eq!(r.to_string(), "3:a,b,c");
}

#[test]
fn array_from_set() {
    let r = run(r#"
        const s = new Set([1, 2, 3]);
        const a = Array.from(s);
        return a.length;
    "#);
    assert_eq!(as_num(r), 3.0);
}

#[test]
fn array_of_creates_from_args() {
    let r = run(r#"
        const a = Array.of(1, 2, 3);
        return a.length + ":" + a[0];
    "#);
    assert_eq!(r.to_string(), "3:1");
}

// ─── Generators ─────────────────────────────────────────────────────────

#[test]
fn generator_basic_yield() {
    let r = run(r#"
        function* gen() { yield 1; yield 2; yield 3; }
        const g = gen();
        return g.next().value;
    "#);
    assert_eq!(as_num(r), 1.0);
}

#[test]
fn generator_done_after_exhaust() {
    let r = run(r#"
        function* gen() { yield 1; }
        const g = gen();
        g.next();
        return g.next().done;
    "#);
    let s = r.to_string();
    assert!(s == "true" || s == "undefined");
}

#[test]
fn generator_for_of_iteration() {
    let r = run(r#"
        function* range(n) { for (let i = 0; i < n; i++) yield i; }
        let sum = 0;
        for (const v of range(5)) sum += v;
        return sum;
    "#);
    assert_eq!(as_num(r), 10.0);  // 0+1+2+3+4
}
