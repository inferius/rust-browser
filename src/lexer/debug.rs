use std::io::Write;
use crate::lexer::base::Lexer;
use crate::tokens::{Token, TokenKind};

impl Lexer {
    pub fn debug_print_tokens(tokens: Vec<Token>) {
        println!("{:<5} {:<5} {:<5} {:<25} {}", "Line", "Col", "Start", "Kind", "Lexeme");
        println!("{}", "-".repeat(70));
        for t in &tokens {
            if matches!(t.kind, TokenKind::Whitespace | TokenKind::Newline) { continue; }
            println!("{:<5} {:<5} {:<5} {:<25} {:?}", t.line, t.column, t.start, format!("{}", t.kind), t.lexeme);
        }
    }

    fn get_token_html(token: &Token) -> String {
        let mut html = String::with_capacity(300);

        html.push_str(&format!(
            "<span class='token {0}' title='{1}'>{0}</span>",
                              token.kind.to_string(),
                              token.lexeme
        ));

        if token.kind == TokenKind::Newline {
            html.push_str("<br>");
        }


        html
    }
}