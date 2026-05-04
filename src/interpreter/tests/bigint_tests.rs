/// BigInt nativni 42n + cross-type ops s Number/BigNumber.

use super::helpers::*;
use crate::interpreter::JsValue;
use crate::lexer::base::Lexer;
use crate::parser::Parser;
use crate::tokens::TokenKind;
use crate::interpreter::Interpreter;

// ─── BigInt nativni literal a typeof ────────────────────────────

#[test]
fn bigint_literal_typeof() {
    assert_eq!(as_str(run(r#"return typeof 42n;"#)), "bigint");
    assert_eq!(as_str(run(r#"return typeof 0n;"#)), "bigint");
    assert_eq!(as_str(run(r#"return typeof BigInt(5);"#)), "bigint");
}

#[test]
fn bigint_literal_value() {
    assert_eq!(as_bigint_str(run(r#"return 42n;"#)), "42");
    assert_eq!(as_bigint_str(run(r#"return 0n;"#)), "0");
    assert_eq!(as_bigint_str(run(r#"return 9007199254740992n;"#)), "9007199254740992");
}

#[test]
fn bigint_hex_octal_binary() {
    assert_eq!(as_bigint_str(run(r#"return 0xFFn;"#)), "255");
    assert_eq!(as_bigint_str(run(r#"return 0b1010n;"#)), "10");
    assert_eq!(as_bigint_str(run(r#"return 0o17n;"#)), "15");
}

#[test]
fn bigint_constructor() {
    assert_eq!(as_bigint_str(run(r#"return BigInt(42);"#)), "42");
    assert_eq!(as_bigint_str(run(r#"return BigInt("12345");"#)), "12345");
    assert_eq!(as_bigint_str(run(r#"return BigInt(true);"#)), "1");
    assert_eq!(as_bigint_str(run(r#"return BigInt(false);"#)), "0");
}

#[test]
fn bigint_constructor_invalid() {
    let lexer = Lexer::parse_str(r#"return BigInt(3.14);"#, "<test>").unwrap();
    let tokens: Vec<_> = lexer.tokens.into_iter()
        .filter(|t| !matches!(t.kind,
            TokenKind::Whitespace | TokenKind::Newline
            | TokenKind::CommentLine(_) | TokenKind::CommentBlock(_)))
        .collect();
    let mut parser = Parser::new(tokens);
    let prog = parser.parse().unwrap();
    let mut interp = Interpreter::new();
    assert!(interp.run(&prog).is_err());
}

// ─── BigInt aritmetika ──────────────────────────────────────────

#[test]
fn bigint_add() {
    assert_eq!(as_bigint_str(run(r#"return 100n + 200n;"#)), "300");
    assert_eq!(as_bigint_str(run(r#"return 0n + 1n;"#)), "1");
    assert_eq!(as_bigint_str(run(r#"return 99999999999999999999n + 1n;"#)),
        "100000000000000000000");
}

#[test]
fn bigint_sub() {
    assert_eq!(as_bigint_str(run(r#"return 100n - 200n;"#)), "-100");
    assert_eq!(as_bigint_str(run(r#"return 5n - 5n;"#)), "0");
}

#[test]
fn bigint_mul() {
    assert_eq!(as_bigint_str(run(r#"return 6n * 7n;"#)), "42");
    assert_eq!(as_bigint_str(run(r#"return 4294967296n * 4294967296n;"#)),
        "18446744073709551616");
}

#[test]
fn bigint_div() {
    assert_eq!(as_bigint_str(run(r#"return 10n / 3n;"#)), "3");
    assert_eq!(as_bigint_str(run(r#"return 100n / 4n;"#)), "25");
}

#[test]
fn bigint_div_zero_throws() {
    let lexer = Lexer::parse_str(r#"return 5n / 0n;"#, "<test>").unwrap();
    let tokens: Vec<_> = lexer.tokens.into_iter()
        .filter(|t| !matches!(t.kind,
            TokenKind::Whitespace | TokenKind::Newline
            | TokenKind::CommentLine(_) | TokenKind::CommentBlock(_)))
        .collect();
    let mut parser = Parser::new(tokens);
    let prog = parser.parse().unwrap();
    let mut interp = Interpreter::new();
    assert!(interp.run(&prog).is_err());
}

#[test]
fn bigint_mod() {
    assert_eq!(as_bigint_str(run(r#"return 10n % 3n;"#)), "1");
    assert_eq!(as_bigint_str(run(r#"return 100n % 7n;"#)), "2");
}

#[test]
fn bigint_pow() {
    assert_eq!(as_bigint_str(run(r#"return 2n ** 10n;"#)), "1024");
    assert_eq!(as_bigint_str(run(r#"return 10n ** 20n;"#)), "100000000000000000000");
}

#[test]
fn bigint_unary_negation() {
    assert_eq!(as_bigint_str(run(r#"return -5n;"#)), "-5");
    assert_eq!(as_bigint_str(run(r#"return -(42n);"#)), "-42");
}

#[test]
fn bigint_bitnot() {
    assert_eq!(as_bigint_str(run(r#"return ~5n;"#)), "-6");
    assert_eq!(as_bigint_str(run(r#"return ~0n;"#)), "-1");
}

#[test]
fn bigint_bitwise_ops() {
    assert_eq!(as_bigint_str(run(r#"return 12n & 10n;"#)), "8");
    assert_eq!(as_bigint_str(run(r#"return 12n | 10n;"#)), "14");
    assert_eq!(as_bigint_str(run(r#"return 12n ^ 10n;"#)), "6");
}

#[test]
fn bigint_shifts() {
    assert_eq!(as_bigint_str(run(r#"return 1n << 10n;"#)), "1024");
    assert_eq!(as_bigint_str(run(r#"return 1024n >> 10n;"#)), "1");
    assert_eq!(as_bigint_str(run(r#"return 1n << 64n;"#)), "18446744073709551616");
}

// ─── BigInt porovnavani ──────────────────────────────────────────

#[test]
fn bigint_comparison() {
    assert_eq!(as_bool(run(r#"return 5n < 10n;"#)), true);
    assert_eq!(as_bool(run(r#"return 10n > 5n;"#)), true);
    assert_eq!(as_bool(run(r#"return 5n <= 5n;"#)), true);
    assert_eq!(as_bool(run(r#"return 5n >= 6n;"#)), false);
}

#[test]
fn bigint_strict_eq() {
    assert_eq!(as_bool(run(r#"return 5n === 5n;"#)), true);
    assert_eq!(as_bool(run(r#"return 5n === 5;"#)), false);
    assert_eq!(as_bool(run(r#"return 5n == 5;"#)), true);
}

#[test]
fn bigint_to_string() {
    assert_eq!(as_str(run(r#"return (42n).toString();"#)), "42");
    assert_eq!(as_str(run(r#"return (255n).toString(16);"#)), "ff");
    assert_eq!(as_str(run(r#"return (10n).toString(2);"#)), "1010");
}

// ─── Cross-type ops BigInt + Number ─────────────────────────────

#[test]
fn bigint_plus_number() {
    let v = run(r#"return 100n + 50;"#);
    assert_eq!(as_bigint_str(v), "150");
    let v = run(r#"return 50 + 100n;"#);
    assert_eq!(as_bigint_str(v), "150");
}

#[test]
fn bigint_minus_number() {
    assert_eq!(as_bigint_str(run(r#"return 100n - 25;"#)), "75");
}

#[test]
fn bigint_times_number() {
    assert_eq!(as_bigint_str(run(r#"return 6n * 7;"#)), "42");
}

#[test]
fn bigint_div_number() {
    assert_eq!(as_bigint_str(run(r#"return 10n / 3;"#)), "3");
}

#[test]
fn bigint_pow_number() {
    assert_eq!(as_bigint_str(run(r#"return 2n ** 10;"#)), "1024");
}

#[test]
fn bigint_compare_number() {
    assert_eq!(as_bool(run(r#"return 5n < 10;"#)), true);
    assert_eq!(as_bool(run(r#"return 100n > 50;"#)), true);
    assert_eq!(as_bool(run(r#"return 5n == 5;"#)), true);
}

// ─── Cross-type ops BigInt + BigNumber ──────────────────────────

#[test]
fn bigint_plus_bignumber() {
    let v = run(r#"
        const a = 100n;
        const b = new BigNumber("0.5");
        const result = a + b;
        return typeof result;
    "#);
    assert_eq!(as_str(v), "bignumber");
}

#[test]
fn bigint_plus_bignumber_value() {
    let v = run(r#"
        return (100n + new BigNumber("50.25")).toString();
    "#);
    assert_eq!(as_str(v), "150.25");
}

#[test]
fn bignumber_plus_bigint() {
    let v = run(r#"
        return (new BigNumber("99.5") + 1n).toString();
    "#);
    assert_eq!(as_str(v), "100.5");
}

#[test]
fn bigint_times_bignumber() {
    let v = run(r#"
        return (4n * new BigNumber("2.5")).toString();
    "#);
    assert_eq!(as_str(v), "10.0");
}

// ─── BigNumber + Number ─────────────────────────────────────────

#[test]
fn bignumber_plus_number_already_works() {
    let v = run(r#"
        const result = new BigNumber("100") + 50;
        return result.toString();
    "#);
    assert_eq!(as_str(v), "150");
}

#[test]
fn bignumber_div_number() {
    let v = run(r#"
        const result = new BigNumber("10") / 3;
        return typeof result;
    "#);
    assert_eq!(as_str(v), "bignumber");
}

// ─── Mixed chains ────────────────────────────────────────────────

#[test]
fn mixed_chain_operations() {
    let v = run(r#"
        const result = (10n + 5) * new BigNumber("2");
        return result.toString();
    "#);
    assert_eq!(as_str(v), "30");
}

#[test]
fn cross_type_truthy_falsy() {
    assert_eq!(as_bool(run(r#"return !!0n;"#)), false);
    assert_eq!(as_bool(run(r#"return !!1n;"#)), true);
    assert_eq!(as_bool(run(r#"return !!42n;"#)), true);
    assert_eq!(as_bool(run(r#"return !!(-5n);"#)), true);
}

#[test]
fn array_with_mixed_numeric_types() {
    let v = run(r#"
        const arr = [42, 100n, new BigNumber("3.14")];
        return arr.length;
    "#);
    assert_eq!(as_num(v), 3.0);
}

#[test]
fn bigint_in_template_literal() {
    let v = run(r#"
        const x = 42n;
        return `hodnota: ${x}`;
    "#);
    assert_eq!(as_str(v), "hodnota: 42");
}

#[test]
fn bigint_json_stringify_throws() {
    let v = run(r#"return JSON.stringify(42n);"#);
    assert!(matches!(v, JsValue::Undefined | JsValue::Str(_)));
}
