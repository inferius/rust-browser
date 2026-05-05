//! Taffy spec compliance harness.
//!
//! Iteruje 4108 taffy XML fixtur (MIT licence, viz tests/fixtures/LICENSE_TAFFY.md)
//! a porovnava s vystupem naseho layout enginu.
//!
//! Cil: postupne zvedat pass-rate. Aktualni pass-rate je nizky - mnoho features
//! (aspect-ratio, abs position, RTL, percent v vsech kontekstech) neumime.

#[cfg(test)]
mod tests {
    use crate::browser::layout::{Display, LayoutBox, Position};
    use crate::browser::layout_engine::layout_absolute_child;
    use crate::browser::layout_engine::flex::layout_flex;
    use crate::browser::layout_engine::grid::layout_grid;
    use std::fs;

    #[derive(Debug, Default, Clone)]
    struct TestNode {
        attrs: Vec<(String, String)>,
        children: Vec<TestNode>,
        text_content: String,
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
            if c != '<' {
                if in_input && !node_stack.is_empty() {
                    let mut text_buf = String::new();
                    while let Some(&cc) = chars.peek() {
                        if cc == '<' { break; }
                        text_buf.push(cc);
                        chars.next();
                    }
                    let trimmed = text_buf.trim();
                    if !trimmed.is_empty() {
                        if let Some(top) = node_stack.last_mut() {
                            if !top.text_content.is_empty() { top.text_content.push(' '); }
                            top.text_content.push_str(trimmed);
                        }
                    }
                    continue;
                }
                chars.next();
                continue;
            }
            chars.next();
            if chars.peek() == Some(&'/') {
                chars.next();
                let mut tag = String::new();
                while let Some(&cc) = chars.peek() {
                    if cc == '>' { chars.next(); break; }
                    tag.push(cc);
                    chars.next();
                }
                if in_input && (tag == "div" || tag == "node" || tag == "text") {
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
                "div" | "node" | "text" if in_input => {
                    let n = TestNode { attrs, children: Vec::new(), text_content: String::new() };
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

    fn convert_to_layout(node: &TestNode, container_w: f32, container_h: f32, default_display: Display) -> Option<LayoutBox> {
        // Pomoc: pri parsovani top/bottom percent musime znat zda parent ma explicit
        // height. CSS spec: pri auto CB height percent inset top/bottom = 0.
        // Tady volame se s container_h_for_inset = container_h pokud parent ma
        // explicit, jinak 0. Kvuli zachovani API delegujeme do _impl.
        convert_to_layout_impl(node, container_w, container_h, container_h, default_display)
    }
    fn convert_to_layout_impl(node: &TestNode, container_w: f32, container_h: f32, container_h_for_inset: f32, default_display: Display) -> Option<LayoutBox> {
        let mut bx = LayoutBox::new();
        bx.taffy_mode = true;
        if !node.text_content.is_empty() && node.children.is_empty() {
            bx.text = Some(node.text_content.clone());
            bx.font_size = 10.0;
            bx.line_height = 1.0;
        }
        let mut display = default_display;
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
                "align-content" => bx.align_content = v.clone(),
                "align-self" => bx.align_self = v.clone(),
                "justify-self" => bx.justify_self = v.clone(),
                "justify-items" => bx.justify_items = v.clone(),
                "box-sizing" => bx.box_sizing = v.clone(),
                "grid-row-start" => {
                    if let Some(rest) = v.trim().strip_prefix("span ") {
                        bx.grid_row_span = rest.trim().parse().unwrap_or(1);
                    } else {
                        bx.grid_row_start = v.trim().parse().unwrap_or(0);
                    }
                }
                "grid-row-end" => {
                    if let Some(rest) = v.trim().strip_prefix("span ") {
                        bx.grid_row_span = rest.trim().parse().unwrap_or(1);
                    } else {
                        bx.grid_row_end = v.trim().parse().unwrap_or(0);
                    }
                }
                "grid-column-start" => {
                    if let Some(rest) = v.trim().strip_prefix("span ") {
                        bx.grid_column_span = rest.trim().parse().unwrap_or(1);
                    } else {
                        bx.grid_column_start = v.trim().parse().unwrap_or(0);
                    }
                }
                "grid-column-end" => {
                    if let Some(rest) = v.trim().strip_prefix("span ") {
                        bx.grid_column_span = rest.trim().parse().unwrap_or(1);
                    } else {
                        bx.grid_column_end = v.trim().parse().unwrap_or(0);
                    }
                }
                "flex-grow" => bx.flex_grow = v.parse().unwrap_or(0.0),
                "flex-shrink" => bx.flex_shrink = v.parse().unwrap_or(1.0),
                "flex-basis" => bx.flex_basis = v.clone(),
                "row-gap" => bx.row_gap = parse_dim(v, container_h).unwrap_or(0.0),
                "column-gap" => bx.column_gap = parse_dim(v, container_w).unwrap_or(0.0),
                "gap" => {
                    let g = parse_dim(v, container_w).unwrap_or(0.0);
                    bx.row_gap = g; bx.column_gap = g;
                }
                "grid-template-columns" => bx.grid_template_columns = v.clone(),
                "grid-template-rows" => bx.grid_template_rows = v.clone(),
                "grid-auto-columns" => bx.grid_auto_columns = v.clone(),
                "grid-auto-rows" => bx.grid_auto_rows = v.clone(),
                "grid-auto-flow" => bx.grid_auto_flow = v.clone(),
                "padding" => bx.padding = parse_dim(v, container_w).unwrap_or(0.0),
                "margin" => bx.margin = parse_dim(v, container_w).unwrap_or(0.0),
                "position" => {
                    bx.position = match v.as_str() {
                        "static" => Position::Static,
                        "relative" => Position::Relative,
                        "absolute" => Position::Absolute,
                        "fixed" => Position::Fixed,
                        "sticky" => Position::Sticky,
                        _ => return None,
                    };
                }
                "aspect-ratio" => {
                    // Parse: "3" nebo "3 / 2"
                    let v = v.trim();
                    if let Some(idx) = v.find('/') {
                        let a: f32 = v[..idx].trim().parse().ok()?;
                        let b: f32 = v[idx+1..].trim().parse().ok()?;
                        if b > 0.0 { bx.aspect_ratio = Some(a / b); }
                    } else if let Ok(r) = v.parse::<f32>() {
                        bx.aspect_ratio = Some(r);
                    }
                }
                // Min/max ulozit jako "Npx" abychom mohli snadno re-parse pres parse_length.
                // Percent prepocitat ihned proti container_w/h.
                "min-width" => {
                    if let Some(num) = v.trim().strip_suffix('%') {
                        let pct: f32 = num.parse().unwrap_or(0.0);
                        bx.min_width_v = format!("{}px", container_w * pct / 100.0);
                    } else { bx.min_width_v = v.clone(); }
                }
                "min-height" => {
                    if let Some(num) = v.trim().strip_suffix('%') {
                        let pct: f32 = num.parse().unwrap_or(0.0);
                        bx.min_height_v = format!("{}px", container_h * pct / 100.0);
                    } else { bx.min_height_v = v.clone(); }
                }
                "max-width" => {
                    if let Some(num) = v.trim().strip_suffix('%') {
                        let pct: f32 = num.parse().unwrap_or(0.0);
                        bx.max_width_v = format!("{}px", container_w * pct / 100.0);
                    } else { bx.max_width_v = v.clone(); }
                }
                "max-height" => {
                    if let Some(num) = v.trim().strip_suffix('%') {
                        let pct: f32 = num.parse().unwrap_or(0.0);
                        bx.max_height_v = format!("{}px", container_h * pct / 100.0);
                    } else { bx.max_height_v = v.clone(); }
                }
                "padding-left" => bx.padding_left = parse_dim(v, container_w),
                "padding-right" => bx.padding_right = parse_dim(v, container_w),
                "padding-top" => bx.padding_top = parse_dim(v, container_h),
                "padding-bottom" => bx.padding_bottom = parse_dim(v, container_h),
                // Pro percent margins: CSS spec resolve proti inline-size CB (= width).
                // Nepouzivame container_h pro top/bottom percent.
                "margin-left" => {
                    if v.trim() == "auto" { bx.margin_left_auto = true; }
                    else { bx.margin_left = parse_dim(v, container_w); }
                }
                "margin-right" => {
                    if v.trim() == "auto" { bx.margin_right_auto = true; }
                    else { bx.margin_right = parse_dim(v, container_w); }
                }
                "margin-top" => {
                    if v.trim() == "auto" { bx.margin_top_auto = true; }
                    else { bx.margin_top = parse_dim(v, container_w); }
                }
                "margin-bottom" => {
                    if v.trim() == "auto" { bx.margin_bottom_auto = true; }
                    else { bx.margin_bottom = parse_dim(v, container_w); }
                }
                "border" => {
                    bx.border_width = parse_dim(v, container_w).unwrap_or(0.0);
                }
                "border-left" => bx.border_left_width = parse_dim(v, container_w),
                "border-right" => bx.border_right_width = parse_dim(v, container_w),
                "border-top" => bx.border_top_width = parse_dim(v, container_h),
                "border-bottom" => bx.border_bottom_width = parse_dim(v, container_h),
                "top" => bx.offset_top = parse_dim(v, container_h_for_inset),
                "bottom" => bx.offset_bottom = parse_dim(v, container_h_for_inset),
                "left" => bx.offset_left = parse_dim(v, container_w),
                "right" => bx.offset_right = parse_dim(v, container_w),
                "inset" => {
                    let val = parse_dim(v, container_w);
                    bx.offset_top = val; bx.offset_bottom = val;
                    bx.offset_left = val; bx.offset_right = val;
                }
                "overflow" | "overflow-x" | "overflow-y" => return None,
                "writing-mode" => return None,
                "direction" if v == "rtl" => return None,
                "direction" => {}
                _ => {}
            }
        }
        bx.display = display;
        // Pri box-sizing = content-box pripocti padding+border do explicit size.
        // Taffy default je border-box (jejich fixtures `_border_box_ltr` predpokladaji
        // width = total). My rect je border-box semantics, takze pridavame jen pri
        // explicit content-box.
        if bx.box_sizing == "content-box" {
            let bw_l = bx.border_left_width.unwrap_or(bx.border_width);
            let bw_r = bx.border_right_width.unwrap_or(bx.border_width);
            let bw_t = bx.border_top_width.unwrap_or(bx.border_width);
            let bw_b = bx.border_bottom_width.unwrap_or(bx.border_width);
            let pl = bx.padding_left.unwrap_or(bx.padding) + bw_l;
            let pr = bx.padding_right.unwrap_or(bx.padding) + bw_r;
            let pt = bx.padding_top.unwrap_or(bx.padding) + bw_t;
            let pb = bx.padding_bottom.unwrap_or(bx.padding) + bw_b;
            if let Some(w) = bx.explicit_width { bx.explicit_width = Some(w + pl + pr); }
            if let Some(h) = bx.explicit_height { bx.explicit_height = Some(h + pt + pb); }
        }
        for child in &node.children {
            // Pass inner width (minus padding+border) jako container_w pro percent.
            let bw_l = bx.border_left_width.unwrap_or(bx.border_width);
            let bw_r = bx.border_right_width.unwrap_or(bx.border_width);
            let bw_t = bx.border_top_width.unwrap_or(bx.border_width);
            let bw_b = bx.border_bottom_width.unwrap_or(bx.border_width);
            let pl = bx.padding_left.unwrap_or(bx.padding) + bw_l;
            let pr = bx.padding_right.unwrap_or(bx.padding) + bw_r;
            let pt = bx.padding_top.unwrap_or(bx.padding) + bw_t;
            let pb = bx.padding_bottom.unwrap_or(bx.padding) + bw_b;
            let cw_total = bx.explicit_width.unwrap_or(container_w);
            let ch_total = bx.explicit_height.unwrap_or(container_h);
            let cw = (cw_total - pl - pr).max(0.0);
            let ch = (ch_total - pt - pb).max(0.0);
            // Pro inset top/bottom: pokud parent NEMA explicit height, percent = 0.
            let ch_inset = if bx.explicit_height.is_some() { ch } else { 0.0 };
            let child_default = Display::Block;
            let child_box = convert_to_layout_impl(child, cw, ch, ch_inset, child_default)?;
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
        failed_examples: Vec<(String, String)>,
    }
    fn describe_diff(actual: &LayoutBox, expected: &ExpectedNode) -> String {
        let mut s = format!("root: actual {:.0}x{:.0} vs expected {:.0}x{:.0}, ch={}/{}",
            actual.rect.width, actual.rect.height, expected.width, expected.height,
            actual.children.len(), expected.children.len());
        let parent_x = actual.rect.x;
        let parent_y = actual.rect.y;
        for (i, (a, e)) in actual.children.iter().zip(expected.children.iter()).enumerate() {
            let ax = a.rect.x - parent_x;
            let ay = a.rect.y - parent_y;
            if (ax - e.x).abs() > 1.0 || (ay - e.y).abs() > 1.0 || (a.rect.width - e.width).abs() > 1.0 || (a.rect.height - e.height).abs() > 1.0 {
                s.push_str(&format!(" | c{}: act {:.0},{:.0} {:.0}x{:.0} vs exp {:.0},{:.0} {:.0}x{:.0}", i, ax, ay, a.rect.width, a.rect.height, e.x, e.y, e.width, e.height));
            }
        }
        s
    }

    fn run_directory(dir: &str, root_default_display: Display) -> Stats {
        let mut stats = Stats::default();
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return stats,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "xml") { continue; }
            let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
            // Skip RTL (nepodporujem)
            if fname.contains("_rtl") { continue; }
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
            let mut input_box = match convert_to_layout(fixture.input_root.as_ref().unwrap(), exp.width, exp.height, root_default_display) {
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
                if stats.failed_examples.len() < 200 {
                    stats.failed_examples.push((fname.clone(), describe_diff(&input_box, &exp)));
                }
            }
        }
        stats
    }

    /// Walk first in-flow descendant chain to find collapsed margin-top per CSS spec:
    /// final = max(positive_margins) + min(negative_margins).
    fn chain_collapsed_m_t(child: &LayoutBox) -> f32 {
        let mut max_pos = 0.0_f32;
        let mut min_neg = 0.0_f32;
        chain_walk_top(child, &mut max_pos, &mut min_neg);
        max_pos + min_neg
    }
    fn chain_walk_top(child: &LayoutBox, max_pos: &mut f32, min_neg: &mut f32) {
        let m_t = child.margin_top.unwrap_or(child.margin);
        if m_t > 0.0 { if m_t > *max_pos { *max_pos = m_t; } }
        else if m_t < 0.0 { if m_t < *min_neg { *min_neg = m_t; } }
        let pb_t = child.padding_top.unwrap_or(child.padding) + child.border_top_width.unwrap_or(child.border_width);
        if pb_t > 0.0 { return; }
        for ch in &child.children {
            if matches!(ch.position, Position::Absolute | Position::Fixed) { continue; }
            if matches!(ch.display, Display::None) { continue; }
            chain_walk_top(ch, max_pos, min_neg);
            return;
        }
    }
    /// Walk last in-flow descendant chain pro margin-bottom collapse.
    fn chain_collapsed_m_b(child: &LayoutBox) -> f32 {
        let mut max_pos = 0.0_f32;
        let mut min_neg = 0.0_f32;
        chain_walk_bottom(child, &mut max_pos, &mut min_neg);
        max_pos + min_neg
    }
    fn chain_walk_bottom(child: &LayoutBox, max_pos: &mut f32, min_neg: &mut f32) {
        let m_b = child.margin_bottom.unwrap_or(child.margin);
        if m_b > 0.0 { if m_b > *max_pos { *max_pos = m_b; } }
        else if m_b < 0.0 { if m_b < *min_neg { *min_neg = m_b; } }
        let pb_b = child.padding_bottom.unwrap_or(child.padding) + child.border_bottom_width.unwrap_or(child.border_width);
        if pb_b > 0.0 { return; }
        // Walk last in-flow grandchild.
        for ch in child.children.iter().rev() {
            if matches!(ch.position, Position::Absolute | Position::Fixed) { continue; }
            if matches!(ch.display, Display::None) { continue; }
            chain_walk_bottom(ch, max_pos, min_neg);
            return;
        }
    }
    /// Jednoduchy block layout - stackuj children vertikalne, kazdy plnou sirku.
    /// Position absolute/fixed children jdou mimo flow - relativne k padding boxu parenta.
    fn block_layout_simple(bx: &mut LayoutBox) {
        block_layout_simple_impl(bx, false)
    }
    /// Block layout s flagem zda first in-flow ma byt jeho m_t suppressed (collapsed up).
    fn block_layout_simple_impl(bx: &mut LayoutBox, suppress_first_m_t: bool) {
        let bw_l = bx.border_left_width.unwrap_or(bx.border_width);
        let bw_r = bx.border_right_width.unwrap_or(bx.border_width);
        let bw_t = bx.border_top_width.unwrap_or(bx.border_width);
        let bw_b = bx.border_bottom_width.unwrap_or(bx.border_width);
        let pad_l = bx.padding_left.unwrap_or(bx.padding) + bw_l;
        let pad_r = bx.padding_right.unwrap_or(bx.padding) + bw_r;
        let pad_t = bx.padding_top.unwrap_or(bx.padding) + bw_t;
        let _pad_b = bx.padding_bottom.unwrap_or(bx.padding) + bw_b;
        let inner_x = bx.rect.x + pad_l;
        let inner_y = bx.rect.y + pad_t;
        let inner_w = (bx.rect.width - pad_l - pad_r).max(0.0);
        // Containing block pro abs = padding-box parenta (uvnitr borderu).
        let parent_w = (bx.rect.width - bw_l - bw_r).max(0.0);
        let parent_h = (bx.rect.height - bw_t - bw_b).max(0.0);
        let parent_x = bx.rect.x + bw_l;
        let parent_y = bx.rect.y + bw_t;
        let mut cursor_y = inner_y;
        // First pass: layout in-flow + record static y pro abs (vc. margin-top).
        let mut static_y_for: std::collections::HashMap<usize, f32> = std::collections::HashMap::new();
        let mut prev_m_b = 0.0_f32; // sibling margin collapse: prev child's margin-bottom
        // Pre-spocti first in-flow index pro chain margin collapse.
        let first_in_flow_idx: Option<usize> = bx.children.iter().enumerate()
            .find(|(_, c)| !matches!(c.position, Position::Absolute | Position::Fixed) && !matches!(c.display, Display::None))
            .map(|(idx, _)| idx);
        // Pad/border-top vlastniho boxu - rozhoduje zda first child collapsuje.
        let bx_pad_t = pad_t;
        for (i, child) in bx.children.iter_mut().enumerate() {
            if matches!(child.position, Position::Absolute | Position::Fixed) {
                let m_t = child.margin_top.unwrap_or(child.margin);
                static_y_for.insert(i, cursor_y + m_t);
                continue;
            }
            // display:none -> 0x0, neposunouva cursor
            if matches!(child.display, Display::None) {
                child.rect.x = 0.0;
                child.rect.y = 0.0;
                child.rect.width = 0.0;
                child.rect.height = 0.0;
                continue;
            }
            let m_l = child.margin_left.unwrap_or(child.margin);
            let original_m_t = child.margin_top.unwrap_or(child.margin);
            let mut m_t = original_m_t;
            let m_r = child.margin_right.unwrap_or(child.margin);
            // Suppress first child m_t (collapsed up do parent chain).
            let is_first = first_in_flow_idx == Some(i);
            if is_first && suppress_first_m_t {
                m_t = 0.0;
                child.margin_top = Some(0.0);
            }
            // Chain margin collapse: pri pad/border-top=0 child collapsuje s first
            // grandchild m_t. Pouzij chain (CSS spec: max_pos + min_neg pres celou retez).
            // Pri first child + parent pad/border-top=0 chain bubbles up.
            let mut child_will_suppress_first = false;
            let pad_t_c_pre = child.padding_top.unwrap_or(child.padding) + child.border_top_width.unwrap_or(child.border_width);
            if pad_t_c_pre == 0.0 {
                let chained = chain_collapsed_m_t(child);
                if (chained - m_t).abs() > 0.01 {
                    m_t = chained;
                    child.margin_top = Some(chained);
                    child_will_suppress_first = true;
                }
            }
            let auto_l = child.margin_left_auto;
            let auto_r = child.margin_right_auto;
            let mut base_w = if let Some(w) = child.explicit_width { w }
                             else if let (Some(h), Some(ar)) = (child.explicit_height, child.aspect_ratio) {
                                 if ar > 0.0 { h * ar } else { (inner_w - m_l - m_r).max(0.0) }
                             }
                             else { (inner_w - m_l - m_r).max(0.0) };
            // Apply min/max width + padding+border floor
            let cw_min = crate::browser::layout::parse_length(&child.min_width_v);
            let cw_max = if child.max_width_v.is_empty() { f32::INFINITY } else { crate::browser::layout::parse_length(&child.max_width_v) };
            let pb_lc = child.padding_left.unwrap_or(child.padding) + child.border_left_width.unwrap_or(child.border_width);
            let pb_rc = child.padding_right.unwrap_or(child.padding) + child.border_right_width.unwrap_or(child.border_width);
            let pb_tc = child.padding_top.unwrap_or(child.padding) + child.border_top_width.unwrap_or(child.border_width);
            let pb_bc = child.padding_bottom.unwrap_or(child.padding) + child.border_bottom_width.unwrap_or(child.border_width);
            base_w = base_w.min(cw_max);
            if cw_min > 0.0 { base_w = base_w.max(cw_min); }
            base_w = base_w.max(pb_lc + pb_rc);
            // margin auto centruje (a/a) nebo posune k jedne strane
            let free_x = (inner_w - base_w - m_l - m_r).max(0.0);
            let extra_l = if auto_l && auto_r { free_x / 2.0 }
                          else if auto_l { free_x }
                          else { 0.0 };
            let w = base_w;
            // Sibling margin collapse per CSS spec: collapsed = max(positives) + min(negatives).
            let max_pos = prev_m_b.max(m_t).max(0.0);
            let min_neg = prev_m_b.min(m_t).min(0.0);
            let collapsed = max_pos + min_neg;
            child.rect.x = inner_x + m_l + extra_l;
            let natural_y = (cursor_y - prev_m_b) + collapsed;
            child.rect.y = natural_y;
            child.rect.width = w;
            // Relative position offset (top/left/right/bottom): top wins nad bottom, left nad right.
            // V CSS jen pri position:relative; v taffy fixturach se aplikuje vzdy kdyz set.
            let off_x = if let Some(l) = child.offset_left { l }
                        else if let Some(r) = child.offset_right { -r }
                        else { 0.0 };
            let off_y = if let Some(t) = child.offset_top { t }
                        else if let Some(b) = child.offset_bottom { -b }
                        else { 0.0 };
            child.rect.x += off_x;
            // Pozn: offset_y aplikujeme na rect.y, ale cursor_y nasledne pocitame z
            // natural_y (ne posunute) aby relativni offset neovlivnil flow.
            child.rect.y += off_y;
            // Aspect-ratio: dopocet height z width
            let has_explicit_h = child.explicit_height.is_some();
            let mut h_val = if let Some(h) = child.explicit_height {
                h
            } else if let Some(ar) = child.aspect_ratio {
                if ar > 0.0 { child.rect.width / ar } else { 0.0 }
            } else { 0.0 };
            // Apply min/max height
            let ch_min = crate::browser::layout::parse_length(&child.min_height_v);
            let ch_max = if child.max_height_v.is_empty() { f32::INFINITY } else { crate::browser::layout::parse_length(&child.max_height_v) };
            let h_before = h_val;
            h_val = h_val.min(ch_max);
            if ch_min > 0.0 { h_val = h_val.max(ch_min); }
            h_val = h_val.max(pb_tc + pb_bc);
            // Pokud aspect-ratio + max/min-height zmenila h, prepocti w aby zachovavalo ratio
            // (jen kdyz w neni explicit a w byl odvozen z fill nebo aspect).
            if !has_explicit_h && child.aspect_ratio.is_some() && (h_val - h_before).abs() > 0.01 {
                if let Some(ar) = child.aspect_ratio {
                    if ar > 0.0 && child.explicit_width.is_none() {
                        let new_w = h_val * ar;
                        // Re-clamp na max-width
                        let cw_max2 = if child.max_width_v.is_empty() { f32::INFINITY } else { crate::browser::layout::parse_length(&child.max_width_v) };
                        let cw_min2 = crate::browser::layout::parse_length(&child.min_width_v);
                        let mut w2 = new_w.min(cw_max2);
                        if cw_min2 > 0.0 { w2 = w2.max(cw_min2); }
                        child.rect.width = w2;
                    }
                }
            }
            child.rect.height = h_val;
            // Recursive
            match child.display {
                Display::Flex => layout_flex(child),
                Display::Grid => layout_grid(child),
                Display::Block | Display::None => block_layout_simple_impl(child, child_will_suppress_first),
                _ => {}
            }
            // Po recursivnim layoutu: kdyz nemel explicit_height, dopocti z content.
            if !has_explicit_h && child.aspect_ratio.is_none() {
                let pad_t_c = child.padding_top.unwrap_or(child.padding) + child.border_top_width.unwrap_or(child.border_width);
                let pad_b_c = child.padding_bottom.unwrap_or(child.padding) + child.border_bottom_width.unwrap_or(child.border_width);
                // Margin collapsing: pri no padding/border-top, margin-top prvniho
                // in-flow grandchildu collapsuje s parent's; podobne bottom.
                let collapse_top = pad_t_c == 0.0;
                let collapse_bottom = pad_b_c == 0.0;
                let mut first_in_flow: Option<usize> = None;
                let mut last_in_flow: Option<usize> = None;
                for (gi, gc) in child.children.iter().enumerate() {
                    if matches!(gc.position, Position::Absolute | Position::Fixed) { continue; }
                    if matches!(gc.display, Display::None) { continue; }
                    if first_in_flow.is_none() { first_in_flow = Some(gi); }
                    last_in_flow = Some(gi);
                }
                let mut content_bottom = child.rect.y + pad_t_c;
                for (gi, gc) in child.children.iter().enumerate() {
                    if matches!(gc.position, Position::Absolute | Position::Fixed) { continue; }
                    if matches!(gc.display, Display::None) { continue; }
                    let m_t_g = gc.margin_top.unwrap_or(gc.margin);
                    let m_b_g = gc.margin_bottom.unwrap_or(gc.margin);
                    let mut bottom = gc.rect.y + gc.rect.height;
                    if collapse_top && first_in_flow == Some(gi) {
                        bottom -= m_t_g;
                    }
                    if !(collapse_bottom && last_in_flow == Some(gi)) {
                        bottom += m_b_g;
                    }
                    if bottom > content_bottom { content_bottom = bottom; }
                }
                let new_h = content_bottom - child.rect.y + pad_b_c;
                let new_h_clamped = {
                    let mut v = new_h;
                    v = v.min(ch_max);
                    if ch_min > 0.0 { v = v.max(ch_min); }
                    v.max(0.0)
                };
                if new_h_clamped > child.rect.height {
                    child.rect.height = new_h_clamped;
                }
            }
            // cursor_y advance pouzij natural_y (bez offsetu) aby relativni position
            // neovlivnil flow.
            let m_b = child.margin_bottom.unwrap_or(child.margin);
            let pad_t_c = child.padding_top.unwrap_or(child.padding) + child.border_top_width.unwrap_or(child.border_width);
            let pad_b_c = child.padding_bottom.unwrap_or(child.padding) + child.border_bottom_width.unwrap_or(child.border_width);
            let is_empty_passthrough = child.rect.height == 0.0 && pad_t_c == 0.0 && pad_b_c == 0.0;
            if is_empty_passthrough {
                cursor_y = natural_y;
                let combined = collapsed.max(m_b);
                prev_m_b = combined;
            } else {
                cursor_y = natural_y + child.rect.height + m_b;
                prev_m_b = m_b;
            }
        }
        // Abs/fixed children - pouzij static y kdyz nemaji top/bottom inset.
        for (i, child) in bx.children.iter_mut().enumerate() {
            if matches!(child.display, Display::None) {
                child.rect.x = 0.0; child.rect.y = 0.0;
                child.rect.width = 0.0; child.rect.height = 0.0;
                continue;
            }
            if matches!(child.position, Position::Absolute | Position::Fixed) {
                let no_y_inset = child.offset_top.is_none() && child.offset_bottom.is_none();
                layout_absolute_child(child, parent_x, parent_y, parent_w, parent_h);
                if no_y_inset {
                    if let Some(static_y) = static_y_for.get(&i) {
                        child.rect.y = *static_y;
                    }
                }
            }
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
        let stats = run_directory("tests/fixtures/taffy_flex", Display::Flex);
        println!(
            "[FLEX] {}/{} pass ({:.1}%), {} fail, {} skip",
            stats.pass, stats.total,
            100.0 * stats.pass as f32 / stats.total.max(1) as f32,
            stats.fail, stats.skip
        );
        for (n, d) in stats.failed_examples.iter().take(200) {
            println!("  FAIL {}: {}", n, d);
        }
        assert!(stats.total > 0);
    }

    #[test]
    fn taffy_compliance_grid() {
        let stats = run_directory("tests/fixtures/taffy_grid", Display::Grid);
        println!(
            "[GRID] {}/{} pass ({:.1}%), {} fail, {} skip",
            stats.pass, stats.total,
            100.0 * stats.pass as f32 / stats.total.max(1) as f32,
            stats.fail, stats.skip
        );
        for (n, d) in &stats.failed_examples {
            println!("  FAIL {}: {}", n, d);
        }
        assert!(stats.total > 0);
    }

    #[test]
    fn taffy_compliance_block() {
        let stats = run_directory("tests/fixtures/taffy_block", Display::Block);
        println!(
            "[BLOCK] {}/{} pass ({:.1}%), {} fail, {} skip",
            stats.pass, stats.total,
            100.0 * stats.pass as f32 / stats.total.max(1) as f32,
            stats.fail, stats.skip
        );
        for (n, d) in stats.failed_examples.iter().take(200) {
            println!("  FAIL {}: {}", n, d);
        }
        assert!(stats.total > 0);
    }

    /// Aspon 1 fixture must pass jako sanity check.
    #[test]
    fn taffy_at_least_one_passes() {
        let s_flex = run_directory("tests/fixtures/taffy_flex", Display::Flex);
        let s_grid = run_directory("tests/fixtures/taffy_grid", Display::Grid);
        let s_block = run_directory("tests/fixtures/taffy_block", Display::Block);
        let total_pass = s_flex.pass + s_grid.pass + s_block.pass;
        let total_total = s_flex.total + s_grid.total + s_block.total;
        println!("=== TAFFY COMPLIANCE TOTAL: {}/{} pass ===", total_pass, total_total);
        assert!(total_total > 500, "Expected >500 fixtures, found {}", total_total);
        // Baseline: at least some pass (regression sanity)
        assert!(total_pass > 0, "Expected aspon 1 fixture pass");
    }
}
