use crate::lexer::base::Lexer;
use crate::tokens::{Token, TokenKind};
use crate::utils::string_utils::AdvancedStringMethods;
use crate::utils::utf8_cursor::Utf8Cursor;

impl Lexer {
    pub fn read_template_string(&mut self, reader: &mut Utf8Cursor) -> Vec<Token> {
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
            tokens.extend(self.read_expression(reader, &mut string_buffer, &mut string_byte_start));
        }

        tokens
    }

    pub fn read_string(&mut self, reader: &mut Utf8Cursor, start_ch: char) -> Vec<Token> {
        let mut buffer = String::with_capacity(1000);
        let string_buffer_start_line = self.current_line;
        let mut string_buffer_position_minus_one = reader.pos();
        let string_buffer_start_column = self.current_column;
        let mut string_byte_start = reader.pos();
        let mut tokens: Vec<Token> = Vec::new();

        buffer.push(start_ch);

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
                //continue;
            }
            else if ch == '$' && reader.peek().unwrap() == '{' {
                let buffer_clone = buffer.clone();
                buffer.push(start_ch);
                tokens.push(Token {
                    kind: TokenKind::StringLiteral(buffer_clone.substring_start(1)),
                    lexeme: buffer.clone(),
                    line: string_buffer_start_line,
                    column: string_buffer_start_column,
                    start: string_byte_start,
                    end: string_buffer_position_minus_one,
                });
                buffer.clear();
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

                tokens.extend(self.read_template_string(reader));
                continue;
            }
            else if ch == start_ch {
                if !is_escaped {
                    break;
                }
            }

            buffer.push(ch);
            string_buffer_position_minus_one = reader.pos();
        }
        buffer.push(start_ch);

        tokens.push(Token {
            kind: TokenKind::StringLiteral(buffer.clone()),
            lexeme: buffer,
            line: string_buffer_start_line,
            column: string_buffer_start_column,
            start: string_byte_start,
            end: reader.pos(),
        });

        tokens
    }
}