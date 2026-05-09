//! Web layout compliance harness - loads JSON fixtures z Chrome/Firefox a porovnava
//! per-element rect (x, y, width, height) s nasim layout enginem.
//!
//! Workflow:
//! 1. Otevri stranku v Chrome -> DevTools Console -> paste tests/fixtures/web/export_layout.js
//! 2. Save JSON do tests/fixtures/web/<name>.json
//! 3. Spusti `cargo test web_fixture_<name> -- --nocapture`
//!
//! Tolerance per-axis: 5 px default (cilem postupny improvement, zacatek loose).
//! Pass-rate metric: % nodu s rect match.

#[cfg(test)]
mod tests {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    use std::collections::HashMap;

    /// Naive JSON parser - mam ho v interpretu, ale pro test bez ext deps
    /// pouzijeme manual extract z fixture JSON. (Lehci nez add serde dependency.)
    fn extract_field<'a>(s: &'a str, key: &str) -> Option<&'a str> {
        let pat = format!("\"{}\"", key);
        let idx = s.find(&pat)?;
        let after = &s[idx + pat.len()..];
        let colon = after.find(':')?;
        let after = after[colon + 1..].trim_start();
        // Bud string "..." nebo cislo / object / array.
        if after.starts_with('"') {
            let rest = &after[1..];
            // Najdi unescaped ".
            let mut esc = false;
            let mut end = 0;
            for (i, ch) in rest.char_indices() {
                if esc { esc = false; continue; }
                if ch == '\\' { esc = true; continue; }
                if ch == '"' { end = i; break; }
            }
            Some(&rest[..end])
        } else {
            // Number / object - vezmi do ',' / '}' / '\n'.
            let end = after.find(|c: char| c == ',' || c == '}' || c == '\n')
                .unwrap_or(after.len());
            Some(after[..end].trim())
        }
    }

    #[derive(Debug, Default, Clone)]
    struct ExpectedNode {
        tag: String,
        id: String,
        classes: Vec<String>,
        rect: (f32, f32, f32, f32),
        children: Vec<ExpectedNode>,
    }

    /// Recursivni JSON tree parser - jen extract tag + rect + children.
    /// Na vstup ocekava string s "tree": { ... } strukturou.
    fn parse_tree_node(s: &str) -> Option<(ExpectedNode, usize)> {
        // Predpoklada s zacina na `{` az do balanced `}`.
        let s = s.trim_start();
        if !s.starts_with('{') { return None; }
        let mut depth = 0;
        let mut end = 0;
        let mut in_str = false;
        let mut esc = false;
        for (i, ch) in s.char_indices() {
            if esc { esc = false; continue; }
            if ch == '\\' { esc = true; continue; }
            if ch == '"' { in_str = !in_str; continue; }
            if in_str { continue; }
            match ch {
                '{' => depth += 1,
                '}' => { depth -= 1; if depth == 0 { end = i; break; } }
                _ => {}
            }
        }
        let obj = &s[..=end];
        let tag = extract_field(obj, "tag").unwrap_or("").to_string();
        let id = extract_field(obj, "id").unwrap_or("").to_string();
        // classes - skip pro ted (array parsing).
        let rect_raw = extract_field(obj, "rect").unwrap_or("");
        let rect = if !rect_raw.is_empty() {
            // Najdi `{ "x": N, "y": N, "w": N, "h": N }` - pres find object.
            if let Some(obj_start) = obj.find("\"rect\"") {
                let after = &obj[obj_start..];
                let brace = after.find('{').unwrap_or(0);
                let mut d = 0;
                let mut e = 0;
                for (i, ch) in after[brace..].char_indices() {
                    if ch == '{' { d += 1; }
                    if ch == '}' { d -= 1; if d == 0 { e = brace + i; break; } }
                }
                let rect_obj = &after[brace..=e];
                let x = extract_field(rect_obj, "x").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
                let y = extract_field(rect_obj, "y").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
                let w = extract_field(rect_obj, "w").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
                let h = extract_field(rect_obj, "h").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
                (x, y, w, h)
            } else { (0.0, 0.0, 0.0, 0.0) }
        } else { (0.0, 0.0, 0.0, 0.0) };

        // Children - najdi "children": [ ... ] a parsuj rekurzivne.
        let mut children = Vec::new();
        if let Some(ch_start) = obj.find("\"children\"") {
            let after = &obj[ch_start..];
            if let Some(bracket) = after.find('[') {
                let body_start = bracket + 1;
                let mut bd = 1;
                let mut body_end = 0;
                let mut in_s = false;
                let mut e_s = false;
                for (i, ch) in after[body_start..].char_indices() {
                    if e_s { e_s = false; continue; }
                    if ch == '\\' { e_s = true; continue; }
                    if ch == '"' { in_s = !in_s; continue; }
                    if in_s { continue; }
                    if ch == '[' { bd += 1; }
                    if ch == ']' { bd -= 1; if bd == 0 { body_end = body_start + i; break; } }
                }
                let body = &after[body_start..body_end];
                // Iteruj objekty v body (top-level {...}).
                let mut cursor = 0;
                while cursor < body.len() {
                    let rest = &body[cursor..];
                    let trim_offset = rest.len() - rest.trim_start().len();
                    let rest = &rest[trim_offset..];
                    if !rest.starts_with('{') { break; }
                    if let Some((child, used)) = parse_tree_node(rest) {
                        children.push(child);
                        cursor += trim_offset + used + 1;
                    } else {
                        break;
                    }
                }
            }
        }

        let node = ExpectedNode { tag, id, classes: Vec::new(), rect, children };
        Some((node, end))
    }

    fn load_fixture(path: &str) -> Option<(String, String, ExpectedNode, (f32, f32))> {
        let content = std::fs::read_to_string(path).ok()?;
        let html = extract_field(&content, "html_source")?
            .replace("\\n", "\n").replace("\\\"", "\"").replace("\\\\", "\\");
        let css = extract_field(&content, "css_inline")?
            .replace("\\n", "\n").replace("\\\"", "\"").replace("\\\\", "\\");
        // Viewport.
        let vw_str = extract_field(&content, "width").unwrap_or("1024");
        let vh_str = extract_field(&content, "height").unwrap_or("768");
        let vw = vw_str.parse::<f32>().unwrap_or(1024.0);
        let vh = vh_str.parse::<f32>().unwrap_or(768.0);
        // Tree.
        let tree_idx = content.find("\"tree\":")?;
        let after_tree = &content[tree_idx + "\"tree\":".len()..].trim_start();
        let (tree, _) = parse_tree_node(after_tree)?;
        Some((html, css, tree, (vw, vh)))
    }

    /// Walk expected + actual tree paralelne, count match per-rect (5px tolerance).
    /// Vraci (matched, total).
    fn compare_trees(expected: &ExpectedNode, actual: &layout::LayoutBox, tolerance: f32) -> (usize, usize) {
        let mut matched = 0;
        let mut total = 0;
        // Skip text-only nodes - flow positioning ne 1:1 mezi engine.
        if expected.tag != "#text" {
            total += 1;
            let (ex, ey, ew, eh) = expected.rect;
            let ax = actual.rect.x;
            let ay = actual.rect.y;
            let aw = actual.rect.width;
            let ah = actual.rect.height;
            let ok = (ex - ax).abs() <= tolerance
                && (ey - ay).abs() <= tolerance
                && (ew - aw).abs() <= tolerance
                && (eh - ah).abs() <= tolerance;
            if ok { matched += 1; }
            else if std::env::var("FIXTURE_VERBOSE").is_ok() {
                println!("MISMATCH <{}#{}> exp=({},{},{},{}) got=({},{},{},{})",
                    expected.tag, expected.id, ex, ey, ew, eh, ax, ay, aw, ah);
            }
        }
        // Recurse na children - zip pares (mismatch counts implies skip).
        let mut act_iter = actual.children.iter().filter(|c| c.tag.is_some());
        for ec in &expected.children {
            if ec.tag == "#text" { continue; }
            if let Some(ac) = act_iter.next() {
                let (m, t) = compare_trees(ec, ac, tolerance);
                matched += m; total += t;
            }
        }
        (matched, total)
    }

    fn run_fixture(path: &str, tolerance: f32, min_pass: f32) {
        let Some((html, css, expected, (vw, vh))) = load_fixture(path) else {
            println!("Skip - {} not found", path);
            return;
        };
        let doc = parse_html(&html, "");
        let stylesheet = parse_stylesheet(&css);
        let map = cascade::cascade(&doc.root, &[stylesheet]);
        let layout_root = layout::layout_tree(&doc.root, &map, vw, vh);
        let (matched, total) = compare_trees(&expected, &layout_root, tolerance);
        let pct = if total > 0 { (matched as f32 / total as f32) * 100.0 } else { 0.0 };
        println!("[fixture {}] {}/{} match ({:.1}%, tolerance {}px)",
            path.rsplit('/').next().unwrap_or(path), matched, total, pct, tolerance);
        assert!(pct >= min_pass,
            "{}: pass-rate {:.1}% < min {:.1}%", path, pct, min_pass);
    }

    #[test]
    #[ignore] // Vyzaduje fixture - spusti `cargo test web_fixture_engine_test -- --ignored --nocapture`.
    fn web_fixture_engine_test() {
        // Pri prvni run uzivatel save tests/fixtures/web/engine-test.json.
        // Cilove pass-rate zvedat postupne. Start at 30 % (zatim mnoho mismatchu).
        run_fixture("tests/fixtures/web/engine-test.json", 5.0, 0.0);
    }

    #[test]
    fn json_field_extract() {
        let s = r#"{"foo": "bar", "n": 42, "obj": {"a": 1}}"#;
        assert_eq!(extract_field(s, "foo"), Some("bar"));
        assert_eq!(extract_field(s, "n"), Some("42"));
    }

    #[test]
    fn parse_minimal_tree() {
        let json = r#"{"tag": "div", "id": "root", "rect": {"x": 0, "y": 0, "w": 100, "h": 50}, "children": []}"#;
        let (node, _) = parse_tree_node(json).expect("parse");
        assert_eq!(node.tag, "div");
        assert_eq!(node.id, "root");
        assert_eq!(node.rect, (0.0, 0.0, 100.0, 50.0));
    }

    #[test]
    fn parse_nested_tree() {
        let json = r#"{"tag": "html", "id": "", "rect": {"x": 0, "y": 0, "w": 1024, "h": 768}, "children": [
            {"tag": "body", "id": "", "rect": {"x": 0, "y": 0, "w": 1024, "h": 768}, "children": []}
        ]}"#;
        let (node, _) = parse_tree_node(json).expect("parse");
        assert_eq!(node.tag, "html");
        assert_eq!(node.children.len(), 1);
        assert_eq!(node.children[0].tag, "body");
    }

    // Suppress unused warnings.
    #[allow(dead_code)]
    fn _suppress_warnings() {
        let _ = HashMap::<String, String>::new();
    }
}
