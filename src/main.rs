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
mod browser;
mod debug_view;

use lexer::base::Lexer;
use parser::Parser;
use interpreter::Interpreter;
use tokens::TokenKind;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Debug viewer: cargo run -- debug [file.js] [output.html]
    if args.len() > 1 && args[1] == "debug" {
        let (source, source_name) = if args.len() > 2 {
            let path = &args[2];
            match std::fs::read_to_string(path) {
                Ok(s) => (s, path.clone()),
                Err(e) => { eprintln!("Nelze nacist {path}: {e}"); return; }
            }
        } else {
            // Default ukazka
            (r#"// Demo JS pro debug viewer
const greeting = `Ahoj svete!`;
function fact(n) {
    if (n <= 1) return 1n;
    return BigInt(n) * fact(n - 1);
}
const result = fact(10);
console.log(greeting, result);
"#.to_string(), "demo.js".to_string())
        };
        let out_path = if args.len() > 3 { args[3].clone() } else { "debug.html".to_string() };
        let html = debug_view::generate_debug_html(&source, &source_name);
        if let Err(e) = std::fs::write(&out_path, &html) {
            eprintln!("Nelze zapsat {out_path}: {e}");
            return;
        }
        println!("Debug HTML zapsan: {out_path}");
        println!("Otevri v prohlizeci: file:///{}/{}",
            std::env::current_dir().unwrap().display().to_string().replace('\\', "/"),
            out_path);
        return;
    }

    // DevTools: cargo run -- devtools [file.html] [output.html]
    if args.len() > 1 && args[1] == "devtools" {
        let html_path = args.get(2).cloned().unwrap_or_else(|| "static/test.html".to_string());
        let html = match std::fs::read_to_string(&html_path) {
            Ok(s) => s,
            Err(e) => { eprintln!("Nelze nacist {html_path}: {e}"); return; }
        };
        let css_path = html_path.replace(".html", ".css");
        let css = std::fs::read_to_string(&css_path).unwrap_or_default();

        let document = browser::html_parser::parse_html(&html, &html_path);
        let stylesheets = vec![browser::css_parser::parse_stylesheet(&css)];

        // Extract <script> obsah pro Sources panel
        let scripts: Vec<String> = document.root.get_elements_by_tag("script")
            .iter().map(|s| s.text_content()).collect();
        let script_src = scripts.iter().find(|s| !s.trim().is_empty()).cloned();

        // Spust JS v interpreteru aby se zachytily console.log + fetch logy
        let mut interp = interpreter::Interpreter::new();
        interp.set_document(browser::html_parser::parse_html(&html, &html_path));
        for src in &scripts {
            if src.trim().is_empty() { continue; }
            let lex = match lexer::base::Lexer::parse_str(src, "<script>") {
                Ok(l) => l, Err(_) => continue,
            };
            let tokens: Vec<_> = lex.tokens.into_iter()
                .filter(|t| !matches!(t.kind,
                    tokens::TokenKind::Whitespace | tokens::TokenKind::Newline
                    | tokens::TokenKind::CommentLine(_) | tokens::TokenKind::CommentBlock(_)))
                .collect();
            let mut parser = parser::Parser::new(tokens);
            if let Ok(prog) = parser.parse() {
                let _ = interp.run(&prog);
            }
        }

        let console_log = interp.console_log.borrow().clone();
        let network_log = interp.network_log.borrow().clone();

        let html_out = debug_view::devtools::generate_devtools_html(
            &document,
            &stylesheets,
            script_src.as_deref(),
            &console_log,
            &network_log,
        );

        let out_path = args.get(3).cloned().unwrap_or_else(|| "devtools.html".to_string());
        if let Err(e) = std::fs::write(&out_path, &html_out) {
            eprintln!("Nelze zapsat {out_path}: {e}");
            return;
        }
        println!("DevTools HTML zapsan: {out_path}");
        println!("Console logs: {}, Network calls: {}", console_log.len(), network_log.len());
        return;
    }

    // Browser mode: cargo run -- browser nebo cargo run -- window [path/to/file.html]
    if args.len() > 1 && (args[1] == "browser" || args[1] == "window") {
        // Default: static/test.html
        let html_path = args.get(2).cloned().unwrap_or_else(|| "static/test.html".to_string());
        let html = match std::fs::read_to_string(&html_path) {
            Ok(s) => s,
            Err(e) => { eprintln!("Nelze nacist {html_path}: {e}"); return; }
        };

        // CSS: nacti z <link> nebo z .css souboru se stejnym nazvem
        let css_path = html_path.replace(".html", ".css");
        let css = std::fs::read_to_string(&css_path).unwrap_or_default();

        if args[1] == "window" {
            if let Err(e) = browser::render::run_window_with_html(html, css) {
                eprintln!("Chyba okna: {e}");
            }
        } else {
            browser::render::run_browser(&html, &css);
        }
        return;
    }

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
