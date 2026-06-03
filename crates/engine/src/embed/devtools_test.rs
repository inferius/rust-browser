//! Devtools standalone test mode.
//!
//! Misto F12 split (page + dev WV), nalovat devtools-mockup primo do main WV
//! s precomputed mock CDP data. Mock data simuluji backend response z
//! `static/test.html` (nebo jine target stranky) -> DOM tree + matched styles
//! per node + computed styles.
//!
//! Vyuziti: rapid iteration na devtools UI bez F12 multi-WV complexity.
//! `cargo run -p rwe-shell -- --devtools static/test.html` ->
//! standalone devtools UI s fixed test page data.
//!
//! Mock CDP wire override: `window.__MOCK_CDP__` map (method,params) -> result.
//! cdp.send checks mock first, falls back na real native (none v standalone).
//!
//! Implementace: parse target HTML/CSS pres exiting modules. Build CDP-shape
//! JSON. Embed pres `<script>window.__MOCK_CDP__ = { ... }</script>` v
//! devtools-mockup template.

use serde_json::{json, Value};
use std::path::PathBuf;
use std::rc::Rc;

use crate::browser::{cascade, css_parser, dom, html_parser};

/// Result z generate_mock_data - mock JSON + base URL + override JS.
pub struct MockData {
    pub mock_json: String,
    pub base_url: String,
    pub override_js: &'static str,
}

/// Build mock CDP data z target page (test.html). Shell wraps pres
/// devtools-frontend INDEX_HTML template.
pub fn generate_mock_data(target_path: &str) -> Option<MockData> {
    // Load target page (html + css).
    let target = std::fs::canonicalize(target_path).ok()?;
    let html_src = std::fs::read_to_string(&target).ok()?;
    let target_dir = target.parent().map(|p| p.to_path_buf()).unwrap_or_default();

    // CSS - co-located + <link>.
    let mut css_src = String::new();
    let mut loaded: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    for href in extract_links(&html_src) {
        let css_file = target_dir.join(&href);
        let canon = std::fs::canonicalize(&css_file).unwrap_or(css_file.clone());
        if loaded.insert(canon) {
            if let Ok(c) = std::fs::read_to_string(&css_file) {
                css_src.push('\n');
                css_src.push_str(&c);
            }
        }
    }
    let co_path = target_path.replace(".html", ".css");
    let co_buf = PathBuf::from(&co_path);
    let co_canon = std::fs::canonicalize(&co_buf).unwrap_or(co_buf.clone());
    if loaded.insert(co_canon) {
        if let Ok(c) = std::fs::read_to_string(&co_path) {
            css_src.push('\n');
            css_src.push_str(&c);
        }
    }

    let target_url = format!("file:///{}", target.display().to_string().replace('\\', "/"));

    // Parse.
    let doc = html_parser::parse_html(&html_src, &target_url);
    let stylesheet = css_parser::parse_stylesheet(&css_src);

    // DOM tree dump pres CDP DOM.getDocument shape.
    let dom_tree = serialize_dom(&doc.root);

    // Matched styles per node_id - pre-compute pres VSE elementy.
    let mut matched_map: serde_json::Map<String, Value> = serde_json::Map::new();
    let mut computed_map: serde_json::Map<String, Value> = serde_json::Map::new();
    let style_map = cascade::cascade(&doc.root, &[stylesheet.clone()]);
    walk_elements(&doc.root, &mut |node| {
        let node_id = Rc::as_ptr(node) as usize as u64;
        let matched = build_matched_rules(node, &stylesheet);
        matched_map.insert(node_id.to_string(), matched);
        let computed = build_computed_style(node, &style_map);
        computed_map.insert(node_id.to_string(), computed);
    });

    // Mock JSON object.
    let mock_data = json!({
        "dom_root": dom_tree,
        "matched_styles": matched_map,
        "computed_styles": computed_map,
        "target_url": target_url,
    });
    let mock_json = serde_json::to_string(&mock_data).unwrap_or_else(|_| "{}".into());

    // base_url pro relative resources (icons, etc).
    let base = target_dir.display().to_string().replace('\\', "/");
    Some(MockData {
        mock_json,
        base_url: format!("file:///{}/", base),
        override_js: MOCK_CDP_OVERRIDE_JS,
    })
}

/// Walk vsechny Element nody (skip text/comment/document).
fn walk_elements<F: FnMut(&Rc<dom::Node>)>(node: &Rc<dom::Node>, f: &mut F) {
    if matches!(node.kind, dom::NodeKind::Element { .. }) {
        f(node);
    }
    for child in node.children.borrow().iter() {
        walk_elements(child, f);
    }
}

/// CDP DOM.getDocument shape - recursive Node tree.
fn serialize_dom(node: &Rc<dom::Node>) -> Value {
    let node_id = Rc::as_ptr(node) as usize as u64;
    let (node_type, node_name, node_value) = match &node.kind {
        dom::NodeKind::Document => (9, "#document".to_string(), Value::Null),
        dom::NodeKind::Element(tag) => (1, tag.to_uppercase(), Value::Null),
        dom::NodeKind::Text(t) => (3, "#text".to_string(), Value::String(t.clone())),
        dom::NodeKind::Comment(t) => (8, "#comment".to_string(), Value::String(t.clone())),
        dom::NodeKind::Cdata(t) => (4, "#cdata-section".to_string(), Value::String(t.clone())),
        dom::NodeKind::DocType(t) => (10, t.clone(), Value::Null),
        dom::NodeKind::DocumentFragment => (11, "#document-fragment".to_string(), Value::Null),
    };
    let attrs = node.attributes.borrow();
    let mut attr_flat: Vec<Value> = Vec::with_capacity(attrs.len() * 2);
    for (k, v) in attrs.iter() {
        attr_flat.push(Value::String(k.clone()));
        attr_flat.push(Value::String(v.clone()));
    }
    let children: Vec<Value> = node.children.borrow().iter()
        .map(serialize_dom).collect();
    let child_count = node.children.borrow().len() as u64;
    json!({
        "node_id": node_id,
        "node_type": node_type,
        "node_name": node_name,
        "node_value": node_value,
        "attributes": attr_flat,
        "children": children,
        "child_node_count": child_count,
    })
}

/// CDP CSS.getMatchedStylesForNode shape.
fn build_matched_rules(node: &Rc<dom::Node>, sheet: &css_parser::Stylesheet) -> Value {
    let mut matched_rules: Vec<Value> = Vec::new();
    let inline_style = node.attr("style").and_then(|s| {
        if s.is_empty() { return None; }
        let mut props = Vec::new();
        for pair in s.split(';') {
            if let Some(idx) = pair.find(':') {
                let name = pair[..idx].trim().to_string();
                let value = pair[idx+1..].trim().trim_end_matches("!important").trim().to_string();
                let important = pair[idx+1..].contains("!important");
                if !name.is_empty() {
                    props.push(json!({
                        "name": name, "value": value,
                        "important": important, "disabled": false,
                    }));
                }
            }
        }
        if props.is_empty() { None } else { Some(json!({ "properties": props })) }
    });
    for rule in &sheet.rules {
        let mut matching: Vec<u32> = Vec::new();
        let mut sel_strs: Vec<String> = Vec::with_capacity(rule.selectors.len());
        for (i, sel) in rule.selectors.iter().enumerate() {
            sel_strs.push(super::devtools_target::format_selector_pub(sel));
            if cascade::matches_selector(node, sel) {
                matching.push(i as u32);
            }
        }
        if matching.is_empty() { continue; }
        let props: Vec<Value> = rule.declarations.iter().map(|d| json!({
            "name": d.property, "value": d.value,
            "important": d.important, "disabled": false,
        })).collect();
        matched_rules.push(json!({
            "rule": {
                "selector_list": sel_strs,
                "style": { "properties": props },
                "origin": "regular",
            },
            "matching_selectors": matching,
        }));
    }
    let mut obj = serde_json::Map::new();
    obj.insert("matched_rules".into(), Value::Array(matched_rules));
    if let Some(ist) = inline_style {
        obj.insert("inline_style".into(), ist);
    }
    Value::Object(obj)
}

/// CDP CSS.getComputedStyleForNode shape.
fn build_computed_style(node: &Rc<dom::Node>, style_map: &cascade::StyleMap) -> Value {
    let entry = cascade::get_styles(style_map, node);
    let mut props: Vec<Value> = Vec::new();
    if let Some(m) = entry {
        let mut keys: Vec<&String> = m.keys().collect();
        keys.sort();
        for k in keys {
            if let Some(v) = m.get(k) {
                props.push(json!({
                    "name": k, "value": v,
                    "important": false, "disabled": false,
                }));
            }
        }
    }
    json!({ "computed_style": props })
}

/// Extract <link rel=stylesheet href=...> hrefs z HTML.
fn extract_links(html: &str) -> Vec<String> {
    let lc = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut cursor = 0;
    while cursor < lc.len() {
        let link_pos = match lc[cursor..].find("<link") {
            Some(p) => cursor + p,
            None => break,
        };
        let end = match lc[link_pos..].find('>') {
            Some(p) => link_pos + p,
            None => break,
        };
        let tag = &html[link_pos..end];
        if tag.to_ascii_lowercase().contains("stylesheet") {
            if let Some(href_pos) = tag.to_ascii_lowercase().find("href=") {
                let after = &tag[href_pos+5..];
                let q = after.chars().next().unwrap_or(' ');
                if q == '"' || q == '\'' {
                    if let Some(end_q) = after[1..].find(q) {
                        out.push(after[1..1+end_q].to_string());
                    }
                }
            }
        }
        cursor = end + 1;
    }
    out
}

/// Mock CDP override JS - replaces window.cdp pres precomputed data lookup.
/// Inserted INSTEAD of cdp.js v standalone mode.
const MOCK_CDP_OVERRIDE_JS: &str = r#"
// Mock CDP - serves precomputed data from window.__MOCK_CDP__.
// Promise resolves instantly (Promise.resolve), no async wire.
(function() {
    var pendingId = 1;
    var eventListeners = new Map();
    function send(method, params) {
        params = params || {};
        var mock = window.__MOCK_CDP__ || {};
        if (method === 'DOM.getDocument') {
            return Promise.resolve({ root: mock.dom_root || {} });
        }
        if (method === 'CSS.getMatchedStylesForNode') {
            var nid = String(params.node_id);
            var data = (mock.matched_styles || {})[nid];
            return Promise.resolve(data || { matched_rules: [] });
        }
        if (method === 'CSS.getComputedStyleForNode') {
            var nid = String(params.node_id);
            var data = (mock.computed_styles || {})[nid];
            return Promise.resolve(data || { computed_style: [] });
        }
        if (method === 'Overlay.highlightNode' || method === 'Overlay.hideHighlight') {
            return Promise.resolve({});
        }
        if (method === 'Page.reload') {
            return Promise.resolve({});
        }
        // Default - empty success.
        return Promise.resolve({});
    }
    var docUpdatedFired = false;
    function on(method, callback) {
        var list = eventListeners.get(method);
        if (!list) { list = []; eventListeners.set(method, list); }
        list.push(callback);
        // Pres prvni registraci DOM.documentUpdated, fire event po malem delay.
        // Mockup wire pres cdp.on('DOM.documentUpdated', renderDomTree) trigger
        // initial DOM tree render. Bez tohoto mockup zustal idle.
        if (method === 'DOM.documentUpdated' && !docUpdatedFired) {
            docUpdatedFired = true;
            setTimeout(function() {
                var ls = eventListeners.get('DOM.documentUpdated');
                if (ls) for (var i = 0; i < ls.length; i++) {
                    try { ls[i]({}); } catch (e) { console.error('mock event: ' + e); }
                }
            }, 30);
        }
    }
    function off(method, callback) {
        var list = eventListeners.get(method);
        if (!list) return;
        var i = list.indexOf(callback);
        if (i >= 0) list.splice(i, 1);
    }
    function pollEvents() {} // no-op pres standalone
    window.cdp = { send: send, on: on, off: off, pollEvents: pollEvents };
})();
"#;
