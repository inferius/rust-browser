//! Diagnostic test: parsuje engine-test.html, reportuje pouzite CSS props +
//! features + neimplementovane vlastnosti. Spusti
//! `cargo test engine_test_html_diagnostic -- --ignored --nocapture` pro report.

use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade};

/// Properties ktere parser akceptuje ale layout/render je IGNORUJE (no-op).
/// Detect pres cargo grep - kdyz prop neni v match arm v layout::compute_box,
/// je to "parser-only".
const UNIMPLEMENTED_PROPS: &[&str] = &[
    "column-count",       // Multi-column Layout L1 - rozdeleni textu do N sloupcu
    "columns",            // shorthand
    "column-rule",        // separator mezi sloupci
    "column-rule-color",
    "column-rule-style",
    "column-rule-width",
    "column-span",
    "column-fill",
    "column-width",
    "break-inside",       // pro multi-col / printer
    "break-before",
    "break-after",
    "page-break-inside",
    "page-break-before",
    "page-break-after",
    "writing-mode",       // vertical-rl / vertical-lr / sideways - jen partially
    "direction",          // ltr/rtl - text shaping
    "unicode-bidi",
    "hyphens",            // word break / hyphenation
    "text-decoration-skip-ink",
    "font-variant-caps",  // small-caps font features
    "font-variant-east-asian",
    "font-variant-ligatures",
    "font-feature-settings", // OpenType features
    "font-variation-settings", // variable font axes (mam variable_fonts ale neni full)
    "tab-size",
    "white-space-collapse",
    "text-wrap",          // pretty/balance/stable
    "text-emphasis",
    "ruby-position",
    "scroll-margin",
    "scroll-padding",
    "overscroll-behavior-y",  // OK partially; behavior-x/y separate
    "shape-image-threshold",
    "shape-margin",
    "scrollbar-color",    // jen track barva
    "caret-color",
    "appearance",         // form control native styling
    "-webkit-appearance",
    "color-scheme",       // jen storage; ne real switch UA stylu
    "list-style-image",
];

#[test]
#[ignore] // Run manually pro report.
fn engine_test_html_diagnostic() {
    let path = "static/engine-test.html";
    let html = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => { println!("Skip: {} - {}", path, e); return; }
    };
    let doc = parse_html(&html, "");
    let styles = doc.root.get_elements_by_tag("style");
    let css: String = styles.iter().map(|s| s.text_content()).collect::<Vec<_>>().join("\n");
    let sheet = parse_stylesheet(&css);

    println!("\n=== ENGINE-TEST.HTML DIAGNOSTIC ===");
    println!("Rules count: {}", sheet.rules.len());
    println!("Keyframes: {}", sheet.keyframes.len());
    println!("Media queries: {}", sheet.media_queries.len());
    println!("Container queries: {}", sheet.container_queries.len());
    println!("Font-faces: {}", sheet.font_faces.len());

    use std::collections::BTreeMap;
    let mut prop_counts: BTreeMap<String, usize> = BTreeMap::new();
    for rule in &sheet.rules {
        for d in &rule.declarations {
            *prop_counts.entry(d.property.clone()).or_insert(0) += 1;
        }
    }
    println!("\n=== UNIQUE PROPS ({}) ===", prop_counts.len());
    for (p, c) in &prop_counts {
        let mark = if UNIMPLEMENTED_PROPS.contains(&p.as_str()) { " [NEPODPOROVANO]" } else { "" };
        println!("  {:>4} x {}{}", c, p, mark);
    }
    let used_unimpl: Vec<_> = prop_counts.iter()
        .filter(|(p, _)| UNIMPLEMENTED_PROPS.contains(&p.as_str()))
        .collect();
    println!("\n=== NEPODPOROVANE PROPS V TETO STRANCE ({}) ===", used_unimpl.len());
    for (p, c) in &used_unimpl {
        println!("  {:>4} x {}", c, p);
    }

    // Cascade na body.
    let body_nodes = doc.root.get_elements_by_tag("body");
    if let Some(body) = body_nodes.first() {
        let style_map = cascade::cascade(&doc.root, &[sheet]);
        let body_id = std::rc::Rc::as_ptr(body) as usize;
        println!("\n=== BODY COMPUTED STYLES ===");
        if let Some(decls) = style_map.get(&body_id) {
            let mut keys: Vec<_> = decls.keys().collect();
            keys.sort();
            for k in keys.iter().take(50) {
                println!("  {}: {}", k, decls.get(*k).cloned().unwrap_or_default());
            }
        } else {
            println!("(body neni v style map)");
        }
        println!("Style map size: {}", style_map.len());
    }
}
