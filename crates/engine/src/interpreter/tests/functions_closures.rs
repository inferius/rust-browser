/// Funkce, arrow, closures, default/rest params, spread, optional chaining.

use super::helpers::*;
use crate::interpreter::JsValue;

#[test]
fn function_declaration_and_call() {
    assert_eq!(as_num(run(r#"
        function add(a, b) { return a + b; }
        return add(3, 4);
    "#)), 7.0);
}

#[test]
fn function_recursion() {
    assert_eq!(as_num(run(r#"
        function fact(n) {
            if (n <= 1) return 1;
            return n * fact(n - 1);
        }
        return fact(5);
    "#)), 120.0);
}

#[test]
fn arrow_function() {
    assert_eq!(as_num(run(r#"
        const square = x => x * x;
        return square(5);
    "#)), 25.0);
}

#[test]
fn arrow_paren_params() {
    assert_eq!(as_num(run(r#"
        const add = (a, b) => a + b;
        return add(3, 4);
    "#)), 7.0);
}

#[test]
fn closure() {
    assert_eq!(as_num(run(r#"
        function makeAdder(x) {
            return (y) => x + y;
        }
        const add5 = makeAdder(5);
        return add5(3);
    "#)), 8.0);
}

#[test]
fn default_params_basic() {
    assert_eq!(as_num(run(r#"
        function greet(x, y = 10) { return x + y; }
        return greet(5);
    "#)), 15.0);
}

#[test]
fn default_params_override() {
    assert_eq!(as_num(run(r#"
        function greet(x, y = 10) { return x + y; }
        return greet(5, 3);
    "#)), 8.0);
}

#[test]
fn default_params_undefined_triggers_default() {
    assert_eq!(as_num(run(r#"
        function f(a = 42) { return a; }
        return f(undefined);
    "#)), 42.0);
}

#[test]
fn rest_params_collect() {
    assert_eq!(as_num(run(r#"
        function sum(...nums) {
            let total = 0;
            for (let n of nums) total += n;
            return total;
        }
        return sum(1, 2, 3, 4);
    "#)), 10.0);
}

#[test]
fn rest_params_after_fixed() {
    assert_eq!(as_num(run(r#"
        function f(first, ...rest) { return rest.length; }
        return f(1, 2, 3, 4);
    "#)), 3.0);
}

#[test]
fn spread_in_call() {
    assert_eq!(as_num(run(r#"
        function add(a, b, c) { return a + b + c; }
        const args = [1, 2, 3];
        return add(...args);
    "#)), 6.0);
}

#[test]
fn optional_chaining_null_prop() {
    assert!(matches!(run(r#"
        const obj = null;
        return obj?.foo;
    "#), JsValue::Undefined));
}

#[test]
fn optional_chaining_null_call() {
    assert!(matches!(run(r#"
        const obj = null;
        return obj?.foo();
    "#), JsValue::Undefined));
}

#[test]
fn optional_chaining_valid_prop() {
    assert_eq!(as_num(run(r#"
        const obj = { x: 42 };
        return obj?.x;
    "#)), 42.0);
}

#[test]
fn optional_chaining_nested() {
    assert!(matches!(run(r#"
        const obj = { a: null };
        return obj?.a?.b;
    "#), JsValue::Undefined));
}

#[test]
fn template_no_substitution() {
    assert_eq!(as_str(run(r#"return `hello world`;"#)), "hello world");
}

#[test]
fn template_with_expr() {
    assert_eq!(as_str(run(r#"
        let name = "World";
        return `Hello ${name}!`;
    "#)), "Hello World!");
}

#[test]
fn template_arithmetic() {
    assert_eq!(as_str(run(r#"return `result: ${1 + 2}`;"#)), "result: 3");
}

#[test]
fn function_call_method() {
    assert_eq!(as_num(run(r#"
        function add(a, b) { return a + b; }
        return add.call(null, 3, 4);
    "#)), 7.0);
}

#[test]
fn function_apply_method() {
    assert_eq!(as_num(run(r#"
        function add(a, b) { return a + b; }
        return add.apply(null, [5, 6]);
    "#)), 11.0);
}

#[test]
fn function_bind_method() {
    assert_eq!(as_num(run(r#"
        function add(a, b) { return a + b; }
        const add5 = add.bind(null, 5);
        return add5(3);
    "#)), 8.0);
}

#[test]
fn bind_preserves_this() {
    assert_eq!(as_str(run(r#"
        const obj = { name: "world" };
        function greet() { return "hello " + this.name; }
        const fn2 = greet.bind(obj);
        return fn2();
    "#)), "hello world");
}
