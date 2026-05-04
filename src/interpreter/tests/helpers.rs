/// Sdilene pomocne funkce pro vsechny testove moduly interpreteru.

use crate::interpreter::*;
use crate::lexer::base::Lexer;
use crate::parser::Parser;
use crate::tokens::TokenKind;

/// Spusti JS kod a vrati posledni return hodnotu.
pub fn run(src: &str) -> JsValue {
    let lexer = Lexer::parse_str(src, "<test>").unwrap();
    let tokens: Vec<_> = lexer.tokens.into_iter()
        .filter(|t| !matches!(t.kind,
            TokenKind::Whitespace | TokenKind::Newline
            | TokenKind::CommentLine(_) | TokenKind::CommentBlock(_)))
        .collect();
    let mut parser = Parser::new(tokens);
    let program = parser.parse().unwrap();
    let mut interp = Interpreter::new();
    interp.run(&program).unwrap()
}

/// Spusti JS vyraz (obali do `return ...;`).
pub fn eval(expr: &str) -> JsValue {
    run(&format!("return {expr};"))
}

/// Spusti JS s pre-registrovanymi virtualnimi moduly.
pub fn run_with_modules(src: &str, modules: &[(&str, &str)]) -> JsValue {
    let lexer = Lexer::parse_str(src, "<test>").unwrap();
    let tokens: Vec<_> = lexer.tokens.into_iter()
        .filter(|t| !matches!(t.kind,
            TokenKind::Whitespace | TokenKind::Newline
            | TokenKind::CommentLine(_) | TokenKind::CommentBlock(_)))
        .collect();
    let mut parser = Parser::new(tokens);
    let program = parser.parse().unwrap();
    let mut interp = Interpreter::new();
    for (k, v) in modules {
        interp.add_virtual_module(k, v);
    }
    interp.run(&program).unwrap()
}

/// Spusti src ale vrati Result (pro testy ocekavajici chybu).
pub fn try_run(src: &str) -> Result<JsValue, JsError> {
    let lexer = Lexer::parse_str(src, "<test>").unwrap();
    let tokens: Vec<_> = lexer.tokens.into_iter()
        .filter(|t| !matches!(t.kind,
            TokenKind::Whitespace | TokenKind::Newline
            | TokenKind::CommentLine(_) | TokenKind::CommentBlock(_)))
        .collect();
    let mut parser = Parser::new(tokens);
    let program = parser.parse().unwrap();
    let mut interp = Interpreter::new();
    interp.run(&program)
}

pub fn as_num(v: JsValue) -> f64 {
    match v { JsValue::Number(n) => n, other => panic!("Ocekavano Number, nalezeno {other:?}") }
}

pub fn as_str(v: JsValue) -> String {
    match v { JsValue::Str(s) => s, other => panic!("Ocekavan Str, nalezeno {other:?}") }
}

pub fn as_bool(v: JsValue) -> bool {
    match v { JsValue::Bool(b) => b, other => panic!("Ocekavan Bool, nalezeno {other:?}") }
}

pub fn as_bigint_str(v: JsValue) -> String {
    match v {
        JsValue::BigInt(n) => n.to_string(),
        other => panic!("Ocekavan BigInt, nalezeno {other:?}"),
    }
}
