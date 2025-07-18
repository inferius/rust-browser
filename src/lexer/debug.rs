use std::io::Write;
use crate::lexer::base::Lexer;
use crate::tokens::{Token, TokenKind};

impl Lexer {
    pub fn debug_print_tokens(tokens: Vec<Token>, target_file: &str) {
        let mut file = std::fs::File::create(target_file).unwrap();
        let mut writer = std::io::BufWriter::new(&mut file);
        let mut html = String::with_capacity(10000);
        for token in tokens {
            html = Self::get_token_html(&token);
            writer.write_fmt(format_args!("{}\n", html)).unwrap();
            //writeln!(writer, "{}", html).unwrap();
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