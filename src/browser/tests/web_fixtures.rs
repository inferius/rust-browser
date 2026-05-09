//! Web layout compliance harness - loads JSON fixtures z Chrome/Firefox a porovnava
//! per-element rect (x, y, width, height) s nasim layout enginem.
//!
//! Workflow:
//! 1. Otevri stranku v Chrome -> DevTools Console -> paste tests/fixtures/web/export_layout.js
//! 2. Save JSON do tests/fixtures/web/<name>.json
//! 3. Spusti `cargo test web_fixture_<name> -- --ignored --nocapture`
//!
//! Tolerance per-axis: 5 px default. Pass-rate metric: % nodu s rect match.

#[cfg(test)]
mod tests {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    use serde_json::Value;

    #[derive(Debug, Default, Clone)]
    struct ExpectedNode {
        tag: String,
        id: String,
        rect: (f32, f32, f32, f32),
        children: Vec<ExpectedNode>,
    }

    fn parse_node(v: &Value) -> Option<ExpectedNode> {
        let obj = v.as_object()?;
        let tag = obj.get("tag")?.as_str()?.to_string();
        let id = obj.get("id").and_then(|s| s.as_str()).unwrap_or("").to_string();
        let rect = if let Some(r) = obj.get("rect").and_then(|r| r.as_object()) {
            let x = r.get("x").and_then(|n| n.as_f64()).unwrap_or(0.0) as f32;
            let y = r.get("y").and_then(|n| n.as_f64()).unwrap_or(0.0) as f32;
            let w = r.get("w").and_then(|n| n.as_f64()).unwrap_or(0.0) as f32;
            let h = r.get("h").and_then(|n| n.as_f64()).unwrap_or(0.0) as f32;
            (x, y, w, h)
        } else { (0.0, 0.0, 0.0, 0.0) };
        let mut children = Vec::new();
        if let Some(arr) = obj.get("children").and_then(|c| c.as_array()) {
            for c in arr {
                if let Some(n) = parse_node(c) { children.push(n); }
            }
        }
        Some(ExpectedNode { tag, id, rect, children })
    }

    struct Fixture {
        html: String,
        css: String,
        viewport: (f32, f32),
        tree: ExpectedNode,
    }

    fn load_fixture(path: &str) -> Option<Fixture> {
        let content = std::fs::read_to_string(path).ok()?;
        let v: Value = serde_json::from_str(&content).ok()?;
        let html = v.get("html_source")?.as_str()?.to_string();
        let css = v.get("css_inline").and_then(|s| s.as_str()).unwrap_or("").to_string();
        let vp = v.get("viewport")?.as_object()?;
        let vw = vp.get("width").and_then(|n| n.as_f64()).unwrap_or(1024.0) as f32;
        let vh = vp.get("height").and_then(|n| n.as_f64()).unwrap_or(768.0) as f32;
        let tree = parse_node(v.get("tree")?)?;
        Some(Fixture { html, css, viewport: (vw, vh), tree })
    }

    /// Walk expected + actual paralelne, count rect match per non-text node.
    /// Pri verbose mode vypise mismatches.
    fn compare_trees(
        expected: &ExpectedNode,
        actual: &layout::LayoutBox,
        tolerance: f32,
        path: &str,
        stats: &mut Stats,
    ) {
        if std::env::var("FIXTURE_DEBUG").is_ok() {
            println!("[debug] compare {} tag={} children_exp={} children_act_total={} children_act_tagged={}",
                path, expected.tag, expected.children.len(),
                actual.children.len(),
                actual.children.iter().filter(|c| c.tag.is_some()).count());
        }
        const SKIP_FOR_COUNT: &[&str] = &[
            "#text", "head", "script", "style", "meta", "link", "title",
            "noscript", "template", "base", "param", "source", "track",
        ];
        if !SKIP_FOR_COUNT.contains(&expected.tag.as_str()) {
            stats.total += 1;
            let (ex, ey, ew, eh) = expected.rect;
            let ax = actual.rect.x;
            let ay = actual.rect.y;
            let aw = actual.rect.width;
            let ah = actual.rect.height;
            let dx = (ex - ax).abs();
            let dy = (ey - ay).abs();
            let dw = (ew - aw).abs();
            let dh = (eh - ah).abs();
            let ok = dx <= tolerance && dy <= tolerance && dw <= tolerance && dh <= tolerance;
            if ok {
                stats.matched += 1;
            } else {
                stats.fails.push((
                    path.to_string(),
                    expected.tag.clone(),
                    expected.id.clone(),
                    (ex, ey, ew, eh),
                    (ax, ay, aw, ah),
                ));
            }
        }
        // Recurse - zip non-text children paralelne. Skip non-rendered tags
        // v expected tree (head/script/style/meta/title/link - pres CSS
        // display:none). Layout engine tyto neuvede do actual tree.
        const SKIP_TAGS: &[&str] = &[
            "#text", "head", "script", "style", "meta", "link", "title",
            "noscript", "template", "base", "param", "source", "track",
        ];
        let is_skip = |t: &str| SKIP_TAGS.contains(&t);
        let mut act_iter = actual.children.iter()
            .filter(|c| c.tag.as_ref().map(|t| !is_skip(t)).unwrap_or(false));
        let mut idx = 0;
        for ec in &expected.children {
            if is_skip(&ec.tag) { continue; }
            if let Some(ac) = act_iter.next() {
                let new_path = format!("{}/{}#{}", path, ec.tag, idx);
                compare_trees(ec, ac, tolerance, &new_path, stats);
                idx += 1;
            }
        }
    }

    #[derive(Default)]
    struct Stats {
        matched: usize,
        total: usize,
        fails: Vec<(String, String, String, (f32, f32, f32, f32), (f32, f32, f32, f32))>,
    }

    fn run_fixture(path: &str, tolerance: f32, min_pass: f32) {
        let Some(fx) = load_fixture(path) else {
            println!("Skip - {} not found nebo invalid JSON", path);
            return;
        };
        let doc = parse_html(&fx.html, "");
        let stylesheet = parse_stylesheet(&fx.css);
        let map = cascade::cascade(&doc.root, &[stylesheet]);
        let layout_root = layout::layout_tree(&doc.root, &map, fx.viewport.0, fx.viewport.1);
        // Layout vraci document-level box. expected tree zacina <html>.
        // Najdi html v actual children pro paralelni walk od stejne urovne.
        let actual_html = layout_root.children.iter()
            .find(|c| c.tag.as_deref() == Some("html"))
            .unwrap_or(&layout_root);
        if std::env::var("DUMP_TREE").is_ok() {
            fn dump(b: &layout::LayoutBox, depth: usize) {
                if depth > 4 { return; }
                let id = b.node.as_ref().and_then(|n| n.attr("id")).unwrap_or_default();
                let cls = b.node.as_ref().and_then(|n| n.attr("class")).unwrap_or_default();
                println!("{:indent$}<{}#{}.{}> y={} h={} children={}",
                    "", b.tag.as_deref().unwrap_or("?"), id, cls,
                    b.rect.y, b.rect.height, b.children.len(), indent = depth * 2);
                for c in &b.children { dump(c, depth + 1); }
            }
            dump(actual_html, 0);
        }
        let mut stats = Stats::default();
        compare_trees(&fx.tree, actual_html, tolerance, "", &mut stats);
        let pct = if stats.total > 0 {
            (stats.matched as f32 / stats.total as f32) * 100.0
        } else { 0.0 };
        println!("[fixture {}] {}/{} match ({:.1}%, tolerance {}px, viewport {}x{})",
            path.rsplit('/').next().unwrap_or(path),
            stats.matched, stats.total, pct, tolerance, fx.viewport.0, fx.viewport.1);
        // Verbose mismatches.
        if std::env::var("FIXTURE_VERBOSE").is_ok() {
            let limit = std::env::var("FIXTURE_LIMIT").ok()
                .and_then(|s| s.parse::<usize>().ok()).unwrap_or(50);
            for (p, tag, id, exp, got) in stats.fails.iter().take(limit) {
                let id_str = if id.is_empty() { String::new() } else { format!("#{}", id) };
                println!("  MISMATCH {}<{}{}> exp=({:.0},{:.0},{:.0},{:.0}) got=({:.0},{:.0},{:.0},{:.0}) diff=({:.0},{:.0},{:.0},{:.0})",
                    p, tag, id_str,
                    exp.0, exp.1, exp.2, exp.3,
                    got.0, got.1, got.2, got.3,
                    (exp.0 - got.0).abs(),
                    (exp.1 - got.1).abs(),
                    (exp.2 - got.2).abs(),
                    (exp.3 - got.3).abs());
            }
            if stats.fails.len() > limit {
                println!("  ... + {} dalsich mismatchu (FIXTURE_LIMIT={} ke zvyseni)",
                    stats.fails.len() - limit, limit);
            }
        }
        assert!(pct >= min_pass,
            "{}: pass-rate {:.1}% < min {:.1}%", path, pct, min_pass);
    }

    #[test]
    #[ignore]
    fn web_fixture_engine_test() {
        run_fixture("tests/fixtures/web/engine-test.json", 5.0, 0.0);
    }

    /// Loose tolerance variant - overi celkovy progres bez 15px Chrome scrollbar
    /// + per-glyph font width diff. Pro zacatek zatim relevant.
    #[test]
    #[ignore]
    fn web_fixture_engine_test_loose() {
        run_fixture("tests/fixtures/web/engine-test.json", 20.0, 0.0);
    }
}
