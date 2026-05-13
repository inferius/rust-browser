/// Tokenizace regularniho vyrazu.
///
/// Regularni vyraz zacina `/` a konci `/flags`. Komplikace je, ze `/`
/// muze byt take operator deleni - lexer musi rozhodnout podle kontextu
/// (predchazejici vyznamny token).
///
/// # Kontextova pravidla (ECMAScript spec)
///
/// `/` je zacatek regularniho vyrazu kdyz predchazejici vyznamny token byl:
/// - Operator (krome `++`, `--`, `)`, `]`)
/// - Klicove slovo (krome `this`, `true`, `false`, `null`)
/// - Zacatek vstupu
///
/// `/` je operator deleni kdyz predchazejici vyznamny token byl:
/// - Identifikator
/// - Ciselny/retezdovy/template literal
/// - `this`, `true`, `false`, `null`
/// - `)`, `]`, `++`, `--` (= za vyrazem)

use crate::lexer::base::Lexer;
use crate::specifications::lexer_errors::{LexerError, LexerErrorKind, Span};
use crate::tokens::{KeywordEnum, OperatorEnum, Token, TokenKind};
use crate::utils::utf8_cursor::Utf8Cursor;

impl Lexer {
    /// Rozhodne jestli `/` na aktualni pozici je zacatek regex literalu.
    ///
    /// `last` = posledni vyznamny token (bez trivia).
    pub fn slash_is_regex_start(last: Option<&TokenKind>) -> bool {
        match last {
            // Zacatek vstupu -> regex
            None => true,
            Some(k) => match k {
                // Po operatoru: regex, KROME postfix ++ -- a zaviracich zavorek
                TokenKind::Operator(op) => !matches!(op,
                    OperatorEnum::PlusPlus | OperatorEnum::MinusMinus |
                    OperatorEnum::RParen   | OperatorEnum::RBracket
                ),
                // Po klicovem slove: regex, KROME hodnot (this/true/false/null)
                TokenKind::Keyword(kw) => !matches!(kw,
                    KeywordEnum::This  | KeywordEnum::True |
                    KeywordEnum::False | KeywordEnum::Null
                ),
                // Po hodnotovych tokenech -> deleni
                TokenKind::Identifier(_)               => false,
                TokenKind::NumericLiteral { .. }       => false,
                TokenKind::StringLiteral { .. }        => false,
                TokenKind::RegexLiteral { .. }         => false,
                TokenKind::NoSubstitutionTemplate(_)   => false,
                TokenKind::TemplateTail(_)             => false,
                // Vse ostatni -> regex (napr. zacatek bloku {)
                _ => true,
            }
        }
    }

    /// Precte regex literal ze vstupu. Cursor musi stát na oteviraci `/`.
    ///
    /// Vraci `TokenKind::RegexLiteral { pattern, flags }`.
    ///
    /// # Chyby
    /// `UnterminatedRegex` kdyz je konec radku nebo EOF pred zavirajici `/`.
    pub fn read_regex_literal(&mut self, r: &mut Utf8Cursor, start: usize) -> Result<Token, LexerError> {
        // Spotreba oteviraci /
        r.advance(); self.bump('/');

        let mut pattern = String::new();
        let mut in_class = false;   // uvnitr znakove tridy [...]

        loop {
            if r.eof() || r.peek().map(Token::is_line_break).unwrap_or(false) {
                return Err(LexerError {
                    kind: LexerErrorKind::UnterminatedRegex,
                    span: Span { start, end: r.pos() },
                });
            }

            let ch = r.peek().unwrap();

            if ch == '\\' {
                // Escape sekvence v regexu - zahrneme oba znaky tak jak jsou
                r.advance(); self.bump('\\');
                pattern.push('\\');
                if let Some(next) = r.peek() {
                    if !Token::is_line_break(next) {
                        r.advance(); self.bump(next);
                        pattern.push(next);
                    }
                }
            } else if ch == '[' {
                // Zacatek znakove tridy - uvnitr [...] lomitko nezakonci regex
                in_class = true;
                r.advance(); self.bump(ch);
                pattern.push(ch);
            } else if ch == ']' && in_class {
                in_class = false;
                r.advance(); self.bump(ch);
                pattern.push(ch);
            } else if ch == '/' && !in_class {
                // Konec regexu
                r.advance(); self.bump('/');
                break;
            } else {
                r.advance(); self.bump(ch);
                pattern.push(ch);
            }
        }

        // Flagy: d, g, i, m, s, u, v, y (libovolna ASCII pismena)
        let flags = self.eat_while(r, |c| c.is_ascii_alphabetic());

        let raw = format!("/{pattern}/{flags}");
        Ok(self.tok(TokenKind::RegexLiteral { pattern, flags }, raw, start))
    }
}

#[cfg(test)]
#[path = "tests/regex.rs"]
mod regex;
