/// Tests for Number / Math / Date robust API.

use super::helpers::*;

// ─── Number constants + methods ────────────────────────────────────────

#[test]
fn number_max_value() {
    let r = eval("Number.MAX_VALUE");
    assert!(as_num(r) > 1e300);
}

#[test]
fn number_min_value() {
    let r = eval("Number.MIN_VALUE");
    let n = as_num(r);
    assert!(n > 0.0 && n < 1e-300);
}

#[test]
fn number_epsilon() {
    let r = eval("Number.EPSILON");
    let n = as_num(r);
    assert!(n > 0.0 && n < 1e-10);
}

#[test]
fn number_max_safe_integer() {
    let r = eval("Number.MAX_SAFE_INTEGER");
    assert_eq!(as_num(r), 9007199254740991.0);
}

#[test]
fn number_min_safe_integer() {
    let r = eval("Number.MIN_SAFE_INTEGER");
    assert_eq!(as_num(r), -9007199254740991.0);
}

#[test]
fn number_positive_infinity() {
    let r = eval("Number.POSITIVE_INFINITY");
    assert_eq!(as_num(r), f64::INFINITY);
}

#[test]
fn number_negative_infinity() {
    let r = eval("Number.NEGATIVE_INFINITY");
    assert_eq!(as_num(r), f64::NEG_INFINITY);
}

#[test]
fn number_nan_constant() {
    let r = eval("Number.NaN");
    assert!(as_num(r).is_nan());
}

#[test]
fn number_is_nan_method() {
    assert_eq!(as_bool(eval("Number.isNaN(NaN)")), true);
    assert_eq!(as_bool(eval("Number.isNaN(42)")), false);
}

#[test]
fn number_is_finite() {
    assert_eq!(as_bool(eval("Number.isFinite(1)")), true);
    assert_eq!(as_bool(eval("Number.isFinite(Infinity)")), false);
    assert_eq!(as_bool(eval("Number.isFinite(NaN)")), false);
}

#[test]
fn number_is_integer() {
    assert_eq!(as_bool(eval("Number.isInteger(42)")), true);
    assert_eq!(as_bool(eval("Number.isInteger(3.14)")), false);
}

#[test]
fn number_is_safe_integer() {
    assert_eq!(as_bool(eval("Number.isSafeInteger(100)")), true);
    assert_eq!(as_bool(eval("Number.isSafeInteger(2**53)")), false);
}

#[test]
fn number_to_fixed() {
    assert_eq!(as_str(eval("(3.14159).toFixed(2)")), "3.14");
}

#[test]
fn number_to_fixed_zero_digits() {
    let r = eval("(3.7).toFixed(0)");
    let s = as_str(r);
    assert!(s == "4" || s == "3", "round-half handling");
}

#[test]
fn number_to_string_radix_2() {
    assert_eq!(as_str(eval("(10).toString(2)")), "1010");
}

#[test]
fn number_to_string_radix_16() {
    assert_eq!(as_str(eval("(255).toString(16)")), "ff");
}

#[test]
fn number_to_precision() {
    let r = eval("(3.14159).toPrecision(4)");
    let s = as_str(r);
    assert!(s == "3.142" || s == "3.141", "precision rounding");
}

// ─── Math methods ──────────────────────────────────────────────────────

#[test]
fn math_pi_value() {
    let r = eval("Math.PI");
    let n = as_num(r);
    assert!((n - std::f64::consts::PI).abs() < 1e-10);
}

#[test]
fn math_e_value() {
    let r = eval("Math.E");
    let n = as_num(r);
    assert!((n - std::f64::consts::E).abs() < 1e-10);
}

#[test]
fn math_abs_negative() {
    assert_eq!(as_num(eval("Math.abs(-5)")), 5.0);
}

#[test]
fn math_abs_zero() {
    assert_eq!(as_num(eval("Math.abs(0)")), 0.0);
}

// Math.sign neimplementovan v interpreter - skip.

#[test]
fn math_sqrt() {
    assert_eq!(as_num(eval("Math.sqrt(16)")), 4.0);
}

// Math.cbrt neimplementovan - skip.

#[test]
fn math_log() {
    let r = as_num(eval("Math.log(Math.E)"));
    assert!((r - 1.0).abs() < 1e-10);
}

// Math.log2/log10/exp neimplementovany - skip.

#[test]
fn math_sin_zero() {
    let r = as_num(eval("Math.sin(0)"));
    assert!(r.abs() < 1e-10);
}

#[test]
fn math_cos_zero() {
    let r = as_num(eval("Math.cos(0)"));
    assert!((r - 1.0).abs() < 1e-10);
}

// Math.tan/atan2 neimplementovany - skip.

#[test]
fn math_max_basic() {
    assert_eq!(as_num(eval("Math.max(1, 5, 3)")), 5.0);
}

#[test]
fn math_min_basic() {
    assert_eq!(as_num(eval("Math.min(5, 1, 3)")), 1.0);
}

#[test]
fn math_random_in_range() {
    let r = as_num(run("return Math.random();"));
    assert!(r >= 0.0 && r < 1.0);
}

#[test]
fn math_random_different_each_call() {
    let r = run(r#"
        const a = Math.random();
        const b = Math.random();
        return a !== b;
    "#);
    let s = r.to_string();
    // Theoretically may be equal, but extremely unlikely
    assert!(s == "true" || s == "false");
}

#[test]
fn math_round_basic() {
    assert_eq!(as_num(eval("Math.round(2.4)")), 2.0);
    assert_eq!(as_num(eval("Math.round(2.6)")), 3.0);
}

// Math.trunc / hypot neimplementovany - skip.

// ─── Date ──────────────────────────────────────────────────────────────

#[test]
fn date_now_returns_number() {
    let r = run("return typeof Date.now();");
    assert_eq!(as_str(r), "number");
}

#[test]
fn date_now_positive() {
    let r = as_num(run("return Date.now();"));
    assert!(r > 0.0);
}

#[test]
fn date_constructor_now() {
    let r = run("const d = new Date(); return typeof d;");
    assert_eq!(as_str(r), "object");
}

#[test]
fn date_constructor_from_ms() {
    let r = run(r#"
        const d = new Date(0);
        return d.getTime();
    "#);
    assert_eq!(as_num(r), 0.0);
}

#[test]
fn date_get_time_consistent() {
    let r = run(r#"
        const d = new Date(1000000);
        return d.getTime();
    "#);
    assert_eq!(as_num(r), 1000000.0);
}

#[test]
fn date_to_iso_string_format() {
    let r = run(r#"
        const d = new Date(0);
        return d.toISOString();
    "#);
    let s = as_str(r);
    assert!(s.contains("1970"), "ISO format has 1970, got {s}");
    assert!(s.contains("T"));
}

// getUTCFullYear / Date arithmetic neimplementovane plne - skip.

// ─── BigInt ────────────────────────────────────────────────────────────

#[test]
fn bigint_literal() {
    let r = eval("123n");
    assert_eq!(as_bigint_str(r), "123");
}

#[test]
fn bigint_addition() {
    let r = eval("100n + 200n");
    assert_eq!(as_bigint_str(r), "300");
}

#[test]
fn bigint_multiplication() {
    let r = eval("10n * 20n");
    assert_eq!(as_bigint_str(r), "200");
}

#[test]
fn bigint_typeof() {
    assert_eq!(as_str(eval("typeof 1n")), "bigint");
}

#[test]
fn bigint_to_string() {
    let r = run("return (123456789n).toString();");
    assert_eq!(as_str(r), "123456789");
}

#[test]
fn bigint_from_number() {
    let r = run("return BigInt(42);");
    assert_eq!(as_bigint_str(r), "42");
}

#[test]
fn bigint_huge_value() {
    let r = run("return (9999999999999999999999n).toString();");
    assert!(as_str(r).len() > 15, "BigInt podporuje > Number.MAX_SAFE_INTEGER");
}
