use std::fs;
use crate::specifications::lexer_errors::{LexerError, LexerErrorKind, Span};
use crate::tokens::{CommentKind, OperatorEnum, StringKind, Token, TokenKind};
use crate::utils::utf8_cursor::Utf8Cursor;

pub struct Lexer {
    pub tokens: Vec<Token>,
    pub source_name: String,
    pub current_line: usize,
    pub current_column: usize,
}

impl Lexer {
    pub fn new() -> Self {
        Lexer { tokens: Vec::new(), source_name: String::new(), current_line: 1, current_column: 0 }
    }

    pub fn parse_file(path: &str) -> Result<Self, LexerError> {
        let src = fs::read_to_string(path).map_err(|_| LexerError {
            kind: LexerErrorKind::UnexpectedEOF,
            span: Span { start: 0, end: 0 },
        })?;
        Self::parse_str(&src, path)
    }

    pub fn parse_str(source: &str, name: &str) -> Result<Self, LexerError> {
        let mut lex = Lexer { tokens: Vec::new(), source_name: name.to_string(), current_line: 1, current_column: 0 };
        let mut cursor = Utf8Cursor::new(source);
        lex.tokens = lex.lex(&mut cursor)?;
        Ok(lex)
    }

    // ─── Hlavní smyčka ────────────────────────────────────────────────────────

    fn lex(&mut self, r: &mut Utf8Cursor) -> Result<Vec<Token>, LexerError> {
        let mut tokens: Vec<Token> = Vec::new();

        // Hashbang (#! ...) – přeskočit celý první řádek
        if r.peek() == Some('#') && r.peek_n(1) == Some('!') {
            while r.peek().map(|c| !Token::is_line_break(c)).unwrap_or(false) {
                r.advance();
            }
        }

        // Stack pro template literály:
        // Při vstupu do ${ uložíme aktuální hloubku závorek.
        // Při výskytu } s hloubkou == uložené → zavíráme template výraz.
        let mut tmpl_stack: Vec<i32> = Vec::new();
        let mut brace_depth: i32 = 0;

        while !r.eof() {
            let start = r.pos();
            let ch = r.peek().unwrap();

            // ── Whitespace ───────────────────────────────────────────────────
            if Token::is_white_space(ch) {
                let s = self.eat_while(r, |c| Token::is_white_space(c));
                tokens.push(self.tok(TokenKind::Whitespace, s, start));
                continue;
            }

            // ── Newline ──────────────────────────────────────────────────────
            if Token::is_line_break(ch) {
                let s = self.eat_newline(r);
                tokens.push(self.tok(TokenKind::Newline, s, start));
                continue;
            }

            // ── Template: } zavírá výraz v šabloně ──────────────────────────
            if ch == '}' && !tmpl_stack.is_empty() && brace_depth == *tmpl_stack.last().unwrap() {
                tmpl_stack.pop();
                r.advance(); self.bump('}');
                // Dočteme zbytek šablony
                let (text, is_tail) = self.lex_template_text(r, start)?;
                if is_tail {
                    tokens.push(self.tok(TokenKind::TemplateTail(text.clone()), text, start));
                } else {
                    tokens.push(self.tok(TokenKind::TemplateMiddle(text.clone()), text, start));
                    tmpl_stack.push(brace_depth);
                }
                continue;
            }

            // ── Sledování hloubky závorek (pro template výrazy) ──────────────
            match ch {
                '{' => brace_depth += 1,
                '}' => brace_depth -= 1,
                _   => {}
            }

            // ── Komentáře ────────────────────────────────────────────────────
            let ck = Token::is_comment_start(ch, r.peek_n(1));
            if ck != CommentKind::None {
                r.advance(); r.advance(); self.bump('/'); self.bump('/');
                let text = if ck == CommentKind::SingleLine {
                    let t = self.eat_while(r, |c| !Token::is_line_break(c));
                    tokens.push(self.tok(TokenKind::CommentLine(t.clone()), t.clone(), start));
                    t
                } else {
                    let t = self.read_block_comment(r, start)?;
                    tokens.push(self.tok(TokenKind::CommentBlock(t.clone()), t.clone(), start));
                    t
                };
                let _ = text;
                continue;
            }

            // ── HTML komentář <!-- ───────────────────────────────────────────
            if ch == '<' && r.peek_n(1) == Some('!') && r.peek_n(2) == Some('-') && r.peek_n(3) == Some('-') {
                for c in ['<','!','-','-'] { r.advance(); self.bump(c); }
                tokens.push(self.tok(TokenKind::HtmlCommentStart, "<!--".into(), start));
                continue;
            }

            // ── Template literál ─────────────────────────────────────────────
            if ch == '`' {
                r.advance(); self.bump('`');
                let (text, is_tail) = self.lex_template_text(r, start)?;
                if is_tail {
                    tokens.push(self.tok(TokenKind::NoSubstitutionTemplate(text.clone()), text, start));
                } else {
                    tokens.push(self.tok(TokenKind::TemplateHead(text.clone()), text, start));
                    tmpl_stack.push(brace_depth);
                }
                continue;
            }

            // ── Řetězce ──────────────────────────────────────────────────────
            let sk = Token::is_string_start(ch);
            if sk != StringKind::None && sk != StringKind::TemplateString {
                r.advance(); self.bump(ch);
                tokens.push(self.read_string(r, ch, start)?);
                continue;
            }

            // ── Čísla ────────────────────────────────────────────────────────
            if Token::is_number_start(ch, r.peek_n(1)) {
                tokens.push(self.read_numeric_literal(r)?);
                continue;
            }

            // ── Identifikátory / klíčová slova ───────────────────────────────
            if Token::is_valid_identifier_start(ch) {
                let ident = self.read_identifier(r);
                let kind = if let Some(kw) = Token::get_keyword(&ident) {
                    TokenKind::Keyword(kw)
                } else {
                    TokenKind::Identifier(ident.clone())
                };
                tokens.push(self.tok(kind, ident, start));
                continue;
            }

            // ── Operátory (greedy, nejdelší shoda) ───────────────────────────
            if Token::is_operator_start(ch) {
                tokens.push(self.read_operator(r, start)?);
                continue;
            }

            // ── Neznámý znak ─────────────────────────────────────────────────
            r.advance();
            return Err(LexerError {
                kind: LexerErrorKind::UnexpectedCharacter { found: ch },
                span: Span { start, end: r.pos() },
            });
        }

        tokens.push(Token { kind: TokenKind::Eof, lexeme: String::new(), start: r.pos(), end: r.pos(), line: self.current_line, column: self.current_column });
        Ok(tokens)
    }

    // ─── Pomocné metody ───────────────────────────────────────────────────────

    /// Vytvoří token s danou pozicí a aktuálním line/column.
    pub fn tok(&self, kind: TokenKind, lexeme: String, start: usize) -> Token {
        Token { kind, end: start + lexeme.len(), lexeme, start, line: self.current_line, column: self.current_column }
    }

    /// Aktualizuje čítač řádků/sloupců.
    pub fn bump(&mut self, ch: char) {
        if Token::is_line_break(ch) { self.current_line += 1; self.current_column = 0; }
        else { self.current_column += 1; }
    }

    /// Čte znaky dokud predikát vrátí true, vrátí přečtený řetězec.
    pub fn eat_while<F: Fn(char) -> bool>(&mut self, r: &mut Utf8Cursor, pred: F) -> String {
        let mut buf = String::new();
        while let Some(ch) = r.peek() {
            if pred(ch) { r.advance(); self.bump(ch); buf.push(ch); } else { break; }
        }
        buf
    }

    /// Spotřebuje newline sekvenci (LF, CR, CR+LF, LS, PS).
    pub fn eat_newline(&mut self, r: &mut Utf8Cursor) -> String {
        let ch = r.advance().unwrap();
        self.bump(ch);
        let mut s = ch.to_string();
        if ch == '\r' && r.peek() == Some('\n') {
            let lf = r.advance().unwrap(); self.bump(lf); s.push(lf);
        }
        s
    }

    /// Blokový komentář – čte až do `*/`.
    fn read_block_comment(&mut self, r: &mut Utf8Cursor, start: usize) -> Result<String, LexerError> {
        let mut buf = String::new();
        loop {
            if r.eof() { return Err(LexerError { kind: LexerErrorKind::UnterminatedComment, span: Span { start, end: r.pos() } }); }
            let ch = r.advance().unwrap(); self.bump(ch);
            if ch == '*' && r.peek() == Some('/') { r.advance(); self.bump('/'); break; }
            buf.push(ch);
        }
        Ok(buf)
    }

    /// Čte identifikátor.
    pub fn read_identifier(&mut self, r: &mut Utf8Cursor) -> String {
        let mut buf = String::new();
        while let Some(ch) = r.peek() {
            if (buf.is_empty() && Token::is_valid_identifier_start(ch))
                || (!buf.is_empty() && Token::is_valid_identifier_continue(ch))
            { r.advance(); self.bump(ch); buf.push(ch); } else { break; }
        }
        buf
    }

    /// Greedy matching operátorů – vybere nejdelší shodu (max 4 znaky).
    pub fn read_operator(&mut self, r: &mut Utf8Cursor, start: usize) -> Result<Token, LexerError> {
        let cs: [Option<char>; 4] = [r.peek(), r.peek_n(1), r.peek_n(2), r.peek_n(3)];
        for len in [4usize, 3, 2, 1] {
            if cs[..len].iter().any(|c| c.is_none()) { continue; }
            let s: String = cs[..len].iter().map(|c| c.unwrap()).collect();
            if let Some(op) = OperatorEnum::from_str(&s) {
                for _ in 0..len { let c = r.advance().unwrap(); self.bump(c); }
                return Ok(self.tok(TokenKind::Operator(op), s, start));
            }
        }
        let ch = r.advance().unwrap();
        Err(LexerError { kind: LexerErrorKind::UnexpectedCharacter { found: ch }, span: Span { start, end: r.pos() } })
    }

    /// Čte textovou část template literálu až do `${` nebo `` ` ``.
    /// Vrací (text, is_tail): is_tail=true pokud jsme narazili na `` ` ``.
    pub fn lex_template_text(&mut self, r: &mut Utf8Cursor, start: usize) -> Result<(String, bool), LexerError> {
        let mut buf = String::new();
        loop {
            if r.eof() { return Err(LexerError { kind: LexerErrorKind::UnterminatedTemplate, span: Span { start, end: r.pos() } }); }
            let ch = r.peek().unwrap();
            if ch == '`' { r.advance(); self.bump(ch); return Ok((buf, true)); }
            if ch == '$' && r.peek_n(1) == Some('{') {
                r.advance(); r.advance(); self.bump('$'); self.bump('{');
                return Ok((buf, false));
            }
            if ch == '\\' {
                let esc = self.read_escape_sequence(r, start)?;
                // Ignorujeme nulový char z line continuation
                if esc.character != '\0' { buf.push(esc.character); }
                continue;
            }
            r.advance(); self.bump(ch); buf.push(ch);
        }
    }
}
