use std::fs::File;
use std::io::{BufReader, Read};
use crate::tokens::{CommentKind, StringKind, Token, TokenKind};
use crate::utils::utf8_cursor::Utf8Cursor;

pub struct Lexer {
    pub tokens: Vec<Token>,
    pub original_file_name: String,
    pub current_line: usize,
    pub current_column: usize,
}

impl Lexer {
    /*pub fn parse_template_string(tokens: &mut Peekable<Token>, level: u16) {

    }*/

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
    pub fn parse_file(file_path: &str) -> Lexer {
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

        instance.tokens = instance.lex_string(&mut reader);

        instance
    }

    fn lex_string(&mut self, reader: &mut Utf8Cursor) -> Vec<Token> {
        let mut tokens: Vec<Token> = Vec::new();
        let mut string_buffer = String::with_capacity(1000);
        let mut string_buffer_start_line = 0;
        let mut string_buffer_start_column = 0;
        let mut string_byte_start = reader.pos();

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

            self.read_expression(reader, &mut string_buffer, &mut string_byte_start);

        }

        tokens
    }

    pub fn read_expression(&mut self, reader: &mut Utf8Cursor, buffer: &mut String, string_byte_start_r: &mut usize) -> Vec<Token> {
        let ch = reader.advance().unwrap();
        let ch_next = reader.peek().unwrap();
        let mut tokens: Vec<Token> = Vec::new();
        let mut string_byte_start = reader.pos();

        self.update_current_position(ch);

        self.read_while(reader, |&c| Token::is_white_space(c));

        let is_comment = Token::is_comment_start(ch, ch_next);
        if is_comment != CommentKind::None {
            if is_comment == CommentKind::SingleLine {
                let comment = self.read_while(reader, |&c| Token::is_white_space(c));
                tokens.push(Token {
                    kind: TokenKind::CommentLine(comment.clone()),
                    lexeme: comment,
                    line: self.current_line,
                    column:  self.current_column,
                    start: string_byte_start.clone(),
                    end: reader.pos(),
                });
                string_byte_start = reader.pos();
            }
            else {
                let comment = self.read_while(reader, |&c| Token::is_comment_end(c, ch_next) == CommentKind::MultiLine);
                tokens.push(Token {
                    kind: TokenKind::CommentBlock(comment.clone()),
                    lexeme: comment,
                    line: self.current_line,
                    column:  self.current_column,
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
        }



        *string_byte_start_r = string_byte_start;
        tokens

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
    where T: Fn(&char) -> bool
    {
        let mut buffer = String::new();
        while reader.peek().is_some() {
            let ch = reader.advance().unwrap();

            self.update_current_position(ch);

            if predicate(&ch) {
                buffer.push(reader.advance().unwrap());
            } else {
                break;
            }
        }

        buffer
    }
}