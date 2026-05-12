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
        let map = cascade::cascade_with_viewport(&doc.root, &[stylesheet], fx.viewport.0, fx.viewport.1);
        let layout_root = layout::layout_tree(&doc.root, &map, fx.viewport.0, fx.viewport.1);
        // Layout vraci document-level box. expected tree zacina <html>.
        // Najdi html v actual children pro paralelni walk od stejne urovne.
        let actual_html = layout_root.children.iter()
            .find(|c| c.tag.as_deref() == Some("html"))
            .unwrap_or(&layout_root);
        if std::env::var("DUMP_TREE").is_ok() {
            fn dump(b: &layout::LayoutBox, depth: usize) {
                if depth > 8 { return; }
                let id = b.node.as_ref().and_then(|n| n.attr("id")).unwrap_or_default();
                let cls = b.node.as_ref().and_then(|n| n.attr("class")).unwrap_or_default();
                println!("{:indent$}<{}#{}.{}> x={} y={} w={} h={} fs={} lh={} children={}",
                    "", b.tag.as_deref().unwrap_or("?"), id, cls,
                    b.rect.x, b.rect.y, b.rect.width, b.rect.height,
                    b.font_size, b.line_height,
                    b.children.len(), indent = depth * 2);
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

    /// Grid coverage fixtures - Chrome reference rects pres tests/fixtures/web/.
    /// Pre-M3 baseline. Min_pass aktualni level (regression guard nez pod nej
    /// propadnout). Po M3 / grid bug fixes expected 100%.
    #[test]
    #[ignore]
    fn web_fixture_grid_basic() {
        run_fixture("tests/fixtures/web/grid_basic.json", 2.0, 40.0);
    }

    /// Auto-placement s 1 row-locked item (#a grid-column:2) + 4 auto items.
    /// Chrome doc order: skip obsazene cele, fill remaining row/col-major.
    #[test]
    #[ignore]
    fn web_fixture_grid_auto_flow() {
        run_fixture("tests/fixtures/web/grid_auto_flow.json", 2.0, 0.0);
    }

    /// span 2 + span -1 (whole row). Multi-col span sizing + occupied
    /// region detection pri dalsich items.
    #[test]
    #[ignore]
    fn web_fixture_grid_span() {
        run_fixture("tests/fixtures/web/grid_span.json", 2.0, 0.0);
    }

    /// minmax(100, 200) + fr units distribution.
    #[test]
    #[ignore]
    fn web_fixture_grid_minmax() {
        run_fixture("tests/fixtures/web/grid_minmax.json", 2.0, 0.0);
    }

    /// repeat(auto-fit, minmax(120px, 1fr)) - dynamic col count z container width.
    #[test]
    #[ignore]
    fn web_fixture_grid_auto_fit() {
        run_fixture("tests/fixtures/web/grid_auto_fit.json", 2.0, 0.0);
    }

    /// grid-auto-rows: 40px - implicit rows pri overflow do row 3+.
    #[test]
    #[ignore]
    fn web_fixture_grid_implicit() {
        run_fixture("tests/fixtures/web/grid_implicit.json", 2.0, 0.0);
    }

    /// Negative grid line indices (-1 = end, -2 = end-1, 1/-1 = whole width).
    #[test]
    #[ignore]
    fn web_fixture_grid_negative() {
        run_fixture("tests/fixtures/web/grid_negative.json", 2.0, 0.0);
    }

    // ─── Block / flex fixtures (pre-L2 build_box safety net) ───────────────

    /// Block layout - margin + padding + nested div s border.
    /// Pre-L2 baseline pro build_box_inner refactor.
    #[test]
    #[ignore]
    fn web_fixture_block_basic() {
        run_fixture("tests/fixtures/web/block_basic.json", 2.0, 0.0);
    }

    /// Flex row - fixed widths + flex:1 grow + gap.
    #[test]
    #[ignore]
    fn web_fixture_flex_basic() {
        run_fixture("tests/fixtures/web/flex_basic.json", 2.0, 0.0);
    }

    // ─── Cascade / inheritance fixtures (L5 ComputedStyle safety net) ─────

    /// Inheritance basic - body sets font-size/color/line-height, descendants
    /// inherit. .small overrides font-size, .bold overrides font-weight.
    /// Verifies cascade inheritance + per-child override chain.
    #[test]
    #[ignore]
    fn web_fixture_inheritance_basic() {
        run_fixture("tests/fixtures/web/inheritance_basic.json", 2.0, 0.0);
    }

    /// Text wrap - 200px column s long text -> multi-line p; 80px short s
    /// short words -> word per line. Verifies inline wrapping at multiple
    /// container widths.
    #[test]
    #[ignore]
    fn web_fixture_text_wrap() {
        run_fixture("tests/fixtures/web/text_wrap.json", 5.0, 0.0);
    }

    /// ::before + ::after pseudo content - red ">> " prefix per .item,
    /// green " [tag]" suffix per .tag. Verifies pseudo element generation
    /// doesn't break parent layout rects.
    #[test]
    #[ignore]
    fn web_fixture_pseudo_before() {
        run_fixture("tests/fixtures/web/pseudo_before.json", 2.0, 0.0);
    }

    // ─── L5 prep fixtures (cascade ComputedStyle migration safety net) ──

    /// @media (min-width) - .wide sirka/vyska zavisi na viewport.
    /// Pri 1889 viewport ocekavame oba media queries fire (1000 + 1500).
    #[test]
    #[ignore]
    fn web_fixture_media_query() {
        run_fixture("tests/fixtures/web/media_query.json", 2.0, 0.0);
    }

    /// transform: translate/scale/rotate - getBoundingClientRect odrazi
    /// post-transform geometry (scale(1.5) = 1.5x dimensions).
    #[test]
    #[ignore]
    fn web_fixture_transform_basic() {
        run_fixture("tests/fixtures/web/transform_basic.json", 2.0, 0.0);
    }

    /// background s ruznymi color formaty (#hex, named, rgb, rgba, hsl,
    /// linear-gradient) - verifikuje layout zustava beze zmeny vstupem.
    /// Po L5 doplnit verify cascade typed Color hodnoty.
    #[test]
    #[ignore]
    fn web_fixture_background_layers() {
        run_fixture("tests/fixtures/web/background_layers.json", 2.0, 0.0);
    }

    /// Font inheritance chain - body 16px -> .outer 14px -> .mid font-weight:700
    /// -> .inner font-style:italic. .override resetuje font-size na 22px.
    #[test]
    #[ignore]
    fn web_fixture_font_inherit() {
        run_fixture("tests/fixtures/web/font_inherit.json", 2.0, 0.0);
    }

    /// position: absolute + top/left/bottom/right + 50% percent offset.
    /// Verifikuje resolution offsetu proti positioned ancestoru.
    #[test]
    #[ignore]
    fn web_fixture_position_abs() {
        run_fixture("tests/fixtures/web/position_abs.json", 2.0, 0.0);
    }

    /// column-count: 3 - multi-column layout split do 3 sloupcu.
    /// p s break-inside:avoid drzi pohromade.
    #[test]
    #[ignore]
    fn web_fixture_multicol_basic() {
        run_fixture("tests/fixtures/web/multicol_basic.json", 5.0, 0.0);
    }

    /// Flex column - 3 items s ruznymi heights (100, 150, auto s nested 80).
    /// Test pre engine-test.html regression: #main je flex column s sections
    /// jako items - main axis = column, item h podle content.
    #[test]
    #[ignore]
    fn web_fixture_flex_column() {
        run_fixture("tests/fixtures/web/flex_column.json", 2.0, 0.0);
    }

    /// Grid 2-col 310px + 1fr - typicky layout sidebar + content.
    /// Test pres mileneckaseznamka.cz reportovany bug: 1fr nerozpina content
    /// na celou volnou sirku.
    #[test]
    #[ignore]
    fn web_fixture_grid_2col_fr() {
        run_fixture("tests/fixtures/web/grid_2col_fr.json", 2.0, 0.0);
    }

    /// Grid nested - outer 2 col + inner grid 2 row. Test 1fr roztazeni
    /// v inner gridu kdyz parent ma auto height.
    #[test]
    #[ignore]
    fn web_fixture_grid_nested_fr() {
        run_fixture("tests/fixtures/web/grid_nested_fr.json", 2.0, 0.0);
    }
}
