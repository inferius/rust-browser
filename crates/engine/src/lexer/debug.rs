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
}