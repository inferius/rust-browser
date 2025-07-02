use crate::lexer::base::Lexer;
use crate::specifications::lexer_errors::{LexerError, LexerErrorKind, Span};
use crate::specifications::number_literal::NumberLiteral;
use crate::tokens::{NumericBase, Token, TokenKind};
use crate::utils::utf8_cursor::Utf8Cursor;

struct NumberResult {
    buffer: String,
    no_raw_value: String,
    is_bigint: bool,
}

impl Lexer {

    pub fn read_numeric_literal(&mut self, reader: &mut Utf8Cursor) -> Result<Token, LexerError> {
        let mut buffer = String::with_capacity(1000);
        let mut string_byte_start = reader.pos();
        let mut number_base = NumericBase::Decimal;
        let mut is_legacy_octal = false;
        let mut value: f64 = f64::NAN;
        let mut is_bigint = false;
        let mut is_valid_number = false;
        let mut has_punctuation = false;
        let mut no_raw_value = String::with_capacity(1000);


        while !reader.eof() {
            let ch = reader.advance().unwrap();
            let ch_next = reader.peek().unwrap();
            self.update_current_position(ch);

            // Pokud zacina nulou, musime zkontrolovat co za typ cisla to je
            if ch == '0' && buffer.len() == 0 {
                if NumberLiteral::is_non_octal_digit(ch_next) {
                    buffer.push(ch);
                    continue;
                }
                else if NumberLiteral::is_octal_digit(ch_next) {
                    buffer.push(ch);
                    number_base = NumericBase::Octal;
                    is_legacy_octal = true;
                    continue;
                }
                else if NumberLiteral::is_hex_digit(ch_next) {
                    buffer.push(ch);
                    buffer.push(reader.advance().unwrap());
                    number_base = NumericBase::Hex;
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
                buffer.push(ch);
                continue;
            }

            no_raw_value.push(ch);
            buffer.push(ch);
        }

        if !is_bigint {
            value = no_raw_value.parse::<f64>().unwrap();
        }

        Ok(Token {
            kind: TokenKind::NumericLiteral {
                base: number_base,
                legacy_octal: is_legacy_octal,
                raw: buffer.clone(),
                is_bigint,
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
            let ch_next = reader.peek().unwrap();
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
                    if !NumberLiteral::is_binary_digit(ch_next) {
                        return Err(LexerError {
                            kind: LexerErrorKind::InvalidDigit {
                                base: NumericBase::Binary,
                                found: ch_next,
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

        }

        Ok(NumberResult {buffer, no_raw_value, is_bigint})
    }

    fn read_decimal_literal(&mut self, reader: &mut Utf8Cursor, buffer_i: String, no_raw_value_i: String, string_byte_start: usize) -> Result<NumberResult, LexerError> {
        let mut no_raw_value = no_raw_value_i.clone();
        let mut buffer = buffer_i.clone();
        let mut is_bigint = false;


        while !reader.eof() {
            let ch = reader.advance().unwrap();
            let ch_next = reader.peek().unwrap();
            self.update_current_position(ch);

            

        }

        Ok(NumberResult {buffer, no_raw_value, is_bigint})
    }
    
}