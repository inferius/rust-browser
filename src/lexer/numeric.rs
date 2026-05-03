use num_bigint::BigInt;
use crate::lexer::base::Lexer;
use crate::specifications::lexer_errors::{LexerError, LexerErrorKind, Span};
use crate::specifications::number_literal::NumberLiteral;
use crate::tokens::{NumericBase, Token, TokenKind};
use crate::utils::utf8_cursor::Utf8Cursor;

// Interní výsledek parsování číslice
struct NumResult {
    raw: String,    // přesný text ze zdrojáku (vč. oddělovačů _)
    digits: String, // jen číslice (pro parse na hodnotu)
    is_bigint: bool,
    has_exponent: bool,
}

impl Lexer {
    pub fn read_numeric_literal(&mut self, r: &mut Utf8Cursor) -> Result<Token, LexerError> {
        let start = r.pos();
        let first = r.peek().unwrap();
        let second = r.peek_n(1);

        let (result, base, legacy_octal) = match (first, second) {
            ('0', Some(s)) if NumberLiteral::is_hex_start(s) => {
                r.advance(); r.advance(); self.bump('0'); self.bump(s);
                (self.read_based(r, start, NumericBase::Hex, NumberLiteral::is_hex_digit)?, NumericBase::Hex, false)
            }
            ('0', Some(s)) if NumberLiteral::is_binary_start(s) => {
                r.advance(); r.advance(); self.bump('0'); self.bump(s);
                (self.read_based(r, start, NumericBase::Binary, NumberLiteral::is_binary_digit)?, NumericBase::Binary, false)
            }
            ('0', Some(s)) if NumberLiteral::is_octal_start(s) => {
                r.advance(); r.advance(); self.bump('0'); self.bump(s);
                (self.read_based(r, start, NumericBase::Octal, NumberLiteral::is_octal_digit)?, NumericBase::Octal, false)
            }
            ('0', Some(s)) if NumberLiteral::is_octal_digit(s) => {
                // Legacy octal: 0777
                (self.read_decimal(r, start)?, NumericBase::Octal, true)
            }
            _ => (self.read_decimal(r, start)?, NumericBase::Decimal, false),
        };

        // Výpočet hodnoty
        let (value, bigint_value) = if result.is_bigint {
            let bv = match base {
                NumericBase::Decimal => BigInt::parse_bytes(result.digits.as_bytes(), 10),
                NumericBase::Hex     => BigInt::parse_bytes(result.digits.as_bytes(), 16),
                NumericBase::Binary  => BigInt::parse_bytes(result.digits.as_bytes(), 2),
                NumericBase::Octal   => BigInt::parse_bytes(result.digits.as_bytes(), 8),
            };
            (f64::NAN, bv)
        } else {
            let v: f64 = match base {
                NumericBase::Decimal => result.digits.parse().unwrap_or(f64::NAN),
                NumericBase::Hex     => i64::from_str_radix(&result.digits, 16).map(|n| n as f64).unwrap_or(f64::NAN),
                NumericBase::Binary  => i64::from_str_radix(&result.digits, 2).map(|n| n as f64).unwrap_or(f64::NAN),
                NumericBase::Octal   => i64::from_str_radix(&result.digits, 8).map(|n| n as f64).unwrap_or(f64::NAN),
            };
            (v, None)
        };

        let raw = result.raw.clone();
        Ok(self.tok(TokenKind::NumericLiteral {
            base, legacy_octal, raw: raw.clone(),
            bigint_value, is_bigint: result.is_bigint,
            has_exponent: result.has_exponent,
            is_valid: true,  // BUG FIX: v originálu nikdy nebylo nastaveno na true
            value,
        }, raw, start))
    }

    /// Čte číslice v dané soustavě (hex/octal/binary).
    fn read_based<F: Fn(char) -> bool>(
        &mut self, r: &mut Utf8Cursor, start: usize,
        base: NumericBase, is_digit: F,
    ) -> Result<NumResult, LexerError> {
        let mut raw = String::new();
        let mut digits = String::new();
        let mut is_bigint = false;

        while let Some(ch) = r.peek() {
            if NumberLiteral::is_separator(ch) {
                r.advance(); self.bump(ch); raw.push(ch);
                // Po oddělovači musí být platná číslice
                if r.peek().map(|c| !is_digit(c)).unwrap_or(true) {
                    return Err(LexerError { kind: LexerErrorKind::InvalidDigit { base, found: r.peek().unwrap_or('?') }, span: Span { start, end: r.pos() } });
                }
            } else if NumberLiteral::is_bigint_suffix(ch) {
                r.advance(); self.bump(ch); raw.push(ch); is_bigint = true; break;
            } else if is_digit(ch) {
                r.advance(); self.bump(ch); raw.push(ch); digits.push(ch);
            } else if ch.is_alphanumeric() {
                return Err(LexerError { kind: LexerErrorKind::InvalidDigit { base, found: ch }, span: Span { start, end: r.pos() } });
            } else {
                break;
            }
        }

        Ok(NumResult { raw, digits, is_bigint, has_exponent: false })
    }

    /// Čte decimální číslo (int, float, vědecká notace).
    fn read_decimal(&mut self, r: &mut Utf8Cursor, start: usize) -> Result<NumResult, LexerError> {
        let mut raw = String::new();
        let mut digits = String::new();
        let mut has_dot = false;
        let mut has_exp = false;
        let mut is_bigint = false;

        while let Some(ch) = r.peek() {
            if ch.is_ascii_digit() {
                r.advance(); self.bump(ch); raw.push(ch); digits.push(ch);
            } else if ch == '.' && !has_dot && !has_exp {
                r.advance(); self.bump(ch); has_dot = true; raw.push(ch); digits.push(ch);
            } else if NumberLiteral::is_exponent(ch) && !has_exp {
                r.advance(); self.bump(ch); has_exp = true; raw.push(ch); digits.push(ch);
                // Volitelné znaménko exponentu
                if let Some(s) = r.peek() {
                    if NumberLiteral::is_exponent_sign(s) { r.advance(); self.bump(s); raw.push(s); digits.push(s); }
                }
            } else if NumberLiteral::is_separator(ch) {
                r.advance(); self.bump(ch); raw.push(ch); // _ nepatří do digits
            } else if NumberLiteral::is_bigint_suffix(ch) && !has_dot && !has_exp {
                r.advance(); self.bump(ch); raw.push(ch); is_bigint = true; break;
            } else {
                break;
            }
        }

        if digits.is_empty() {
            return Err(LexerError { kind: LexerErrorKind::UnexpectedNumber, span: Span { start, end: r.pos() } });
        }

        Ok(NumResult { raw, digits, is_bigint, has_exponent: has_exp })
    }
}
