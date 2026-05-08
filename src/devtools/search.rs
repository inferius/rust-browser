//! Element search - tag / class / id / CSS selector / XPath.
//!
//! Auto-detect mode: prefix `//` -> XPath, `.foo` -> class, `#bar` -> id, jinak
//! tag nebo CSS selector pres selectors crate (try selector parse, fallback tag match).

use std::rc::Rc;
use crate::browser::dom::{NodeData, NodeKind};
use super::SearchMode;

/// Vyhleda matching nodes. Vraci IDs (Rc::as_ptr cast).
pub fn search(
    root: &Rc<NodeData>,
    query: &str,
    mode: SearchMode,
) -> Vec<usize> {
    let q = query.trim();
    if q.is_empty() { return Vec::new(); }
    let detected = match mode {
        SearchMode::Auto => detect_mode(q),
        m => m,
    };
    match detected {
        SearchMode::XPath => xpath_search(root, q),
        SearchMode::Css => css_search(root, q),
        SearchMode::Tag => tag_search(root, q),
        SearchMode::Auto => {
            // Try CSS, fallback tag.
            let css = css_search(root, q);
            if !css.is_empty() { css } else { tag_search(root, q) }
        }
    }
}

fn detect_mode(q: &str) -> SearchMode {
    if q.starts_with("//") || q.starts_with("/") { return SearchMode::XPath; }
    if q.starts_with('.') || q.starts_with('#') || q.contains(' ') || q.contains('>')
        || q.contains('[') || q.contains(':') || q.contains(',')
        || q.contains('.') || q.contains('#') { return SearchMode::Css; }
    SearchMode::Tag
}

fn tag_search(root: &Rc<NodeData>, tag: &str) -> Vec<usize> {
    let lower = tag.to_ascii_lowercase();
    let mut out = Vec::new();
    walk(root, &mut |n| {
        if let NodeKind::Element(t) = &n.kind {
            if t.eq_ignore_ascii_case(&lower) {
                out.push(Rc::as_ptr(n) as usize);
            }
        }
    });
    out
}

fn css_search(root: &Rc<NodeData>, sel: &str) -> Vec<usize> {
    // Simple selector implementation: support `tag`, `.class`, `#id`, `tag.class`,
    // `tag#id`, `[attr]`, `[attr=val]`, descendant `a b`, child `a > b`.
    // Plne selectors crate je heavy - pouzivame lite parser + matcher.
    let parsed = match parse_simple_selector(sel) {
        Some(p) => p,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    walk(root, &mut |n| {
        if matches_compound(n, &parsed) {
            out.push(Rc::as_ptr(n) as usize);
        }
    });
    out
}

#[derive(Debug)]
struct CompoundSelector {
    tag: Option<String>,
    classes: Vec<String>,
    id: Option<String>,
    attrs: Vec<(String, Option<String>)>,
    /// Ancestor chain (foo bar) - target je posledni prvek, ancestor = vsechny pred.
    ancestors: Vec<CompoundPart>,
}

#[derive(Debug)]
struct CompoundPart {
    tag: Option<String>,
    classes: Vec<String>,
    id: Option<String>,
    attrs: Vec<(String, Option<String>)>,
    /// True = direct child relation k nasledujicimu (`>`).
    direct_child: bool,
}

fn parse_simple_selector(s: &str) -> Option<CompoundSelector> {
    // Split na descendants podle whitespace + `>`.
    let mut parts: Vec<(String, bool)> = Vec::new();
    let mut cur = String::new();
    let mut next_direct = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == ' ' || c == '\t' {
            if !cur.is_empty() { parts.push((std::mem::take(&mut cur), next_direct)); next_direct = false; }
        } else if c == '>' {
            if !cur.is_empty() { parts.push((std::mem::take(&mut cur), next_direct)); }
            next_direct = true;
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() { parts.push((cur, next_direct)); }
    if parts.is_empty() { return None; }
    let (last_str, _last_direct) = parts.pop()?;
    let last = parse_compound_part(&last_str, false)?;
    let ancestors: Vec<CompoundPart> = parts.into_iter()
        .filter_map(|(s, d)| parse_compound_part(&s, d))
        .collect();
    Some(CompoundSelector {
        tag: last.tag,
        classes: last.classes,
        id: last.id,
        attrs: last.attrs,
        ancestors,
    })
}

fn parse_compound_part(s: &str, direct_child: bool) -> Option<CompoundPart> {
    let mut tag: Option<String> = None;
    let mut classes = Vec::new();
    let mut id: Option<String> = None;
    let mut attrs: Vec<(String, Option<String>)> = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    // Tag (ASCII alpha + digits) - jen jeden, na zacatku.
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_' || bytes[i] == b'*') {
        i += 1;
    }
    if i > 0 {
        let t = &s[..i];
        if t != "*" { tag = Some(t.to_ascii_lowercase()); }
    }
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'.' => {
                i += 1;
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_') { i += 1; }
                if i > start { classes.push(s[start..i].to_string()); }
            }
            b'#' => {
                i += 1;
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_') { i += 1; }
                if i > start { id = Some(s[start..i].to_string()); }
            }
            b'[' => {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != b']' { i += 1; }
                if i >= bytes.len() { return None; }
                let inner = &s[start..i];
                i += 1;
                if let Some(eq) = inner.find('=') {
                    let key = inner[..eq].trim().to_string();
                    let val = inner[eq+1..].trim().trim_matches(|c| c == '"' || c == '\'').to_string();
                    attrs.push((key, Some(val)));
                } else {
                    attrs.push((inner.trim().to_string(), None));
                }
            }
            _ => return None,
        }
    }
    Some(CompoundPart { tag, classes, id, attrs, direct_child })
}

fn matches_compound(node: &Rc<NodeData>, sel: &CompoundSelector) -> bool {
    if !match_part_simple(node, &sel.tag, &sel.classes, &sel.id, &sel.attrs) {
        return false;
    }
    if sel.ancestors.is_empty() { return true; }
    // Walk vzhuru, testuj ancestors v reverse poradi.
    let mut cur = node.parent.borrow().upgrade();
    let mut idx = sel.ancestors.len();
    while idx > 0 {
        idx -= 1;
        let part = &sel.ancestors[idx];
        let direct = part.direct_child;
        loop {
            let Some(n) = cur.clone() else { return false };
            cur = n.parent.borrow().upgrade();
            if match_part_simple(&n, &part.tag, &part.classes, &part.id, &part.attrs) {
                break;
            }
            if direct { return false; }
        }
    }
    true
}

fn match_part_simple(
    node: &Rc<NodeData>,
    tag: &Option<String>,
    classes: &[String],
    id: &Option<String>,
    attrs: &[(String, Option<String>)],
) -> bool {
    let NodeKind::Element(t) = &node.kind else { return false };
    if let Some(want) = tag {
        if !t.eq_ignore_ascii_case(want) { return false; }
    }
    let attrs_map = node.attributes.borrow();
    if let Some(want_id) = id {
        let have = attrs_map.iter().find(|(k, _)| k.as_str() == "id").map(|(_, v)| v.as_str()).unwrap_or("");
        if have != want_id.as_str() { return false; }
    }
    if !classes.is_empty() {
        let class_attr = attrs_map.iter().find(|(k, _)| k.as_str() == "class").map(|(_, v)| v.as_str()).unwrap_or("");
        let have_classes: Vec<&str> = class_attr.split_whitespace().collect();
        for c in classes {
            if !have_classes.iter().any(|h| h == &c.as_str()) { return false; }
        }
    }
    for (k, v) in attrs {
        let have = attrs_map.iter().find(|(ak, _)| ak.as_str() == k.as_str()).map(|(_, av)| av.as_str()).unwrap_or("");
        if let Some(want) = v {
            if have != want.as_str() { return false; }
        } else {
            if !attrs_map.iter().any(|(ak, _)| ak.as_str() == k.as_str()) { return false; }
        }
    }
    true
}

// ─── XPath (subset) ──────────────────────────────────────────────────────
// Podporuje: //tag, //tag[@attr], //tag[@attr="val"], /tag/tag, //tag[N].

fn xpath_search(root: &Rc<NodeData>, q: &str) -> Vec<usize> {
    let steps = parse_xpath_steps(q);
    if steps.is_empty() { return Vec::new(); }
    let mut current_set: Vec<Rc<NodeData>> = vec![Rc::clone(root)];
    for step in &steps {
        let mut next_set: Vec<Rc<NodeData>> = Vec::new();
        for n in &current_set {
            let candidates: Vec<Rc<NodeData>> = if step.descendant_or_self {
                let mut all = Vec::new();
                collect_descendants(n, &mut all);
                all
            } else {
                n.children.borrow().iter().cloned().collect()
            };
            let mut nth = 0;
            for c in candidates {
                if !match_xpath_step(&c, step) { continue; }
                nth += 1;
                if let Some(want_idx) = step.index {
                    if want_idx != nth { continue; }
                }
                next_set.push(c);
            }
        }
        current_set = next_set;
    }
    current_set.iter().map(|n| Rc::as_ptr(n) as usize).collect()
}

#[derive(Debug)]
struct XPathStep {
    tag: String,
    descendant_or_self: bool,
    attrs: Vec<(String, Option<String>)>,
    index: Option<usize>,
}

fn parse_xpath_steps(q: &str) -> Vec<XPathStep> {
    let mut steps = Vec::new();
    let mut chars = q.chars().peekable();
    let mut buf = String::new();
    let mut descendant = false;
    let mut bracket_depth = 0i32;
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            buf.push(c);
            if c == q { quote = None; }
            continue;
        }
        if c == '"' || c == '\'' {
            quote = Some(c);
            buf.push(c);
            continue;
        }
        if c == '[' { bracket_depth += 1; buf.push(c); continue; }
        if c == ']' { bracket_depth -= 1; buf.push(c); continue; }
        if c == '/' && bracket_depth == 0 {
            if !buf.is_empty() { if let Some(s) = parse_step(&buf, descendant) { steps.push(s); } buf.clear(); }
            if chars.peek() == Some(&'/') { chars.next(); descendant = true; } else { descendant = false; }
        } else {
            buf.push(c);
        }
    }
    if !buf.is_empty() { if let Some(s) = parse_step(&buf, descendant) { steps.push(s); } }
    steps
}

fn parse_step(s: &str, descendant: bool) -> Option<XPathStep> {
    let mut tag = String::new();
    let mut attrs = Vec::new();
    let mut index = None;
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c == '[' { break; }
        chars.next();
        tag.push(c);
    }
    while let Some(c) = chars.next() {
        if c == '[' {
            let mut inner = String::new();
            while let Some(c2) = chars.next() {
                if c2 == ']' { break; }
                inner.push(c2);
            }
            let inner = inner.trim();
            if let Ok(n) = inner.parse::<usize>() {
                index = Some(n);
            } else if let Some(rest) = inner.strip_prefix('@') {
                if let Some(eq) = rest.find('=') {
                    let k = rest[..eq].trim().to_string();
                    let v = rest[eq+1..].trim().trim_matches(|c| c == '"' || c == '\'').to_string();
                    attrs.push((k, Some(v)));
                } else {
                    attrs.push((rest.trim().to_string(), None));
                }
            }
        }
    }
    if tag.is_empty() { return None; }
    Some(XPathStep { tag: tag.to_ascii_lowercase(), descendant_or_self: descendant, attrs, index })
}

fn match_xpath_step(node: &Rc<NodeData>, step: &XPathStep) -> bool {
    let NodeKind::Element(t) = &node.kind else { return false };
    if step.tag != "*" && !t.eq_ignore_ascii_case(&step.tag) { return false; }
    let attrs_map = node.attributes.borrow();
    for (k, v) in &step.attrs {
        let have = attrs_map.iter().find(|(ak, _)| ak.as_str() == k.as_str()).map(|(_, av)| av.as_str());
        match (v, have) {
            (Some(want), Some(have)) => if have != want.as_str() { return false; },
            (None, Some(_)) => {}
            (_, None) => return false,
        }
    }
    true
}

fn collect_descendants(n: &Rc<NodeData>, out: &mut Vec<Rc<NodeData>>) {
    out.push(Rc::clone(n));
    for ch in n.children.borrow().iter() {
        collect_descendants(ch, out);
    }
}

fn walk<F: FnMut(&Rc<NodeData>)>(node: &Rc<NodeData>, f: &mut F) {
    f(node);
    for ch in node.children.borrow().iter() {
        walk(ch, f);
    }
}

#[cfg(test)]
#[path = "tests/search_tests.rs"]
mod tests;
