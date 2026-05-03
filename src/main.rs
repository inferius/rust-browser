#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#[macro_use]
mod utils;

mod tokens;
mod specifications;
mod ast;
mod lexer;
mod parser;
mod interpreter;

use lexer::base::Lexer;
use parser::Parser;
use interpreter::Interpreter;
use tokens::TokenKind;

fn main() {
    let source = r#"
function foo(a, b) {
    return a + b;
}

const arrow = (x) => x * x;

let x = 42;

if (x > 5) {
    console.log("vetsi");
} else {
    console.log("mensi nebo rovno");
}

let arr = [1, 2, 3];
arr[0] = 10;

const obj = { a: 1, b: "two" };

let name = "svete";
const tpl = `Ahoj ${name}!`;
console.log(tpl);

let cond = x > 10 ? "big" : "small";
console.log(cond);

let num = 6.5e-2;
console.log(num);

let result = foo(x, arr[2]);
console.log(result);

let sum = 0;
for (let i = 0; i < 5; i++) {
    sum += i;
}
console.log(sum);
"#;

    // ── 1. Tokenizace ─────────────────────────────────────────────────────────
    let lexer = match Lexer::parse_str(source, "<inline>") {
        Ok(l) => l,
        Err(e) => { eprintln!("Chyba lexeru: {e}"); return; }
    };

    println!("=== TOKENY ===");
    Lexer::debug_print_tokens(lexer.tokens.clone());
    println!();

    // ── 2. Parsování ──────────────────────────────────────────────────────────
    let tokens: Vec<_> = lexer.tokens.into_iter()
        .filter(|t| !matches!(t.kind, TokenKind::CommentLine(_) | TokenKind::CommentBlock(_)))
        .collect();

    let program = {
        let mut parser = Parser::new(tokens);
        match parser.parse() {
            Ok(p) => p,
            Err(e) => { eprintln!("Chyba parseru: {e}"); return; }
        }
    };

    println!("=== AST ===");
    println!("Program s {} příkazy\n", program.body.len());

    // ── 3. Interpretace ───────────────────────────────────────────────────────
    println!("=== VÝSTUP ===");
    let mut interp = Interpreter::new();
    if let Err(e) = interp.run(&program) {
        eprintln!("Chyba při běhu: {e}");
    }
}
