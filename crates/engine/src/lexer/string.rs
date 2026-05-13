use crate::lexer::base::Lexer;
use crate::specifications::lexer_errors::{LexerError, LexerErrorKind, Span};
use crate::specifications::number_literal::NumberLiteral;
use crate::tokens::{EscapeInfo, EscapeKind, Token, TokenKind};
use crate::utils::utf8_cursor::Utf8Cursor;

#[derive(Debug)]
pub struct EscapeResult {
    pub kind: EscapeKind,
    pub character: char,
    pub raw: String,
}

impl Lexer {
    pub fn read_string(&mut self, r: &mut Utf8Cursor, quote: char, start: usize) -> Result<Token, LexerError> {
        let mut value = String::new();
        let mut raw = quote.to_string();
        let mut escapes: Vec<EscapeInfo> = Vec::new();

        loop {
            if r.eof() {
                return Err(LexerError { kind: LexerErrorKind::UnterminatedString, span: Span { start, end: r.pos() } });
            }
            let ch = r.peek().unwrap();

            if Token::is_line_break(ch) {
                return Err(LexerError { kind: LexerErrorKind::UnterminatedString, span: Span { start, end: r.pos() } });
            }
            if ch == quote {
                r.advance(); self.bump(ch); raw.push(ch); break;
            }
            if ch == '\\' {
                let esc_pos = r.pos();
                let esc = self.read_escape_sequence(r, start)?;
                raw.push_str(&esc.raw);
                if esc.character != '\0' { value.push(esc.character); }
                escapes.push(EscapeInfo { kind: esc.kind, raw: esc.raw, resolved_char: esc.character, position_in_raw: esc_pos });
                continue;
            }
            r.advance(); self.bump(ch); raw.push(ch); value.push(ch);
        }

        let lexeme = raw.clone();
        Ok(self.tok(TokenKind::StringLiteral { value, raw, escapes }, lexeme, start))
    }

    pub fn read_escape_sequence(&mut self, r: &mut Utf8Cursor, start: usize) -> Result<EscapeResult, LexerError> {
        let mk_err = |end: usize| LexerError { kind: LexerErrorKind::InvalidEscapeSequence, span: Span { start, end } };

        r.advance(); self.bump('\\');
        let mut raw = String::from('\\');

        if r.eof() { return Err(mk_err(r.pos())); }

        let esc = r.advance().unwrap();
        self.bump(esc);
        raw.push(esc);

        match esc {
            'n'  => Ok(EscapeResult { kind: EscapeKind::Simple, character: '\n', raw }),
            'r'  => Ok(EscapeResult { kind: EscapeKind::Simple, character: '\r', raw }),
            't'  => Ok(EscapeResult { kind: EscapeKind::Simple, character: '\t', raw }),
            'b'  => Ok(EscapeResult { kind: EscapeKind::Simple, character: '\u{0008}', raw }),
            'f'  => Ok(EscapeResult { kind: EscapeKind::Simple, character: '\u{000C}', raw }),
            'v'  => Ok(EscapeResult { kind: EscapeKind::Simple, character: '\u{000B}', raw }),
            '0'  => Ok(EscapeResult { kind: EscapeKind::Simple, character: '\0', raw }),
            '\'' | '"' | '\\' | '`' => Ok(EscapeResult { kind: EscapeKind::Simple, character: esc, raw }),
            'x'  => {
                let hex = self.read_n_hex(r, 2, start)?;
                raw.push_str(&hex);
                let code = u32::from_str_radix(&hex, 16).unwrap();
                let ch = char::from_u32(code).ok_or_else(|| mk_err(r.pos()))?;
                Ok(EscapeResult { kind: EscapeKind::Hex, character: ch, raw })
            }
            'u'  => {
                if r.peek() == Some('{') {
                    r.advance(); self.bump('{'); raw.push('{');
                    let mut hex = String::new();
                    loop {
                        match r.peek() {
                            Some('}') => { r.advance(); self.bump('}'); raw.push('}'); break; }
                            Some(c) if c.is_ascii_hexdigit() => { r.advance(); self.bump(c); hex.push(c); raw.push(c); }
                            _ => return Err(mk_err(r.pos())),
                        }
                    }
                    let code = u32::from_str_radix(&hex, 16).map_err(|_| mk_err(r.pos()))?;
                    let ch = char::from_u32(code).ok_or_else(|| mk_err(r.pos()))?;
                    Ok(EscapeResult { kind: EscapeKind::Unicode, character: ch, raw })
                } else {
                    let hex = self.read_n_hex(r, 4, start)?;
                    raw.push_str(&hex);
                    let code = u32::from_str_radix(&hex, 16).unwrap();
                    let ch = char::from_u32(code).ok_or_else(|| mk_err(r.pos()))?;
                    Ok(EscapeResult { kind: EscapeKind::Unicode, character: ch, raw })
                }
            }
            '\n' | '\r' | '\u{2028}' | '\u{2029}' => {
                if esc == '\r' && r.peek() == Some('\n') { let lf = r.advance().unwrap(); self.bump(lf); raw.push(lf); }
                Ok(EscapeResult { kind: EscapeKind::Simple, character: '\0', raw })
            }
            '1'..='7' => {
                let mut oct = esc.to_string();
                for _ in 0..2 {
                    if let Some(c) = r.peek() {
                        if NumberLiteral::is_octal_digit(c) { r.advance(); self.bump(c); oct.push(c); raw.push(c); }
                        else { break; }
                    }
                }
                let code = u32::from_str_radix(&oct, 8).unwrap();
                Ok(EscapeResult { kind: EscapeKind::Octal, character: char::from_u32(code).unwrap_or('\0'), raw })
            }
            _ => Ok(EscapeResult { kind: EscapeKind::Simple, character: esc, raw }),
        }
    }

    fn read_n_hex(&mut self, r: &mut Utf8Cursor, n: usize, start: usize) -> Result<String, LexerError> {
        let mut s = String::with_capacity(n);
        for _ in 0..n {
            match r.peek() {
                Some(c) if c.is_ascii_hexdigit() => { r.advance(); self.bump(c); s.push(c); }
                _ => return Err(LexerError { kind: LexerErrorKind::InvalidEscapeSequence, span: Span { start, end: r.pos() } }),
            }
        }
        Ok(s)
    }
}

#[cfg(test)]
#[path = "tests/string.rs"]
mod tests;
