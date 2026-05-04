/// Aritmetika, porovnani, logicke operatory, typeof, promenne, scope.

use super::helpers::*;
use crate::interpreter::JsValue;

#[test]
fn arithmetic_basic() {
    assert_eq!(as_num(eval("1 + 2")), 3.0);
    assert_eq!(as_num(eval("10 - 3")), 7.0);
    assert_eq!(as_num(eval("3 * 4")), 12.0);
    assert_eq!(as_num(eval("10 / 4")), 2.5);
    assert_eq!(as_num(eval("10 % 3")), 1.0);
    assert_eq!(as_num(eval("2 ** 10")), 1024.0);
}

#[test]
fn arithmetic_precedence() {
    assert_eq!(as_num(eval("2 + 3 * 4")), 14.0);
    assert_eq!(as_num(eval("(2 + 3) * 4")), 20.0);
}

#[test]
fn unary_minus() {
    assert_eq!(as_num(eval("-5")), -5.0);
    assert_eq!(as_num(eval("-(3 + 2)")), -5.0);
}

#[test]
fn comparisons() {
    assert!(as_bool(eval("1 < 2")));
    assert!(!as_bool(eval("2 < 1")));
    assert!(as_bool(eval("2 <= 2")));
    assert!(as_bool(eval("3 > 2")));
    assert!(as_bool(eval("1 === 1")));
    assert!(!as_bool(eval("1 === 2")));
    assert!(as_bool(eval("1 !== 2")));
}

#[test]
fn loose_equality() {
    assert!(as_bool(eval("1 == 1")));
    assert!(as_bool(eval(r#"1 == "1""#)));
    assert!(!as_bool(eval("1 === \"1\"")));
}

#[test]
fn logical_and_or() {
    assert!(as_bool(eval("true && true")));
    assert!(!as_bool(eval("true && false")));
    assert!(as_bool(eval("false || true")));
    assert!(!as_bool(eval("false || false")));
}

#[test]
fn nullish_coalescing() {
    assert_eq!(as_num(eval("null ?? 42")), 42.0);
    assert_eq!(as_num(eval("undefined ?? 7")), 7.0);
    assert_eq!(as_num(eval("5 ?? 42")), 5.0);
}

#[test]
fn let_declaration() {
    assert_eq!(as_num(run("let x = 10; return x;")), 10.0);
}

#[test]
fn const_declaration() {
    assert_eq!(as_num(run("const PI = 3.14; return PI;")), 3.14);
}

#[test]
fn var_hoisting() {
    assert_eq!(as_num(run("var x = 5; return x;")), 5.0);
}

#[test]
fn block_scope() {
    assert_eq!(as_num(run(r#"
        let x = 1;
        { let x = 2; }
        return x;
    "#)), 1.0);
}

#[test]
fn typeof_values() {
    assert_eq!(as_str(eval("typeof 42")), "number");
    assert_eq!(as_str(eval(r#"typeof "hello""#)), "string");
    assert_eq!(as_str(eval("typeof true")), "boolean");
    assert_eq!(as_str(eval("typeof undefined")), "undefined");
    assert_eq!(as_str(eval("typeof null")), "object");
}

#[test]
fn logical_and_assign() {
    assert_eq!(as_num(run(r#"
        let x = 5;
        x &&= 10;
        return x;
    "#)), 10.0);
}

#[test]
fn logical_and_assign_falsy() {
    assert_eq!(as_num(run(r#"
        let x = 0;
        x &&= 10;
        return x;
    "#)), 0.0);
}

#[test]
fn logical_or_assign() {
    assert_eq!(as_num(run(r#"
        let x = 0;
        x ||= 42;
        return x;
    "#)), 42.0);
}

#[test]
fn logical_or_assign_truthy() {
    assert_eq!(as_num(run(r#"
        let x = 5;
        x ||= 42;
        return x;
    "#)), 5.0);
}

#[test]
fn nullish_assign() {
    assert_eq!(as_num(run(r#"
        let x = null;
        x ??= 99;
        return x;
    "#)), 99.0);
}

#[test]
fn nullish_assign_non_null() {
    assert_eq!(as_num(run(r#"
        let x = 5;
        x ??= 99;
        return x;
    "#)), 5.0);
}

// Marks unused warning suppression
#[allow(unused)]
fn _use_jsvalue(_: JsValue) {}
