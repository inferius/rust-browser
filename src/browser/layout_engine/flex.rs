/// Flex layout - vlastni implementace.
///
/// Inspirovano `taffy` (MIT licence, https://github.com/DioxusLabs/taffy/blob/main/src/compute/flexbox/mod.rs)
/// + CSS Flexbox L1 spec (https://www.w3.org/TR/css-flexbox-1/).
///
/// Podpora:
/// - flex-direction: row / row-reverse / column / column-reverse
/// - flex-wrap: nowrap / wrap / wrap-reverse
/// - justify-content: flex-start / flex-end / center / space-between / space-around / space-evenly
/// - align-items: flex-start / flex-end / center / stretch / baseline
/// - align-content: stejne hodnoty pro multi-line
/// - gap (row-gap / column-gap)
/// - per-item flex-grow / flex-shrink / flex-basis
///
/// Algoritmus (zjednoduseny CSS Flexbox 9.7 Layout Algorithm):
/// 1. Resolve flex-basis -> hypothetical main size
/// 2. Determine main size of container
/// 3. Collect items into lines (wrap)
/// 4. Resolve flexible lengths (grow/shrink)
/// 5. Determine cross size of each line
/// 6. Align items along cross axis (align-items)
/// 7. Pack lines along cross axis (align-content)
/// 8. Justify items along main axis (justify-content)

use super::super::layout::LayoutBox;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlexDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

impl FlexDirection {
    fn is_row(&self) -> bool {
        matches!(self, FlexDirection::Row | FlexDirection::RowReverse)
    }
    fn is_reverse(&self) -> bool {
        matches!(self, FlexDirection::RowReverse | FlexDirection::ColumnReverse)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlexWrap {
    NoWrap,
    Wrap,
    WrapReverse,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JustifyContent {
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AlignItems {
    FlexStart,
    FlexEnd,
    Center,
    Stretch,
    Baseline,
}

/// Flex layout entry-point.
/// Layoutuje `bx.children` v ramci `bx`.
/// Pouziva CSS props: flex-direction, flex-wrap, justify-content, align-items, gap.
pub fn layout_flex(bx: &mut LayoutBox) {
    let bw_l = bx.border_left_width.unwrap_or(bx.border_width);
    let bw_r = bx.border_right_width.unwrap_or(bx.border_width);
    let bw_t = bx.border_top_width.unwrap_or(bx.border_width);
    let bw_b = bx.border_bottom_width.unwrap_or(bx.border_width);
    let pad_l = bx.padding_left.unwrap_or(bx.padding) + bw_l;
    let pad_r = bx.padding_right.unwrap_or(bx.padding) + bw_r;
    let pad_t = bx.padding_top.unwrap_or(bx.padding) + bw_t;
    let pad_b = bx.padding_bottom.unwrap_or(bx.padding) + bw_b;
    let inner_x = bx.rect.x + pad_l + bx.margin;
    let inner_y = bx.rect.y + pad_t + bx.margin;
    let inner_w = (bx.rect.width - pad_l - pad_r - 2.0 * bx.margin).max(0.0);

    // Parse CSS props
    let direction = parse_flex_direction(&bx.flex_direction);
    let wrap = parse_flex_wrap(&bx.flex_wrap);
    let justify = parse_justify_content(&bx.justify_content);
    let align = parse_align_items(&bx.align_items);
    let row_gap = bx.row_gap.max(0.0);
    let col_gap = bx.column_gap.max(0.0);

    if bx.children.is_empty() { return; }

    // 0. Collect in-flow indices (abs/fixed jdou mimo flex flow)
    let in_flow: Vec<usize> = bx.children.iter().enumerate()
        .filter(|(_, c)| !super::is_out_of_flow(c))
        .map(|(i, _)| i)
        .collect();

    // 1. Estimate item sizes (flex-basis or content)
    let mut items: Vec<FlexItem> = Vec::with_capacity(in_flow.len());
    for &i in &in_flow {
        let ch = &bx.children[i];
        let mut est_w = ch.explicit_width.unwrap_or_else(|| {
            if let Some(t) = &ch.text {
                super::super::layout::measure_text_width(t, ch.font_size)
            } else { 0.0 }
        });
        let mut est_h = ch.explicit_height.unwrap_or_else(|| {
            if ch.text.is_some() { ch.font_size * 1.4 } else { 0.0 }
        });
        // flex-basis override main size kdyz nastaveno (a neni "auto")
        let basis_v = ch.flex_basis.trim();
        if !basis_v.is_empty() && basis_v != "auto" && basis_v != "content" {
            let basis = if let Some(num) = basis_v.strip_suffix("px") {
                num.parse::<f32>().ok()
            } else if let Some(num) = basis_v.strip_suffix('%') {
                num.parse::<f32>().ok().map(|p| {
                    let cont = if direction.is_row() { inner_w } else { (bx.rect.height - pad_t - pad_b - 2.0 * bx.margin).max(0.0) };
                    cont * p / 100.0
                })
            } else { basis_v.parse::<f32>().ok() };
            if let Some(b) = basis {
                if direction.is_row() { est_w = b; } else { est_h = b; }
            }
        }
        // Aspect-ratio dopocet
        if let Some(ar) = ch.aspect_ratio {
            if ar > 0.0 {
                if ch.explicit_width.is_some() && ch.explicit_height.is_none() {
                    est_h = est_w / ar;
                } else if ch.explicit_height.is_some() && ch.explicit_width.is_none() {
                    est_w = est_h * ar;
                }
            }
        }
        let m_l = ch.margin_left.unwrap_or(ch.margin);
        let m_r = ch.margin_right.unwrap_or(ch.margin);
        let m_t = ch.margin_top.unwrap_or(ch.margin);
        let m_b = ch.margin_bottom.unwrap_or(ch.margin);
        let (mm_s, mm_e, mc_s, mc_e) = if direction.is_row() {
            (m_l, m_r, m_t, m_b)
        } else {
            (m_t, m_b, m_l, m_r)
        };
        items.push(FlexItem {
            main_size: if direction.is_row() { est_w } else { est_h },
            cross_size: if direction.is_row() { est_h } else { est_w },
            flex_grow: ch.flex_grow,
            flex_shrink: ch.flex_shrink,
            margin_main_start: mm_s,
            margin_main_end: mm_e,
            margin_cross_start: mc_s,
            margin_cross_end: mc_e,
        });
    }

    // 2. Container main size
    let inner_h = (bx.rect.height - pad_t - pad_b - 2.0 * bx.margin).max(0.0);
    let container_main = if direction.is_row() { inner_w } else { inner_h };

    // Apply min/max width/height na items (z bx props - to nas zdrojuje)
    for (i, &real_idx) in in_flow.iter().enumerate() {
        let ch = &bx.children[real_idx];
        let cw_min = super::super::layout::parse_length(&ch.min_width_v);
        let cw_max = if ch.max_width_v.is_empty() { f32::INFINITY } else { super::super::layout::parse_length(&ch.max_width_v) };
        let ch_min = super::super::layout::parse_length(&ch.min_height_v);
        let ch_max = if ch.max_height_v.is_empty() { f32::INFINITY } else { super::super::layout::parse_length(&ch.max_height_v) };
        if direction.is_row() {
            if cw_min > 0.0 { items[i].main_size = items[i].main_size.max(cw_min); }
            items[i].main_size = items[i].main_size.min(cw_max);
            if ch_min > 0.0 { items[i].cross_size = items[i].cross_size.max(ch_min); }
            items[i].cross_size = items[i].cross_size.min(ch_max);
        } else {
            if ch_min > 0.0 { items[i].main_size = items[i].main_size.max(ch_min); }
            items[i].main_size = items[i].main_size.min(ch_max);
            if cw_min > 0.0 { items[i].cross_size = items[i].cross_size.max(cw_min); }
            items[i].cross_size = items[i].cross_size.min(cw_max);
        }
    }

    // 3. Collect lines (wrap)
    let lines = collect_lines(&items, container_main, wrap, if direction.is_row() { col_gap } else { row_gap });

    // 4. Resolve flexible lengths per line
    let mut resolved_lines: Vec<ResolvedLine> = Vec::new();
    for line_indices in &lines {
        let resolved = resolve_flexible_lengths(&items, line_indices, container_main,
            if direction.is_row() { col_gap } else { row_gap });
        resolved_lines.push(resolved);
    }

    // 5. Compute total cross size
    let line_cross_sizes: Vec<f32> = resolved_lines.iter().map(|l| l.cross_size).collect();
    let line_gap = if direction.is_row() { row_gap } else { col_gap };
    let total_cross = line_cross_sizes.iter().sum::<f32>()
        + line_gap * (line_cross_sizes.len().saturating_sub(1) as f32);

    // 6. Position items
    let main_gap = if direction.is_row() { col_gap } else { row_gap };
    let mut cross_cursor = 0.0_f32;

    let line_iter: Box<dyn Iterator<Item = &Vec<usize>>> = if matches!(wrap, FlexWrap::WrapReverse) {
        Box::new(lines.iter().rev())
    } else {
        Box::new(lines.iter())
    };

    let resolved_iter: Vec<&ResolvedLine> = if matches!(wrap, FlexWrap::WrapReverse) {
        resolved_lines.iter().rev().collect()
    } else {
        resolved_lines.iter().collect()
    };

    for (line_idx, line_indices) in line_iter.enumerate() {
        let resolved = &resolved_iter[line_idx];

        // Justify items v main axis - vc. margin item totalu
        let used_main: f32 = resolved.main_sizes.iter().enumerate()
            .map(|(k, s)| s + items[line_indices[k]].margin_main_start + items[line_indices[k]].margin_main_end)
            .sum::<f32>()
            + main_gap * (resolved.main_sizes.len().saturating_sub(1) as f32);
        let free_main = (container_main - used_main).max(0.0);
        let (start_main, between_main) = compute_justify_offsets(justify, free_main, resolved.main_sizes.len(), main_gap);

        let main_iter: Box<dyn Iterator<Item = (usize, &usize)>> = if direction.is_reverse() {
            Box::new(line_indices.iter().enumerate().rev())
        } else {
            Box::new(line_indices.iter().enumerate())
        };

        let mut main_cursor = start_main;
        let mut first = true;
        for (i_in_line, &item_idx) in main_iter {
            let main_size = resolved.main_sizes[i_in_line];
            let cross_size = resolved.cross_size;
            let it = items[item_idx];

            // Pridat gap + between extra space pred kazdym non-first item
            if !first {
                main_cursor += main_gap + between_main;
            }
            first = false;
            // Margin pred itemem (start)
            main_cursor += it.margin_main_start;

            let item_cross_size = it.cross_size;
            let cross_offset_align = compute_align_offset(align, cross_size, item_cross_size + it.margin_cross_start + it.margin_cross_end);
            let cross_offset = cross_offset_align + it.margin_cross_start;

            // Apply to child (item_idx je do in_flow, prevest na real index)
            let real_idx = in_flow[item_idx];
            let child = &mut bx.children[real_idx];
            if direction.is_row() {
                child.rect.x = inner_x + main_cursor;
                child.rect.y = inner_y + cross_cursor + cross_offset;
                child.rect.width = main_size;
                child.rect.height = if matches!(align, AlignItems::Stretch) && child.explicit_height.is_none() {
                    (cross_size - it.margin_cross_start - it.margin_cross_end).max(0.0)
                } else { item_cross_size };
            } else {
                child.rect.x = inner_x + cross_cursor + cross_offset;
                child.rect.y = inner_y + main_cursor;
                child.rect.height = main_size;
                child.rect.width = if matches!(align, AlignItems::Stretch) && child.explicit_width.is_none() {
                    (cross_size - it.margin_cross_start - it.margin_cross_end).max(0.0)
                } else { item_cross_size };
            }

            main_cursor += main_size + it.margin_main_end;
        }

        cross_cursor += resolved.cross_size + line_gap;
    }

    // 7. Update parent height
    let needed = if direction.is_row() {
        total_cross + pad_t + pad_b
    } else {
        // V column direction main axis je vertical -> potreba content height
        let main_used: f32 = resolved_lines.iter()
            .map(|l| l.main_sizes.iter().sum::<f32>()
                + main_gap * (l.main_sizes.len().saturating_sub(1) as f32))
            .fold(0.0_f32, f32::max);
        main_used + pad_t + pad_b
    };
    if bx.rect.height < needed {
        bx.rect.height = needed;
    }

    // 8. Position absolute/fixed children (parent jako CB)
    let parent_x = bx.rect.x;
    let parent_y = bx.rect.y;
    let parent_w = bx.rect.width;
    let parent_h = bx.rect.height;
    for ch in bx.children.iter_mut() {
        if super::is_out_of_flow(ch) {
            super::layout_absolute_child(ch, parent_x, parent_y, parent_w, parent_h);
        }
    }

    // 9. Recursive layout uvnitr child boxu (jen non-abs - abs uz layoutnut)
    for ch in bx.children.iter_mut() {
        if super::is_out_of_flow(ch) { continue; }
        super::super::layout::layout_block(ch);
    }
}

#[derive(Debug, Clone, Copy)]
struct FlexItem {
    main_size: f32,
    cross_size: f32,
    flex_grow: f32,
    flex_shrink: f32,
    /// margin start v main axis (left pro row, top pro column)
    margin_main_start: f32,
    margin_main_end: f32,
    margin_cross_start: f32,
    margin_cross_end: f32,
}

struct ResolvedLine {
    main_sizes: Vec<f32>,
    cross_size: f32,
}

/// Sber items do lines podle wrap policy. Margins se zapocitavaji do velikosti.
fn collect_lines(items: &[FlexItem], container_main: f32, wrap: FlexWrap, gap: f32) -> Vec<Vec<usize>> {
    if matches!(wrap, FlexWrap::NoWrap) {
        return vec![(0..items.len()).collect()];
    }
    let mut lines: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut used = 0.0_f32;
    for (i, item) in items.iter().enumerate() {
        let item_total = item.main_size + item.margin_main_start + item.margin_main_end;
        let with_gap = if current.is_empty() { item_total } else { item_total + gap };
        if !current.is_empty() && used + with_gap > container_main {
            lines.push(current);
            current = Vec::new();
            current.push(i);
            used = item_total;
        } else {
            current.push(i);
            used += with_gap;
        }
    }
    if !current.is_empty() { lines.push(current); }
    lines
}

/// Resolve flexible lengths per line podle flex-grow / flex-shrink.
/// Margins se odecitaji od container_main pred vypoctem free space.
fn resolve_flexible_lengths(items: &[FlexItem], indices: &[usize], container_main: f32, gap: f32) -> ResolvedLine {
    let count = indices.len();
    if count == 0 {
        return ResolvedLine { main_sizes: Vec::new(), cross_size: 0.0 };
    }
    let total_gap = gap * (count.saturating_sub(1) as f32);
    let total_margins: f32 = indices.iter()
        .map(|&i| items[i].margin_main_start + items[i].margin_main_end)
        .sum();
    let initial: f32 = indices.iter().map(|&i| items[i].main_size).sum();
    let free = container_main - initial - total_gap - total_margins;
    let mut sizes: Vec<f32> = indices.iter().map(|&i| items[i].main_size).collect();

    if free > 0.0 {
        // Grow
        let total_grow: f32 = indices.iter().map(|&i| items[i].flex_grow).sum();
        if total_grow > 0.0 {
            for (k, &i) in indices.iter().enumerate() {
                let factor = items[i].flex_grow / total_grow;
                sizes[k] += free * factor;
            }
        }
    } else if free < 0.0 {
        // Shrink
        let total_shrink: f32 = indices.iter().map(|&i| items[i].flex_shrink * items[i].main_size).sum();
        if total_shrink > 0.0 {
            for (k, &i) in indices.iter().enumerate() {
                let factor = items[i].flex_shrink * items[i].main_size / total_shrink;
                sizes[k] += free * factor;
                if sizes[k] < 0.0 { sizes[k] = 0.0; }
            }
        }
    }

    let cross_size = indices.iter()
        .map(|&i| items[i].cross_size + items[i].margin_cross_start + items[i].margin_cross_end)
        .fold(0.0_f32, f32::max);

    ResolvedLine { main_sizes: sizes, cross_size }
}

fn compute_justify_offsets(justify: JustifyContent, free: f32, count: usize, gap: f32) -> (f32, f32) {
    let _ = gap;
    if count == 0 { return (0.0, 0.0); }
    match justify {
        JustifyContent::FlexStart => (0.0, 0.0),
        JustifyContent::FlexEnd => (free, 0.0),
        JustifyContent::Center => (free / 2.0, 0.0),
        JustifyContent::SpaceBetween => {
            if count == 1 { (0.0, 0.0) }
            else { (0.0, free / (count - 1) as f32) }
        }
        JustifyContent::SpaceAround => {
            let g = free / count as f32;
            (g / 2.0, g)
        }
        JustifyContent::SpaceEvenly => {
            let g = free / (count + 1) as f32;
            (g, g)
        }
    }
}

fn compute_align_offset(align: AlignItems, line_cross: f32, item_cross: f32) -> f32 {
    match align {
        AlignItems::FlexStart | AlignItems::Stretch | AlignItems::Baseline => 0.0,
        AlignItems::FlexEnd => line_cross - item_cross,
        AlignItems::Center => (line_cross - item_cross) / 2.0,
    }
}

fn parse_flex_direction(s: &str) -> FlexDirection {
    match s {
        "row-reverse" => FlexDirection::RowReverse,
        "column" => FlexDirection::Column,
        "column-reverse" => FlexDirection::ColumnReverse,
        _ => FlexDirection::Row,
    }
}

fn parse_flex_wrap(s: &str) -> FlexWrap {
    match s {
        "wrap" => FlexWrap::Wrap,
        "wrap-reverse" => FlexWrap::WrapReverse,
        _ => FlexWrap::NoWrap,
    }
}

fn parse_justify_content(s: &str) -> JustifyContent {
    match s {
        "flex-end" | "end" => JustifyContent::FlexEnd,
        "center" => JustifyContent::Center,
        "space-between" => JustifyContent::SpaceBetween,
        "space-around" => JustifyContent::SpaceAround,
        "space-evenly" => JustifyContent::SpaceEvenly,
        _ => JustifyContent::FlexStart,
    }
}

fn parse_align_items(s: &str) -> AlignItems {
    match s {
        "flex-start" | "start" => AlignItems::FlexStart,
        "flex-end" | "end" => AlignItems::FlexEnd,
        "center" => AlignItems::Center,
        "stretch" => AlignItems::Stretch,
        "baseline" => AlignItems::Baseline,
        _ => AlignItems::Stretch, // CSS default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_direction_basic() {
        assert_eq!(parse_flex_direction("row"), FlexDirection::Row);
        assert_eq!(parse_flex_direction("row-reverse"), FlexDirection::RowReverse);
        assert_eq!(parse_flex_direction("column"), FlexDirection::Column);
        assert_eq!(parse_flex_direction("column-reverse"), FlexDirection::ColumnReverse);
    }

    #[test]
    fn parse_wrap_basic() {
        assert_eq!(parse_flex_wrap("wrap"), FlexWrap::Wrap);
        assert_eq!(parse_flex_wrap("nowrap"), FlexWrap::NoWrap);
        assert_eq!(parse_flex_wrap("wrap-reverse"), FlexWrap::WrapReverse);
    }

    #[test]
    fn justify_offsets_flex_start() {
        let (s, b) = compute_justify_offsets(JustifyContent::FlexStart, 100.0, 3, 0.0);
        assert_eq!(s, 0.0);
        assert_eq!(b, 0.0);
    }

    #[test]
    fn justify_offsets_center() {
        let (s, b) = compute_justify_offsets(JustifyContent::Center, 100.0, 3, 0.0);
        assert_eq!(s, 50.0);
        assert_eq!(b, 0.0);
    }

    #[test]
    fn justify_offsets_space_between() {
        let (s, b) = compute_justify_offsets(JustifyContent::SpaceBetween, 100.0, 3, 0.0);
        assert_eq!(s, 0.0);
        assert_eq!(b, 50.0);
    }

    #[test]
    fn justify_offsets_space_evenly() {
        let (s, b) = compute_justify_offsets(JustifyContent::SpaceEvenly, 100.0, 3, 0.0);
        // 100 / 4 = 25
        assert_eq!(s, 25.0);
        assert_eq!(b, 25.0);
    }

    #[test]
    fn align_offset_center() {
        let off = compute_align_offset(AlignItems::Center, 100.0, 30.0);
        assert_eq!(off, 35.0);
    }

    #[test]
    fn collect_lines_no_wrap() {
        let items = vec![
            FlexItem { main_size: 50.0, cross_size: 30.0, flex_grow: 0.0, flex_shrink: 1.0, margin_main_start: 0.0, margin_main_end: 0.0, margin_cross_start: 0.0, margin_cross_end: 0.0 };
            5
        ];
        let lines = collect_lines(&items, 100.0, FlexWrap::NoWrap, 0.0);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].len(), 5);
    }

    #[test]
    fn collect_lines_wrap_overflow() {
        let items = vec![
            FlexItem { main_size: 60.0, cross_size: 30.0, flex_grow: 0.0, flex_shrink: 1.0, margin_main_start: 0.0, margin_main_end: 0.0, margin_cross_start: 0.0, margin_cross_end: 0.0 };
            3
        ];
        let lines = collect_lines(&items, 100.0, FlexWrap::Wrap, 0.0);
        // 60 + 60 = 120 > 100 -> 2 prvni nenajdou se v 1 line
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn resolve_grow_distributes_free_space() {
        let items = vec![
            FlexItem { main_size: 50.0, cross_size: 30.0, flex_grow: 1.0, flex_shrink: 1.0, margin_main_start: 0.0, margin_main_end: 0.0, margin_cross_start: 0.0, margin_cross_end: 0.0 },
            FlexItem { main_size: 50.0, cross_size: 30.0, flex_grow: 1.0, flex_shrink: 1.0, margin_main_start: 0.0, margin_main_end: 0.0, margin_cross_start: 0.0, margin_cross_end: 0.0 },
        ];
        let resolved = resolve_flexible_lengths(&items, &[0, 1], 200.0, 0.0);
        // Free = 200 - 100 = 100, dist 50/50
        assert_eq!(resolved.main_sizes[0], 100.0);
        assert_eq!(resolved.main_sizes[1], 100.0);
    }
}
