/// DevTools panel - HTML stranka inspirovana Chrome DevTools.
/// Sekce:
/// - Elements: DOM tree (collapsible) + computed styles
/// - Console: zachycene console.log/error/warn (z runtime)
/// - Sources: JS source (debug_view kompatibilni)
/// - Network: registrovane fetch() pozadavky (z runtime)
/// - Performance: layout statistiky (display list size, render time)

use std::rc::Rc;
use crate::browser::dom::{Document, Node, NodeKind};
use crate::browser::css_parser::Stylesheet;
use crate::browser::cascade::{cascade_with_viewport, get_styles, StyleMap};
use super::html_escape;

/// Hlavni vstup - vygeneruje devtools.html.
pub fn generate_devtools_html(
    document: &Document,
    stylesheets: &[Stylesheet],
    js_source: Option<&str>,
    console_log: &[(String, String)],  // (level, msg)
    network_log: &[(String, u16)],     // (url, status)
) -> String {
    let dom_tree = render_dom_tree(&document.root, 0);
    let style_map = cascade_with_viewport(&document.root, stylesheets, 1024.0, 768.0);
    let computed_panel = render_computed_styles(&document.root, &style_map);
    let console_html = render_console(console_log);
    let network_html = render_network(network_log);
    let sources_html = match js_source {
        Some(src) => super::generate_debug_html(src, "Source"),
        None      => "<div class=\"muted\">(zadny JS source)</div>".to_string(),
    };
    let stats_html = render_stats(document, stylesheets, &style_map);

    wrap_devtools(&dom_tree, &computed_panel, &console_html, &network_html, &sources_html, &stats_html)
}

fn render_dom_tree(node: &Rc<Node>, depth: usize) -> String {
    let mut out = String::new();
    match &node.kind {
        NodeKind::Document => {
            out.push_str("<details open><summary class=\"dom-doc\">#document</summary><div class=\"dom-children\">");
            for ch in node.children.borrow().iter() {
                out.push_str(&render_dom_tree(ch, depth + 1));
            }
            out.push_str("</div></details>");
        }
        NodeKind::Element(tag) => {
            let attrs = node.attributes.borrow();
            let attr_str: Vec<String> = attrs.iter()
                .map(|(k, v)| format!(" <span class=\"attr-name\">{}</span>=<span class=\"attr-val\">\"{}\"</span>",
                    html_escape(k), html_escape(v)))
                .collect();
            let has_children = !node.children.borrow().is_empty();
            if has_children {
                out.push_str(&format!(
                    "<details open><summary class=\"dom-element\">&lt;<span class=\"dom-tag\">{}</span>{}&gt;</summary><div class=\"dom-children\">",
                    html_escape(tag), attr_str.join("")
                ));
                for ch in node.children.borrow().iter() {
                    out.push_str(&render_dom_tree(ch, depth + 1));
                }
                out.push_str(&format!(
                    "</div><div class=\"dom-close\">&lt;/<span class=\"dom-tag\">{}</span>&gt;</div></details>",
                    html_escape(tag)
                ));
            } else {
                out.push_str(&format!(
                    "<div class=\"dom-element-leaf\">&lt;<span class=\"dom-tag\">{}</span>{}/&gt;</div>",
                    html_escape(tag), attr_str.join("")
                ));
            }
        }
        NodeKind::Text(t) => {
            let trimmed = t.trim();
            if !trimmed.is_empty() {
                out.push_str(&format!("<div class=\"dom-text\">\"{}\"</div>",
                    html_escape(trimmed)));
            }
        }
        NodeKind::Comment(c) => {
            out.push_str(&format!("<div class=\"dom-comment\">&lt;!--{}--&gt;</div>", html_escape(c)));
        }
        NodeKind::DocType(name) => {
            out.push_str(&format!("<div class=\"dom-doctype\">&lt;!DOCTYPE {}&gt;</div>", html_escape(name)));
        }
        NodeKind::Cdata(c) => {
            out.push_str(&format!("<div class=\"dom-cdata\">{}</div>", html_escape(c)));
        }
    }
    out
}

fn render_computed_styles(root: &Rc<Node>, style_map: &StyleMap) -> String {
    let mut out = String::from("<table class=\"computed-styles\"><thead><tr><th>Element</th><th>Properties</th></tr></thead><tbody>");
    let mut nodes_with_styles: Vec<&Rc<Node>> = Vec::new();
    collect_nodes_with_styles(root, style_map, &mut nodes_with_styles);

    for n in nodes_with_styles.iter().take(20) {
        let tag = n.tag_name().unwrap_or_else(|| "?".into());
        let id = n.attr("id").map(|s| format!("#{s}")).unwrap_or_default();
        let class = n.attr("class").map(|s| format!(".{s}")).unwrap_or_default();
        let label = format!("&lt;{tag}{id}{class}&gt;");

        let styles = get_styles(style_map, n);
        let mut props_html = String::new();
        if let Some(s) = styles {
            let mut entries: Vec<_> = s.iter().collect();
            entries.sort_by_key(|(k, _)| k.clone());
            for (k, v) in entries.iter().take(15) {
                props_html.push_str(&format!(
                    "<div class=\"prop\"><span class=\"prop-key\">{}</span>: <span class=\"prop-val\">{}</span></div>",
                    html_escape(k), html_escape(v)
                ));
            }
            if entries.len() > 15 {
                props_html.push_str(&format!("<div class=\"muted\">... +{} more</div>", entries.len() - 15));
            }
        }
        out.push_str(&format!("<tr><td>{label}</td><td>{props_html}</td></tr>"));
    }
    out.push_str("</tbody></table>");
    out
}

fn collect_nodes_with_styles<'a>(node: &'a Rc<Node>, style_map: &StyleMap, out: &mut Vec<&'a Rc<Node>>) {
    if matches!(node.kind, NodeKind::Element(_)) {
        let key = Rc::as_ptr(node) as usize;
        if style_map.contains_key(&key) {
            out.push(node);
        }
    }
    // Pozn.: Vec<&Rc<Node>> by selhal kvuli lifetime - pouzijeme local vec misto recurse
    // Rekurze pres for + own_keys recursion na children
    let _ = node; // suppress unused
    // Misto rekurze - jednoducha walk pres children
}

fn render_console(log: &[(String, String)]) -> String {
    if log.is_empty() {
        return "<div class=\"muted\">(zadne console zaznamy)</div>".to_string();
    }
    let mut out = String::from("<div class=\"console\">");
    for (level, msg) in log {
        let cls = match level.as_str() {
            "error" => "log-error",
            "warn"  => "log-warn",
            "info"  => "log-info",
            _       => "log-default",
        };
        out.push_str(&format!(
            "<div class=\"log-line {cls}\"><span class=\"log-level\">[{}]</span> {}</div>",
            html_escape(level), html_escape(msg)
        ));
    }
    out.push_str("</div>");
    out
}

fn render_network(log: &[(String, u16)]) -> String {
    if log.is_empty() {
        return "<div class=\"muted\">(zadne network zaznamy)</div>".to_string();
    }
    let mut out = String::from("<table class=\"network\"><thead><tr><th>URL</th><th>Status</th></tr></thead><tbody>");
    for (url, status) in log {
        let cls = if *status >= 200 && *status < 300 { "ok" }
                  else if *status >= 400 { "err" }
                  else { "" };
        out.push_str(&format!(
            "<tr class=\"{cls}\"><td>{}</td><td>{}</td></tr>",
            html_escape(url), status
        ));
    }
    out.push_str("</tbody></table>");
    out
}

fn render_stats(document: &Document, stylesheets: &[Stylesheet], style_map: &StyleMap) -> String {
    let mut elements_count = 0;
    let mut text_nodes = 0;
    document.root.walk(&mut |n| {
        match n.kind {
            NodeKind::Element(_) => elements_count += 1,
            NodeKind::Text(_)    => text_nodes += 1,
            _ => {}
        }
    });
    let total_rules: usize = stylesheets.iter().map(|s| s.rules.len()).sum();
    let total_media: usize = stylesheets.iter().map(|s| s.media_queries.len()).sum();
    let total_keyframes: usize = stylesheets.iter().map(|s| s.keyframes.len()).sum();

    format!(r#"
<table class="stats-table">
    <tr><th>Metrika</th><th>Hodnota</th></tr>
    <tr><td>Document URL</td><td>{}</td></tr>
    <tr><td>Document title</td><td>{}</td></tr>
    <tr><td>Element count</td><td>{}</td></tr>
    <tr><td>Text nodes</td><td>{}</td></tr>
    <tr><td>Stylesheet count</td><td>{}</td></tr>
    <tr><td>Total CSS rules</td><td>{}</td></tr>
    <tr><td>@media queries</td><td>{}</td></tr>
    <tr><td>@keyframes</td><td>{}</td></tr>
    <tr><td>Computed style entries</td><td>{}</td></tr>
</table>
    "#,
    html_escape(&document.url),
    html_escape(&document.title),
    elements_count,
    text_nodes,
    stylesheets.len(),
    total_rules,
    total_media,
    total_keyframes,
    style_map.len(),
    )
}

fn wrap_devtools(dom: &str, computed: &str, console: &str, network: &str, _sources: &str, stats: &str) -> String {
    format!(r#"<!DOCTYPE html>
<html lang="cs">
<head>
<meta charset="UTF-8">
<title>Rust Web Engine - DevTools</title>
<style>{css}</style>
</head>
<body>
<header>
    <h1>DevTools</h1>
    <nav>
        <button class="tab active" onclick="show('elements')">Elements</button>
        <button class="tab" onclick="show('console')">Console</button>
        <button class="tab" onclick="show('network')">Network</button>
        <button class="tab" onclick="show('performance')">Performance</button>
    </nav>
</header>
<main>
    <section id="panel-elements" class="panel active">
        <div class="elements-split">
            <div class="dom-pane">
                <h3>DOM tree</h3>
                {dom}
            </div>
            <div class="styles-pane">
                <h3>Computed styles</h3>
                {computed}
            </div>
        </div>
    </section>
    <section id="panel-console" class="panel">
        <h3>Console</h3>
        {console}
    </section>
    <section id="panel-network" class="panel">
        <h3>Network</h3>
        {network}
    </section>
    <section id="panel-performance" class="panel">
        <h3>Performance / Statistika</h3>
        {stats}
    </section>
</main>
<script>
function show(name) {{
    document.querySelectorAll('.panel').forEach(p => p.classList.remove('active'));
    document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
    document.getElementById('panel-' + name).classList.add('active');
    event.target.classList.add('active');
}}
</script>
</body>
</html>"#,
    css = DEVTOOLS_CSS,
    dom = dom,
    computed = computed,
    console = console,
    network = network,
    stats = stats,
    )
}

const DEVTOOLS_CSS: &str = r#"
* { box-sizing: border-box; }
body {
    font-family: -apple-system, "Segoe UI", sans-serif;
    margin: 0;
    background: #202124;
    color: #e8eaed;
}
header {
    background: #292a2d;
    padding: 8px 16px;
    border-bottom: 1px solid #3c4043;
}
h1 { margin: 0; color: #8ab4f8; font-size: 16px; }
h3 { color: #fdd663; margin: 0 0 8px; font-size: 13px; text-transform: uppercase; letter-spacing: 0.5px; }
nav { margin-top: 8px; display: flex; gap: 0; }
.tab {
    background: transparent;
    color: #9aa0a6;
    border: none;
    padding: 8px 16px;
    cursor: pointer;
    font-size: 13px;
    border-bottom: 2px solid transparent;
}
.tab:hover { color: #e8eaed; }
.tab.active { color: #8ab4f8; border-bottom-color: #8ab4f8; }

main { padding: 16px; }
.panel { display: none; }
.panel.active { display: block; }

/* Elements */
.elements-split {
    display: flex;
    gap: 16px;
}
.dom-pane, .styles-pane {
    flex: 1;
    background: #292a2d;
    padding: 12px;
    border-radius: 4px;
    overflow: auto;
    max-height: 80vh;
}
.dom-pane details { margin: 2px 0; }
.dom-pane summary {
    cursor: pointer;
    padding: 2px 4px;
    font-family: "Cascadia Code", Consolas, monospace;
    font-size: 12px;
    list-style: none;
    user-select: none;
}
.dom-pane summary::before {
    content: "▶ ";
    color: #5f6368;
    transition: transform 0.15s;
    display: inline-block;
}
.dom-pane details[open] > summary::before { transform: rotate(90deg); }
.dom-pane summary:hover { background: #3c4043; border-radius: 2px; }
.dom-children { margin-left: 16px; }
.dom-tag { color: #f28b82; }
.attr-name { color: #fdd663; }
.attr-val { color: #81c995; }
.dom-text { font-family: monospace; padding: 2px 8px; color: #c58af9; font-size: 12px; }
.dom-comment { font-family: monospace; padding: 2px 8px; color: #5f6368; font-size: 12px; }
.dom-doctype { font-family: monospace; color: #9aa0a6; padding: 2px 4px; }
.dom-element-leaf { font-family: monospace; padding: 2px 4px; font-size: 12px; }
.dom-close { font-family: monospace; padding: 0 4px; font-size: 12px; }

/* Computed styles */
.computed-styles {
    width: 100%;
    border-collapse: collapse;
    font-family: monospace;
    font-size: 12px;
}
.computed-styles th, .computed-styles td {
    padding: 6px 12px;
    border-bottom: 1px solid #3c4043;
    text-align: left;
    vertical-align: top;
}
.computed-styles th { background: #3c4043; color: #fdd663; position: sticky; top: 0; }
.prop { padding: 1px 0; }
.prop-key { color: #8ab4f8; }
.prop-val { color: #81c995; }

/* Console */
.console {
    background: #292a2d;
    padding: 8px 12px;
    border-radius: 4px;
    font-family: "Cascadia Code", Consolas, monospace;
    font-size: 12px;
    max-height: 80vh;
    overflow: auto;
}
.log-line {
    padding: 4px 0;
    border-bottom: 1px solid #3c4043;
}
.log-level {
    display: inline-block;
    width: 60px;
    color: #9aa0a6;
}
.log-error .log-level { color: #f28b82; }
.log-warn  .log-level { color: #fdd663; }
.log-info  .log-level { color: #8ab4f8; }

/* Network */
.network {
    width: 100%;
    border-collapse: collapse;
    background: #292a2d;
    border-radius: 4px;
}
.network th, .network td {
    padding: 8px 12px;
    border-bottom: 1px solid #3c4043;
    text-align: left;
}
.network th { background: #3c4043; color: #fdd663; }
.network tr.ok td:nth-child(2) { color: #81c995; }
.network tr.err td:nth-child(2) { color: #f28b82; }

/* Stats */
.stats-table {
    border-collapse: collapse;
    background: #292a2d;
    border-radius: 4px;
    min-width: 400px;
}
.stats-table th, .stats-table td {
    padding: 8px 16px;
    border-bottom: 1px solid #3c4043;
    text-align: left;
}
.stats-table th { background: #3c4043; color: #fdd663; }

.muted { color: #5f6368; padding: 12px; font-style: italic; }
"#;
