// dead_code allow: mnoho fns je expose pro tests (compiluji se zvlast) +
// pro budouci pub API (DOM/CSS variant exhaustivnost). unused_imports +
// unused_variables zustavaji aktivni - chceme je videt + opravit.
#![allow(dead_code)]

// Pub modules - tvori vetsinu povrchu enginu. Shell crate + externi
// uzivatele sahnou primo skrz tyto moduly. High-level facade pridana
// v Phase 2 (Engine struct).

#[macro_use]
pub mod utils;

pub mod tokens;
pub mod specifications;
pub mod ast;
pub mod lexer;
pub mod parser;
pub mod interpreter;
pub mod browser;
pub mod debug_view;
pub mod devtools;
pub mod debug_bp;

// Embeddable API contract - stable high-level facade pro hostujici aplikace
// (shell crate, third-party UI). Phase 2 = stubs, Phase 3-5 = naplnuje.
pub mod embed;
pub use embed::{Engine, EventResponse, InputEvent, WebView};

use lexer::base::Lexer;
use parser::Parser;
use interpreter::Interpreter;
use tokens::TokenKind;

// Page resource loader helpers (resolve_css_imports, extract_stylesheet_hrefs,
// extract_inline_styles) presunute do `embed::loader` pro sdileni s WebView.
use embed::loader::{extract_inline_styles, extract_stylesheet_hrefs, resolve_css_imports};

/// CLI dispatcher. Volano z bin/main.rs shim.
/// V Phase 1 je tohle ekvivalent puvodniho `real_main` z src/main.rs.
/// Phase 6 (--no-shell default) zredukuje na pure engine demo + presune
/// shell-zavisle rezimy do shell crate.
pub fn run_cli(args: Vec<String>) {
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
    if args.len() > 1 && (args[1] == "browser" || args[1] == "window") {
        let mut target: Option<String> = None;
        let mut auto_devtools = false;
        // Po refaktoru shell-as-crate (Session N+21) engine renderuje JEN naked
        // viewport. Chrome bar (tabs, addr, find, bookmarks) zustal jako Phase
        // 99 task pro shell crate. `browser` + `window` jsou aliasy.
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
                if let Some(c) = browser::render::fetch_text_url(&resolved) {
                    let imported = resolve_css_imports(&c, &resolved, 0);
                    // Diagnostic: kolik rules + chars parsed per sheet.
                    let chars = imported.len();
                    let rules = browser::css_parser::parse_stylesheet(&imported).rules.len();
                    println!("[fetch css] {resolved} ({chars} chars, {rules} rules)");
                    css_combined.push('\n');
                    css_combined.push_str(&imported);
                } else {
                    println!("[fetch css FAIL] {resolved}");
                }
            }
            // <style> inline blocks taky pridat (s @import resolution).
            for (idx, inline) in extract_inline_styles(&html).into_iter().enumerate() {
                let resolved = resolve_css_imports(&inline, &target, 0);
                let rules = browser::css_parser::parse_stylesheet(&resolved).rules.len();
                println!("[inline style #{idx}] {} chars, {rules} rules", resolved.len());
                css_combined.push('\n');
                css_combined.push_str(&resolved);
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

        let result = browser::render::run_window_with_options(
            html, css, current_path, auto_devtools, base_url);
        if let Err(e) = result {
            eprintln!("Chyba okna: {e}");
        }
        return;
    }

    // Dump mode: cargo run -- dump <url|path> [out.txt] [--selector=.foo]
    // Vystup: full layout box tree + matched CSS rules + computed styles per box.
    // Diff vs Chrome devtools `getComputedStyle()` ukaze co chyba.
    if args.len() > 1 && args[1] == "dump" {
        let target = args.get(2).cloned().unwrap_or_else(|| "static/test.html".to_string());
        let out_path = args.iter().skip(3).find(|a| !a.starts_with("--"))
            .cloned().unwrap_or_else(|| "dump.txt".to_string());
        let selector_filter = args.iter()
            .find_map(|a| a.strip_prefix("--selector=").map(String::from));

        let is_url = target.starts_with("http://") || target.starts_with("https://");
        let (html, css, base_url) = if is_url {
            println!("[fetch] {target}");
            let html = match browser::render::fetch_text_url(&target) {
                Some(s) => s,
                None => { eprintln!("Nelze fetch {target}"); return; }
            };
            let mut css = String::new();
            for href in extract_stylesheet_hrefs(&html) {
                let resolved = browser::render::resolve_url(&target, &href);
                if let Some(c) = browser::render::fetch_text_url(&resolved) {
                    css.push('\n');
                    css.push_str(&resolve_css_imports(&c, &resolved, 0));
                }
            }
            for inline in extract_inline_styles(&html) {
                css.push('\n');
                css.push_str(&resolve_css_imports(&inline, &target, 0));
            }
            (html, css, target.clone())
        } else {
            let html = match std::fs::read_to_string(&target) {
                Ok(s) => s,
                Err(e) => { eprintln!("Nelze read {target}: {e}"); return; }
            };
            let css_path = target.replace(".html", ".css");
            let mut css = std::fs::read_to_string(&css_path).unwrap_or_default();
            for inline in extract_inline_styles(&html) {
                css.push('\n'); css.push_str(&inline);
            }
            (html, css, format!("file:///{target}"))
        };

        let doc = browser::html_parser::parse_html(&html, &base_url);
        let stylesheets = vec![browser::css_parser::parse_stylesheet(&css)];
        let viewport_w = 1280.0;
        let viewport_h = 900.0;
        let style_map = browser::cascade::cascade_with_viewport(
            &doc.root, &stylesheets, viewport_w, viewport_h);
        let mut layout_root = browser::layout::layout_tree(
            &doc.root, &style_map, viewport_w, viewport_h);
        let _ = &stylesheets; // pouzite vys pri cascade

        // Walk layout tree + dump kazdy box.
        let mut out = String::new();
        out.push_str(&format!("# Dump: {target}\n"));
        out.push_str(&format!("# Stylesheets: {}\n", stylesheets.len()));
        let total_rules: usize = stylesheets.iter().map(|s| s.rules.len()).sum();
        out.push_str(&format!("# Total rules: {total_rules}\n"));
        if let Some(sel) = &selector_filter {
            out.push_str(&format!("# Filter selector: {sel}\n"));
        }
        out.push_str("\n");

        let filter_sel = selector_filter.as_ref()
            .map(|s| browser::css_parser::parse_selectors(s));

        fn dump_box(
            bx: &browser::layout::LayoutBox,
            depth: usize,
            out: &mut String,
            style_map: &browser::cascade::StyleMap,
            filter: Option<&Vec<browser::css_parser::Selector>>,
            _stylesheets: &[browser::css_parser::Stylesheet],
        ) {
            let indent = "  ".repeat(depth);
            let tag = bx.tag.as_deref().unwrap_or("(text)");
            let id = bx.node.as_ref().and_then(|n| n.attr("id")).unwrap_or_default();
            let class = bx.node.as_ref().and_then(|n| n.attr("class")).unwrap_or_default();
            let match_filter = if let Some(sels) = filter {
                if let Some(node) = &bx.node {
                    sels.iter().any(|s| browser::cascade::matches_selector(node, s))
                } else { false }
            } else { true };

            if match_filter {
                out.push_str(&format!(
                    "{indent}[{tag}#{id} .{class} rect=({:.0},{:.0},{:.0}x{:.0})]\n",
                    bx.rect.x, bx.rect.y, bx.rect.width, bx.rect.height
                ));
                // Matched CSS rules: selektor + zda matchne.
                if let Some(node) = &bx.node {
                    out.push_str(&format!("{indent}  --- matched rules ---\n"));
                    let mut matched_count = 0;
                    for sheet in _stylesheets {
                        for rule in &sheet.rules {
                            for sel in &rule.selectors {
                                if browser::cascade::matches_selector(node, sel) {
                                    matched_count += 1;
                                    out.push_str(&format!("{indent}    {} {{\n", sel));
                                    for d in &rule.declarations {
                                        let imp = if d.important { " !important" } else { "" };
                                        out.push_str(&format!("{indent}      {}: {}{imp};\n",
                                            d.property, d.value));
                                    }
                                    out.push_str(&format!("{indent}    }}\n"));
                                    break; // jeden match na rule staci
                                }
                            }
                        }
                    }
                    out.push_str(&format!("{indent}  --- {matched_count} rules matched ---\n"));
                    out.push_str(&format!("{indent}  --- computed styles ---\n"));
                    let styles = browser::cascade::get_styles(style_map, node);
                    if let Some(s) = styles {
                        let mut keys: Vec<&String> = s.keys().collect();
                        keys.sort();
                        for k in keys {
                            let v = &s[k];
                            out.push_str(&format!("{indent}    {k}: {v}\n"));
                        }
                    }
                }
                // LayoutBox derived state.
                out.push_str(&format!("{indent}  --- LayoutBox state ---\n"));
                out.push_str(&format!("{indent}    display: {:?}\n", bx.display));
                out.push_str(&format!("{indent}    position: {:?}\n", bx.position));
                out.push_str(&format!("{indent}    flex_direction: {:?}\n", bx.flex_direction));
                out.push_str(&format!("{indent}    justify_content: {:?}\n", bx.justify_content));
                out.push_str(&format!("{indent}    align_items: {:?}\n", bx.align_items));
                out.push_str(&format!("{indent}    width_pct: {:?}\n", bx.width_pct));
                out.push_str(&format!("{indent}    height_pct: {:?}\n", bx.height_pct));
                out.push_str(&format!("{indent}    explicit_w/h: {:?}/{:?}\n",
                    bx.explicit_width, bx.explicit_height));
                if bx.bold { out.push_str(&format!("{indent}    bold: true\n")); }
                out.push_str(&format!("{indent}    font-size: {}\n", bx.font_size));
            }
            for child in &bx.children {
                dump_box(child, depth + 1, out, style_map, filter, _stylesheets);
            }
        }
        dump_box(&mut layout_root, 0, &mut out, &style_map, filter_sel.as_ref(), &stylesheets);
        let _ = layout_root; // suppres unused mut warning

        if let Err(e) = std::fs::write(&out_path, &out) {
            eprintln!("Nelze zapsat {out_path}: {e}");
            return;
        }
        println!("[dump] {out_path} ({} bytes, {} rules)", out.len(), total_rules);
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

    // 1. Tokenizace
    let lexer = match Lexer::parse_str(source, "<inline>") {
        Ok(l) => l,
        Err(e) => { eprintln!("Chyba lexeru: {e}"); return; }
    };

    println!("=== TOKENY ===");
    Lexer::debug_print_tokens(lexer.tokens.clone());
    println!();

    // 2. Parsovani
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
    println!("Program s {} prikazy\n", program.body.len());

    // 3. Interpretace
    println!("=== VYSTUP ===");
    let mut interp = Interpreter::new();
    if let Err(e) = interp.run(&program) {
        eprintln!("Chyba pri behu: {e}");
    }
}
