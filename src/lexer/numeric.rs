use std::i64;
use num_bigint::BigInt;
use crate::lexer::base::Lexer;
use crate::specifications::lexer_errors::{LexerError, LexerErrorKind, Span};
use crate::specifications::number_literal::NumberLiteral;
use crate::tokens::{NumericBase, Token, TokenKind};
use crate::utils::utf8_cursor::Utf8Cursor;

struct NumberResult {
    buffer: String,
    no_raw_value: String,
    is_bigint: bool,
    has_exponent: bool,
}

impl Lexer {

    pub fn read_numeric_literal(&mut self, reader: &mut Utf8Cursor) -> Result<Token, LexerError> {
        let mut buffer = String::with_capacity(1000);
        let mut string_byte_start = reader.pos();
        let mut number_base = NumericBase::Decimal;
        let mut is_legacy_octal = false;
        let mut value: f64 = f64::NAN;
        let mut is_bigint = false;
        let mut has_exponent = false;
        let mut is_valid_number = false;
        let mut has_punctuation = false;
        let mut no_raw_value = String::with_capacity(1000);
        let mut bigint_value: Option<BigInt> = None;


        while !reader.eof() {
            let ch = reader.advance().unwrap();
            let ch_next = reader.peek();
            self.update_current_position(ch);

            // Pokud zacina nulou, musime zkontrolovat co za typ cisla to je
            if ch == '0' && buffer.len() == 0 && ch_next.is_some() {
                let ch_next = ch_next.unwrap();
                if NumberLiteral::is_non_octal_digit(ch_next) {
                    buffer.push(ch);
                    continue;
                }
                else if NumberLiteral::is_octal_digit(ch_next) {
                    buffer.push(ch);
                    number_base = NumericBase::Octal;
                    is_legacy_octal = true;

                    let result = self.read_octal_literal(reader, buffer.clone(), no_raw_value.clone(), string_byte_start)?;

                    is_bigint = result.is_bigint;
                    buffer = result.buffer;
                    no_raw_value = result.no_raw_value;
                    continue;
                }
                else if NumberLiteral::is_hex_start(ch_next) {
                    buffer.push(ch);
                    buffer.push(reader.advance().unwrap());
                    number_base = NumericBase::Hex;

                    let result = self.read_hex_literal(reader, buffer.clone(), no_raw_value.clone(), string_byte_start)?;

                    is_bigint = result.is_bigint;
                    buffer = result.buffer;
                    no_raw_value = result.no_raw_value;
                    
                    continue;
                }
                else if NumberLiteral::is_binary_start(ch_next) {
                    buffer.push(ch);
                    buffer.push(reader.advance().unwrap());
                    number_base = NumericBase::Binary;
                    let result = self.read_binary_literal(reader, buffer.clone(), no_raw_value.clone(), string_byte_start)?;

                    is_bigint = result.is_bigint;
                    buffer = result.buffer;
                    no_raw_value = result.no_raw_value;

                    continue;
                }
                else if NumberLiteral::is_octal_start(ch_next) {
                    buffer.push(ch);
                    buffer.push(reader.advance().unwrap());
                    number_base = NumericBase::Octal;

                    let result = self.read_octal_literal(reader, buffer.clone(), no_raw_value.clone(), string_byte_start)?;

                    is_bigint = result.is_bigint;
                    buffer = result.buffer;
                    no_raw_value = result.no_raw_value;
                    continue;
                }
            }

            else if NumberLiteral::is_numeric_literal_separator(ch) {
                buffer.push(ch);
                continue;
            }

            else if NumberLiteral::is_decimal_separator(ch) {
                if number_base != NumericBase::Decimal {
                    reader.undo();
                    break;
                }
                has_punctuation = true;
                if no_raw_value.len() == 0 {
                    no_raw_value.push('0');
                }
                buffer.push(ch);
                no_raw_value.push(ch);

                let result = self.read_decimal_literal(reader, buffer.clone(), no_raw_value.clone(), string_byte_start, has_punctuation)?;
                is_bigint = result.is_bigint;
                buffer = result.buffer;
                no_raw_value = result.no_raw_value;
                has_exponent = result.has_exponent;
                continue;
            }
            else if NumberLiteral::is_bigint_suffix(ch) {
                buffer.push(ch);
                if has_punctuation {
                    return Err(LexerError {
                        kind: LexerErrorKind::InvalidBigInt {
                            reason: String::from("invalid character")
                        },
                        span: Span { start: string_byte_start, end: reader.pos() },
                    });
                }
            }

            else if NumberLiteral::is_decimal_digit(ch) {
                reader.undo();
                let result = self.read_decimal_literal(reader, buffer.clone(), no_raw_value.clone(), string_byte_start, has_punctuation)?;
                is_bigint = result.is_bigint;
                buffer = result.buffer;
                no_raw_value = result.no_raw_value;
                has_exponent = result.has_exponent;
                continue;
            }

            no_raw_value.push(ch);
            buffer.push(ch);
        }

        if !is_bigint {
            match number_base {
                NumericBase::Decimal => { value = no_raw_value.parse::<f64>().unwrap(); }
                NumericBase::Hex => { value = i64::from_str_radix(&no_raw_value, 16).unwrap() as f64; }
                NumericBase::Binary => {value = i64::from_str_radix(&no_raw_value, 2).unwrap() as f64; }
                NumericBase::Octal => { value = i64::from_str_radix(&no_raw_value, 8).unwrap() as f64; }
            }
        }
        else {
            match number_base {
                NumericBase::Decimal => { bigint_value = BigInt::parse_bytes(&no_raw_value.as_bytes(), 10) }
                NumericBase::Hex => { bigint_value = BigInt::parse_bytes(&no_raw_value.as_bytes(), 16); }
                NumericBase::Binary => {bigint_value = BigInt::parse_bytes(&no_raw_value.as_bytes(), 2); }
                NumericBase::Octal => { bigint_value = BigInt::parse_bytes(&no_raw_value.as_bytes(), 8); }
            }
        }

        Ok(Token {
            kind: TokenKind::NumericLiteral {
                base: number_base,
                legacy_octal: is_legacy_octal,
                raw: buffer.clone(),
                bigint_value,
                is_bigint,
                has_exponent,
                is_valid: is_valid_number,
                value
            },
            lexeme: buffer,
            line: self.current_line,
            column:  self.current_column,
            start: string_byte_start,
            end: reader.pos(),
        })

    }

    fn read_binary_literal(&mut self, reader: &mut Utf8Cursor, buffer_i: String, no_raw_value_i: String, string_byte_start: usize) -> Result<NumberResult, LexerError> {
        let mut no_raw_value = no_raw_value_i.clone();
        let mut buffer = buffer_i.clone();
        let mut is_bigint = false;


        while !reader.eof() {
            let ch = reader.advance().unwrap();
            let ch_next = reader.peek();
            self.update_current_position(ch);

            if !NumberLiteral::is_binary_digit(ch) {
                if NumberLiteral::is_decimal_digit(ch) {
                    return Err(LexerError {
                        kind: LexerErrorKind::InvalidDigit {
                            base: NumericBase::Binary,
                            found: ch,
                        },
                        span: Span { start: string_byte_start, end: reader.pos() },
                    });
                }
                else if NumberLiteral::is_numeric_literal_separator(ch) {
                    // pokud za separatorem neni validni binary cislo vyhodit chybu
                    if ch_next.is_some() && !NumberLiteral::is_binary_digit(ch_next.unwrap()) {
                        return Err(LexerError {
                            kind: LexerErrorKind::InvalidDigit {
                                base: NumericBase::Binary,
                                found: ch_next.unwrap(),
                            },
                            span: Span { start: string_byte_start, end: reader.pos() },
                        });
                    }
                    buffer.push(ch);
                    continue;
                }
                else if NumberLiteral::is_bigint_suffix(ch) {
                    is_bigint = true;
                    break;
                }
                reader.undo();
                break;
            }
            else {
                buffer.push(ch);
                no_raw_value.push(ch);
            }

        }

        Ok(NumberResult {buffer, no_raw_value, is_bigint, has_exponent: false})
    }

    fn read_octal_literal(&mut self, reader: &mut Utf8Cursor, buffer_i: String, no_raw_value_i: String, string_byte_start: usize) -> Result<NumberResult, LexerError> {
        let mut no_raw_value = no_raw_value_i.clone();
        let mut buffer = buffer_i.clone();
        let mut is_bigint = false;


        while !reader.eof() {
            let ch = reader.advance().unwrap();
            let ch_next = reader.peek();
            self.update_current_position(ch);

            if !NumberLiteral::is_octal_digit(ch) {
                if NumberLiteral::is_non_octal_digit(ch) {
                    return Err(LexerError {
                        kind: LexerErrorKind::InvalidDigit {
                            base: NumericBase::Octal,
                            found: ch,
                        },
                        span: Span { start: string_byte_start, end: reader.pos() },
                    });
                }
                else if NumberLiteral::is_numeric_literal_separator(ch) {
                    // pokud za separatorem neni validni binary cislo vyhodit chybu
                    if ch_next.is_some() && !NumberLiteral::is_octal_digit(ch_next.unwrap()) {
                        return Err(LexerError {
                            kind: LexerErrorKind::InvalidDigit {
                                base: NumericBase::Octal,
                                found: ch_next.unwrap(),
                            },
                            span: Span { start: string_byte_start, end: reader.pos() },
                        });
                    }
                    buffer.push(ch);
                    continue;
                }
                else if NumberLiteral::is_bigint_suffix(ch) {
                    is_bigint = true;
                    break;
                }
                reader.undo();
                break;
            }
            else {
                buffer.push(ch);
                no_raw_value.push(ch);
            }

        }

        Ok(NumberResult {buffer, no_raw_value, is_bigint, has_exponent: false})
    }

    fn read_hex_literal(&mut self, reader: &mut Utf8Cursor, buffer_i: String, no_raw_value_i: String, string_byte_start: usize) -> Result<NumberResult, LexerError> {
        let mut no_raw_value = no_raw_value_i.clone();
        let mut buffer = buffer_i.clone();
        let mut is_bigint = false;


        while !reader.eof() {
            let ch = reader.advance().unwrap();
            let ch_next = reader.peek();
            self.update_current_position(ch);

            if !NumberLiteral::is_hex_digit(ch) {
                if NumberLiteral::is_numeric_literal_separator(ch) {
                    // pokud za separatorem neni validni binary cislo vyhodit chybu
                    if ch_next.is_some() && !NumberLiteral::is_hex_digit(ch_next.unwrap()) {
                        return Err(LexerError {
                            kind: LexerErrorKind::InvalidDigit {
                                base: NumericBase::Hex,
                                found: ch_next.unwrap(),
                            },
                            span: Span { start: string_byte_start, end: reader.pos() },
                        });
                    }
                    buffer.push(ch);
                    continue;
                }
                else if NumberLiteral::is_bigint_suffix(ch) {
                    is_bigint = true;
                    break;
                }
                reader.undo();
                break;
            }
            else {
                buffer.push(ch);
                no_raw_value.push(ch);
            }

        }

        Ok(NumberResult {buffer, no_raw_value, is_bigint, has_exponent: false})
    }

    fn read_decimal_literal(&mut self, reader: &mut Utf8Cursor, buffer_i: String, no_raw_value_i: String, string_byte_start: usize, deny_decimal_separator: bool) -> Result<NumberResult, LexerError> {
        let mut no_raw_value = no_raw_value_i.clone();
        let mut buffer = buffer_i.clone();
        let mut is_bigint = false;
        let mut has_punctuation = false;
        let mut has_exponent = false;


        while !reader.eof() {
            let ch = reader.advance().unwrap();
            //let ch_next = reader.peek();
            self.update_current_position(ch);

            if has_exponent {
                if NumberLiteral::is_exponent_sign(ch) {
                    buffer.push(ch);
                    no_raw_value.push(ch);
                    continue;
                }
            }

            if NumberLiteral::is_decimal_separator(ch) {
                if deny_decimal_separator || has_punctuation || has_exponent {
                    return Err(LexerError {
                        kind: LexerErrorKind::UnexpectedNumber,
                        span: Span { start: string_byte_start, end: reader.pos() },
                    })
                }
                has_punctuation = true;
                buffer.push(ch);
                no_raw_value.push(ch);
                continue;
            }
            else if NumberLiteral::is_decimal_digit(ch) {
                buffer.push(ch);
                no_raw_value.push(ch);
                continue;
            }
            else if NumberLiteral::is_bigint_suffix(ch) {
                is_bigint = true;
                break;
            }
            else if NumberLiteral::is_numeric_literal_separator(ch) {
                buffer.push(ch);
                continue;
            }
            else if NumberLiteral::is_exponent_indicator(ch) {
                has_exponent = true;
                buffer.push(ch);
                no_raw_value.push(ch);
                continue;
            }


        }

        Ok(NumberResult {buffer, no_raw_value, is_bigint, has_exponent})
    }

    fn number_result_to_string(result: NumberResult) -> String {
        format!("{}|{}|{}", result.buffer, result.no_raw_value, result.is_bigint)
    }

    fn token_to_string(result: Token) -> String {
        format!("{}", result.kind)
    }
    
}

#[cfg(test)]
#[path = "tests/numeric.rs"]
mod numeric;