/// Testy Error types + class inheritance + custom errors.

use super::helpers::*;

#[test]
fn error_basic_message() {
    let r = run(r#"
        const e = new Error("oops");
        return e.message;
    "#);
    assert_eq!(as_str(r), "oops");
}

#[test]
fn error_name_property() {
    let r = run(r#"
        return new Error("x").name;
    "#);
    assert_eq!(as_str(r), "Error");
}

#[test]
fn type_error_name() {
    let r = run(r#"
        return new TypeError("x").name;
    "#);
    assert_eq!(as_str(r), "TypeError");
}

#[test]
fn range_error_name() {
    let r = run(r#"
        return new RangeError("x").name;
    "#);
    assert_eq!(as_str(r), "RangeError");
}

#[test]
fn syntax_error_name() {
    let r = run(r#"
        return new SyntaxError("x").name;
    "#);
    assert_eq!(as_str(r), "SyntaxError");
}

#[test]
fn reference_error_name() {
    let r = run(r#"
        return new ReferenceError("x").name;
    "#);
    assert_eq!(as_str(r), "ReferenceError");
}

// Error.toString() - skip (ne plne impl, vraci [object Object]).

#[test]
fn throw_string() {
    let r = run(r#"
        try { throw "string error"; }
        catch (e) { return typeof e + ":" + e; }
    "#);
    assert_eq!(as_str(r), "string:string error");
}

#[test]
fn throw_number() {
    let r = run(r#"
        try { throw 42; }
        catch (e) { return typeof e + ":" + e; }
    "#);
    assert_eq!(as_str(r), "number:42");
}

#[test]
fn throw_object() {
    let r = run(r#"
        try { throw { code: 404, msg: "not found" }; }
        catch (e) { return e.code + ":" + e.msg; }
    "#);
    assert_eq!(as_str(r), "404:not found");
}

#[test]
fn throw_in_function_propagates() {
    let r = run(r#"
        function failing() { throw new Error("inner"); }
        try { failing(); }
        catch (e) { return e.message; }
    "#);
    assert_eq!(as_str(r), "inner");
}

#[test]
fn throw_in_nested_function() {
    let r = run(r#"
        function a() { b(); }
        function b() { c(); }
        function c() { throw "deep"; }
        try { a(); }
        catch (e) { return e; }
    "#);
    assert_eq!(as_str(r), "deep");
}

// ─── class inheritance ────────────────────────────────────────────────

#[test]
fn class_basic_instance() {
    let r = run(r#"
        class Foo {
            constructor(x) { this.x = x; }
            getX() { return this.x; }
        }
        return new Foo(42).getX();
    "#);
    assert_eq!(as_num(r), 42.0);
}

#[test]
fn class_extends_inherits_methods() {
    let r = run(r#"
        class Animal {
            speak() { return "generic"; }
        }
        class Dog extends Animal {
            bark() { return "woof"; }
        }
        const d = new Dog();
        return d.speak() + ":" + d.bark();
    "#);
    assert_eq!(as_str(r), "generic:woof");
}

#[test]
fn class_extends_super_call() {
    let r = run(r#"
        class A {
            constructor() { this.value = "from A"; }
        }
        class B extends A {
            constructor() {
                super();
                this.extra = "from B";
            }
        }
        const b = new B();
        return b.value + ":" + b.extra;
    "#);
    assert_eq!(as_str(r), "from A:from B");
}

#[test]
fn class_method_override() {
    let r = run(r#"
        class A { foo() { return "A.foo"; } }
        class B extends A { foo() { return "B.foo"; } }
        return new B().foo();
    "#);
    assert_eq!(as_str(r), "B.foo");
}

#[test]
fn class_static_method_callable() {
    let r = run(r#"
        class C {
            static factory() { return 42; }
        }
        return C.factory();
    "#);
    assert_eq!(as_num(r), 42.0);
}

#[test]
fn instance_of_basic() {
    let r = run(r#"
        class Foo {}
        const f = new Foo();
        return f instanceof Foo;
    "#);
    assert_eq!(as_bool(r), true);
}

#[test]
fn instance_of_unrelated_false() {
    let r = run(r#"
        class A {}
        class B {}
        return (new A()) instanceof B;
    "#);
    assert_eq!(as_bool(r), false);
}

// ─── Function arguments ──────────────────────────────────────────────

#[test]
fn function_arguments_object() {
    let r = run(r#"
        function fn() { return arguments.length; }
        return fn(1, 2, 3, 4);
    "#);
    assert_eq!(as_num(r), 4.0);
}

#[test]
fn function_arguments_index_access() {
    let r = run(r#"
        function fn() { return arguments[0] + arguments[1]; }
        return fn(10, 20);
    "#);
    assert_eq!(as_num(r), 30.0);
}

#[test]
fn function_default_param() {
    let r = run(r#"
        function fn(x = 100) { return x; }
        return fn();
    "#);
    assert_eq!(as_num(r), 100.0);
}

#[test]
fn function_default_param_overridden() {
    let r = run(r#"
        function fn(x = 100) { return x; }
        return fn(42);
    "#);
    assert_eq!(as_num(r), 42.0);
}

// ─── Object inheritance ───────────────────────────────────────────────

#[test]
fn object_create_proto_chain() {
    let r = run(r#"
        const proto = { greet() { return "hi"; } };
        const obj = Object.create(proto);
        return obj.greet();
    "#);
    assert_eq!(as_str(r), "hi");
}

#[test]
fn object_assign_merges() {
    let r = run(r#"
        const o = Object.assign({}, { a: 1 }, { b: 2 });
        return o.a + ":" + o.b;
    "#);
    assert_eq!(as_str(r), "1:2");
}

#[test]
fn object_keys_values() {
    let r = run(r#"
        const o = { x: 1, y: 2, z: 3 };
        return Object.keys(o).length + ":" + Object.values(o).length;
    "#);
    assert_eq!(as_str(r), "3:3");
}

#[test]
fn object_entries_pairs() {
    let r = run(r#"
        const o = { a: 1 };
        const ent = Object.entries(o);
        return ent.length + ":" + ent[0][0] + "=" + ent[0][1];
    "#);
    assert_eq!(as_str(r), "1:a=1");
}

#[test]
fn object_freeze_blocks_mutation() {
    let r = run(r#"
        const o = Object.freeze({ x: 1 });
        try { o.x = 2; } catch(e) {}
        return o.x;
    "#);
    // Pri strict mode throw, jinak silently fails
    assert_eq!(as_num(r), 1.0);
}

// Object.isExtensible neimplementovan - skip.

// Object.isFrozen ne plne impl - skip.

#[test]
fn json_stringify_with_indent() {
    let r = run(r#"
        return JSON.stringify({ a: 1 }, null, 2);
    "#);
    let s = as_str(r);
    assert!(s.contains("\"a\""));
    assert!(s.contains("1"));
}

// JSON.stringify circular reference - skip (interpreter ne plne handluje, stack overflow).

#[test]
fn json_parse_invalid_throws() {
    let r = run(r#"
        try {
            JSON.parse("{invalid");
            return "no_throw";
        } catch (e) {
            return "caught";
        }
    "#);
    assert_eq!(as_str(r), "caught");
}

// --- JSON.stringify circular reference ---

#[test]
fn json_stringify_circular_throws() {
    let r = run(r#"
        try {
            const obj = {};
            obj.self = obj;
            JSON.stringify(obj);
            return "no_throw";
        } catch (e) {
            return "circular";
        }
    "#);
    assert_eq!(as_str(r), "circular");
}

#[test]
fn json_stringify_circular_array() {
    let r = run(r#"
        try {
            const a = [1, 2];
            a.push(a);
            JSON.stringify(a);
            return "no_throw";
        } catch (e) {
            return "circular";
        }
    "#);
    assert_eq!(as_str(r), "circular");
}

#[test]
fn json_stringify_non_circular_nested_ok() {
    let r = run(r#"
        const shared = { x: 1 };
        const obj = { a: shared, b: shared };
        return JSON.stringify(obj);
    "#);
    // Shared reference (ne circular) musi projit
    let s = as_str(r);
    assert!(s.contains("\"x\""), "got: {s}");
}
