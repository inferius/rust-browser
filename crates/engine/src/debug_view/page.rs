/// HTML page wrapper - embedded CSS+JS, layout pro debug viewer.

use super::html_escape;

pub fn wrap_page(title: &str, source: &str, tokens_html: &str, ast_html: &str) -> String {
    format!(r#"<!DOCTYPE html>
<html lang="cs">
<head>
<meta charset="UTF-8">
<title>{title} - Rust Web Engine Debug</title>
<style>{css}</style>
</head>
<body>
<header>
  <h1>Rust Web Engine - Debug Viewer</h1>
  <h2>{title}</h2>
  <nav>
    <button onclick="showPanel('source')" class="tab active">Source</button>
    <button onclick="showPanel('tokens')" class="tab">Tokens</button>
    <button onclick="showPanel('ast')" class="tab">AST</button>
  </nav>
</header>

<main>
  <section id="panel-source" class="panel active">
    <h3>Zdrojovy kod</h3>
    <pre class="source">{source_escaped}</pre>
  </section>

  <section id="panel-tokens" class="panel">
    <h3>Tokeny (najedte mysi pro detail)</h3>
    <div class="legend">
      <span class="token tk-keyword">keyword</span>
      <span class="token tk-ident">identifier</span>
      <span class="token tk-number">number</span>
      <span class="token tk-string">string</span>
      <span class="token tk-regex">regex</span>
      <span class="token tk-template">template</span>
      <span class="token tk-operator">operator</span>
      <span class="token tk-comment">comment</span>
      <span class="token tk-whitespace">·1·</span>
      <span class="token tk-newline">↵</span>
    </div>
    {tokens_html}
  </section>

  <section id="panel-ast" class="panel">
    <h3>AST (klikni pro rozbaleni)</h3>
    <div class="ast-controls">
      <button onclick="expandAll()">Expand all</button>
      <button onclick="collapseAll()">Collapse all</button>
    </div>
    {ast_html}
  </section>
</main>

<div id="tooltip" class="tooltip"></div>

<script>{js}</script>
</body>
</html>"#,
        title = html_escape(title),
        source_escaped = html_escape(source),
        tokens_html = tokens_html,
        ast_html = ast_html,
        css = CSS,
        js = JS,
    )
}

const CSS: &str = r#"
* { box-sizing: border-box; }
body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    margin: 0;
    background: #1e1e2e;
    color: #cdd6f4;
}
header {
    background: #181825;
    padding: 16px 24px;
    border-bottom: 2px solid #313244;
}
h1 { margin: 0 0 4px; color: #89b4fa; font-size: 20px; }
h2 { margin: 0 0 12px; color: #94a3b8; font-size: 14px; font-weight: normal; }
h3 { color: #f9e2af; }
nav { display: flex; gap: 8px; }
.tab {
    background: #313244;
    color: #cdd6f4;
    border: none;
    padding: 8px 16px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 14px;
}
.tab:hover { background: #45475a; }
.tab.active { background: #89b4fa; color: #1e1e2e; }
main { padding: 24px; max-width: 1400px; margin: 0 auto; }
.panel { display: none; }
.panel.active { display: block; }
.source {
    background: #181825;
    padding: 16px;
    border-radius: 6px;
    border: 1px solid #313244;
    overflow-x: auto;
    color: #cdd6f4;
    font-family: "Cascadia Code", "Fira Code", Consolas, monospace;
    line-height: 1.5;
}

/* Tokens */
.tokens {
    background: #181825;
    padding: 16px;
    border-radius: 6px;
    border: 1px solid #313244;
    line-height: 2.4;
}
.token {
    display: inline-block;
    padding: 2px 6px;
    margin: 2px;
    border-radius: 3px;
    font-family: "Cascadia Code", Consolas, monospace;
    font-size: 13px;
    cursor: help;
    border: 1px solid transparent;
    transition: transform 0.1s;
    white-space: pre;
}
.token:hover {
    transform: scale(1.1);
    border-color: #f9e2af;
    z-index: 10;
    position: relative;
}
.tk-keyword    { background: #f38ba8; color: #1e1e2e; }
.tk-ident      { background: #cdd6f4; color: #1e1e2e; }
.tk-number     { background: #fab387; color: #1e1e2e; }
.tk-string     { background: #a6e3a1; color: #1e1e2e; }
.tk-regex      { background: #cba6f7; color: #1e1e2e; }
.tk-template   { background: #a6e3a1; color: #1e1e2e; font-style: italic; }
.tk-operator   { background: #f9e2af; color: #1e1e2e; }
.tk-comment    { background: #6c7086; color: #cdd6f4; font-style: italic; }
.tk-whitespace { background: #313244; color: #6c7086; font-size: 11px; }
.tk-newline    { background: #313244; color: #74c7ec; }
.tk-eof        { background: #45475a; color: #cdd6f4; }

.legend {
    background: #181825;
    padding: 8px 12px;
    border-radius: 4px;
    margin-bottom: 12px;
    border: 1px solid #313244;
}

/* Stats */
.stats {
    margin-top: 24px;
    background: #181825;
    padding: 16px;
    border-radius: 6px;
    border: 1px solid #313244;
}
.stats h3 { margin-top: 0; }
.stats table { border-collapse: collapse; min-width: 300px; }
.stats th, .stats td {
    padding: 6px 12px;
    border-bottom: 1px solid #313244;
    text-align: left;
}
.stats th { background: #313244; }

/* AST */
.ast-tree {
    background: #181825;
    padding: 16px;
    border-radius: 6px;
    border: 1px solid #313244;
    font-family: "Cascadia Code", Consolas, monospace;
    font-size: 13px;
}
.ast-tree details { margin-left: 0; }
.ast-tree summary {
    cursor: pointer;
    padding: 3px 6px;
    border-radius: 3px;
    user-select: none;
    list-style: none;
}
.ast-tree summary::before {
    content: "▶ ";
    display: inline-block;
    transition: transform 0.15s;
    color: #6c7086;
}
.ast-tree details[open] > summary::before {
    transform: rotate(90deg);
}
.ast-tree summary:hover { background: #313244; }
.ast-children {
    margin-left: 20px;
    border-left: 1px dashed #45475a;
    padding-left: 8px;
}
.ast-leaf {
    padding: 3px 6px;
    margin: 2px 0;
    color: #94a3b8;
}
.ast-stmt    { color: #f38ba8; }
.ast-expr    { color: #89b4fa; }
.ast-literal { color: #fab387; }
.ast-ident   { color: #a6e3a1; }
.ast-decl    { color: #cba6f7; }
.ast-field   { color: #74c7ec; }
.ast-root    { color: #f9e2af; font-weight: bold; }
.ast-controls { margin-bottom: 12px; }
.ast-controls button {
    background: #313244;
    color: #cdd6f4;
    border: none;
    padding: 6px 12px;
    margin-right: 8px;
    border-radius: 4px;
    cursor: pointer;
}
.ast-controls button:hover { background: #45475a; }

/* Tooltip */
.tooltip {
    position: fixed;
    background: #11111b;
    color: #cdd6f4;
    padding: 8px 12px;
    border: 1px solid #f9e2af;
    border-radius: 4px;
    font-family: "Cascadia Code", Consolas, monospace;
    font-size: 12px;
    pointer-events: none;
    white-space: pre;
    z-index: 1000;
    display: none;
    max-width: 500px;
    box-shadow: 0 4px 12px rgba(0,0,0,0.4);
}

.error { color: #f38ba8; padding: 12px; background: #181825; border-radius: 4px; }
.muted { color: #6c7086; padding: 12px; }
"#;

const JS: &str = r#"
// Tab switching
function showPanel(name) {
    document.querySelectorAll('.panel').forEach(p => p.classList.remove('active'));
    document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
    const panel = document.getElementById('panel-' + name);
    if (panel) panel.classList.add('active');
    event.target.classList.add('active');
}

// Token tooltip
const tooltip = document.getElementById('tooltip');
document.addEventListener('mouseover', (e) => {
    const tk = e.target.closest('.token');
    if (tk && tk.dataset.tip) {
        tooltip.textContent = tk.dataset.tip;
        tooltip.style.display = 'block';
    }
});
document.addEventListener('mousemove', (e) => {
    if (tooltip.style.display === 'block') {
        const x = e.clientX + 12;
        const y = e.clientY + 12;
        tooltip.style.left = x + 'px';
        tooltip.style.top  = y + 'px';
    }
});
document.addEventListener('mouseout', (e) => {
    if (e.target.closest('.token')) {
        tooltip.style.display = 'none';
    }
});

// AST expand/collapse all
function expandAll() {
    document.querySelectorAll('.ast-tree details').forEach(d => d.open = true);
}
function collapseAll() {
    document.querySelectorAll('.ast-tree details').forEach(d => d.open = false);
    // Krome rootu
    const root = document.querySelector('.ast-tree > details');
    if (root) root.open = true;
}
"#;
