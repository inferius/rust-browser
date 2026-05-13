/// Batch H zbyle: Number instance metody, eval(), setTimeout/clearTimeout.

use super::helpers::*;

// ─── Number instance metody ─────────────────────────────────────

#[test]
fn number_to_fixed() {
    assert_eq!(as_str(eval("(3.14159).toFixed(2)")), "3.14");
    assert_eq!(as_str(eval("(1.005).toFixed(2)")), "1.00");
    assert_eq!(as_str(eval("(42).toFixed(0)")), "42");
}

#[test]
fn number_to_string_radix() {
    assert_eq!(as_str(eval("(255).toString(16)")), "ff");
    assert_eq!(as_str(eval("(255).toString(2)")), "11111111");
    assert_eq!(as_str(eval("(8).toString(8)")), "10");
    assert_eq!(as_str(eval("(42).toString()")), "42");
}

#[test]
fn number_to_exponential() {
    assert_eq!(as_str(eval("(123456).toExponential(2)")), "1.23e+5");
}

#[test]
fn number_to_locale_string() {
    assert_eq!(as_str(eval("(1234567).toLocaleString()")), "1,234,567");
}

// ─── Number staticke metody ─────────────────────────────────────

#[test]
fn number_is_integer() {
    assert_eq!(as_bool(run(r#"return Number.isInteger(42);"#)), true);
    assert_eq!(as_bool(run(r#"return Number.isInteger(42.5);"#)), false);
    assert_eq!(as_bool(run(r#"return Number.isInteger("42");"#)), false);
}

#[test]
fn number_is_finite() {
    assert_eq!(as_bool(run(r#"return Number.isFinite(42);"#)), true);
    assert_eq!(as_bool(run(r#"return Number.isFinite(Infinity);"#)), false);
    assert_eq!(as_bool(run(r#"return Number.isFinite(NaN);"#)), false);
}

#[test]
fn number_is_nan() {
    assert_eq!(as_bool(run(r#"return Number.isNaN(NaN);"#)), true);
    assert_eq!(as_bool(run(r#"return Number.isNaN(42);"#)), false);
    assert_eq!(as_bool(run(r#"return Number.isNaN("hello");"#)), false);
}

#[test]
fn number_max_safe_integer() {
    assert_eq!(as_num(run(r#"return Number.MAX_SAFE_INTEGER;"#)), 9007199254740991.0);
}

#[test]
fn number_parse_int() {
    assert_eq!(as_num(run(r#"return Number.parseInt("42");"#)), 42.0);
    assert_eq!(as_num(run(r#"return Number.parseInt("ff", 16);"#)), 255.0);
}

#[test]
fn number_parse_float() {
    assert_eq!(as_num(run(r#"return Number.parseFloat("3.14");"#)), 3.14);
}

// ─── eval() ─────────────────────────────────────────────────────

#[test]
fn eval_basic_expression() {
    assert_eq!(as_num(run(r#"return eval("1 + 2");"#)), 3.0);
}

#[test]
fn eval_uses_current_scope() {
    assert_eq!(as_num(run(r#"
        let x = 10;
        eval("x = 42;");
        return x;
    "#)), 42.0);
}

#[test]
fn eval_defines_variable_in_scope() {
    assert_eq!(as_str(run(r#"
        eval("var greeting = 'hello';");
        return greeting;
    "#)), "hello");
}

#[test]
fn eval_non_string_passthrough() {
    assert_eq!(as_num(run(r#"return eval(42);"#)), 42.0);
}

// ─── setTimeout / clearTimeout / setInterval ────────────────────

#[test]
fn set_timeout_basic() {
    assert_eq!(as_num(run(r#"
        let x = 0;
        setTimeout(() => { x = 99; }, 0);
        return x;
    "#)), 0.0);
}

#[test]
fn set_timeout_with_args() {
    let v = run(r#"
        let result = 0;
        function cb(a, b) { result = a + b; }
        setTimeout(cb, 0, 10, 20);
        return result;
    "#);
    assert_eq!(as_num(v), 0.0);
}

#[test]
fn clear_timeout_cancels() {
    let v = run(r#"
        let x = 0;
        const id = setTimeout(() => { x = 1; }, 0);
        clearTimeout(id);
        return x;
    "#);
    assert_eq!(as_num(v), 0.0);
}

#[test]
fn set_timeout_returns_id() {
    let v = run(r#"return typeof setTimeout(() => {}, 0);"#);
    assert_eq!(as_str(v), "number");
}
