// dead_code allow: mnoho fns je expose pro tests (compiluji se zvlast) +
// pro budouci pub API (DOM/CSS variant exhaustivnost). unused_imports +
// unused_variables zustavaji aktivni - chceme je videt + opravit.
#![allow(dead_code)]

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
mod devtools;
mod debug_bp;

use lexer::base::Lexer;
use parser::Parser;
use interpreter::Interpreter;
use tokens::TokenKind;

/// Resolve @import statements v CSS. Pro kazdy `@import "url";` nebo `@import url("url");`
/// fetchne externi CSS proti `base_url` a pripoji obsah pred toto pravidlo.
/// Recursivni - nested @imports v fetched CSS se taky resolvuji (max depth 5).
fn resolve_css_imports(css: &str, base_url: &str, depth: u32) -> String {
    if depth > 5 { return css.to_string(); }
    let mut out = String::with_capacity(css.len());
    let mut bytes = css.bytes().peekable();
    let mut buf = String::new();
    while let Some(b) = bytes.next() {
        buf.push(b as char);
        if buf.ends_with("@import") {
            // Skip @import.
            buf.truncate(buf.len() - 7);
            out.push_str(&buf);
            buf.clear();
            // Read until ';' (end of @import).
            let mut rest = String::new();
            while let Some(b) = bytes.next() {
                if b == b';' { break; }
                rest.push(b as char);
            }
            // Extract URL from rest. Forms: "url" / 'url' / url("url") / url("url") layer(name) media...
            let trimmed = rest.trim();
            let url_part = if let Some(stripped) = trimmed.strip_prefix("url(") {
                if let Some(end) = stripped.find(')') {
                    stripped[..end].trim().trim_matches('"').trim_matches('\'').to_string()
                } else { String::new() }
            } else if trimmed.starts_with('"') || trimmed.starts_with('\'') {
                let q = &trimmed[..1];
                let after = &trimmed[1..];
                if let Some(end) = after.find(q) {
                    after[..end].to_string()
                } else { String::new() }
            } else { String::new() };
            if !url_part.is_empty() {
                let resolved = browser::render::resolve_url(base_url, &url_part);
                println!("[fetch @import] {resolved}");
                if let Some(c) = browser::render::fetch_text_url(&resolved) {
                    let nested = resolve_css_imports(&c, &resolved, depth + 1);
                    out.push('\n');
                    out.push_str(&nested);
                }
            }
            // Pokracujem dalsim CSS.
        }
    }
    out.push_str(&buf);
    out
}

/// Extract <link rel="stylesheet" href="..."> hrefs z HTML.
fn extract_stylesheet_hrefs(html: &str) -> Vec<String> {
    let document = browser::html_parser::parse_html(html, "about:blank");
    let mut out = Vec::new();
    for link in document.root.get_elements_by_tag("link") {
        let rel = link.attr("rel").unwrap_or_default().to_lowercase();
        if rel.contains("stylesheet") {
            if let Some(href) = link.attr("href") {
                out.push(href);
            }
        }
    }
    out
}

/// Extract inline <style> ... </style> blocky.
fn extract_inline_styles(html: &str) -> Vec<String> {
    let document = browser::html_parser::parse_html(html, "about:blank");
    document.root.get_elements_by_tag("style")
        .iter().map(|s| s.text_content()).collect()
}

fn main() {
    // Spawn worker thread s 256 MB stack pro main work.
    // Windows main thread default = 1 MB; v debug buildu (no inline) layout/paint
    // recursion ma velke frames (30+ KB). Linker /STACK flag dava 64 MB ale
    // dedicated thread je robustnejsi (vetsi rezerva pro winit + interpreter).
    let handle = std::thread::Builder::new()
        .name("rwe-main".into())
        .stack_size(256 * 1024 * 1024)
        .spawn(real_main)
        .expect("nelze spawnout main worker thread");
    let _ = handle.join();
}

fn real_main() {
    let args: Vec<String> = std::env::args().collect();

    // Safe-mode: --safe-mode flag resetuje profile config (theme/dock/sirku
    // panelu) na default. Pouzite po crash z bad config (napr. dock NaN).
    if args.iter().any(|a| a == "--safe-mode" || a == "-safe-mode") {
        if let Some(dir) = devtools::profile::ensure_profile_dir(devtools::profile::active_profile()) {
            // Smaze dock_position.json a theme.json - jine soubory zachovat
            // (history, bookmarks, downloads chce uzivatel preserve).
            let _ = std::fs::remove_file(dir.join("dock_position.json"));
            let _ = std::fs::remove_file(dir.join("theme.json"));
            eprintln!("[safe-mode] reset profile config v {}", dir.display());
        }
    }

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

    // Browser mode: cargo run -- browser [path nebo URL] [--devtools]
    // Path muze byt:
    //   - http(s):// URL    -> fetch HTML pres ureq, extract <link> CSS taky pres HTTP
    //   - file system path  -> read local
    //   - default static/test.html
    if args.len() > 1 && (args[1] == "browser" || args[1] == "window" || args[1] == "shell") {
        let mut target: Option<String> = None;
        let mut auto_devtools = false;
        // browser + shell = shell mode (chrome bar nahore, persistent URL bar).
        // window = naked viewport (engine demo, bez chrome).
        // --no-shell flag forcuje naked variant pro browser.
        let mut shell_mode = args[1] != "window";
        for a in &args[2..] {
            if a == "--no-shell" { shell_mode = false; }
        }
        for a in &args[2..] {
            if a == "--devtools" || a == "-d" { auto_devtools = true; }
            else if let Some(name) = a.strip_prefix("--profile=") {
                browser::devtools_panel::set_profile(name);
            }
            else if !a.starts_with('-') && target.is_none() { target = Some(a.clone()); }
        }
        let target = target.unwrap_or_else(|| "static/test.html".to_string());

        // URL mode: http://, https://
        let is_url = target.starts_with("http://") || target.starts_with("https://");
        let (html, css, base_url, current_path) = if is_url {
            println!("[fetch] {target}");
            let html = match browser::render::fetch_text_url(&target) {
                Some(s) => s,
                None => { eprintln!("Nelze fetchnout {target}"); return; }
            };
            // Extract <link rel="stylesheet" href="..."> + fetch each.
            let mut css_combined = String::new();
            for href in extract_stylesheet_hrefs(&html) {
                let resolved = browser::render::resolve_url(&target, &href);
                println!("[fetch css] {resolved}");
                if let Some(c) = browser::render::fetch_text_url(&resolved) {
                    let imported = resolve_css_imports(&c, &resolved, 0);
                    css_combined.push('\n');
                    css_combined.push_str(&imported);
                }
            }
            // <style> inline blocks taky pridat (s @import resolution).
            for inline in extract_inline_styles(&html) {
                css_combined.push('\n');
                css_combined.push_str(&resolve_css_imports(&inline, &target, 0));
            }
            (html, css_combined, Some(target.clone()), None)
        } else {
            let html = match std::fs::read_to_string(&target) {
                Ok(s) => s,
                Err(e) => { eprintln!("Nelze nacist {target}: {e}"); return; }
            };
            let path_buf = std::path::PathBuf::from(&target);
            let abs_path = std::fs::canonicalize(&path_buf).unwrap_or(path_buf.clone());
            let base = format!("file:///{}", abs_path.display().to_string().replace('\\', "/"));
            // CSS: <link rel=stylesheet href=...> + <style> inline + co-located .css.
            let mut css_combined = String::new();
            // Co-located file s same name (legacy support: test.html -> test.css)
            let css_path = target.replace(".html", ".css");
            if let Ok(c) = std::fs::read_to_string(&css_path) {
                css_combined.push('\n');
                css_combined.push_str(&c);
            }
            // <link rel=stylesheet href> hrefs - resolve proti dir HTML.
            let html_dir = path_buf.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            for href in extract_stylesheet_hrefs(&html) {
                // Pokud absolute URL -> fetch HTTP.
                if href.starts_with("http://") || href.starts_with("https://") {
                    if let Some(c) = browser::render::fetch_text_url(&href) {
                        css_combined.push('\n');
                        css_combined.push_str(&c);
                    }
                } else {
                    // Relative -> resolve proti html_dir.
                    let css_file = html_dir.join(&href);
                    if let Ok(c) = std::fs::read_to_string(&css_file) {
                        css_combined.push('\n');
                        css_combined.push_str(&c);
                    }
                }
            }
            // <style> inline blocky.
            for inline in extract_inline_styles(&html) {
                css_combined.push('\n');
                css_combined.push_str(&inline);
            }
            (html, css_combined, Some(base), Some(abs_path))
        };

        let result = if shell_mode {
            browser::render::run_window_with_shell(html, css, current_path, auto_devtools, base_url)
        } else {
            browser::render::run_window_with_options(html, css, current_path, auto_devtools, base_url)
        };
        if let Err(e) = result {
            eprintln!("Chyba okna: {e}");
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
