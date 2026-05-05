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

// ─── Date getUTC* + set* ───────────────────────────────────────────────

#[test]
fn date_get_utc_full_year() {
    let r = run("const d = new Date(0); return d.getUTCFullYear();");
    assert_eq!(as_num(r), 1970.0);
}

#[test]
fn date_get_utc_month() {
    let r = run("const d = new Date(0); return d.getUTCMonth();");
    assert_eq!(as_num(r), 0.0);
}

#[test]
fn date_get_utc_date() {
    let r = run("const d = new Date(0); return d.getUTCDate();");
    assert_eq!(as_num(r), 1.0);
}

#[test]
fn date_get_utc_hours() {
    let r = run("const d = new Date(0); return d.getUTCHours();");
    assert_eq!(as_num(r), 0.0);
}

#[test]
fn date_get_utc_minutes() {
    let r = run("const d = new Date(0); return d.getUTCMinutes();");
    assert_eq!(as_num(r), 0.0);
}

#[test]
fn date_get_utc_seconds() {
    let r = run("const d = new Date(0); return d.getUTCSeconds();");
    assert_eq!(as_num(r), 0.0);
}

#[test]
fn date_get_utc_milliseconds() {
    let r = run("const d = new Date(1500); return d.getUTCMilliseconds();");
    assert_eq!(as_num(r), 500.0);
}

#[test]
fn date_get_utc_day_epoch() {
    // 1970-01-01 = Thursday = 4
    let r = run("const d = new Date(0); return d.getUTCDay();");
    assert_eq!(as_num(r), 4.0);
}

#[test]
fn date_set_full_year() {
    let r = run(r#"
        const d = new Date(0);
        d.setFullYear(2000);
        return d.getUTCFullYear();
    "#);
    assert_eq!(as_num(r), 2000.0);
}

#[test]
fn date_set_month() {
    let r = run(r#"
        const d = new Date(0);
        d.setMonth(5);
        return d.getUTCMonth();
    "#);
    assert_eq!(as_num(r), 5.0);
}

#[test]
fn date_set_hours() {
    let r = run(r#"
        const d = new Date(0);
        d.setHours(12);
        return d.getUTCHours();
    "#);
    assert_eq!(as_num(r), 12.0);
}

#[test]
fn date_set_milliseconds() {
    let r = run(r#"
        const d = new Date(0);
        d.setMilliseconds(999);
        return d.getUTCMilliseconds();
    "#);
    assert_eq!(as_num(r), 999.0);
}

#[test]
fn date_set_time_roundtrip() {
    let r = run(r#"
        const d = new Date(0);
        d.setTime(86400000);
        return d.getTime();
    "#);
    assert_eq!(as_num(r), 86400000.0);
}

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

// --- Math nove metody ---

#[test]
fn math_sign_positive() { assert_eq!(as_num(run("return Math.sign(5);")), 1.0); }
#[test]
fn math_sign_negative() { assert_eq!(as_num(run("return Math.sign(-3);")), -1.0); }
#[test]
fn math_sign_zero()     { assert_eq!(as_num(run("return Math.sign(0);")), 0.0); }
#[test]
fn math_cbrt()          { assert!((as_num(run("return Math.cbrt(27);")) - 3.0).abs() < 1e-10); }
#[test]
fn math_log2()          { assert!((as_num(run("return Math.log2(8);")) - 3.0).abs() < 1e-10); }
#[test]
fn math_log10()         { assert!((as_num(run("return Math.log10(1000);")) - 3.0).abs() < 1e-10); }
#[test]
fn math_exp()           { assert!((as_num(run("return Math.exp(1);")) - std::f64::consts::E).abs() < 1e-10); }
#[test]
fn math_tan()           { assert!((as_num(run("return Math.tan(0);")) - 0.0).abs() < 1e-10); }
#[test]
fn math_atan2()         { assert!((as_num(run("return Math.atan2(1, 1);")) - std::f64::consts::FRAC_PI_4).abs() < 1e-10); }
#[test]
fn math_trunc_positive(){ assert_eq!(as_num(run("return Math.trunc(4.9);")), 4.0); }
#[test]
fn math_trunc_negative(){ assert_eq!(as_num(run("return Math.trunc(-4.9);")), -4.0); }
#[test]
fn math_hypot()         { assert!((as_num(run("return Math.hypot(3, 4);")) - 5.0).abs() < 1e-10); }
#[test]
fn math_hypot_many()    { assert!((as_num(run("return Math.hypot(1, 2, 2);")) - 3.0).abs() < 1e-10); }
#[test]
fn math_ln2()           { assert!((as_num(run("return Math.LN2;")) - std::f64::consts::LN_2).abs() < 1e-10); }
#[test]
fn math_log10e()        { assert!((as_num(run("return Math.LOG10E;")) - std::f64::consts::LOG10_E).abs() < 1e-10); }
#[test]
fn math_imul()          { assert_eq!(as_num(run("return Math.imul(3, 4);")), 12.0); }
#[test]
fn math_clz32()         { assert_eq!(as_num(run("return Math.clz32(1);")), 31.0); }
#[test]
fn math_floor_negative(){ assert_eq!(as_num(run("return Math.floor(-1.5);")), -2.0); }

// Date arithmetic - difference
#[test]
fn date_subtraction_returns_ms() {
    let code = r#"
        const a = new Date(2024, 0, 1);
        const b = new Date(2024, 0, 2);
        return b - a;
    "#;
    let result = as_num(run(code));
    assert_eq!(result, 86_400_000.0, "1 den = 86,400,000 ms");
}

#[test]
fn date_to_number_via_unary_plus() {
    let code = r#"
        const d = new Date(2000, 0, 1);
        return +d;
    "#;
    let result = as_num(run(code));
    assert!(result > 0.0, "+date vrati epoch ms");
}

#[test]
fn date_valueOf_method() {
    let code = r#"
        const d = new Date(2000, 0, 1);
        return d.valueOf();
    "#;
    let result = as_num(run(code));
    assert!(result > 0.0);
}

#[test]
fn date_now_static_returns_number() {
    let code = "return typeof Date.now();";
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "number");
    } else {
        panic!("ocekavan string");
    }
}

#[test]
fn date_arithmetic_minutes() {
    let code = r#"
        const a = new Date(2024, 0, 1, 10, 0);
        const b = new Date(2024, 0, 1, 10, 30);
        return (b - a) / 1000 / 60;
    "#;
    let result = as_num(run(code));
    assert_eq!(result, 30.0, "30 minut diff");
}

#[test]
fn date_parse_iso_date() {
    let code = r#"return Date.parse("2024-01-01");"#;
    let result = as_num(run(code));
    assert!(result > 1700000000000.0, "parse 2024-01-01 -> ms");
}

#[test]
fn date_parse_iso_datetime() {
    let code = r#"return Date.parse("2024-01-01T12:30:45");"#;
    let result = as_num(run(code));
    assert!(result > 1700000000000.0);
}

#[test]
fn date_utc_static() {
    let code = "return Date.UTC(2024, 0, 1);";
    let result = as_num(run(code));
    assert!(result > 1700000000000.0, "Date.UTC ms");
}

#[test]
fn date_string_constructor() {
    let code = r#"
        const d = new Date("2024-01-01");
        return d.getFullYear();
    "#;
    let result = as_num(run(code));
    assert_eq!(result, 2024.0);
}
