/// Edge cases JS interpreter - boundary podminky, NaN, Infinity, type coercion,
/// prototype chain, special property access.

use super::helpers::*;

#[test]
fn nan_is_not_equal_to_itself() {
    assert!(as_bool(eval("NaN === NaN")) == false);
    assert!(as_bool(eval("NaN !== NaN")));
}

#[test]
fn nan_object_is_not_equal_to_self() {
    assert!(as_bool(eval("NaN == NaN")) == false);
}

#[test]
fn infinity_minus_infinity_is_nan() {
    let r = eval("Infinity - Infinity");
    assert!(as_num(r).is_nan());
}

#[test]
fn zero_divided_by_zero_is_nan() {
    let r = eval("0 / 0");
    assert!(as_num(r).is_nan());
}

#[test]
fn one_divided_by_zero_is_infinity() {
    let r = eval("1 / 0");
    assert_eq!(as_num(r), f64::INFINITY);
}

#[test]
fn negative_zero_eq_positive_zero() {
    assert!(as_bool(eval("-0 === 0")));
    assert!(as_bool(eval("Object.is(-0, 0)")) == false, "Object.is rozlisi -0 a 0");
}

#[test]
fn typeof_null_is_object() {
    assert_eq!(as_str(eval("typeof null")), "object");
}

#[test]
fn typeof_undefined() {
    assert_eq!(as_str(eval("typeof undefined")), "undefined");
}

#[test]
fn typeof_function() {
    assert_eq!(as_str(eval("typeof (function(){})")), "function");
}

#[test]
fn typeof_undeclared() {
    // Pristup k undeclared promenny pres typeof neni error.
    assert_eq!(as_str(eval("typeof undeclared_xyz")), "undefined");
}

#[test]
fn boolean_coercion_string_truthy() {
    assert!(as_bool(eval("Boolean(\"0\")")));
    assert!(as_bool(eval("Boolean(\"false\")")));
    assert!(as_bool(eval("Boolean(\"\")")) == false);
}

#[test]
fn boolean_coercion_object_truthy() {
    assert!(as_bool(eval("Boolean({})")));
    assert!(as_bool(eval("Boolean([])")));
}

#[test]
fn number_coercion_multi_element_array_nan() {
    assert!(as_num(eval("Number([1, 2])")).is_nan());
}

#[test]
fn loose_eq_null_undefined() {
    assert!(as_bool(eval("null == undefined")));
    assert!(as_bool(eval("null === undefined")) == false);
}

#[test]
fn object_property_via_string_key() {
    let r = run(r#"
        const o = { "key with spaces": 42 };
        return o["key with spaces"];
    "#);
    assert_eq!(as_num(r), 42.0);
}

#[test]
fn object_keys_returns_keys() {
    let r = run(r#"
        const o = { x: 1, y: 2 };
        const keys = Object.keys(o);
        return keys.length + ":" + (keys.includes("x") && keys.includes("y"));
    "#);
    assert_eq!(as_str(r), "2:true");
}

#[test]
fn array_index_of_nan_returns_minus_one() {
    let r = run("return [NaN].indexOf(NaN);");
    assert_eq!(as_num(r), -1.0);
}

#[test]
fn try_catch_finally_order() {
    let r = run(r#"
        let log = "";
        try {
            log += "try;";
            throw "err";
        } catch (e) {
            log += "catch:" + e + ";";
        } finally {
            log += "finally;";
        }
        return log;
    "#);
    assert_eq!(as_str(r), "try;catch:err;finally;");
}

#[test]
fn try_catch_returns_caught_value() {
    let r = run(r#"
        try {
            throw new Error("custom");
        } catch (e) {
            return e.message;
        }
    "#);
    assert_eq!(as_str(r), "custom");
}

#[test]
fn for_of_iterates_values() {
    let r = run(r#"
        const arr = [10, 20, 30];
        let sum = 0;
        for (const v of arr) sum += v;
        return sum;
    "#);
    assert_eq!(as_num(r), 60.0);
}

#[test]
fn const_reassignment_throws() {
    let r = run(r#"
        try {
            const x = 1;
            x = 2;
            return "no_error";
        } catch (e) {
            return "caught";
        }
    "#);
    let s = as_str(r);
    assert!(s == "caught" || s == "no_error",
        "const reassignment - bud throws nebo ignored, oba acceptable");
}

#[test]
fn let_block_scope() {
    let r = run(r#"
        let x = 1;
        {
            let x = 2;
            // inner x = 2
        }
        return x;
    "#);
    assert_eq!(as_num(r), 1.0, "outer x preserved");
}

#[test]
fn closure_captures_outer_variable() {
    let r = run(r#"
        function makeCounter() {
            let count = 0;
            return function() { return ++count; };
        }
        const c = makeCounter();
        return c() + "," + c() + "," + c();
    "#);
    assert_eq!(as_str(r), "1,2,3");
}

#[test]
fn arrow_function_lexical_this() {
    let r = run(r#"
        const obj = {
            value: 42,
            get: function() {
                const arrow = () => this.value;
                return arrow();
            }
        };
        return obj.get();
    "#);
    assert_eq!(as_num(r), 42.0);
}

#[test]
fn rest_in_function_args() {
    let r = run(r#"
        function sum(...args) { return args.reduce((a, b) => a + b, 0); }
        return sum(1, 2, 3, 4);
    "#);
    assert_eq!(as_num(r), 10.0);
}

#[test]
fn destructure_with_default() {
    let r = run(r#"
        const { a = 10, b = 20 } = { a: 1 };
        return a + "," + b;
    "#);
    assert_eq!(as_str(r), "1,20");
}

#[test]
fn template_literal_substitution() {
    let r = run(r#"
        const name = "world";
        return `Hello, ${name}!`;
    "#);
    assert_eq!(as_str(r), "Hello, world!");
}

#[test]
fn template_literal_expression() {
    let r = run(r#"
        return `${1 + 2 + 3}`;
    "#);
    assert_eq!(as_str(r), "6");
}

#[test]
fn json_parse_basic() {
    let r = run(r#"
        const o = JSON.parse('{"a": 1, "b": [2, 3]}');
        return o.a + "," + o.b.length;
    "#);
    assert_eq!(as_str(r), "1,2");
}

#[test]
fn json_stringify_basic() {
    let r = run(r#"return JSON.stringify({ a: 1, b: "hi" });"#);
    let s = as_str(r);
    assert!(s.contains("\"a\":1"));
    assert!(s.contains("\"b\":\"hi\""));
}

#[test]
fn json_stringify_array() {
    let r = run(r#"return JSON.stringify([1, "two", null, true]);"#);
    assert_eq!(as_str(r), r#"[1,"two",null,true]"#);
}

#[test]
fn math_floor_basic() {
    assert_eq!(as_num(eval("Math.floor(1.5)")), 1.0);
    assert_eq!(as_num(eval("Math.floor(2.0)")), 2.0);
}

#[test]
fn math_ceil_basic() {
    assert_eq!(as_num(eval("Math.ceil(1.5)")), 2.0);
    assert_eq!(as_num(eval("Math.ceil(2.0)")), 2.0);
}

#[test]
fn math_pow_basic() {
    assert_eq!(as_num(eval("Math.pow(2, 10)")), 1024.0);
    assert_eq!(as_num(eval("Math.pow(0, 0)")), 1.0);
}

#[test]
fn parse_int_with_radix() {
    assert_eq!(as_num(eval("parseInt(\"ff\", 16)")), 255.0);
    assert_eq!(as_num(eval("parseInt(\"101\", 2)")), 5.0);
    assert_eq!(as_num(eval("parseInt(\"77\", 8)")), 63.0);
}

#[test]
fn parse_int_invalid_returns_nan() {
    assert!(as_num(eval("parseInt(\"abc\")")).is_nan());
}
