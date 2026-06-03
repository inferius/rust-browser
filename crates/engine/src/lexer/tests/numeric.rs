use crate::lexer::base::Lexer;
use crate::tokens::{NumericBase, TokenKind};
use crate::utils::utf8_cursor::Utf8Cursor;

// --- pomocne funkce ---

fn num_token(src: &str) -> TokenKind {
    let mut lexer = Lexer::new();
    let mut cursor = Utf8Cursor::new(src);
    lexer.read_numeric_literal(&mut cursor).unwrap().kind
}

fn num_val(src: &str) -> f64 {
    match num_token(src) {
        TokenKind::NumericLiteral { value, .. } => value,
        other => panic!("Ocekavan NumericLiteral, nalezeno {other:?}"),
    }
}

fn num_base(src: &str) -> NumericBase {
    match num_token(src) {
        TokenKind::NumericLiteral { base, .. } => base,
        other => panic!("Ocekavan NumericLiteral, nalezeno {other:?}"),
    }
}

fn is_bigint(src: &str) -> bool {
    match num_token(src) {
        TokenKind::NumericLiteral { is_bigint, .. } => is_bigint,
        other => panic!("Ocekavan NumericLiteral, nalezeno {other:?}"),
    }
}

// --- decimal ---

#[test]
fn decimal_integer() {
    assert_eq!(num_val("0"), 0.0);
    assert_eq!(num_val("1"), 1.0);
    assert_eq!(num_val("42"), 42.0);
    assert_eq!(num_val("1000000"), 1_000_000.0);
}

#[test]
fn decimal_float() {
    assert_eq!(num_val("1.5"), 1.5);
    assert_eq!(num_val(".5"), 0.5);
    assert_eq!(num_val("0.001"), 0.001);
    assert_eq!(num_val("1."), 1.0);
}

#[test]
fn decimal_exponent() {
    assert_eq!(num_val("1e3"), 1000.0);
    assert_eq!(num_val("1.2e3"), 1200.0);
    assert_eq!(num_val("1.2e-3"), 0.0012);
    assert_eq!(num_val("1.2e+3"), 1200.0);
    assert_eq!(num_val("6.5e-2"), 0.065);
}

#[test]
fn decimal_separator() {
    assert_eq!(num_val("1_000_000"), 1_000_000.0);
    assert_eq!(num_val("1_000.5_00"), 1000.5);
}

#[test]
fn decimal_bigint() {
    assert!(is_bigint("123n"));
    assert!(!is_bigint("123"));
    // BigInt ukrada f64::NAN — skutecna hodnota je v bigint_value (BigInt typ)
    assert!(num_val("123n").is_nan());
}

// --- hex ---

#[test]
fn hex_literals() {
    assert_eq!(num_val("0x0"), 0.0);
    assert_eq!(num_val("0xFF"), 255.0);
    assert_eq!(num_val("0x123"), 291.0);
    assert_eq!(num_val("0xDEAD"), 57005.0);
    assert_eq!(num_base("0xFF"), NumericBase::Hex);
}

#[test]
fn hex_bigint() {
    assert!(is_bigint("0xFFn"));
    assert!(num_val("0xFFn").is_nan());
}

// --- binary ---

#[test]
fn binary_literals() {
    assert_eq!(num_val("0b0"), 0.0);
    assert_eq!(num_val("0b1"), 1.0);
    assert_eq!(num_val("0b101010"), 42.0);
    assert_eq!(num_val("0b11111111"), 255.0);
    assert_eq!(num_base("0b101"), NumericBase::Binary);
}

#[test]
fn binary_bigint() {
    assert!(is_bigint("0b1010n"));
    assert!(num_val("0b1010n").is_nan());
}

// --- octal ---

#[test]
fn octal_modern() {
    assert_eq!(num_val("0o0"), 0.0);
    assert_eq!(num_val("0o7"), 7.0);
    assert_eq!(num_val("0o132"), 90.0);
    assert_eq!(num_val("0o777"), 511.0);
    assert_eq!(num_base("0o132"), NumericBase::Octal);
}

#[test]
fn octal_legacy() {
    assert_eq!(num_val("0132"), 90.0);
    assert_eq!(num_val("0777"), 511.0);
}
