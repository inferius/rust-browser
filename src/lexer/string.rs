use std::num::ParseIntError;
use crate::lexer::base::Lexer;
use crate::specifications::lexer_errors::{LexerError, LexerErrorKind, Span};
use crate::specifications::number_literal::NumberLiteral;
use crate::tokens::{EscapeInfo, EscapeKind, Token, TokenKind};
use crate::utils::string_utils::AdvancedStringMethods;
use crate::utils::utf8_cursor::Utf8Cursor;

#[derive(Debug, PartialEq)]
pub enum EscapeSequenceKind {
    Hex,
    Unicode,
    All,
    Octal,
    Simple,
}

pub struct EscapeSequenceResult {
    pub kind: EscapeKind,
    pub character: char,
    pub start: usize,
    pub end: usize,
    pub raw: String,
}

impl Lexer {
    pub fn read_template_string(&mut self, reader: &mut Utf8Cursor) -> Result<Vec<Token>, LexerError> {
        let mut tokens: Vec<Token> = Vec::new();
        let mut string_buffer = String::with_capacity(1000);
        let mut brace_count = 0;
        let mut string_byte_start = reader.pos();

        while !reader.eof() {
            let ch = reader.peek().unwrap();
            if ch == '}' && brace_count == 0 {
                tokens.push(Token {
                    kind: TokenKind::RightBrace,
                    lexeme: String::from("}"),
                    line: self.current_line,
                    column:  self.current_column,
                    start: string_byte_start,
                    end: reader.pos(),
                });
                break;
            }
            else if ch == '{' {
                brace_count += 1;
            }
            tokens.extend(self.read_expression(reader, &mut string_buffer, &mut string_byte_start)?);
        }

        Ok(tokens)
    }

    pub fn is_escape_sequence(ch: char, reader: &mut Utf8Cursor) -> bool {
        let peek = reader.peek();
        if peek.is_none() { return false; }
        if ch == '\\' && (peek.unwrap() == 'u' || peek.unwrap() == 'x') {
            return true;
        }
        false

    }

    pub fn read_escape_sequence(reader: &mut Utf8Cursor, kind: EscapeSequenceKind) -> Result<EscapeSequenceResult, LexerError> {
        let mut buffer = String::with_capacity(6);
        let mut raw_buffer = String::with_capacity(10);
        let mut is_escaped = false;
        let mut is_hex = false;
        let mut is_unicode = false;
        let mut is_octal = false;
        let mut is_simple = false;
        let start = reader.pos();
        let mut count: i8 = 0;
        let mut is_scalar_value = false;
        let error_kind = LexerErrorKind::InvalidEscapeSequence;
        let mut output_kind: EscapeKind = EscapeKind::Simple;

        // TODO: Zvazit jestli tu neposouvat column a line
        while !reader.eof() {
            if !is_scalar_value {
                if count == 2 && is_hex {
                    break;
                }
                if count == 4 && is_unicode {
                    break;
                }
                if count == 3 && is_octal {
                    break;
                }
            }
            else {
                if count >= 7 {
                    return Err(LexerError {
                        kind: error_kind,
                        span: Span { start, end: reader.pos() },
                    });
                }
            }
            let ch = reader.advance().unwrap();

            if ch == '\\' && !is_escaped {
                is_escaped = true;
                raw_buffer.push(ch);
                continue;
            }
            if !is_hex && !is_unicode && !is_octal && is_escaped {
                if (kind == EscapeSequenceKind::Hex || kind == EscapeSequenceKind::All) && ch == 'x' {
                    is_hex = true;
                }
                else if (kind == EscapeSequenceKind::Unicode || kind == EscapeSequenceKind::All) && ch == 'u' {
                    is_unicode = true;
                }
                else if (kind == EscapeSequenceKind::Octal || kind == EscapeSequenceKind::All) && NumberLiteral::is_octal_digit(ch) {
                    is_octal = true;
                    buffer.push(ch);
                    count += 1;
                }
                else if (kind == EscapeSequenceKind::Simple || kind == EscapeSequenceKind::All) && Token::is_single_escape_char(ch) {
                    buffer.push(ch);
                    raw_buffer.push(ch);
                    is_simple = true;
                    break;
                }
                else {
                    return Err(LexerError {
                        kind: error_kind,
                        span: Span { start, end: reader.pos() },
                    });
                }

                raw_buffer.push(ch);
                continue;
            }

            if ch == '{' {
                if !is_unicode {
                    return Err(LexerError {
                        kind: error_kind,
                        span: Span { start, end: reader.pos() },
                    });
                }
                is_scalar_value = true;
                raw_buffer.push(ch);
                continue;
            }
            if ch == '}' {
                if !is_scalar_value {
                    return Err(LexerError {
                        kind: error_kind,
                        span: Span { start, end: reader.pos() },
                    });
                }
                raw_buffer.push(ch);
                break;
            }

            if is_octal && NumberLiteral::is_octal_digit(ch) {
                buffer.push(ch);
                raw_buffer.push(ch);
                count += 1;
            }
            else if !is_octal && NumberLiteral::is_hex_digit(ch) {
                buffer.push(ch);
                raw_buffer.push(ch);
                count += 1;
            }
            else {
                return Err(LexerError {
                    kind: error_kind,
                    span: Span { start, end: reader.pos() },
                });
            }
        }

        let mut ch_conv: Option<char> = None;
        if !is_simple {
            let mut ch_r1: Result<u32, ParseIntError>;
            if is_octal {
                ch_r1 = u32::from_str_radix(&buffer, 8);
            }
            else {
                ch_r1 = u32::from_str_radix(&buffer, 16);
            }

            if ch_r1.is_err() {
                return Err(LexerError {
                    kind: error_kind,
                    span: Span { start, end: reader.pos() },
                });
            }
            ch_conv = char::from_u32(ch_r1.unwrap());
            if ch_conv.is_none() {
                return Err(LexerError {
                    kind: error_kind,
                    span: Span { start, end: reader.pos() },
                });
            }
        }
        else {
            ch_conv = buffer.chars().next();
        }

        if is_unicode {output_kind = EscapeKind::Unicode; }
        else if is_hex {output_kind = EscapeKind::Hex; }
        else if is_octal {output_kind = EscapeKind::Octal; }
        else if is_simple {output_kind = EscapeKind::Simple; }

        Ok(EscapeSequenceResult {
            kind: output_kind,
            character: ch_conv.unwrap(),
            start,
            end: reader.pos(),
            raw: raw_buffer,
        })
    }

    fn escaped_result2escaped_info(escaped_result: EscapeSequenceResult) -> EscapeInfo {
        EscapeInfo {
            kind: escaped_result.kind,
            raw: escaped_result.raw,
            position_in_raw: escaped_result.start,
            resolved_char: escaped_result.character,
        }
    }

    pub fn read_string(&mut self, reader: &mut Utf8Cursor, start_ch: char) -> Result<Vec<Token>, LexerError> {
        let mut buffer = String::with_capacity(1000);
        let mut buffer_raw = String::with_capacity(1000);
        let string_buffer_start_line = self.current_line;
        let mut string_buffer_position_minus_one = reader.pos();
        let string_buffer_start_column = self.current_column;
        let mut string_byte_start = reader.pos();
        let mut tokens: Vec<Token> = Vec::new();
        let mut escaped: Vec<EscapeInfo> = Vec::new();


        buffer.push(start_ch);
        buffer_raw.push(start_ch);

        let mut is_escaped = false;
        while !reader.eof() {
            let ch = reader.advance().unwrap();
            self.update_current_position(ch);
            if ch == start_ch && !is_escaped {
                break;
            }
            is_escaped = false;

            if ch == '\\' {
                is_escaped = true;
                reader.undo();
                let esc = Lexer::read_escape_sequence(reader, EscapeSequenceKind::All)?;
                buffer.push(esc.character);
                buffer_raw.extend(esc.raw.chars());

                if esc.character == '\n' || esc.character == '\r' {
                    self.update_current_position(esc.character);
                }

                escaped.push(Lexer::escaped_result2escaped_info(esc));
                continue;
            }
            else if ch == '$' && reader.peek().unwrap() == '{' {
                let buffer_clone = buffer.clone();
                let buffer_raw_clone = buffer_raw.clone();
                buffer.push(start_ch);
                buffer_raw.push(start_ch);
                tokens.push(Token {
                    kind: TokenKind::StringLiteral {
                        raw: buffer_raw_clone.substring_start(1),
                        value: buffer_clone.substring_start(1),
                        escapes: escaped.clone(),
                    },//(buffer_clone.substring_start(1)),
                    lexeme: buffer.clone(),
                    line: string_buffer_start_line,
                    column: string_buffer_start_column,
                    start: string_byte_start,
                    end: string_buffer_position_minus_one,
                });
                escaped.clear();
                buffer.clear();
                buffer_raw.clear();
                string_byte_start = reader.pos();
                reader.advance().unwrap();
                reader.advance().unwrap();

                tokens.push(Token {
                    kind: TokenKind::DollarCurlyOpen,
                    lexeme: String::from("${"),
                    line: string_buffer_start_line,
                    column: string_buffer_start_column,
                    start: string_byte_start,
                    end: string_buffer_position_minus_one,
                });

                tokens.extend(self.read_template_string(reader)?);
                continue;
            }
            else if ch == start_ch {
                if !is_escaped {
                    break;
                }
            }

            buffer.push(ch);
            buffer_raw.push(ch);
            string_buffer_position_minus_one = reader.pos();
        }
        buffer.push(start_ch);
        buffer_raw.push(start_ch);

        tokens.push(Token {
            kind: TokenKind::StringLiteral {
                raw: buffer_raw,
                value: buffer.clone(),
                escapes: escaped.clone(),
            },
            lexeme: buffer,
            line: string_buffer_start_line,
            column: string_buffer_start_column,
            start: string_byte_start,
            end: reader.pos(),
        });

        Ok(tokens)
    }
}

#[cfg(test)]
#[path = "tests/string.rs"]
mod string;