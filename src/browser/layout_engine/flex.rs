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

    // 0. Collect in-flow indices (abs/fixed jdou mimo flex flow, display:none vyradit zcela)
    let in_flow: Vec<usize> = bx.children.iter().enumerate()
        .filter(|(_, c)| !super::is_out_of_flow(c) && !matches!(c.display, super::super::layout::Display::None))
        .map(|(i, _)| i)
        .collect();
    // display:none -> 0x0
    for ch in bx.children.iter_mut() {
        if matches!(ch.display, super::super::layout::Display::None) {
            ch.rect.x = 0.0;
            ch.rect.y = 0.0;
            ch.rect.width = 0.0;
            ch.rect.height = 0.0;
        }
    }

    // 0.5 Pre-pass: pre items bez explicit size, recursivne lay out (do "0,0,0,0")
    // a zmeri grandchildren content. Zachova nature item rect. Toto pomaha intrinsic
    // sizing items v flex contextu.
    for &i in &in_flow {
        let ch = &mut bx.children[i];
        if ch.explicit_width.is_some() && ch.explicit_height.is_some() { continue; }
        if ch.children.is_empty() { continue; }
        // Pre-set rect na 0 pro layout; ulozime puvodni
        let saved_rect = ch.rect.clone();
        ch.rect.x = 0.0; ch.rect.y = 0.0;
        // Pri unset rozmerech, set na 0 aby vnitrek zjistil natural size.
        if ch.explicit_width.is_none() { ch.rect.width = 0.0; }
        if ch.explicit_height.is_none() { ch.rect.height = 0.0; }
        // Recursivni layout: nemenime explicit values, jen rect.
        match ch.display {
            super::super::layout::Display::Flex => super::flex::layout_flex(ch),
            super::super::layout::Display::Grid => super::grid::layout_grid(ch),
            _ => {}
        }
        // Po layoutu: rect.width/height muze byt rozsiren z content (auto-grow).
        // Restore rect.x/y, zachovaj nove width/height jako "intrinsic" pro nasledny ze.
        ch.rect.x = saved_rect.x;
        ch.rect.y = saved_rect.y;
    }

    // 1. Estimate item sizes (flex-basis or content)
    let mut items: Vec<FlexItem> = Vec::with_capacity(in_flow.len());
    for &i in &in_flow {
        let ch = &bx.children[i];
        let mut est_w = ch.explicit_width.unwrap_or_else(|| {
            if let Some(t) = &ch.text {
                super::super::layout::measure_text_width(t, ch.font_size)
            } else if ch.rect.width > 0.0 { ch.rect.width } else { 0.0 }
        });
        let mut est_h = ch.explicit_height.unwrap_or_else(|| {
            if ch.text.is_some() { ch.font_size * 1.4 } else if ch.rect.height > 0.0 { ch.rect.height } else { 0.0 }
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
        // Apply min-w/h pred aspect ratio dopoctem
        let min_w_pre = super::super::layout::parse_length(&ch.min_width_v);
        let min_h_pre = super::super::layout::parse_length(&ch.min_height_v);
        let max_w_pre = if ch.max_width_v.is_empty() { f32::INFINITY } else { super::super::layout::parse_length(&ch.max_width_v) };
        let max_h_pre = if ch.max_height_v.is_empty() { f32::INFINITY } else { super::super::layout::parse_length(&ch.max_height_v) };
        if min_w_pre > 0.0 { est_w = est_w.max(min_w_pre); }
        if min_h_pre > 0.0 { est_h = est_h.max(min_h_pre); }
        // Aspect-ratio dopocet
        if let Some(ar) = ch.aspect_ratio {
            if ar > 0.0 {
                let has_w = ch.explicit_width.is_some() || min_w_pre > 0.0 || max_w_pre.is_finite();
                let has_h = ch.explicit_height.is_some() || min_h_pre > 0.0 || max_h_pre.is_finite();
                if has_w && !has_h && est_w > 0.0 {
                    est_h = est_w / ar;
                } else if has_h && !has_w && est_h > 0.0 {
                    est_w = est_h * ar;
                } else if est_w > 0.0 && est_h == 0.0 {
                    est_h = est_w / ar;
                } else if est_h > 0.0 && est_w == 0.0 {
                    est_w = est_h * ar;
                }
            }
        }
        let m_l = ch.margin_left.unwrap_or(ch.margin);
        let m_r = ch.margin_right.unwrap_or(ch.margin);
        let m_t = ch.margin_top.unwrap_or(ch.margin);
        let m_b = ch.margin_bottom.unwrap_or(ch.margin);
        let (mm_s, mm_e, mc_s, mc_e, am_s, am_e, ac_s, ac_e) = if direction.is_row() {
            (m_l, m_r, m_t, m_b, ch.margin_left_auto, ch.margin_right_auto, ch.margin_top_auto, ch.margin_bottom_auto)
        } else {
            (m_t, m_b, m_l, m_r, ch.margin_top_auto, ch.margin_bottom_auto, ch.margin_left_auto, ch.margin_right_auto)
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
            min_main: 0.0,
            max_main: f32::INFINITY,
            auto_main_start: am_s,
            auto_main_end: am_e,
            auto_cross_start: ac_s,
            auto_cross_end: ac_e,
        });
    }

    // 2. Container main size
    let inner_h = (bx.rect.height - pad_t - pad_b - 2.0 * bx.margin).max(0.0);
    let container_main = if direction.is_row() { inner_w } else { inner_h };

    // Apply min/max width/height na items - ulozit pro resolve_flexible_lengths.
    for (i, &real_idx) in in_flow.iter().enumerate() {
        let ch = &bx.children[real_idx];
        let cw_min = super::super::layout::parse_length(&ch.min_width_v);
        let cw_max = if ch.max_width_v.is_empty() { f32::INFINITY } else { super::super::layout::parse_length(&ch.max_width_v) };
        let ch_min = super::super::layout::parse_length(&ch.min_height_v);
        let ch_max = if ch.max_height_v.is_empty() { f32::INFINITY } else { super::super::layout::parse_length(&ch.max_height_v) };
        let (min_m, max_m, min_c, max_c) = if direction.is_row() {
            (cw_min, cw_max, ch_min, ch_max)
        } else {
            (ch_min, ch_max, cw_min, cw_max)
        };
        items[i].min_main = min_m;
        items[i].max_main = max_m;
        // Clamp cross size na cross min/max ihned (cross neresolvuje grow/shrink)
        if min_c > 0.0 { items[i].cross_size = items[i].cross_size.max(min_c); }
        items[i].cross_size = items[i].cross_size.min(max_c);
        // Initial main clamp jen pro respektovani min (max nepouzivat dokud nedistribuji)
        // - to je konzistentni se spec: hypothetical = clamped(flex-basis)
        if min_m > 0.0 { items[i].main_size = items[i].main_size.max(min_m); }
        items[i].main_size = items[i].main_size.min(max_m);
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
    let line_gap = if direction.is_row() { row_gap } else { col_gap };
    let container_cross = if direction.is_row() { (bx.rect.height - pad_t - pad_b - 2.0 * bx.margin).max(0.0) } else { inner_w };
    let nline = resolved_lines.len();
    let total_gap_cross = line_gap * nline.saturating_sub(1) as f32;
    let lines_natural_total: f32 = resolved_lines.iter().map(|l| l.cross_size).sum::<f32>() + total_gap_cross;
    // align-content: kdyz neset nebo "stretch" -> rozdel container cross rovnomerne mezi lines.
    let ac = bx.align_content.trim();
    let stretch_lines = ac.is_empty() || ac == "stretch" || ac == "normal";
    if container_cross > 0.0 && stretch_lines && lines_natural_total < container_cross && nline > 0 {
        let extra_per_line = (container_cross - lines_natural_total) / nline as f32;
        for line in &mut resolved_lines {
            line.cross_size += extra_per_line;
        }
    } else if matches!(wrap, FlexWrap::NoWrap) && nline == 1 && container_cross > 0.0 {
        // Single line nowrap: line zabira cely container cross (CSS spec).
        resolved_lines[0].cross_size = resolved_lines[0].cross_size.max(container_cross);
    }
    let line_cross_sizes: Vec<f32> = resolved_lines.iter().map(|l| l.cross_size).collect();
    let total_cross = line_cross_sizes.iter().sum::<f32>()
        + line_gap * (line_cross_sizes.len().saturating_sub(1) as f32);

    // 6. Position items
    let main_gap = if direction.is_row() { col_gap } else { row_gap };
    // align-content positioning of lines podel cross axis (krome stretch ktere uz pripocteno).
    let (ac_start, ac_between) = if !stretch_lines && container_cross > 0.0 {
        let used = total_cross;
        let free = (container_cross - used).max(0.0);
        match ac {
            "flex-end" | "end" => (free, 0.0),
            "center" => (free / 2.0, 0.0),
            "space-between" => {
                if nline <= 1 { (0.0, 0.0) }
                else { (0.0, free / (nline - 1) as f32) }
            }
            "space-around" => {
                let g = free / nline.max(1) as f32;
                (g / 2.0, g)
            }
            "space-evenly" => {
                let g = free / (nline + 1) as f32;
                (g, g)
            }
            _ => (0.0, 0.0),
        }
    } else {
        (0.0, 0.0)
    };
    let mut cross_cursor = ac_start;

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
        // Spocti auto margin slots v main axis - kazdy dostane equal share free.
        let auto_main_count: usize = line_indices.iter()
            .map(|&i| (items[i].auto_main_start as usize) + (items[i].auto_main_end as usize))
            .sum();
        let auto_main_share = if auto_main_count > 0 { free_main / auto_main_count as f32 } else { 0.0 };
        let effective_free = if auto_main_count > 0 { 0.0 } else { free_main };
        let (start_main, between_main) = compute_justify_offsets(justify, effective_free, resolved.main_sizes.len(), main_gap);

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
            // Margin pred itemem (start) + auto absorbed share
            main_cursor += it.margin_main_start;
            if it.auto_main_start { main_cursor += auto_main_share; }

            let item_cross_size = it.cross_size;
            // align-self per item (override align-items)
            let real_idx_for_align = in_flow[item_idx];
            let self_str = bx.children[real_idx_for_align].align_self.clone();
            let item_align = if self_str.is_empty() || self_str == "auto" {
                align
            } else {
                parse_align_items(&self_str)
            };
            // Pro baseline pouzij natural cross (max bez stretch), ne cross_size kte
            // muze byt stretchnut na container.
            let align_box = if matches!(item_align, AlignItems::Baseline) {
                resolved.natural_cross
            } else {
                cross_size
            };
            let cross_offset_align = compute_align_offset(item_align, align_box, item_cross_size + it.margin_cross_start + it.margin_cross_end);
            let mut cross_offset = cross_offset_align + it.margin_cross_start;
            // Auto cross margin absorb
            let cross_free = (cross_size - item_cross_size - it.margin_cross_start - it.margin_cross_end).max(0.0);
            let auto_cross_count = (it.auto_cross_start as usize) + (it.auto_cross_end as usize);
            if auto_cross_count > 0 {
                let share = cross_free / auto_cross_count as f32;
                if it.auto_cross_start { cross_offset += share; }
                // auto_cross_end neovlivni offset, jen zabere sve mismi
                let _ = share;
            }

            // Apply to child (item_idx je do in_flow, prevest na real index)
            let real_idx = in_flow[item_idx];
            let child = &mut bx.children[real_idx];
            // Pre-load child max/min cross
            let cw_max_c = if child.max_width_v.is_empty() { f32::INFINITY } else { super::super::layout::parse_length(&child.max_width_v) };
            let ch_max_c = if child.max_height_v.is_empty() { f32::INFINITY } else { super::super::layout::parse_length(&child.max_height_v) };
            let cw_min_c = super::super::layout::parse_length(&child.min_width_v);
            let ch_min_c = super::super::layout::parse_length(&child.min_height_v);
            if direction.is_row() {
                child.rect.x = inner_x + main_cursor;
                child.rect.y = inner_y + cross_cursor + cross_offset;
                child.rect.width = main_size;
                let mut h = if matches!(item_align, AlignItems::Stretch) && child.explicit_height.is_none() {
                    (cross_size - it.margin_cross_start - it.margin_cross_end).max(0.0)
                } else { item_cross_size };
                h = h.min(ch_max_c);
                if ch_min_c > 0.0 { h = h.max(ch_min_c); }
                child.rect.height = h;
            } else {
                child.rect.x = inner_x + cross_cursor + cross_offset;
                child.rect.y = inner_y + main_cursor;
                child.rect.height = main_size;
                let mut w = if matches!(item_align, AlignItems::Stretch) && child.explicit_width.is_none() {
                    (cross_size - it.margin_cross_start - it.margin_cross_end).max(0.0)
                } else { item_cross_size };
                w = w.min(cw_max_c);
                if cw_min_c > 0.0 { w = w.max(cw_min_c); }
                child.rect.width = w;
            }

            main_cursor += main_size + it.margin_main_end;
            if it.auto_main_end { main_cursor += auto_main_share; }
        }

        cross_cursor += resolved.cross_size + line_gap + ac_between;
    }

    // 7. Update parent height jen kdyz neni explicit set.
    if bx.explicit_height.is_none() {
        let needed = if direction.is_row() {
            total_cross + pad_t + pad_b
        } else {
            let main_used: f32 = resolved_lines.iter()
                .map(|l| l.main_sizes.iter().sum::<f32>()
                    + main_gap * (l.main_sizes.len().saturating_sub(1) as f32))
                .fold(0.0_f32, f32::max);
            main_used + pad_t + pad_b
        };
        if bx.rect.height < needed {
            bx.rect.height = needed;
        }
        // Apply max/min-height clamp na container kdyz auto.
        if !bx.max_height_v.is_empty() {
            let mh = super::super::layout::parse_length(&bx.max_height_v);
            if mh > 0.0 && bx.rect.height > mh { bx.rect.height = mh; }
        }
        let mnh = super::super::layout::parse_length(&bx.min_height_v);
        if mnh > 0.0 && bx.rect.height < mnh { bx.rect.height = mnh; }
    }

    // 8. Position absolute/fixed children (CB = padding-box parenta)
    let cb_x = bx.rect.x + bw_l;
    let cb_y = bx.rect.y + bw_t;
    let cb_w = (bx.rect.width - bw_l - bw_r).max(0.0);
    let cb_h = (bx.rect.height - bw_t - bw_b).max(0.0);
    for ch in bx.children.iter_mut() {
        if super::is_out_of_flow(ch) {
            // Pre-layout: pokud abs nema inset v dane ose, pouzij flex-container
            // alignment (justify-content / align-items) pro static position.
            super::layout_absolute_child(ch, cb_x, cb_y, cb_w, cb_h);
            // Override pri zadnem insetu: respektuj justify-content / align-items / align-self.
            let no_inset_x = ch.offset_left.is_none() && ch.offset_right.is_none();
            let no_inset_y = ch.offset_top.is_none() && ch.offset_bottom.is_none();
            if no_inset_x || no_inset_y {
                let m_l_c = ch.margin_left.unwrap_or(ch.margin);
                let m_t_c = ch.margin_top.unwrap_or(ch.margin);
                let m_r_c = ch.margin_right.unwrap_or(ch.margin);
                let m_b_c = ch.margin_bottom.unwrap_or(ch.margin);
                let self_str = ch.align_self.clone();
                let self_align = if self_str.is_empty() || self_str == "auto" { align } else { parse_align_items(&self_str) };
                if direction.is_row() {
                    if no_inset_x {
                        let free = (cb_w - ch.rect.width - m_l_c - m_r_c).max(0.0);
                        let off = match justify {
                            JustifyContent::FlexEnd => free,
                            JustifyContent::Center => free / 2.0,
                            _ => 0.0,
                        };
                        ch.rect.x = cb_x + m_l_c + off;
                    }
                    if no_inset_y {
                        let use_align = if !ch.align_self.is_empty() && ch.align_self != "auto" { self_align } else { align };
                        let free = (cb_h - ch.rect.height - m_t_c - m_b_c).max(0.0);
                        let off = match use_align {
                            AlignItems::FlexEnd => free,
                            AlignItems::Center => free / 2.0,
                            _ => 0.0,
                        };
                        ch.rect.y = cb_y + m_t_c + off;
                    }
                } else {
                    if no_inset_y {
                        let free = (cb_h - ch.rect.height - m_t_c - m_b_c).max(0.0);
                        let off = match justify {
                            JustifyContent::FlexEnd => free,
                            JustifyContent::Center => free / 2.0,
                            _ => 0.0,
                        };
                        ch.rect.y = cb_y + m_t_c + off;
                    }
                    if no_inset_x {
                        let use_align = if !ch.align_self.is_empty() && ch.align_self != "auto" { self_align } else { align };
                        let free = (cb_w - ch.rect.width - m_l_c - m_r_c).max(0.0);
                        let off = match use_align {
                            AlignItems::FlexEnd => free,
                            AlignItems::Center => free / 2.0,
                            _ => 0.0,
                        };
                        ch.rect.x = cb_x + m_l_c + off;
                    }
                }
            }
        }
    }

    // 9. Recursive layout uvnitr child boxu (jen non-abs - abs uz layoutnut)
    for ch in bx.children.iter_mut() {
        if super::is_out_of_flow(ch) { continue; }
        // Aplikuj relative position offset (top/left/bottom/right) na in-flow items.
        let off_x = if let Some(l) = ch.offset_left { l }
                    else if let Some(r) = ch.offset_right { -r }
                    else { 0.0 };
        let off_y = if let Some(t) = ch.offset_top { t }
                    else if let Some(b) = ch.offset_bottom { -b }
                    else { 0.0 };
        ch.rect.x += off_x;
        ch.rect.y += off_y;
        match ch.display {
            super::super::layout::Display::Flex => super::flex::layout_flex(ch),
            super::super::layout::Display::Grid => super::grid::layout_grid(ch),
            _ => super::super::layout::layout_block(ch),
        }
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
    /// Min/max main axis pro proper flex resolve (CSS Flex L1 9.7).
    min_main: f32,
    max_main: f32,
    /// auto flagy - absorbuji free space.
    auto_main_start: bool,
    auto_main_end: bool,
    auto_cross_start: bool,
    auto_cross_end: bool,
}

struct ResolvedLine {
    main_sizes: Vec<f32>,
    cross_size: f32,
    /// Natural max item cross (vc. margin) - pro baseline a stretch reset.
    natural_cross: f32,
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
/// Implementuje iterative freeze pri min/max violation (CSS Flex L1 9.7.4).
fn resolve_flexible_lengths(items: &[FlexItem], indices: &[usize], container_main: f32, gap: f32) -> ResolvedLine {
    let count = indices.len();
    if count == 0 {
        return ResolvedLine { main_sizes: Vec::new(), cross_size: 0.0, natural_cross: 0.0 };
    }
    let total_gap = gap * (count.saturating_sub(1) as f32);
    let total_margins: f32 = indices.iter()
        .map(|&i| items[i].margin_main_start + items[i].margin_main_end)
        .sum();
    let initial: f32 = indices.iter().map(|&i| items[i].main_size).sum();
    let total_free = container_main - initial - total_gap - total_margins;
    let mut sizes: Vec<f32> = indices.iter().map(|&i| items[i].main_size).collect();
    let mut frozen: Vec<bool> = vec![false; count];
    let growing = total_free > 0.0;

    // Iterativne distribuovat free a freeze min/max violators per CSS Flexbox §9.7.4
    for _ in 0..count + 1 {
        let frozen_sum: f32 = indices.iter().enumerate()
            .filter(|(k, _)| frozen[*k])
            .map(|(k, _)| sizes[k])
            .sum();
        let unfrozen_base: f32 = indices.iter().enumerate()
            .filter(|(k, _)| !frozen[*k])
            .map(|(_, &i)| items[i].main_size)
            .sum();
        let free = container_main - frozen_sum - unfrozen_base - total_gap - total_margins;
        let total_factor: f32 = indices.iter().enumerate()
            .filter(|(k, _)| !frozen[*k])
            .map(|(_, &i)| if growing { items[i].flex_grow } else { items[i].flex_shrink * items[i].main_size })
            .sum();

        if total_factor <= 0.0 {
            // Nothing flexible - freeze all unfrozen at base
            for (k, _) in indices.iter().enumerate() {
                if !frozen[k] {
                    sizes[k] = items[indices[k]].main_size;
                    frozen[k] = true;
                }
            }
            break;
        }

        // Distribute free na ne-frozen
        for (k, &i) in indices.iter().enumerate() {
            if frozen[k] { continue; }
            let factor = if growing { items[i].flex_grow } else { items[i].flex_shrink * items[i].main_size };
            sizes[k] = items[i].main_size + free * (factor / total_factor);
        }

        // Compute violations + clamp
        let mut violation_sum: f32 = 0.0;
        let mut violations: Vec<(usize, f32)> = Vec::new(); // (k, clamped_size)
        for (k, &i) in indices.iter().enumerate() {
            if frozen[k] { continue; }
            let original = sizes[k];
            let lo = items[i].min_main.max(0.0);
            let hi = items[i].max_main.max(lo);
            let clamped = original.clamp(lo, hi);
            let diff = clamped - original;
            violation_sum += diff;
            sizes[k] = clamped;
            violations.push((k, clamped));
        }

        if violations.is_empty() { break; }

        if violation_sum.abs() < 0.01 {
            // No net violation - freeze all
            for (k, _) in &violations { frozen[*k] = true; }
            break;
        } else if violation_sum > 0.0 {
            // Positive = min violators (clamped UP) - freeze those
            for (k, _) in &violations {
                let i = indices[*k];
                if items[i].min_main.max(0.0) > 0.0 && (sizes[*k] - items[i].min_main).abs() < 0.01 {
                    frozen[*k] = true;
                }
            }
        } else {
            // Negative = max violators (clamped DOWN) - freeze those
            for (k, _) in &violations {
                let i = indices[*k];
                if items[i].max_main.is_finite() && (sizes[*k] - items[i].max_main).abs() < 0.01 {
                    frozen[*k] = true;
                }
            }
        }
    }

    // Final clamp (min > max safely)
    for (k, &i) in indices.iter().enumerate() {
        let lo = items[i].min_main.max(0.0);
        let hi = items[i].max_main.max(lo);
        sizes[k] = sizes[k].clamp(lo, hi);
        if sizes[k] < 0.0 { sizes[k] = 0.0; }
    }

    let cross_size = indices.iter()
        .map(|&i| items[i].cross_size + items[i].margin_cross_start + items[i].margin_cross_end)
        .fold(0.0_f32, f32::max);

    ResolvedLine { main_sizes: sizes, cross_size, natural_cross: cross_size }
}

fn compute_justify_offsets(justify: JustifyContent, free: f32, count: usize, gap: f32) -> (f32, f32) {
    let _ = gap;
    if count == 0 { return (0.0, 0.0); }
    // Pri negativni free fallback na start (CSS spec pro space-* a center mimo overflow-position).
    let neg = free < 0.0;
    match justify {
        JustifyContent::FlexStart => (0.0, 0.0),
        JustifyContent::FlexEnd => (free, 0.0),
        JustifyContent::Center => (free / 2.0, 0.0),
        JustifyContent::SpaceBetween => {
            if neg || count == 1 { (0.0, 0.0) }
            else { (0.0, free / (count - 1) as f32) }
        }
        JustifyContent::SpaceAround => {
            if neg { (0.0, 0.0) }
            else { let g = free / count as f32; (g / 2.0, g) }
        }
        JustifyContent::SpaceEvenly => {
            if neg { (0.0, 0.0) }
            else { let g = free / (count + 1) as f32; (g, g) }
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
            FlexItem { main_size: 50.0, cross_size: 30.0, flex_grow: 0.0, flex_shrink: 1.0, margin_main_start: 0.0, margin_main_end: 0.0, margin_cross_start: 0.0, margin_cross_end: 0.0, min_main: 0.0, max_main: f32::INFINITY, auto_main_start: false, auto_main_end: false, auto_cross_start: false, auto_cross_end: false };
            5
        ];
        let lines = collect_lines(&items, 100.0, FlexWrap::NoWrap, 0.0);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].len(), 5);
    }

    #[test]
    fn collect_lines_wrap_overflow() {
        let items = vec![
            FlexItem { main_size: 60.0, cross_size: 30.0, flex_grow: 0.0, flex_shrink: 1.0, margin_main_start: 0.0, margin_main_end: 0.0, margin_cross_start: 0.0, margin_cross_end: 0.0, min_main: 0.0, max_main: f32::INFINITY, auto_main_start: false, auto_main_end: false, auto_cross_start: false, auto_cross_end: false };
            3
        ];
        let lines = collect_lines(&items, 100.0, FlexWrap::Wrap, 0.0);
        // 60 + 60 = 120 > 100 -> 2 prvni nenajdou se v 1 line
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn resolve_grow_distributes_free_space() {
        let items = vec![
            FlexItem { main_size: 50.0, cross_size: 30.0, flex_grow: 1.0, flex_shrink: 1.0, margin_main_start: 0.0, margin_main_end: 0.0, margin_cross_start: 0.0, margin_cross_end: 0.0, min_main: 0.0, max_main: f32::INFINITY, auto_main_start: false, auto_main_end: false, auto_cross_start: false, auto_cross_end: false },
            FlexItem { main_size: 50.0, cross_size: 30.0, flex_grow: 1.0, flex_shrink: 1.0, margin_main_start: 0.0, margin_main_end: 0.0, margin_cross_start: 0.0, margin_cross_end: 0.0, min_main: 0.0, max_main: f32::INFINITY, auto_main_start: false, auto_main_end: false, auto_cross_start: false, auto_cross_end: false },
        ];
        let resolved = resolve_flexible_lengths(&items, &[0, 1], 200.0, 0.0);
        // Free = 200 - 100 = 100, dist 50/50
        assert_eq!(resolved.main_sizes[0], 100.0);
        assert_eq!(resolved.main_sizes[1], 100.0);
    }
}
