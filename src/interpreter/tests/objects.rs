/// Objekty - literal, properties, computed, Object staticke metody, prototype chain.
/// Map, Set, Symbol.

use super::helpers::*;
use crate::interpreter::JsValue;

#[test]
fn object_property_access() {
    assert_eq!(as_num(run(r#"
        const obj = { x: 42 };
        return obj.x;
    "#)), 42.0);
}

#[test]
fn object_computed_access() {
    assert_eq!(as_num(run(r#"
        const obj = { x: 99 };
        const key = "x";
        return obj[key];
    "#)), 99.0);
}

#[test]
fn object_mutation() {
    assert_eq!(as_num(run(r#"
        let obj = { a: 1 };
        obj.a = 42;
        return obj.a;
    "#)), 42.0);
}

#[test]
fn object_method_shorthand() {
    assert_eq!(as_num(run(r#"
        const obj = {
            x: 10,
            getX() { return this.x; }
        };
        return obj.getX();
    "#)), 10.0);
}

#[test]
fn object_keys() {
    assert_eq!(as_num(run(r#"
        return Object.keys({ a: 1, b: 2, c: 3 }).length;
    "#)), 3.0);
}

#[test]
fn object_values() {
    assert_eq!(as_num(run(r#"
        const vals = Object.values({ a: 1, b: 2 });
        return vals[0] + vals[1];
    "#)), 3.0);
}

#[test]
fn object_entries() {
    assert_eq!(as_num(run(r#"
        return Object.entries({ x: 10, y: 20 }).length;
    "#)), 2.0);
}

#[test]
fn object_assign() {
    assert_eq!(as_num(run(r#"
        const target = { a: 1 };
        Object.assign(target, { b: 2, c: 3 });
        return target.b + target.c;
    "#)), 5.0);
}

#[test]
fn object_from_entries() {
    assert_eq!(as_num(run(r#"
        const obj = Object.fromEntries([["a", 1], ["b", 2]]);
        return obj.a + obj.b;
    "#)), 3.0);
}

// ─── Prototype chain ─────────────────────────────────────────────────────

#[test]
fn proto_chain_property_lookup() {
    assert_eq!(as_num(run(r#"
        const proto = { x: 42 };
        const obj = Object.create(proto);
        return obj.x;
    "#)), 42.0);
}

#[test]
fn proto_own_overrides_inherited() {
    assert_eq!(as_num(run(r#"
        const proto = { x: 1 };
        const obj = Object.create(proto);
        obj.x = 99;
        return obj.x;
    "#)), 99.0);
}

#[test]
fn object_create_null() {
    assert!(matches!(run(r#"
        const obj = Object.create(null);
        return obj.x;
    "#), JsValue::Undefined));
}

#[test]
fn object_get_prototype_of() {
    assert_eq!(as_bool(run(r#"
        const proto = { x: 1 };
        const obj = Object.create(proto);
        return Object.getPrototypeOf(obj) === proto;
    "#)), true);
}

#[test]
fn object_set_prototype_of() {
    assert_eq!(as_num(run(r#"
        const proto = { y: 77 };
        const obj = {};
        Object.setPrototypeOf(obj, proto);
        return obj.y;
    "#)), 77.0);
}

#[test]
fn has_own_property() {
    assert_eq!(as_bool(run(r#"
        const proto = { inherited: 1 };
        const obj = Object.create(proto);
        obj.own = 2;
        return obj.hasOwnProperty("own");
    "#)), true);
    assert_eq!(as_bool(run(r#"
        const proto = { inherited: 1 };
        const obj = Object.create(proto);
        return obj.hasOwnProperty("inherited");
    "#)), false);
}

#[test]
fn is_prototype_of() {
    assert_eq!(as_bool(run(r#"
        const proto = {};
        const obj = Object.create(proto);
        return proto.isPrototypeOf(obj);
    "#)), true);
}

#[test]
fn is_prototype_of_false() {
    assert_eq!(as_bool(run(r#"
        const a = {};
        const b = {};
        return a.isPrototypeOf(b);
    "#)), false);
}

#[test]
fn property_is_enumerable() {
    assert_eq!(as_bool(run(r#"
        const obj = { x: 1 };
        return obj.propertyIsEnumerable("x");
    "#)), true);
    assert_eq!(as_bool(run(r#"
        const proto = { y: 2 };
        const obj = Object.create(proto);
        return obj.propertyIsEnumerable("y");
    "#)), false);
}

#[test]
fn object_freeze_prevents_mutation() {
    assert_eq!(as_num(run(r#"
        const obj = { x: 5 };
        Object.freeze(obj);
        obj.x = 99;
        return obj.x;
    "#)), 5.0);
}

#[test]
fn object_is_frozen() {
    assert_eq!(as_bool(run(r#"
        const obj = { x: 1 };
        Object.freeze(obj);
        return Object.isFrozen(obj);
    "#)), true);
    assert_eq!(as_bool(run(r#"
        const obj = { x: 1 };
        return Object.isFrozen(obj);
    "#)), false);
}

#[test]
fn object_keys_skip_internal() {
    assert_eq!(as_num(run(r#"
        class Foo { constructor() { this.x = 1; this.y = 2; } }
        const obj = new Foo();
        return Object.keys(obj).length;
    "#)), 2.0);
}

#[test]
fn object_has_own() {
    assert_eq!(as_bool(run(r#"
        const obj = { a: 1 };
        return Object.hasOwn(obj, "a");
    "#)), true);
    assert_eq!(as_bool(run(r#"
        const obj = { a: 1 };
        return Object.hasOwn(obj, "b");
    "#)), false);
}

#[test]
fn object_is_same_value() {
    assert_eq!(as_bool(run(r#"return Object.is(NaN, NaN);"#)), true);
    assert_eq!(as_bool(run(r#"return Object.is(1, 1);"#)), true);
    assert_eq!(as_bool(run(r#"return Object.is(1, 2);"#)), false);
}

#[test]
fn object_define_property_getter() {
    assert_eq!(as_num(run(r#"
        const obj = { _x: 10 };
        Object.defineProperty(obj, "x", {
            get: function() { return this._x * 2; }
        });
        return obj.x;
    "#)), 20.0);
}

#[test]
fn proto_chain_set_prototype_of_null() {
    assert!(matches!(run(r#"
        const proto = { y: 5 };
        const obj = Object.create(proto);
        Object.setPrototypeOf(obj, null);
        return obj.y;
    "#), JsValue::Undefined));
}

#[test]
fn proto_chain_proto_assignment() {
    assert_eq!(as_num(run(r#"
        const proto = { z: 77 };
        const obj = {};
        obj.__proto__ = proto;
        return obj.z;
    "#)), 77.0);
}

#[test]
fn in_operator_walks_proto_chain() {
    assert_eq!(as_bool(run(r#"
        const proto = { inherited: 1 };
        const obj = Object.create(proto);
        return "inherited" in obj;
    "#)), true);
}

#[test]
fn object_values_skip_internal() {
    assert_eq!(as_num(run(r#"
        const obj = { a: 1, b: 2, c: 3 };
        return Object.values(obj).length;
    "#)), 3.0);
}

// ─── Map ─────────────────────────────────────────────────────────────────

#[test]
fn map_basic_set_get() {
    assert_eq!(as_num(run(r#"
        const m = new Map();
        m.set("a", 1);
        m.set("b", 2);
        return m.get("a") + m.get("b");
    "#)), 3.0);
}

#[test]
fn map_has_delete() {
    assert_eq!(as_bool(run(r#"
        const m = new Map();
        m.set("x", 10);
        const had = m.has("x");
        m.delete("x");
        return had && !m.has("x");
    "#)), true);
}

#[test]
fn map_size() {
    assert_eq!(as_num(run(r#"
        const m = new Map();
        m.set(1, "a"); m.set(2, "b"); m.set(3, "c");
        return m.size;
    "#)), 3.0);
}

#[test]
fn map_constructor_with_entries() {
    assert_eq!(as_num(run(r#"
        const m = new Map([["a", 1], ["b", 2], ["c", 3]]);
        return m.size;
    "#)), 3.0);
}

#[test]
fn map_for_of() {
    assert_eq!(as_num(run(r#"
        const m = new Map([["x", 10], ["y", 20]]);
        let sum = 0;
        for (const [k, v] of m) { sum += v; }
        return sum;
    "#)), 30.0);
}

#[test]
fn map_object_key() {
    assert_eq!(as_num(run(r#"
        const m = new Map();
        const key = {};
        m.set(key, 99);
        return m.get(key);
    "#)), 99.0);
}

#[test]
fn map_clear() {
    assert_eq!(as_num(run(r#"
        const m = new Map([["a", 1], ["b", 2]]);
        m.clear();
        return m.size;
    "#)), 0.0);
}

#[test]
fn map_keys_values() {
    assert_eq!(as_num(run(r#"
        const m = new Map([["a", 1], ["b", 2]]);
        let keySum = 0;
        for (const k of m.keys()) { keySum++; }
        let valSum = 0;
        for (const v of m.values()) { valSum += v; }
        return keySum + valSum;
    "#)), 5.0);
}

#[test]
fn map_foreach() {
    assert_eq!(as_num(run(r#"
        const m = new Map([["a", 1], ["b", 2], ["c", 3]]);
        let sum = 0;
        m.forEach((v, k) => { sum += v; });
        return sum;
    "#)), 6.0);
}

#[test]
fn map_update_existing_key() {
    assert_eq!(as_num(run(r#"
        const m = new Map();
        m.set("k", 1); m.set("k", 2);
        return m.get("k");
    "#)), 2.0);
}

// ─── Set ─────────────────────────────────────────────────────────────────

#[test]
fn set_basic_add_has() {
    assert_eq!(as_bool(run(r#"
        const s = new Set();
        s.add(1); s.add(2); s.add(2);
        return s.has(1) && s.has(2) && s.size === 2;
    "#)), true);
}

#[test]
fn set_delete() {
    assert_eq!(as_bool(run(r#"
        const s = new Set([1, 2, 3]);
        s.delete(2);
        return !s.has(2) && s.size === 2;
    "#)), true);
}

#[test]
fn set_for_of() {
    assert_eq!(as_num(run(r#"
        const s = new Set([1, 2, 3, 4, 5]);
        let sum = 0;
        for (const v of s) { sum += v; }
        return sum;
    "#)), 15.0);
}

#[test]
fn set_constructor_with_array() {
    assert_eq!(as_num(run(r#"
        const s = new Set([1, 2, 2, 3, 3, 3]);
        return s.size;
    "#)), 3.0);
}

#[test]
fn set_clear() {
    assert_eq!(as_num(run(r#"
        const s = new Set([1, 2, 3]);
        s.clear();
        return s.size;
    "#)), 0.0);
}

#[test]
fn set_foreach() {
    assert_eq!(as_num(run(r#"
        const s = new Set([10, 20, 30]);
        let sum = 0;
        s.forEach(v => { sum += v; });
        return sum;
    "#)), 60.0);
}

#[test]
fn set_values_iterator() {
    assert_eq!(as_num(run(r#"
        const s = new Set([5, 10, 15]);
        let sum = 0;
        for (const v of s.values()) { sum += v; }
        return sum;
    "#)), 30.0);
}
