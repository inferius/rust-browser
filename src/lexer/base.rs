use std::fs::File;
use std::io::{BufReader, Read};
use crate::specifications::lexer_errors::{LexerError, LexerErrorKind, Span};
use crate::tokens::{CommentKind, OperatorEnum, StringKind, Token, TokenKind};
use crate::utils::utf8_cursor::Utf8Cursor;

pub struct Lexer {
    pub tokens: Vec<Token>,
    pub original_file_name: String,
    pub current_line: usize,
    pub current_column: usize,
}

struct ProcessContext<'a> {
    reader: &'a mut Utf8Cursor,
    buffer: &'a mut String,
    tokens: &'a mut Vec<Token>,
    string_byte_start: usize,
}


impl Lexer {
    /*pub fn parse_template_string(tokens: &mut Peekable<Token>, level: u16) {

    }*/
    
    pub fn new() -> Lexer {
        Lexer {
            tokens: Vec::new(),
            original_file_name: String::new(),
            current_line: 0,
            current_column: 0,
        }
    }

    /// Funkce spustí parsování souboru
    ///
    /// # Parametry
    ///
    /// | Název | Typ | Popis |
    /// |---------|--------|-----------------------|
    /// | `file_path` | `&str` | Cesta k souboru JS |
    ///
    /// # Návratové hodnoty
    ///
    /// `Lexer`
    /// Vrací lexer
    pub fn parse_file(file_path: &str) -> Result<Lexer, LexerError> {
        let mut instance = Lexer {
            tokens: Vec::new(),
            original_file_name: String::from(file_path),
            current_line: 0,
            current_column: 0,
        };

        let file = File::open(file_path);
        if file.is_err() {
            panic!("File not found");
        }
        let mut reader = BufReader::new(file.unwrap());
        let mut buffer = String::new();
        reader.read_to_string(&mut buffer).unwrap(); // Čtení celého obsahu jako textu


        // Iterace znak po znaku
        let mut reader = Utf8Cursor::new(buffer.as_str());

        instance.tokens = instance.lex_string(&mut reader, true)?;

        Ok(instance)
    }

    fn lex_string(&mut self, reader: &mut Utf8Cursor, is_file: bool) -> Result<Vec<Token>, LexerError> {
        let mut tokens: Vec<Token> = Vec::new();
        let mut string_buffer = String::with_capacity(1000);
        let mut string_buffer_start_line = 0;
        let mut string_buffer_start_column = 0;
        let mut string_byte_start = reader.pos();

        /*let mut context = ProcessContext {
            reader,
            buffer: &mut string_buffer,
            tokens: &mut tokens,
            string_byte_start,
        };*/

        while !reader.eof() {
            /*let ch = reader.advance().unwrap();
            let ch_next = reader.peek().unwrap();

            self.update_current_position(ch);

            self.read_while(reader, |&c| Token::is_white_space(c));

            let is_comment = Token::is_comment_start(ch, ch_next);
            if is_comment != CommentKind::None {
                if is_comment == CommentKind::SingleLine {
                    let comment = self.read_while(reader, |&c| Token::is_white_space(c));
                    tokens.push(Token {
                        kind: TokenKind::CommentLine(comment.clone()),
                        lexeme: comment,
                        line: string_buffer_start_line,
                        column: string_buffer_start_column,
                        start: string_byte_start,
                        end: reader.pos(),
                    });
                    string_byte_start = reader.pos();
                }
                else {
                    let comment = self.read_while(reader, |&c| Token::is_comment_end(c, ch_next) == CommentKind::MultiLine);
                    tokens.push(Token {
                        kind: TokenKind::CommentBlock(comment.clone()),
                        lexeme: comment,
                        line: string_buffer_start_line,
                        column: string_buffer_start_column,
                        start: string_byte_start,
                        end: reader.pos(),
                    });
                    string_byte_start = reader.pos();
                }

            }

            let is_string = Token::is_string_start(ch);
            if is_string != StringKind::None {
                if is_string == StringKind::TemplateString {
                    tokens.extend(self.read_string(reader, ch));
                }
                else {
                    tokens.extend(self.read_string(reader, ch));
                }
            }*/

            if is_file && reader.pos() == 0 {
                let ch = reader.peek().unwrap();
                let ch_next = reader.peek_n(1);

                if Token::is_hashbang(ch, ch_next) {
                    self.read_while_line_break(reader);
                }
            }

            tokens.extend(self.read_expression(reader, &mut string_buffer, &mut string_byte_start)?);

        }

        Ok(tokens)
    }

    pub fn read_expression(&mut self, reader: &mut Utf8Cursor, buffer: &mut String, string_byte_start_r: &mut usize) -> Result<Vec<Token>, LexerError> {
        let ch = reader.peek().unwrap();
        let ch_next = reader.peek();
        let mut tokens: Vec<Token> = Vec::new();
        let mut string_byte_start = reader.pos();

        let mut process_buffer = |reader: &mut Utf8Cursor, buffer: &mut String, current_line: usize, current_column: usize| {
            if !buffer.is_empty() {
                if let Some(token) = Lexer::process_buffer(
                    buffer,
                    string_byte_start,
                    current_line,
                    current_column,
                ) {
                    tokens.push(token); // Direct mutation
                    string_byte_start = reader.pos();
                }
            }

        };

        self.update_current_position(ch);

        self.read_while(reader, |&c, &c_next| !Token::is_white_space(c));

        let is_comment = Token::is_comment_start(ch, ch_next);
        if is_comment != CommentKind::None {
            process_buffer(reader, buffer, self.current_line, self.current_column);
            if is_comment == CommentKind::SingleLine {
                let comment = self.read_while_line_break(reader);

                tokens.push(Token {
                    kind: TokenKind::CommentLine(comment.clone()),
                    lexeme: comment,
                    line: self.current_line,
                    column:  self.current_column,
                    start: string_byte_start.clone(),
                    end: reader.pos(),
                });
            }
            else {
                let comment = self.read_while(reader, |&c, &c_next| Token::is_comment_end(c, c_next) == CommentKind::MultiLine);
                tokens.push(Token {
                    kind: TokenKind::CommentBlock(comment.clone()),
                    lexeme: comment,
                    line: self.current_line,
                    column:  self.current_column,
                    start: string_byte_start,
                    end: reader.pos(),
                });
            }
            *string_byte_start_r = reader.pos();
            return Ok(tokens);
        }

        let is_string = Token::is_string_start(ch);
        if is_string != StringKind::None {
            process_buffer(reader, buffer, self.current_line, self.current_column);
            if is_string == StringKind::TemplateString {
                tokens.extend(self.read_string(reader, ch)?);
            }
            else {
                tokens.extend(self.read_string(reader, ch)?);
            }
            *string_byte_start_r = reader.pos();
            return Ok(tokens);
        }

        if buffer.len() == 0 {
            let is_numeric = Token::is_number_start(ch, ch_next);
            if is_numeric {
                tokens.extend(self.read_numeric_literal(reader));
                *string_byte_start_r = reader.pos();
                return Ok(tokens);
            }
        }

        if Token::is_line_break(ch) {
            process_buffer(reader, buffer, self.current_line, self.current_column);


            let mut line_break_str = String::with_capacity(2);

            line_break_str.push(ch);
            if Token::is_multichar_line_break_sequence(ch, ch_next) {
                line_break_str.push(reader.advance().unwrap());
            }

            tokens.push(Token {
                kind: TokenKind::Newline,
                lexeme: line_break_str,
                line: self.current_line,
                column:  self.current_column,
                start: string_byte_start,
                end: reader.pos(),
            });

            return Ok(tokens);
        }

        if Token::is_white_space(ch) {
            process_buffer(reader, buffer, self.current_line, self.current_column);
            let white = self.read_while(reader, |&c, &c_next| !Token::is_white_space(c));
            tokens.push(Token {
                kind: TokenKind::Whitespace,
                lexeme: white,
                line: self.current_line,
                column:  self.current_column,
                start: string_byte_start,
                end: reader.pos(),
            });
            return Ok(tokens);
        }

        if Token::is_operator_start(ch) {
            process_buffer(reader, buffer, self.current_line, self.current_column);

            let operator = self.read_while(reader, |&c, &c_next| Token::is_operator_start(c));
            let operator_kind = OperatorEnum::from_str(&operator);
            if operator_kind.is_none() {
                return Err(LexerError {
                    kind: LexerErrorKind::UnexpectedToken,
                    span: Span { start: string_byte_start, end: reader.pos() },
                });
            }
            tokens.push(Token {
                kind: TokenKind::Operator(operator_kind.unwrap()),
                lexeme: operator,
                line: self.current_line,
                column:  self.current_column,
                start: string_byte_start,
                end: reader.pos(),
            });
        }

        if buffer.len() == 0 && Token::is_valid_identifier_start(ch) || buffer.len() > 0 && Token::is_valid_identifier_continue(ch) {
            buffer.push(ch);
        }


        reader.advance();

        *string_byte_start_r = string_byte_start;
        Ok(tokens)

    }

    pub fn process_buffer(buffer: &mut String, start: usize, current_line: usize, current_column: usize) -> Option<Token> {
        let kw = Token::get_keyword(buffer);
        let mut token: Option<Token> = None;
        if kw.is_some() {
            token = Some(Token {
                kind: TokenKind::Keyword(kw.unwrap()),
                lexeme: buffer.clone(),
                line: current_line,
                column:  current_column,
                start,
                end: buffer.len(),
            });
        }

        let op = Token::get_operator(buffer);
        if op.is_some() {
            token = Some(Token {
                kind: TokenKind::Operator(op.unwrap()),
                lexeme: buffer.clone(),
                line: current_line,
                column:  current_column,
                start,
                end: buffer.len(),
            });
        }

        buffer.clear();

        token
    }

    /// Aktualizuje aktuální pozici
    ///
    /// # Parametry
    ///
    /// | Název | Typ | Popis |
    /// |---------|--------|-----------------------|
    /// | `ch` | `char` | Znak, který je na aktuální pozici, dle kterého se identifikuje zda posunout jen column nebo i řádek |
    pub fn update_current_position(&mut self, ch: char) {
        if Token::is_line_break(ch) {
            self.current_line += 1;
            self.current_column = 0;
        }
        else {
            self.current_column += 1;
        }
    }

    /// Čte data dokud je splněný predikat
    ///
    /// # Parametry
    ///
    /// | Název | Typ | Popis |
    /// |---------|--------|-----------------------|
    /// | `iter` | `&mut Utf8Cursor` | Sdíleny iterator |
    /// | `predicate` | `Fn(&char) -> bool` | Predikát |
    ///
    /// # Návratové hodnoty
    ///
    /// Přečtený blok dat
    ///
    /// # Příklad použití
    pub fn read_while<T>(&mut self, reader: &mut Utf8Cursor, predicate: T) -> String
    where T: Fn(&char, &Option<char>) -> bool
    {
        let mut buffer = String::new();
        while reader.peek().is_some() {
            let ch_test = reader.peek();
            let ch_next = reader.peek_n(1);
            
            if ch_test.is_none() {
                break;
            }
            
            if !predicate(&ch_test.unwrap(), &ch_next) {
                self.update_current_position(ch_test.unwrap());
                
                buffer.push(reader.advance().unwrap());
            } else {
                break;
            }
        }

        buffer
    }

    pub fn read_while_line_break(&mut self, reader: &mut Utf8Cursor) -> String {
        let mut buffer = String::new();
        while reader.peek().is_some() {
            let ch_test = reader.peek();
            let ch_next = reader.peek_n(1);

            if ch_test.is_none() {
                break;
            }

            let is_line_break_seq = Token::is_multichar_line_break_sequence(ch_test.unwrap(), ch_next);

            if !Token::is_line_break(ch_test.unwrap()) {
                self.update_current_position(ch_test.unwrap());


                buffer.push(reader.advance().unwrap());
            } else {
                break;
            }
        }

        buffer
    }
}