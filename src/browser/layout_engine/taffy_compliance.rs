//! Taffy spec compliance harness.
//!
//! Iteruje 4108 taffy XML fixtur (MIT licence, viz tests/fixtures/LICENSE_TAFFY.md)
//! a porovnava s vystupem naseho layout enginu.
//!
//! Cil: postupne zvedat pass-rate. Aktualni pass-rate je nizky - mnoho features
//! (aspect-ratio, abs position, RTL, percent v vsech kontekstech) neumime.

#[cfg(test)]
mod tests {
    use crate::browser::layout::{Display, LayoutBox};
    use crate::browser::layout_engine::flex::layout_flex;
    use crate::browser::layout_engine::grid::layout_grid;
    use std::fs;

    #[derive(Debug, Default, Clone)]
    struct TestNode {
        attrs: Vec<(String, String)>,
        children: Vec<TestNode>,
    }

    #[derive(Debug, Default)]
    struct ExpectedNode {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        children: Vec<ExpectedNode>,
    }

    #[derive(Debug, Default)]
    struct Fixture {
        name: String,
        input_root: Option<TestNode>,
        expected_root: Option<ExpectedNode>,
    }

    fn parse_xml(content: &str) -> Option<Fixture> {
        let mut fixture = Fixture::default();
        let mut chars = content.chars().peekable();
        let mut node_stack: Vec<TestNode> = Vec::new();
        let mut exp_stack: Vec<ExpectedNode> = Vec::new();
        let mut in_input = false;
        let mut in_expectations = false;

        while let Some(&c) = chars.peek() {
            if c.is_whitespace() { chars.next(); continue; }
            if c != '<' { chars.next(); continue; }
            chars.next();
            if chars.peek() == Some(&'/') {
                chars.next();
                let mut tag = String::new();
                while let Some(&cc) = chars.peek() {
                    if cc == '>' { chars.next(); break; }
                    tag.push(cc);
                    chars.next();
                }
                if in_input && (tag == "div" || tag == "node") {
                    if let Some(top) = node_stack.pop() {
                        if let Some(parent) = node_stack.last_mut() {
                            parent.children.push(top);
                        } else {
                            fixture.input_root = Some(top);
                        }
                    }
                } else if in_expectations && tag == "node" {
                    if let Some(top) = exp_stack.pop() {
                        if let Some(parent) = exp_stack.last_mut() {
                            parent.children.push(top);
                        } else {
                            fixture.expected_root = Some(top);
                        }
                    }
                } else if tag == "input" { in_input = false; }
                else if tag == "expectations" { in_expectations = false; }
                continue;
            }
            let mut tag = String::new();
            while let Some(&cc) = chars.peek() {
                if cc == ' ' || cc == '/' || cc == '>' { break; }
                tag.push(cc);
                chars.next();
            }
            let mut attrs: Vec<(String, String)> = Vec::new();
            let mut self_closing = false;
            while let Some(&cc) = chars.peek() {
                if cc == '>' { chars.next(); break; }
                if cc == '/' { self_closing = true; chars.next(); continue; }
                if cc.is_whitespace() { chars.next(); continue; }
                let mut attr_name = String::new();
                while let Some(&dd) = chars.peek() {
                    if dd == '=' || dd == '>' || dd == ' ' { break; }
                    attr_name.push(dd);
                    chars.next();
                }
                if chars.peek() == Some(&'=') {
                    chars.next();
                    if chars.peek() == Some(&'"') { chars.next(); }
                    let mut attr_val = String::new();
                    while let Some(&dd) = chars.peek() {
                        if dd == '"' { chars.next(); break; }
                        attr_val.push(dd);
                        chars.next();
                    }
                    attrs.push((attr_name, attr_val));
                } else if !attr_name.is_empty() {
                    attrs.push((attr_name, String::new()));
                }
            }
            match tag.as_str() {
                "test" => {
                    if let Some(name) = attrs.iter().find(|(k, _)| k == "name") {
                        fixture.name = name.1.clone();
                    }
                }
                "input" => { in_input = true; }
                "expectations" => { in_expectations = true; }
                "div" | "node" if in_input => {
                    let n = TestNode { attrs, children: Vec::new() };
                    if self_closing {
                        if let Some(parent) = node_stack.last_mut() {
                            parent.children.push(n);
                        } else {
                            fixture.input_root = Some(n);
                        }
                    } else {
                        node_stack.push(n);
                    }
                }
                "node" if in_expectations => {
                    let mut e = ExpectedNode::default();
                    for (k, v) in &attrs {
                        let f = v.parse::<f32>().unwrap_or(0.0);
                        match k.as_str() {
                            "x" => e.x = f,
                            "y" => e.y = f,
                            "width" => e.width = f,
                            "height" => e.height = f,
                            _ => {}
                        }
                    }
                    if self_closing {
                        if let Some(parent) = exp_stack.last_mut() {
                            parent.children.push(e);
                        } else {
                            fixture.expected_root = Some(e);
                        }
                    } else {
                        exp_stack.push(e);
                    }
                }
                _ => {}
            }
        }
        Some(fixture)
    }

    fn parse_dim(v: &str, container: f32) -> Option<f32> {
        let v = v.trim();
        if v == "auto" { return None; }
        if v == "max-content" || v == "min-content" || v == "fit-content" { return None; }
        if let Some(num) = v.strip_suffix("px") {
            return num.parse().ok();
        }
        if let Some(num) = v.strip_suffix('%') {
            let pct: f32 = num.parse().ok()?;
            return Some(container * pct / 100.0);
        }
        v.parse().ok()
    }

    fn convert_to_layout(node: &TestNode, container_w: f32, container_h: f32) -> Option<LayoutBox> {
        let mut bx = LayoutBox::new();
        let mut display = Display::Block;
        for (k, v) in &node.attrs {
            match k.as_str() {
                "display" => {
                    display = match v.as_str() {
                        "flex" => Display::Flex,
                        "grid" => Display::Grid,
                        "block" => Display::Block,
                        "none" => Display::None,
                        _ => return None,
                    };
                }
                "width" => bx.explicit_width = parse_dim(v, container_w),
                "height" => bx.explicit_height = parse_dim(v, container_h),
                "flex-direction" => bx.flex_direction = v.clone(),
                "flex-wrap" => bx.flex_wrap = v.clone(),
                "justify-content" => bx.justify_content = v.clone(),
                "align-items" => bx.align_items = v.clone(),
                "flex-grow" => bx.flex_grow = v.parse().unwrap_or(0.0),
                "flex-shrink" => bx.flex_shrink = v.parse().unwrap_or(1.0),
                "row-gap" => bx.row_gap = parse_dim(v, container_h).unwrap_or(0.0),
                "column-gap" => bx.column_gap = parse_dim(v, container_w).unwrap_or(0.0),
                "gap" => {
                    let g = parse_dim(v, container_w).unwrap_or(0.0);
                    bx.row_gap = g; bx.column_gap = g;
                }
                "grid-template-columns" => bx.grid_template_columns = v.clone(),
                "grid-template-rows" => bx.grid_template_rows = v.clone(),
                "padding" => bx.padding = parse_dim(v, container_w).unwrap_or(0.0),
                "margin" => bx.margin = parse_dim(v, container_w).unwrap_or(0.0),
                "position" if v != "static" && v != "relative" => return None,
                "aspect-ratio" => return None,
                "min-width" | "min-height" | "max-width" | "max-height" => return None,
                "padding-left" | "padding-right" | "padding-top" | "padding-bottom" => return None,
                "margin-left" | "margin-right" | "margin-top" | "margin-bottom" => return None,
                "border" | "border-left" | "border-right" | "border-top" | "border-bottom" => return None,
                "top" | "left" | "right" | "bottom" => return None,
                "inset" => return None,
                "overflow" | "overflow-x" | "overflow-y" => return None,
                "writing-mode" => return None,
                "direction" if v == "rtl" => return None,
                "direction" => {}
                _ => {}
            }
        }
        bx.display = display;
        for child in &node.children {
            let cw = bx.explicit_width.unwrap_or(container_w);
            let ch = bx.explicit_height.unwrap_or(container_h);
            let child_box = convert_to_layout(child, cw, ch)?;
            bx.children.push(child_box);
        }
        Some(bx)
    }

    #[derive(Debug, Default)]
    struct Stats {
        total: usize,
        pass: usize,
        fail: usize,
        skip: usize,
    }

    fn run_directory(dir: &str) -> Stats {
        let mut stats = Stats::default();
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return stats,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "xml") { continue; }
            let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
            // Skip RTL + content_box
            if fname.contains("_rtl") || fname.contains("content_box") { continue; }
            stats.total += 1;
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => { stats.skip += 1; continue; }
            };
            let fixture = match parse_xml(&content) {
                Some(f) if f.input_root.is_some() && f.expected_root.is_some() => f,
                _ => { stats.skip += 1; continue; }
            };
            let exp = fixture.expected_root.unwrap();
            let mut input_box = match convert_to_layout(fixture.input_root.as_ref().unwrap(), exp.width, exp.height) {
                Some(b) => b,
                None => { stats.skip += 1; continue; }
            };
            input_box.rect.x = 0.0;
            input_box.rect.y = 0.0;
            input_box.rect.width = exp.width;
            input_box.rect.height = exp.height;
            match input_box.display {
                Display::Flex => layout_flex(&mut input_box),
                Display::Grid => layout_grid(&mut input_box),
                Display::Block => {
                    // Block - jednoduchy stack vertikalne s explicit sizes
                    block_layout_simple(&mut input_box);
                }
                _ => { stats.skip += 1; continue; }
            }
            if compare_layout(&input_box, &exp) {
                stats.pass += 1;
            } else {
                stats.fail += 1;
            }
        }
        stats
    }

    /// Jednoduchy block layout - stackuj children vertikalne, kazdy plnou sirku.
    fn block_layout_simple(bx: &mut LayoutBox) {
        let inner_x = bx.rect.x;
        let inner_y = bx.rect.y;
        let inner_w = bx.rect.width;
        let mut cursor_y = inner_y;
        for child in bx.children.iter_mut() {
            child.rect.x = inner_x;
            child.rect.y = cursor_y;
            child.rect.width = child.explicit_width.unwrap_or(inner_w);
            child.rect.height = child.explicit_height.unwrap_or(0.0);
            // Recursive
            match child.display {
                Display::Flex => layout_flex(child),
                Display::Grid => layout_grid(child),
                Display::Block | Display::None => block_layout_simple(child),
                _ => {}
            }
            cursor_y += child.rect.height;
        }
    }

    fn compare_layout(actual: &LayoutBox, expected: &ExpectedNode) -> bool {
        if (actual.rect.width - expected.width).abs() > 1.0 { return false; }
        if (actual.rect.height - expected.height).abs() > 1.0 { return false; }
        if actual.children.len() != expected.children.len() { return false; }
        let parent_x = actual.rect.x;
        let parent_y = actual.rect.y;
        for (a, e) in actual.children.iter().zip(expected.children.iter()) {
            if (a.rect.x - parent_x - e.x).abs() > 1.0 { return false; }
            if (a.rect.y - parent_y - e.y).abs() > 1.0 { return false; }
            if (a.rect.width - e.width).abs() > 1.0 { return false; }
            if (a.rect.height - e.height).abs() > 1.0 { return false; }
        }
        true
    }

    /// Smoke test: run flex compliance, vypise pass-rate.
    /// Aktualne nizky pass-rate (5-15%) - postupne zvedat.
    #[test]
    fn taffy_compliance_flex() {
        let stats = run_directory("tests/fixtures/taffy_flex");
        println!(
            "[FLEX] {}/{} pass ({:.1}%), {} fail, {} skip",
            stats.pass, stats.total,
            100.0 * stats.pass as f32 / stats.total.max(1) as f32,
            stats.fail, stats.skip
        );
        assert!(stats.total > 0);
    }

    #[test]
    fn taffy_compliance_grid() {
        let stats = run_directory("tests/fixtures/taffy_grid");
        println!(
            "[GRID] {}/{} pass ({:.1}%), {} fail, {} skip",
            stats.pass, stats.total,
            100.0 * stats.pass as f32 / stats.total.max(1) as f32,
            stats.fail, stats.skip
        );
        assert!(stats.total > 0);
    }

    #[test]
    fn taffy_compliance_block() {
        let stats = run_directory("tests/fixtures/taffy_block");
        println!(
            "[BLOCK] {}/{} pass ({:.1}%), {} fail, {} skip",
            stats.pass, stats.total,
            100.0 * stats.pass as f32 / stats.total.max(1) as f32,
            stats.fail, stats.skip
        );
        assert!(stats.total > 0);
    }

    /// Aspon 1 fixture must pass jako sanity check.
    #[test]
    fn taffy_at_least_one_passes() {
        let s_flex = run_directory("tests/fixtures/taffy_flex");
        let s_grid = run_directory("tests/fixtures/taffy_grid");
        let s_block = run_directory("tests/fixtures/taffy_block");
        let total_pass = s_flex.pass + s_grid.pass + s_block.pass;
        let total_total = s_flex.total + s_grid.total + s_block.total;
        println!("=== TAFFY COMPLIANCE TOTAL: {}/{} pass ===", total_pass, total_total);
        assert!(total_total > 500, "Expected >500 fixtures, found {}", total_total);
        // Baseline: at least some pass (regression sanity)
        assert!(total_pass > 0, "Expected aspon 1 fixture pass");
    }
}
