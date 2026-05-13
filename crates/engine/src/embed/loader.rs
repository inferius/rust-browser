//! Page resource loader - sdilene fns pro fetch HTML + CSS + relative resolve.
//! Pouziva `WebView::load_url` + lib.rs CLI dispatcher.
//!
//! Tyto fns byly puvodne v lib.rs jako private helpers; presunute do
//! pub modulu aby je WebView mohl primo volat bez duplikace.

use std::path::PathBuf;

use crate::browser::{self, css_parser, render};

/// Resolve `@import "url";` v CSS retezci. Fetchne externi CSS pres ureq
/// + recursivne resolve nested imports (max depth 5).
pub fn resolve_css_imports(css: &str, base_url: &str, depth: u32) -> String {
    if depth > 5 { return css.to_string(); }
    let mut out = String::with_capacity(css.len());
    let mut bytes = css.bytes().peekable();
    let mut buf = String::new();
    while let Some(b) = bytes.next() {
        buf.push(b as char);
        if buf.ends_with("@import") {
            buf.truncate(buf.len() - 7);
            out.push_str(&buf);
            buf.clear();
            let mut rest = String::new();
            while let Some(b) = bytes.next() {
                if b == b';' { break; }
                rest.push(b as char);
            }
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
                let resolved = render::resolve_url(base_url, &url_part);
                println!("[fetch @import] {resolved}");
                if let Some(c) = render::fetch_text_url(&resolved) {
                    let nested = resolve_css_imports(&c, &resolved, depth + 1);
                    out.push('\n');
                    out.push_str(&nested);
                }
            }
        }
    }
    out.push_str(&buf);
    out
}

/// Vsechny `<link rel="stylesheet" href="...">` hrefs z HTML.
pub fn extract_stylesheet_hrefs(html: &str) -> Vec<String> {
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

/// Vsechny inline `<style> ... </style>` blocky.
pub fn extract_inline_styles(html: &str) -> Vec<String> {
    let document = browser::html_parser::parse_html(html, "about:blank");
    document.root.get_elements_by_tag("style")
        .iter().map(|s| s.text_content()).collect()
}

/// Vysledek loadu - HTML + agregovany CSS + base URL + lokalni file path
/// (pokud file://).
pub struct LoadedPage {
    pub html: String,
    pub css: String,
    pub base_url: Option<String>,
    pub local_path: Option<PathBuf>,
}

/// Nacti page z URL nebo filesystem path. http(s):// jde pres ureq, jine
/// se ctou z disku jako lokalni soubor. CSS sbira z `<link rel=stylesheet>`,
/// `<style>`, co-located `.css` (pro file:// rezim).
pub fn load_page(target: &str) -> Option<LoadedPage> {
    let is_url = target.starts_with("http://") || target.starts_with("https://");
    if is_url {
        println!("[fetch] {target}");
        let html = render::fetch_text_url(target)?;
        let mut css = String::new();
        for href in extract_stylesheet_hrefs(&html) {
            let resolved = render::resolve_url(target, &href);
            if let Some(c) = render::fetch_text_url(&resolved) {
                let imported = resolve_css_imports(&c, &resolved, 0);
                let chars = imported.len();
                let rules = css_parser::parse_stylesheet(&imported).rules.len();
                println!("[fetch css] {resolved} ({chars} chars, {rules} rules)");
                css.push('\n');
                css.push_str(&imported);
            } else {
                println!("[fetch css FAIL] {resolved}");
            }
        }
        for (idx, inline) in extract_inline_styles(&html).into_iter().enumerate() {
            let resolved = resolve_css_imports(&inline, target, 0);
            let rules = css_parser::parse_stylesheet(&resolved).rules.len();
            println!("[inline style #{idx}] {} chars, {rules} rules", resolved.len());
            css.push('\n');
            css.push_str(&resolved);
        }
        Some(LoadedPage {
            html,
            css,
            base_url: Some(target.to_string()),
            local_path: None,
        })
    } else {
        let html = std::fs::read_to_string(target).ok()?;
        let path_buf = PathBuf::from(target);
        let abs_path = std::fs::canonicalize(&path_buf).unwrap_or(path_buf.clone());
        let base = format!("file:///{}", abs_path.display().to_string().replace('\\', "/"));
        let mut css = String::new();
        let css_path = target.replace(".html", ".css");
        if let Ok(c) = std::fs::read_to_string(&css_path) {
            css.push('\n');
            css.push_str(&c);
        }
        let html_dir = path_buf.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        for href in extract_stylesheet_hrefs(&html) {
            if href.starts_with("http://") || href.starts_with("https://") {
                if let Some(c) = render::fetch_text_url(&href) {
                    css.push('\n');
                    css.push_str(&c);
                }
            } else {
                let css_file = html_dir.join(&href);
                if let Ok(c) = std::fs::read_to_string(&css_file) {
                    css.push('\n');
                    css.push_str(&c);
                }
            }
        }
        for inline in extract_inline_styles(&html) {
            css.push('\n');
            css.push_str(&inline);
        }
        Some(LoadedPage {
            html,
            css,
            base_url: Some(base),
            local_path: Some(abs_path),
        })
    }
}
