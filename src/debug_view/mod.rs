/// Graficky debug viewer - generuje HTML stranku s tokeny a AST.
///
/// Vstup: JS source.
/// Vystup: self-contained HTML soubor (CSS+JS embedded).
/// - Tokeny zobrazene jako barevne badge podle typu
/// - Tooltip pri najezdu (typ, lexeme, line:col, hodnota)
/// - AST tree (collapsible) - klikem rozbalit/zabalit uzly

pub mod tokens_view;
pub mod ast_view;
pub mod page;
pub mod devtools;

#[cfg(test)]
mod tests;

use crate::lexer::base::Lexer;
use crate::parser::Parser;
use crate::tokens::TokenKind;

/// Generuje kompletni debug HTML pro dany source.
pub fn generate_debug_html(source: &str, title: &str) -> String {
    // Lexer
    let lex_result = Lexer::parse_str(source, "<debug>");
    let tokens_html = match &lex_result {
        Ok(lex) => tokens_view::render_tokens(&lex.tokens),
        Err(e)  => format!("<div class=\"error\">Lexer error: {}</div>", html_escape(&format!("{e}"))),
    };

    // Parser
    let ast_html = match &lex_result {
        Ok(lex) => {
            let filtered: Vec<_> = lex.tokens.iter().cloned()
                .filter(|t| !matches!(t.kind,
                    TokenKind::Whitespace | TokenKind::Newline
                    | TokenKind::CommentLine(_) | TokenKind::CommentBlock(_)))
                .collect();
            let mut parser = Parser::new(filtered);
            match parser.parse() {
                Ok(prog) => ast_view::render_program(&prog),
                Err(e)   => format!("<div class=\"error\">Parser error: {}</div>", html_escape(&format!("{e}"))),
            }
        }
        Err(_) => "<div class=\"muted\">(lexer failed)</div>".into(),
    };

    page::wrap_page(title, source, &tokens_html, &ast_html)
}

/// HTML escape pro text content / attribute values.
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
     .replace('\'', "&#39;")
}
