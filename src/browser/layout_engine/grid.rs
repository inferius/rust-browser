/// Grid layout - vlastni implementace.
///
/// Inspirovano `taffy` (MIT licence) + CSS Grid L1 spec
/// (https://www.w3.org/TR/css-grid-1/).
///
/// Aktualne velmi minimalisticka impl - rozdeli children rovnomerne do
/// grid cells podle grid-template-columns/rows. Plnejsi impl by vyzadovala
/// track sizing algoritmus (intrinsic sizes, fr units, minmax, repeat).

use super::super::layout::LayoutBox;

/// Grid layout entry-point - rozdeli children do gridu.
/// Pouzije bx.grid_template_columns / bx.grid_template_rows.
/// Resolve fr units, fixed lengths, auto, percent.
pub fn layout_grid(bx: &mut LayoutBox) {
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
    let inner_h = (bx.rect.height - pad_t - pad_b - 2.0 * bx.margin).max(0.0);

    if bx.children.is_empty() { return; }

    // Re-resolve gap pct proti inner content dimension.
    let row_gap = if let Some(p) = bx.row_gap_pct {
        if bx.explicit_height.is_none() { 0.0 } else { inner_h * p }
    } else { bx.row_gap };
    let col_gap = if let Some(p) = bx.column_gap_pct {
        inner_w * p
    } else { bx.column_gap };

    // Parse + resolve column tracks
    let mut col_tracks = resolve_tracks(&bx.grid_template_columns, inner_w, col_gap);
    let (col_token_kinds, col_is_autofit) = parse_track_tokens_with_autofit(&bx.grid_template_columns, inner_w, col_gap);
    if col_tracks.is_empty() { col_tracks = vec![inner_w]; }
    let cols = col_tracks.len();
    let col_is_auto: Vec<bool> = (0..cols).map(|i| match col_token_kinds.get(i) {
        Some(Track::Auto) => true,
        Some(Track::MaxContent) => true,
        Some(Track::MinContent) => true,
        Some(Track::FitContent(_)) => true,
        Some(Track::Minmax(_, max, false)) if !max.is_finite() => true,
        Some(Track::Minmax(min, _, false)) if min.is_nan() => true,
        Some(Track::Minmax(min, _, false)) if (*min - (-1000.0)).abs() < 0.5 => true,
        // Fallback (no template): treat jako auto.
        None => bx.grid_template_columns.is_empty(),
        _ => false,
    }).collect();

    // In-flow item count (abs/fixed/display:none vyradit pri vypoctu rows).
    let in_flow_count = bx.children.iter()
        .filter(|c| !super::is_out_of_flow(c) && !matches!(c.display, super::super::layout::Display::None))
        .count();
    let rows_explicit_str = bx.grid_template_rows.clone();
    // Spocti needed_rows simulaci placement (vc. spans) - drive jen ceil(items/cols).
    let needed_from_placement = {
        let cols_n = cols.max(1);
        let mut occupied: Vec<bool> = Vec::new();
        let mut max_row_used: usize = 0;
        let mut auto_cur = 0usize;
        for child in bx.children.iter() {
            if super::is_out_of_flow(child) || matches!(child.display, super::super::layout::Display::None) { continue; }
            let exp_col = if child.grid_column_start > 0 { Some((child.grid_column_start - 1) as usize) } else { None };
            let exp_row = if child.grid_row_start > 0 { Some((child.grid_row_start - 1) as usize) } else { None };
            let span_col = if child.grid_column_span > 0 { child.grid_column_span as usize }
                           else if child.grid_column_end > 0 && child.grid_column_start > 0 { (child.grid_column_end - child.grid_column_start).max(1) as usize }
                           else { 1 }.min(cols_n);
            let span_row = if child.grid_row_span > 0 { child.grid_row_span as usize }
                           else if child.grid_row_end > 0 && child.grid_row_start > 0 { (child.grid_row_end - child.grid_row_start).max(1) as usize }
                           else { 1 };
            let (row, col) = if let (Some(r), Some(c)) = (exp_row, exp_col) { (r, c) }
                else if let Some(c) = exp_col {
                    let mut r = 0;
                    while occupied.len() <= (r + span_row) * cols_n { occupied.resize(((r + span_row + 1) * cols_n).max(1), false); }
                    while {
                        let mut blocked = false;
                        for dr in 0..span_row {
                            for dc in 0..span_col {
                                let i = (r + dr) * cols_n + (c + dc);
                                if i < occupied.len() && occupied[i] { blocked = true; break; }
                            }
                            if blocked { break; }
                        }
                        blocked
                    } { r += 1; while occupied.len() <= (r + span_row) * cols_n { occupied.resize(((r + span_row + 1) * cols_n).max(1), false); } }
                    (r, c)
                }
                else if let Some(r) = exp_row {
                    while occupied.len() <= (r + span_row) * cols_n { occupied.resize(((r + span_row + 1) * cols_n).max(1), false); }
                    let mut c = 0;
                    while {
                        let mut blocked = false;
                        for dr in 0..span_row {
                            for dc in 0..span_col {
                                let i = (r + dr) * cols_n + (c + dc);
                                if i < occupied.len() && occupied[i] { blocked = true; break; }
                            }
                            if blocked { break; }
                        }
                        blocked && c + span_col <= cols_n
                    } { c += 1; }
                    (r, c)
                }
                else {
                    let mut idx = auto_cur;
                    loop {
                        let r = idx / cols_n; let c = idx % cols_n;
                        if c + span_col > cols_n { idx = (r + 1) * cols_n; continue; }
                        while occupied.len() <= (r + span_row) * cols_n { occupied.resize(((r + span_row + 1) * cols_n).max(1), false); }
                        let mut blocked = false;
                        for dr in 0..span_row {
                            for dc in 0..span_col {
                                let i = (r + dr) * cols_n + (c + dc);
                                if i < occupied.len() && occupied[i] { blocked = true; break; }
                            }
                            if blocked { break; }
                        }
                        if !blocked { auto_cur = idx + 1; break (r, c); }
                        idx += 1;
                    }
                };
            for dr in 0..span_row {
                for dc in 0..span_col {
                    let i = (row + dr) * cols_n + (col + dc);
                    while occupied.len() <= i { occupied.push(false); }
                    if i < occupied.len() { occupied[i] = true; }
                }
            }
            if row + span_row > max_row_used { max_row_used = row + span_row; }
        }
        max_row_used
    };
    let rows = if !rows_explicit_str.is_empty() {
        let count = parse_track_count(&rows_explicit_str);
        count.max(needed_from_placement)
    } else {
        needed_from_placement.max(1)
    };

    // Resolve row tracks
    let mut row_tracks: Vec<f32> = if !rows_explicit_str.is_empty() {
        let resolved = resolve_tracks(&rows_explicit_str, inner_h, row_gap);
        if resolved.is_empty() { vec![inner_h.max(0.0).max(50.0)] }
        else { resolved }
    } else {
        // Bez template: vezmi explicit_height z dite v row, jinak rozdel inner_h.
        let mut out = Vec::with_capacity(rows);
        let mut any_explicit = false;
        for r in 0..rows {
            for c in 0..cols {
                let idx = r * cols + c;
                if let Some(child) = bx.children.get(idx) {
                    if child.explicit_height.is_some() { any_explicit = true; }
                }
            }
        }
        let fallback_h = if any_explicit { 0.0 } else if rows > 0 { (inner_h / rows as f32).max(0.0) } else { inner_h.max(0.0) };
        for r in 0..rows {
            let mut h = fallback_h;
            let mut row_has_explicit = false;
            for c in 0..cols {
                let idx = r * cols + c;
                if let Some(child) = bx.children.get(idx) {
                    if let Some(eh) = child.explicit_height {
                        // Floor by padding+border (item nemuze byt mensi).
                        let pb_t = child.padding_top.unwrap_or(child.padding) + child.border_top_width.unwrap_or(child.border_width);
                        let pb_b = child.padding_bottom.unwrap_or(child.padding) + child.border_bottom_width.unwrap_or(child.border_width);
                        let real_h = eh.max(pb_t + pb_b);
                        h = h.max(real_h);
                        row_has_explicit = true;
                    }
                }
            }
            // Pri row bez explicit + jine rows maji explicit: 0 floor (auto-sizing
            // dorovna z items). Drive 50 hardcoded - to bylo nesprapne pro aspect-ratio.
            if !row_has_explicit && any_explicit { h = 0.0; }
            out.push(h);
        }
        out
    };
    // Grid-auto-rows: tokens pro implicit rows (cycle, default Auto).
    let auto_row_tokens: Vec<Track> = if !bx.grid_auto_rows.is_empty() {
        parse_track_tokens_sized(&bx.grid_auto_rows, inner_h, row_gap)
    } else { Vec::new() };
    let auto_row_resolved: Vec<f32> = if !bx.grid_auto_rows.is_empty() {
        resolve_tracks(&bx.grid_auto_rows, inner_h, row_gap)
    } else { Vec::new() };
    // Implicitni rows: pokud potreba vic nez explicit, doplnit z grid-auto-rows cycle.
    while row_tracks.len() < rows {
        let r = row_tracks.len();
        // Pri grid-auto-rows nastav cyklem (delete zde row-from-children fallback).
        let auto_h = if !auto_row_resolved.is_empty() {
            let explicit_count = if !rows_explicit_str.is_empty() { parse_track_count(&rows_explicit_str) } else { 0 };
            let implicit_idx = r.saturating_sub(explicit_count);
            auto_row_resolved[implicit_idx % auto_row_resolved.len()]
        } else {
            let mut h = 0.0_f32;
            for c in 0..cols {
                let idx = r * cols + c;
                if let Some(child) = bx.children.get(idx) {
                    if let Some(eh) = child.explicit_height {
                        h = h.max(eh);
                    }
                }
            }
            h
        };
        row_tracks.push(auto_h);
    }
    let _ = auto_row_tokens;

    // In-flow indices (skip abs/fixed + display:none)
    let in_flow: Vec<usize> = bx.children.iter().enumerate()
        .filter(|(_, c)| !super::is_out_of_flow(c) && !matches!(c.display, super::super::layout::Display::None))
        .map(|(i, _)| i)
        .collect();
    for ch in bx.children.iter_mut() {
        if matches!(ch.display, super::super::layout::Display::None) {
            ch.rect.x = 0.0;
            ch.rect.y = 0.0;
            ch.rect.width = 0.0;
            ch.rect.height = 0.0;
        }
    }

    // Pre-pass: pre items bez explicit size, recursivne layout pro intrinsic.
    for &i in &in_flow {
        let ch = &mut bx.children[i];
        if ch.explicit_width.is_some() && ch.explicit_height.is_some() { continue; }
        if ch.children.is_empty() { continue; }
        let saved_x = ch.rect.x; let saved_y = ch.rect.y;
        ch.rect.x = 0.0; ch.rect.y = 0.0;
        if ch.explicit_width.is_none() { ch.rect.width = 0.0; }
        if ch.explicit_height.is_none() { ch.rect.height = 0.0; }
        match ch.display {
            super::super::layout::Display::Flex => super::flex::layout_flex(ch),
            super::super::layout::Display::Grid => super::grid::layout_grid(ch),
            _ => {}
        }
        ch.rect.x = saved_x; ch.rect.y = saved_y;
    }

    // Auto track sizing: pro auto cols/rows, najdi max item intrinsic width/height a expand track.
    // Dummy placement: spocti pro kazdy item (row, col, span) a ulozit.
    let any_auto_col = col_is_auto.iter().any(|&b| b);
    if any_auto_col && !in_flow.is_empty() {
        // Dummy placement (jen kvuli zjisteni col placement).
        let mut occupied_d: Vec<bool> = vec![false; rows.max(1) * cols.max(1)];
        let mut auto_cursor_d = 0usize;
        let mut item_placement: Vec<(usize, usize, usize, usize)> = Vec::new(); // (row, col, span_row, span_col)
        for &real_idx in in_flow.iter() {
            let child = &bx.children[real_idx];
            let explicit_col = if child.grid_column_start > 0 { Some((child.grid_column_start - 1) as usize) } else { None };
            let explicit_row = if child.grid_row_start > 0 { Some((child.grid_row_start - 1) as usize) } else { None };
            let resolve_end = |start: i32, end: i32, span: i32, count: usize| -> usize {
                if span > 0 { return span as usize; }
                if end < 0 && start > 0 {
                    let end_line = (count as i32 + 1 + end + 1).max(start + 1);
                    ((end_line - start).max(1)) as usize
                } else if end > 0 && start > 0 {
                    ((end - start).max(1)) as usize
                } else { 1 }
            };
            let span_col = resolve_end(child.grid_column_start, child.grid_column_end, child.grid_column_span, cols);
            let span_row = resolve_end(child.grid_row_start, child.grid_row_end, child.grid_row_span, rows);
            let (row, col) = if let (Some(r), Some(c)) = (explicit_row, explicit_col) {
                (r, c)
            } else if let Some(c) = explicit_col {
                let mut r = 0;
                loop {
                    if r * cols + c >= occupied_d.len() { break; }
                    if !occupied_d[r * cols + c] { break; }
                    r += 1;
                }
                (r, c)
            } else if let Some(r) = explicit_row {
                let mut c = 0;
                loop {
                    if r * cols + c >= occupied_d.len() { break; }
                    if !occupied_d[r * cols + c] { break; }
                    c += 1;
                }
                (r, c)
            } else {
                let mut idx = auto_cursor_d;
                while idx < occupied_d.len() && occupied_d[idx] { idx += 1; }
                auto_cursor_d = idx + 1;
                (idx / cols.max(1), idx % cols.max(1))
            };
            for dr in 0..span_row {
                for dc in 0..span_col {
                    let idx = (row + dr) * cols + (col + dc);
                    if idx < occupied_d.len() { occupied_d[idx] = true; }
                }
            }
            item_placement.push((row, col, span_row, span_col));
        }
        // Pro kazdy auto col, najdi intrinsic width z items.
        // FitContent track aplikuje clamp(min-content, max(min-content, arg), max-content).
        for c_idx in 0..cols {
            if !col_is_auto[c_idx] { continue; }
            let mut max_content = 0.0_f32;
            let mut min_content = 0.0_f32;
            for (i, &real_idx) in in_flow.iter().enumerate() {
                let (_, col, _, span_col) = item_placement[i];
                if col == c_idx && span_col == 1 {
                    let item = &bx.children[real_idx];
                    let text_min = if item.taffy_mode {
                        if let Some(t) = &item.text {
                            let mut max_seg = 0usize; let mut cur = 0usize;
                            for c in t.chars() {
                                if matches!(c, '\u{200B}' | ' ' | '\n' | '\t') {
                                    if cur > max_seg { max_seg = cur; } cur = 0;
                                } else { cur += 1; }
                            }
                            if cur > max_seg { max_seg = cur; }
                            max_seg as f32 * 10.0
                        } else { 0.0 }
                    } else { 0.0 };
                    let text_max = if item.taffy_mode {
                        if let Some(t) = &item.text {
                            t.chars().filter(|c| !matches!(*c, '\u{200B}' | ' ' | '\n' | '\t')).count() as f32 * 10.0
                        } else { 0.0 }
                    } else { 0.0 };
                    let pb_l = item.padding_left.unwrap_or(item.padding) + item.border_left_width.unwrap_or(item.border_width);
                    let pb_r = item.padding_right.unwrap_or(item.padding) + item.border_right_width.unwrap_or(item.border_width);
                    let cw_min_p = super::super::layout::parse_length(&item.min_width_v);
                    let item_max = item.explicit_width.unwrap_or(item.rect.width).max(text_max).max(pb_l + pb_r).max(cw_min_p);
                    // Min-content rekurzivne: pri item bez text + children, walk first
                    // descendant chain a najdi nejvetsi child explicit_width nebo
                    // text_min_content. Drive jen rect.width = max-content.
                    fn deep_min_content(b: &super::super::layout::LayoutBox) -> f32 {
                        if let Some(w) = b.explicit_width { return w; }
                        if b.taffy_mode {
                            if let Some(t) = &b.text {
                                let mut max_seg = 0usize; let mut cur = 0usize;
                                for c in t.chars() {
                                    if matches!(c, '\u{200B}' | ' ' | '\n' | '\t') {
                                        if cur > max_seg { max_seg = cur; } cur = 0;
                                    } else { cur += 1; }
                                }
                                if cur > max_seg { max_seg = cur; }
                                return max_seg as f32 * 10.0;
                            }
                        }
                        let mut m = 0.0_f32;
                        for c in b.children.iter() {
                            if matches!(c.position, super::super::layout::Position::Absolute | super::super::layout::Position::Fixed) { continue; }
                            if matches!(c.display, super::super::layout::Display::None) { continue; }
                            let cm = deep_min_content(c);
                            if cm > m { m = cm; }
                        }
                        m
                    }
                    let recursive_min = if item.taffy_mode && item.text.is_none() && !item.children.is_empty() {
                        deep_min_content(item)
                    } else { 0.0 };
                    let item_min_base = if recursive_min > 0.0 {
                        recursive_min.max(text_min)
                    } else {
                        item.explicit_width.unwrap_or(item.rect.width).max(text_min)
                    };
                    let item_min = item_min_base.max(pb_l + pb_r).max(cw_min_p);
                    if item_max > max_content { max_content = item_max; }
                    if item_min > min_content { min_content = item_min; }
                }
            }
            // Pri FitContent: clamp(min-content, max(min-content, arg), max-content).
            // Pri Auto/MaxContent/MinContent: starting size = min_content.
            // (Distinction MaxContent vs Auto vyuzita az v span distribute extra space.)
            // Pri Minmax(max-content, ...) NaN sentinel: pouzij max_content.
            let track_size = if let Some(Track::FitContent(arg)) = col_token_kinds.get(c_idx) {
                let arg_resolved = if *arg < 0.0 { inner_w * (-arg) } else { *arg };
                max_content.min(arg_resolved.max(min_content))
            } else if let Some(Track::Minmax(min, _, false)) = col_token_kinds.get(c_idx) {
                if min.is_nan() { max_content }
                else if (*min - (-1000.0)).abs() < 0.5 { min_content }
                else { min_content }
            } else {
                min_content
            };
            col_tracks[c_idx] = track_size;
        }
        // Span items: distribute min/max content extra space (CSS Grid §11.5.5).
        // Spustit PRED redistribute leftover - jinak by se tracky uz inflated rovnomerne.
        for (i, &real_idx) in in_flow.iter().enumerate() {
            let (_, col, _, span_col) = item_placement[i];
            if span_col <= 1 { continue; }
            let item = &bx.children[real_idx];
            if item.explicit_width.is_some() { continue; }
            let text_min = if item.taffy_mode {
                if let Some(t) = &item.text {
                    let mut max_seg = 0usize; let mut cur = 0usize;
                    for c in t.chars() {
                        if matches!(c, '\u{200B}' | ' ' | '\n' | '\t') {
                            if cur > max_seg { max_seg = cur; } cur = 0;
                        } else { cur += 1; }
                    }
                    if cur > max_seg { max_seg = cur; }
                    max_seg as f32 * 10.0
                } else { 0.0 }
            } else { 0.0 };
            let text_max = if item.taffy_mode {
                if let Some(t) = &item.text {
                    t.chars().filter(|c| !matches!(*c, '\u{200B}' | ' ' | '\n' | '\t')).count() as f32 * 10.0
                } else { 0.0 }
            } else { 0.0 };
            let item_min = text_min;
            let item_max = text_max;
            if item_max <= 0.0 && item_min <= 0.0 { continue; }
            let span_indices: Vec<usize> = (col..(col+span_col)).collect();
            let total_gap_s = col_gap * (span_col.saturating_sub(1) as f32);
            let cur_sum: f32 = span_indices.iter().map(|&c| col_tracks[c]).sum::<f32>() + total_gap_s;
            if cur_sum < item_min {
                let recipients: Vec<usize> = span_indices.iter().copied().filter(|&c| matches!(col_token_kinds.get(c),
                    Some(Track::Auto) | Some(Track::MaxContent) | Some(Track::MinContent) | Some(Track::FitContent(_)))).collect();
                if !recipients.is_empty() {
                    let deficit = item_min - cur_sum;
                    let share = deficit / recipients.len() as f32;
                    for &c in &recipients { col_tracks[c] += share; }
                }
            }
            let cur_sum2: f32 = span_indices.iter().map(|&c| col_tracks[c]).sum::<f32>() + total_gap_s;
            if cur_sum2 < item_max {
                // Priority tiers (CSS §11.5.5):
                //  1. MaxContent tracks (highest priority)
                //  2. FitContent tracks (capped by arg)
                //  3. Auto tracks (only kdyz tier 1+2 prazdne)
                let tier1: Vec<usize> = span_indices.iter().copied().filter(|&c| matches!(col_token_kinds.get(c), Some(Track::MaxContent))).collect();
                let tier2: Vec<usize> = span_indices.iter().copied().filter(|&c| matches!(col_token_kinds.get(c), Some(Track::FitContent(_)))).collect();
                let tier3: Vec<usize> = span_indices.iter().copied().filter(|&c| matches!(col_token_kinds.get(c), Some(Track::Auto))).collect();
                let recipients = if !tier1.is_empty() {
                    tier1
                } else if !tier2.is_empty() {
                    tier2
                } else {
                    tier3
                };
                if !recipients.is_empty() {
                    let deficit = item_max - cur_sum2;
                    let share = deficit / recipients.len() as f32;
                    for &c in &recipients {
                        let cap = if let Some(Track::FitContent(arg)) = col_token_kinds.get(c) {
                            if *arg < 0.0 { inner_w * (-arg) } else { *arg }
                        } else { f32::INFINITY };
                        col_tracks[c] = (col_tracks[c] + share).min(cap.max(col_tracks[c]));
                    }
                }
            }
        }
        // Distribute leftover free space rovnomerne mezi auto cols (NE FitContent).
        // FitContent jiz ma fixed velikost dle arg.
        // Minmax s finite max: clamp na max.
        if inner_w > 0.0 {
            let total_used: f32 = col_tracks.iter().sum::<f32>() + col_gap * (cols.saturating_sub(1) as f32);
            let leftover = inner_w - total_used;
            let mut redistributable_cols: Vec<usize> = Vec::new();
            for c_idx in 0..cols {
                if !col_is_auto[c_idx] { continue; }
                if matches!(col_token_kinds.get(c_idx), Some(Track::FitContent(_))) { continue; }
                redistributable_cols.push(c_idx);
            }
            if leftover > 0.0 && !redistributable_cols.is_empty() {
                let share = leftover / redistributable_cols.len() as f32;
                for &c_idx in &redistributable_cols {
                    let pre_redist = col_tracks[c_idx];
                    col_tracks[c_idx] += share;
                    // Pri Minmax s finite max: clamp na max(item_min, max_r).
                    if let Some(Track::Minmax(_, max_v, false)) = col_token_kinds.get(c_idx) {
                        let max_r = if max_v.is_nan() { f32::INFINITY }
                                    else if (*max_v - (-1000.0)).abs() < 0.5 { f32::INFINITY }
                                    else if *max_v < 0.0 && *max_v > -2.0 { inner_w * (-max_v) }
                                    else { *max_v };
                        if max_r.is_finite() && col_tracks[c_idx] > max_r {
                            // Pri item_min > max: zachova item_min (CSS spec - min wins).
                            col_tracks[c_idx] = max_r.max(pre_redist);
                        }
                    }
                }
            }
        }
    }
    // Fr-track expansion: pri item spanu 1 fr-only col s explicit_width vetsim nez
    // current track size, expand track na item width a redistribute zbytek mezi
    // ostatnimi fr tracky podle jejich fr factor.
    if inner_w > 0.0 && !in_flow.is_empty() {
        // Najdi fr cols (Track::Fr nebo Minmax(_, fr, true))
        let fr_factors: Vec<f32> = (0..cols).map(|i| match col_token_kinds.get(i) {
            Some(Track::Fr(f)) => *f,
            Some(Track::Minmax(_, f, true)) => *f,
            _ => 0.0,
        }).collect();
        // is_fr_track = jakkoliv fr (vc. 0fr).
        let is_fr_track: Vec<bool> = (0..cols).map(|i| matches!(
            col_token_kinds.get(i),
            Some(Track::Fr(_)) | Some(Track::Minmax(_, _, true))
        )).collect();
        let any_fr = is_fr_track.iter().any(|&b| b);
        if any_fr {
            // Replicovat dummy placement (stejne jako auto-col, ale i kdyz any_auto_col=false)
            let mut occupied_d: Vec<bool> = vec![false; rows.max(1) * cols.max(1)];
            let mut auto_cursor_d = 0usize;
            let mut item_placements: Vec<(usize, usize, usize, usize)> = Vec::new();
            for &real_idx in in_flow.iter() {
                let child = &bx.children[real_idx];
                let explicit_col = if child.grid_column_start > 0 { Some((child.grid_column_start - 1) as usize) } else { None };
                let explicit_row = if child.grid_row_start > 0 { Some((child.grid_row_start - 1) as usize) } else { None };
                let span_col = if child.grid_column_span > 0 { child.grid_column_span as usize }
                               else if child.grid_column_end > 0 && child.grid_column_start > 0 { (child.grid_column_end - child.grid_column_start).max(1) as usize }
                               else { 1 };
                let span_row = if child.grid_row_span > 0 { child.grid_row_span as usize }
                               else if child.grid_row_end > 0 && child.grid_row_start > 0 { (child.grid_row_end - child.grid_row_start).max(1) as usize }
                               else { 1 };
                let (row, col) = if let (Some(r), Some(c)) = (explicit_row, explicit_col) { (r, c) }
                    else if let Some(c) = explicit_col { let mut r = 0; while r * cols + c < occupied_d.len() && occupied_d[r * cols + c] { r += 1; } (r, c) }
                    else if let Some(r) = explicit_row { let mut c = 0; while r * cols + c < occupied_d.len() && occupied_d[r * cols + c] { c += 1; } (r, c) }
                    else { let mut idx = auto_cursor_d; while idx < occupied_d.len() && occupied_d[idx] { idx += 1; } auto_cursor_d = idx + 1; (idx / cols.max(1), idx % cols.max(1)) };
                for dr in 0..span_row { for dc in 0..span_col {
                    let idx = (row + dr) * cols + (col + dc);
                    if idx < occupied_d.len() { occupied_d[idx] = true; }
                }}
                item_placements.push((row, col, span_row, span_col));
            }
            // Track ktere col_tracks byly item-driven (single-span req nebo multispan distribution)
            // - tyto neoverwritnout pri redistribute.
            let mut item_driven: Vec<bool> = vec![false; cols];
            // Pro kazdy item span 1 col, kdyz fr a explicit_width > current track, expand.
            for _iter in 0..3 {
                let mut changed = false;
                let fixed_total: f32 = (0..cols).map(|i| if !is_fr_track[i] { col_tracks[i] } else { 0.0 }).sum();
                let total_gap_w = col_gap * (cols.saturating_sub(1) as f32);
                let mut available_for_fr = (inner_w - fixed_total - total_gap_w).max(0.0);
                let mut active_fr_total: f32 = fr_factors.iter().sum();
                // Najdi nejvyssi requested expansion u fr cols (single-span).
                for c_idx in 0..cols {
                    if !is_fr_track[c_idx] { continue; }
                    let mut req = 0.0_f32;
                    for (i, &real_idx) in in_flow.iter().enumerate() {
                        let (_, col, _, span_col) = item_placements[i];
                        if col == c_idx && span_col == 1 {
                            let item = &bx.children[real_idx];
                            if let Some(w) = item.explicit_width {
                                if w > req { req = w; }
                            }
                            if item.taffy_mode {
                                if let Some(t) = &item.text {
                                    let tw = t.chars().filter(|c| !matches!(*c, '\u{200B}' | ' ' | '\n' | '\t')).count() as f32 * 10.0;
                                    if tw > req { req = tw; }
                                }
                            }
                        }
                    }
                    if req > col_tracks[c_idx] {
                        col_tracks[c_idx] = req;
                        available_for_fr -= req;
                        active_fr_total -= fr_factors[c_idx];
                        item_driven[c_idx] = true;
                        changed = true;
                    } else if item_driven[c_idx] {
                        // Mark cols already exempt z redistribute (vc. predchozich iterations).
                        available_for_fr -= col_tracks[c_idx];
                        active_fr_total -= fr_factors[c_idx];
                    }
                }
                // Multi-span items.
                for (i, &real_idx) in in_flow.iter().enumerate() {
                    let (_, col, _, span_col) = item_placements[i];
                    if span_col <= 1 { continue; }
                    if let Some(w) = bx.children[real_idx].explicit_width {
                        let span_sum: f32 = (col..(col+span_col)).map(|c| col_tracks.get(c).copied().unwrap_or(0.0)).sum();
                        let span_gap = col_gap * (span_col.saturating_sub(1) as f32);
                        let needed = w - span_sum - span_gap;
                        if needed > 0.0 {
                            let fr_in_span: Vec<usize> = (col..(col+span_col)).filter(|&c| is_fr_track.get(c).copied().unwrap_or(false)).collect();
                            if !fr_in_span.is_empty() {
                                let fr_sum_span: f32 = fr_in_span.iter().map(|&c| fr_factors[c]).sum();
                                if fr_sum_span > 0.0 {
                                    for &c in &fr_in_span {
                                        col_tracks[c] += needed * fr_factors[c] / fr_sum_span;
                                        item_driven[c] = true;
                                    }
                                } else {
                                    let share = needed / fr_in_span.len() as f32;
                                    for &c in &fr_in_span {
                                        col_tracks[c] += share;
                                        item_driven[c] = true;
                                    }
                                }
                                changed = true;
                            }
                        }
                    }
                }
                // Maximize non-fr minmax tracks PRED fr distribute (CSS Grid §11.5).
                // Grow minmax(min, max_finite) az do max, beraj z available_for_fr.
                if available_for_fr > 0.0 {
                    for c_idx in 0..cols {
                        if is_fr_track[c_idx] { continue; }
                        if let Some(Track::Minmax(min_v, max_v, false)) = col_token_kinds.get(c_idx) {
                            if max_v.is_finite() && !max_v.is_nan() {
                                // -1000 = min-content sentinel
                                let is_min_content = (*max_v - (-1000.0)).abs() < 0.5;
                                if is_min_content { continue; }
                                let max_r = if *max_v < 0.0 && *max_v > -2.0 { inner_w * (-max_v) } else { *max_v };
                                let min_r = if min_v.is_nan() { col_tracks[c_idx] }
                                            else if (*min_v - (-1000.0)).abs() < 0.5 { col_tracks[c_idx] }
                                            else if *min_v < 0.0 && *min_v > -2.0 { inner_w * (-min_v) }
                                            else { *min_v };
                                let grow_room = (max_r - col_tracks[c_idx].max(min_r)).max(0.0);
                                let grow = grow_room.min(available_for_fr);
                                if grow > 0.0 {
                                    col_tracks[c_idx] += grow;
                                    available_for_fr -= grow;
                                    changed = true;
                                }
                            }
                        }
                    }
                }
                // Redistribute zbytek mezi non-item-driven fr tracky.
                if active_fr_total > 0.0 && available_for_fr >= 0.0 {
                    let fr_size = available_for_fr / active_fr_total.max(1.0);
                    for c_idx in 0..cols {
                        if item_driven[c_idx] { continue; }
                        let f = fr_factors[c_idx];
                        if f > 0.0 {
                            let new_size = fr_size * f;
                            if (col_tracks[c_idx] - new_size).abs() > 0.01 {
                                col_tracks[c_idx] = new_size;
                                changed = true;
                            }
                        }
                    }
                }
                if !changed { break; }
            }
        }
    }
    // Auto rows similar.
    let row_token_kinds = parse_track_tokens_sized(&rows_explicit_str, inner_h, row_gap);
    let row_is_auto: Vec<bool> = (0..rows).map(|i| match row_token_kinds.get(i) {
        Some(Track::Auto) => true,
        Some(Track::Minmax(_, max, false)) if !max.is_finite() => true,
        _ => rows_explicit_str.is_empty(),
    }).collect();
    let any_auto_row = rows_explicit_str.is_empty() || row_is_auto.iter().any(|&b| b);
    if any_auto_row && !in_flow.is_empty() {
        // Pro radky bez template, dej max item explicit_height (uz jsme to delali). Ted jeste rect.height.
        let mut by_row: std::collections::HashMap<usize, f32> = std::collections::HashMap::new();
        let mut occupied_d: Vec<bool> = vec![false; rows.max(1) * cols.max(1)];
        let mut auto_cursor_d = 0usize;
        for &real_idx in in_flow.iter() {
            let child = &bx.children[real_idx];
            let explicit_col = if child.grid_column_start > 0 { Some((child.grid_column_start - 1) as usize) } else { None };
            let explicit_row = if child.grid_row_start > 0 { Some((child.grid_row_start - 1) as usize) } else { None };
            let span_col = if child.grid_column_span > 0 { child.grid_column_span as usize }
                           else if child.grid_column_end > 0 && child.grid_column_start > 0 { (child.grid_column_end - child.grid_column_start).max(1) as usize }
                           else { 1 };
            let span_row = if child.grid_row_span > 0 { child.grid_row_span as usize }
                           else if child.grid_row_end > 0 && child.grid_row_start > 0 { (child.grid_row_end - child.grid_row_start).max(1) as usize }
                           else { 1 };
            let (row, col) = if let (Some(r), Some(c)) = (explicit_row, explicit_col) { (r, c) }
                else if let Some(c) = explicit_col { let mut r = 0; while r * cols + c < occupied_d.len() && occupied_d[r * cols + c] { r += 1; } (r, c) }
                else if let Some(r) = explicit_row { let mut c = 0; while r * cols + c < occupied_d.len() && occupied_d[r * cols + c] { c += 1; } (r, c) }
                else { let mut idx = auto_cursor_d; while idx < occupied_d.len() && occupied_d[idx] { idx += 1; } auto_cursor_d = idx + 1; (idx / cols.max(1), idx % cols.max(1)) };
            for dr in 0..span_row {
                for dc in 0..span_col {
                    let idx = (row + dr) * cols + (col + dc);
                    if idx < occupied_d.len() { occupied_d[idx] = true; }
                }
            }
            if span_row == 1 {
                let item = &bx.children[real_idx];
                let mut h = item.explicit_height.unwrap_or(item.rect.height);
                // Text intrinsic height v taffy_mode = 10 per visible line (estimate by ZWS/whitespace breaks not done).
                if item.taffy_mode && item.text.is_some() && h == 0.0 {
                    h = 10.0;
                }
                // Aspect-ratio: dopocti h z width pri auto height + text item.
                if let Some(ar) = item.aspect_ratio {
                    if ar > 0.0 && item.explicit_height.is_none() {
                        let mut iw = item.explicit_width.unwrap_or(item.rect.width);
                        if iw == 0.0 && item.taffy_mode && item.text.is_some() {
                            if let Some(t) = &item.text {
                                iw = t.chars().filter(|c| !matches!(*c, '\u{200B}' | ' ' | '\n' | '\t')).count() as f32 * 10.0;
                            }
                        }
                        if iw > 0.0 {
                            let h_ar = iw / ar;
                            if h_ar > h { h = h_ar; }
                        }
                    }
                }
                // Pripocti vertikalni margins (CSS Grid item margin uvnitr cell).
                let m_t = item.margin_top.unwrap_or(item.margin);
                let m_b = item.margin_bottom.unwrap_or(item.margin);
                h += m_t + m_b;
                let entry = by_row.entry(row).or_insert(0.0);
                if h > *entry { *entry = h; }
            }
        }
        for (r, h) in by_row {
            if r < row_tracks.len() && row_tracks[r] < h { row_tracks[r] = h; }
        }
    }

    // Auto-fit collapse: tracky bez items collapsuji na 0 (vc. gap mezi nimi).
    let any_autofit = col_is_autofit.iter().any(|&b| b);
    let mut col_collapsed: Vec<bool> = vec![false; cols];
    if any_autofit && !in_flow.is_empty() {
        // Re-compute placement pro detekci occupied cols.
        let mut occupied_cols: Vec<bool> = vec![false; cols];
        let mut occupied_grid: Vec<bool> = vec![false; rows.max(1) * cols.max(1)];
        let mut auto_cur = 0usize;
        for &real_idx in in_flow.iter() {
            let child = &bx.children[real_idx];
            let explicit_col = if child.grid_column_start > 0 { Some((child.grid_column_start - 1) as usize) } else { None };
            let explicit_row = if child.grid_row_start > 0 { Some((child.grid_row_start - 1) as usize) } else { None };
            let span_col = if child.grid_column_span > 0 { child.grid_column_span as usize }
                           else if child.grid_column_end > 0 && child.grid_column_start > 0 { (child.grid_column_end - child.grid_column_start).max(1) as usize }
                           else { 1 };
            let span_row = if child.grid_row_span > 0 { child.grid_row_span as usize }
                           else if child.grid_row_end > 0 && child.grid_row_start > 0 { (child.grid_row_end - child.grid_row_start).max(1) as usize }
                           else { 1 };
            let (row, col) = if let (Some(r), Some(c)) = (explicit_row, explicit_col) { (r, c) }
                else if let Some(c) = explicit_col { let mut r = 0; while r * cols + c < occupied_grid.len() && occupied_grid[r * cols + c] { r += 1; } (r, c) }
                else if let Some(r) = explicit_row { let mut c = 0; while r * cols + c < occupied_grid.len() && occupied_grid[r * cols + c] { c += 1; } (r, c) }
                else { let mut idx = auto_cur; while idx < occupied_grid.len() && occupied_grid[idx] { idx += 1; } auto_cur = idx + 1; (idx / cols.max(1), idx % cols.max(1)) };
            for dr in 0..span_row { for dc in 0..span_col {
                let idx = (row + dr) * cols + (col + dc);
                if idx < occupied_grid.len() { occupied_grid[idx] = true; }
                if col + dc < cols { occupied_cols[col + dc] = true; }
            }}
        }
        for c_idx in 0..cols {
            if col_is_autofit[c_idx] && !occupied_cols[c_idx] {
                col_tracks[c_idx] = 0.0;
                col_collapsed[c_idx] = true;
            }
        }
    }
    let collapsed_count = col_collapsed.iter().filter(|&&b| b).count();
    let active_cols = cols.saturating_sub(collapsed_count);
    // Pri align-content default + explicit container_h + no row template: stretch rows do container.
    // Pri align-items=baseline rows musi byt stretch aby baselines mohly fungovat.
    let ac = bx.align_content.trim();
    let ac_stretch = (ac.is_empty() || ac == "normal" || ac == "stretch")
        && bx.explicit_height.is_some()
        && rows_explicit_str.is_empty()
        && bx.align_items == "baseline";
    if ac_stretch && rows > 0 {
        let total_row_pre: f32 = row_tracks.iter().sum::<f32>() + row_gap * (rows.saturating_sub(1) as f32);
        let extra = inner_h - total_row_pre;
        if extra > 0.0 {
            let share = extra / rows as f32;
            for h in row_tracks.iter_mut() { *h += share; }
        }
    }
    let total_col: f32 = col_tracks.iter().sum::<f32>() + col_gap * (active_cols.saturating_sub(1) as f32);
    let total_row: f32 = row_tracks.iter().sum::<f32>() + row_gap * (rows.saturating_sub(1) as f32);
    let (jc_start, jc_between) = grid_distribute(&bx.justify_content, inner_w - total_col, active_cols.max(1));
    let (ac_start, ac_between) = grid_distribute(&bx.align_content, inner_h - total_row, rows);
    let mut col_positions: Vec<f32> = Vec::with_capacity(cols);
    let mut x_cursor = jc_start;
    for (i, w) in col_tracks.iter().enumerate() {
        col_positions.push(x_cursor);
        x_cursor += *w;
        // Gap+between jen mezi non-collapsed adjacent tracks.
        if i + 1 < cols && !col_collapsed[i] && !col_collapsed[i + 1] {
            x_cursor += col_gap + jc_between;
        }
    }
    let mut row_positions: Vec<f32> = Vec::with_capacity(rows);
    let mut y_cursor = ac_start;
    for (i, h) in row_tracks.iter().enumerate() {
        row_positions.push(y_cursor);
        y_cursor += *h;
        if i + 1 < rows { y_cursor += row_gap + ac_between; }
    }

    // Place items - explicit (grid-row-start/grid-column-start) i auto-flow row major.
    // Track occupied cells.
    let mut occupied: Vec<bool> = vec![false; rows.max(1) * cols.max(1)];
    let mut auto_cursor = 0usize;
    // Per-item placement info pro baseline pass.
    let mut item_row_info: Vec<(usize, usize, f32)> = Vec::new(); // (real_idx, row, cy)
    for &real_idx in in_flow.iter() {
        let child = &bx.children[real_idx];
        // Resolve placement: 1-based start lines -> 0-based cell index
        let explicit_col = if child.grid_column_start > 0 { Some((child.grid_column_start - 1) as usize) } else { None };
        let explicit_row = if child.grid_row_start > 0 { Some((child.grid_row_start - 1) as usize) } else { None };
        // grid-column/row-end < 0 = pocita od konce. -1 = posledni linie = posledni track end.
        let resolve_end = |start: i32, end: i32, span: i32, count: usize| -> usize {
            if span > 0 { return span as usize; }
            if end < 0 && start > 0 {
                let end_line = (count as i32 + 1 + end + 1).max(start + 1);
                ((end_line - start).max(1)) as usize
            } else if end > 0 && start > 0 {
                ((end - start).max(1)) as usize
            } else { 1 }
        };
        let span_col = resolve_end(child.grid_column_start, child.grid_column_end, child.grid_column_span, cols);
        let span_row = resolve_end(child.grid_row_start, child.grid_row_end, child.grid_row_span, rows);
        let (row, col) = if let (Some(r), Some(c)) = (explicit_row, explicit_col) {
            (r, c)
        } else if let Some(c) = explicit_col {
            // Find first auto row with c free
            let mut r = 0;
            loop {
                if r * cols + c >= occupied.len() { break; }
                if !occupied[r * cols + c] { break; }
                r += 1;
            }
            (r, c)
        } else if let Some(r) = explicit_row {
            let mut c = 0;
            loop {
                if r * cols + c >= occupied.len() { break; }
                if !occupied[r * cols + c] { break; }
                c += 1;
            }
            (r, c)
        } else {
            // Auto - najit prvni volnou bunku od auto_cursor
            let mut idx = auto_cursor;
            while idx < occupied.len() && occupied[idx] { idx += 1; }
            auto_cursor = idx + 1;
            (idx / cols, idx % cols)
        };
        // Mark span cells occupied
        for dr in 0..span_row {
            for dc in 0..span_col {
                let idx = (row + dr) * cols + (col + dc);
                if idx < occupied.len() { occupied[idx] = true; }
            }
        }
        // Compute size from spanned tracks
        let cw: f32 = (0..span_col).map(|d| col_tracks.get(col + d).copied().unwrap_or(0.0)).sum::<f32>()
            + col_gap * (span_col.saturating_sub(1) as f32);
        let ch_h: f32 = (0..span_row).map(|d| row_tracks.get(row + d).copied().unwrap_or(0.0)).sum::<f32>()
            + row_gap * (span_row.saturating_sub(1) as f32);
        let cx = col_positions.get(col).copied().unwrap_or(0.0);
        let cy = row_positions.get(row).copied().unwrap_or(0.0);
        item_row_info.push((real_idx, row, cy));
        // Resolve item size + alignment v grid area
        let parent_align_items = bx.align_items.clone();
        let parent_justify_items = bx.justify_items.clone();
        let child = &mut bx.children[real_idx];
        // Resolve margin pct proti grid CELL size (CSS spec: percent margin v gridu
        // resolvuje proti inline-size CB = cell width).
        if let Some(p) = child.margin_left_pct { child.margin_left = Some(cw * p); }
        if let Some(p) = child.margin_right_pct { child.margin_right = Some(cw * p); }
        if let Some(p) = child.margin_top_pct { child.margin_top = Some(cw * p); }
        if let Some(p) = child.margin_bottom_pct { child.margin_bottom = Some(cw * p); }
        let m_l = child.margin_left.unwrap_or(child.margin);
        let m_r = child.margin_right.unwrap_or(child.margin);
        let m_t = child.margin_top.unwrap_or(child.margin);
        let m_b = child.margin_bottom.unwrap_or(child.margin);
        let cw_avail = (cw - m_l - m_r).max(0.0);
        let ch_avail = (ch_h - m_t - m_b).max(0.0);
        let has_w = child.explicit_width.is_some();
        let has_h = child.explicit_height.is_some();
        // Text intrinsic v taffy_mode pri auto margin override (overrides stretch).
        let any_auto_x = child.margin_left_auto || child.margin_right_auto;
        let any_auto_y = child.margin_top_auto || child.margin_bottom_auto;
        let (text_w, text_h) = if child.taffy_mode {
            if let Some(t) = &child.text {
                let tw = t.chars().filter(|c| !matches!(*c, '\u{200B}' | ' ' | '\n' | '\t')).count() as f32 * 10.0;
                (tw, 10.0)
            } else { (0.0, 0.0) }
        } else { (0.0, 0.0) };
        let item_w = child.explicit_width.unwrap_or_else(|| if any_auto_x && text_w > 0.0 { text_w } else { cw_avail });
        let item_h = child.explicit_height.unwrap_or_else(|| if any_auto_y && text_h > 0.0 { text_h } else { ch_avail });
        // Text wrap: pri non-stretch align-self + text width > available, dopocti
        // pocet linek pro height intrinsic.
        let wrapped_text_h = if child.taffy_mode && child.text.is_some() {
            if let Some(t) = &child.text {
                let avail_w = child.explicit_width.unwrap_or(cw_avail);
                let max_w = if !child.max_width_v.is_empty() {
                    let mw = super::super::layout::parse_length(&child.max_width_v);
                    avail_w.min(mw)
                } else { avail_w };
                if max_w > 0.0 && text_w > max_w {
                    // Spocti pocet linek: greedy - kazda linka prijima segments dokud
                    // se vejdou. Segment = chars mezi ZWS/space/newline.
                    let mut lines = 1usize;
                    let mut cur_w = 0.0_f32;
                    let mut seg_w = 0.0_f32;
                    for c in t.chars() {
                        if matches!(c, '\u{200B}' | ' ' | '\n' | '\t') {
                            if cur_w + seg_w <= max_w + 0.01 {
                                cur_w += seg_w;
                            } else {
                                lines += 1;
                                cur_w = seg_w;
                            }
                            seg_w = 0.0;
                        } else {
                            seg_w += 10.0;
                        }
                    }
                    if seg_w > 0.0 {
                        if cur_w + seg_w > max_w + 0.01 { lines += 1; }
                    }
                    Some(lines as f32 * 10.0)
                } else { None }
            } else { None }
        } else { None };
        // justify-self na inline (cols), align-self na block (rows). Default = stretch.
        let js = if !child.justify_self.is_empty() { child.justify_self.clone() } else { parent_justify_items };
        let als = if !child.align_self.is_empty() { child.align_self.clone() } else { parent_align_items };
        let stretch_w = !has_w && !any_auto_x && (js.is_empty() || js == "stretch" || js == "normal");
        let stretch_h = !has_h && !any_auto_y && (als.is_empty() || als == "stretch" || als == "normal");
        let mut final_w = if stretch_w { cw_avail } else { item_w };
        let mut final_h = if stretch_h { ch_avail } else if let Some(wh) = wrapped_text_h { wh } else { item_h };
        // Apply min/max + padding+border floor (item nemuze byt mensi nez padding+border).
        let cw_min = super::super::layout::parse_length(&child.min_width_v);
        let cw_max = if child.max_width_v.is_empty() { f32::INFINITY } else { super::super::layout::parse_length(&child.max_width_v) };
        let ch_min = super::super::layout::parse_length(&child.min_height_v);
        let ch_max = if child.max_height_v.is_empty() { f32::INFINITY } else { super::super::layout::parse_length(&child.max_height_v) };
        let pb_l = child.padding_left.unwrap_or(child.padding) + child.border_left_width.unwrap_or(child.border_width);
        let pb_r = child.padding_right.unwrap_or(child.padding) + child.border_right_width.unwrap_or(child.border_width);
        let pb_t = child.padding_top.unwrap_or(child.padding) + child.border_top_width.unwrap_or(child.border_width);
        let pb_b = child.padding_bottom.unwrap_or(child.padding) + child.border_bottom_width.unwrap_or(child.border_width);
        let min_w_floor = pb_l + pb_r;
        let min_h_floor = pb_t + pb_b;
        final_w = final_w.min(cw_max);
        if cw_min > 0.0 { final_w = final_w.max(cw_min); }
        final_w = final_w.max(min_w_floor);
        // Aspect-ratio override pri stretch + jeden explicit rozmer
        if let Some(ar) = child.aspect_ratio {
            if ar > 0.0 {
                if !has_h { final_h = final_w / ar; }
                else if !has_w && has_h { final_w = final_h * ar; }
            }
        }
        let h_before = final_h;
        final_h = final_h.min(ch_max);
        if ch_min > 0.0 { final_h = final_h.max(ch_min); }
        final_h = final_h.max(min_h_floor);
        // Pokud max/min-h zmenil h a aspect-ratio set + w neni explicit, prepocti w.
        if !has_w && (final_h - h_before).abs() > 0.01 {
            if let Some(ar) = child.aspect_ratio {
                if ar > 0.0 {
                    final_w = final_h * ar;
                    final_w = final_w.min(cw_max);
                    if cw_min > 0.0 { final_w = final_w.max(cw_min); }
                }
            }
        }
        let off_x = if stretch_w { 0.0 } else { match js.as_str() {
            "end" | "flex-end" => cw_avail - final_w,
            "center" => (cw_avail - final_w) / 2.0,
            _ => 0.0,
        }};
        let off_y = if stretch_h { 0.0 } else { match als.as_str() {
            "end" | "flex-end" => ch_avail - final_h,
            "center" => (ch_avail - final_h) / 2.0,
            _ => 0.0,
        }};
        // Auto margin override: pri stretch nebo non-stretch mlze auto margin
        // posunout/centrovat item v gridu cell.
        let auto_l_g = child.margin_left_auto;
        let auto_r_g = child.margin_right_auto;
        let auto_t_g = child.margin_top_auto;
        let auto_b_g = child.margin_bottom_auto;
        let cell_free_x = (cw - final_w).max(0.0);
        let cell_free_y = (ch_h - final_h).max(0.0);
        let (auto_off_x, auto_off_y);
        let mut use_auto_x = false;
        let mut use_auto_y = false;
        if auto_l_g && auto_r_g {
            auto_off_x = cell_free_x / 2.0; use_auto_x = true;
        } else if auto_l_g {
            auto_off_x = cell_free_x; use_auto_x = true;
        } else if auto_r_g {
            auto_off_x = 0.0; use_auto_x = true;
        } else { auto_off_x = 0.0; }
        if auto_t_g && auto_b_g {
            auto_off_y = cell_free_y / 2.0; use_auto_y = true;
        } else if auto_t_g {
            auto_off_y = cell_free_y; use_auto_y = true;
        } else if auto_b_g {
            auto_off_y = 0.0; use_auto_y = true;
        } else { auto_off_y = 0.0; }
        let final_off_x = if use_auto_x { auto_off_x } else { off_x };
        let final_off_y = if use_auto_y { auto_off_y } else { off_y };
        let m_l_pos = if auto_l_g { 0.0 } else { m_l };
        let m_t_pos = if auto_t_g { 0.0 } else { m_t };
        child.rect.x = inner_x + cx + m_l_pos + final_off_x;
        child.rect.y = inner_y + cy + m_t_pos + final_off_y;
        // Relative position offset (top/left/bottom/right)
        if let Some(l) = child.offset_left { child.rect.x += l; }
        else if let Some(r) = child.offset_right { child.rect.x -= r; }
        if let Some(t) = child.offset_top { child.rect.y += t; }
        else if let Some(b) = child.offset_bottom { child.rect.y -= b; }
        child.rect.width = final_w;
        child.rect.height = final_h;
        // Dispatch podle child.display (block/flex/grid) - layout_block jen flowuje
        // grandchildren, neresi grid/flex inner.
        match child.display {
            super::super::layout::Display::Flex => super::flex::layout_flex(child),
            super::super::layout::Display::Grid => super::grid::layout_grid(child),
            _ => super::super::layout::layout_block(child),
        }
    }

    // Update parent height jen kdyz neni explicit set (auto height grow z obsahu).
    if bx.explicit_height.is_none() {
        let total_h = y_cursor + pad_t + pad_b;
        if bx.rect.height < total_h {
            bx.rect.height = total_h;
        }
    }
    // Baseline alignment post-pass: per-row max baseline, adjust y v dane row.
    let parent_align_str = bx.align_items.clone();
    if parent_align_str == "baseline" {
        // Recursive child_baseline walk pro flex/grid items s flex-direction.
        fn child_baseline(c: &super::super::layout::LayoutBox) -> f32 {
            let c_h = c.explicit_height.unwrap_or(c.rect.height);
            let is_flex_or_grid = matches!(c.display,
                super::super::layout::Display::Flex | super::super::layout::Display::Grid);
            let has_flex_attr = !c.flex_direction.is_empty();
            if is_flex_or_grid || has_flex_attr {
                if let Some(gc) = c.children.iter().find(|x|
                    !matches!(x.position, super::super::layout::Position::Absolute | super::super::layout::Position::Fixed)
                    && !matches!(x.display, super::super::layout::Display::None)) {
                    let gc_m_t = gc.margin_top.unwrap_or(gc.margin);
                    let gc_pad_t = c.padding_top.unwrap_or(c.padding) + c.border_top_width.unwrap_or(c.border_width);
                    return gc_pad_t + gc_m_t + child_baseline(gc);
                }
            }
            c_h
        }
        // Compute per-item baseline (above) + below.
        let mut item_above: std::collections::HashMap<usize, f32> = std::collections::HashMap::new();
        let mut item_below: std::collections::HashMap<usize, f32> = std::collections::HashMap::new();
        for &(real_idx, _row, _cy) in &item_row_info {
            let item = &bx.children[real_idx];
            let als_str = item.align_self.clone();
            let item_align = if als_str.is_empty() || als_str == "auto" { parent_align_str.clone() } else { als_str };
            if item_align != "baseline" { continue; }
            let m_t = item.margin_top.unwrap_or(item.margin);
            let m_b = item.margin_bottom.unwrap_or(item.margin);
            let first_child = item.children.iter().find(|c|
                !matches!(c.position, super::super::layout::Position::Absolute | super::super::layout::Position::Fixed)
                && !matches!(c.display, super::super::layout::Display::None));
            let above = match first_child {
                Some(c) => {
                    let pad_t_i = item.padding_top.unwrap_or(item.padding) + item.border_top_width.unwrap_or(item.border_width);
                    let c_m_t = c.margin_top.unwrap_or(c.margin);
                    m_t + pad_t_i + c_m_t + child_baseline(c)
                }
                None => m_t + item.rect.height,
            };
            let item_h = item.rect.height + m_t + m_b;
            let below = (item_h - above).max(0.0);
            item_above.insert(real_idx, above);
            item_below.insert(real_idx, below);
        }
        // Per-row max above/below.
        let mut row_max_above: std::collections::HashMap<usize, f32> = std::collections::HashMap::new();
        let mut row_max_below: std::collections::HashMap<usize, f32> = std::collections::HashMap::new();
        for &(real_idx, row, _cy) in &item_row_info {
            if let Some(&a) = item_above.get(&real_idx) {
                let entry = row_max_above.entry(row).or_insert(0.0);
                if a > *entry { *entry = a; }
            }
            if let Some(&b) = item_below.get(&real_idx) {
                let entry = row_max_below.entry(row).or_insert(0.0);
                if b > *entry { *entry = b; }
            }
        }
        // Expand row_tracks pri baseline rozsireni.
        let mut row_tracks_new = row_tracks.clone();
        for r in 0..rows {
            if let (Some(&a), Some(&b)) = (row_max_above.get(&r), row_max_below.get(&r)) {
                let needed = a + b;
                if needed > row_tracks_new[r] {
                    row_tracks_new[r] = needed;
                }
            }
        }
        // Recompute row positions s expanded tracks.
        let mut row_positions_new: Vec<f32> = Vec::with_capacity(rows);
        let mut yc = ac_start;
        for (i, h) in row_tracks_new.iter().enumerate() {
            row_positions_new.push(yc);
            yc += *h;
            if i + 1 < rows { yc += row_gap + ac_between; }
        }
        // Re-position items (no baseline + baseline) per nove row positions.
        for &(real_idx, row, _old_cy) in &item_row_info {
            let new_cy = row_positions_new.get(row).copied().unwrap_or(0.0);
            let item = &mut bx.children[real_idx];
            let als_str = item.align_self.clone();
            let item_align = if als_str.is_empty() || als_str == "auto" { parent_align_str.clone() } else { als_str };
            let m_t = item.margin_top.unwrap_or(item.margin);
            // Preserve relative offset (top/bottom).
            let off_y = if let Some(t) = item.offset_top { t }
                        else if let Some(b) = item.offset_bottom { -b }
                        else { 0.0 };
            if item_align == "baseline" {
                let own_above = item_above.get(&real_idx).copied().unwrap_or(0.0);
                let row_above = row_max_above.get(&row).copied().unwrap_or(0.0);
                let offset = row_above - own_above;
                item.rect.y = bx.rect.y + pad_t + new_cy + m_t + offset + off_y;
            } else {
                // Non-baseline item zachova start position v ramci nove row.
                item.rect.y = bx.rect.y + pad_t + new_cy + m_t + off_y;
            }
        }
        // Update container height pri auto.
        if bx.explicit_height.is_none() {
            let new_total: f32 = row_tracks_new.iter().sum::<f32>() + row_gap * (rows.saturating_sub(1) as f32);
            let pad_t_total = bx.padding_top.unwrap_or(bx.padding) + bw_t;
            let pad_b_total = bx.padding_bottom.unwrap_or(bx.padding) + bw_b;
            let needed_total = new_total + pad_t_total + pad_b_total;
            if needed_total > bx.rect.height { bx.rect.height = needed_total; }
        }
    }
    let _ = parent_align_str;

    // Position absolute/fixed children (CB = padding-box parenta)
    let cb_x = bx.rect.x + bw_l;
    let cb_y = bx.rect.y + bw_t;
    let cb_w = (bx.rect.width - bw_l - bw_r).max(0.0);
    let cb_h = (bx.rect.height - bw_t - bw_b).max(0.0);
    let parent_align = bx.align_items.clone();
    let parent_justify = bx.justify_items.clone();
    for ch in bx.children.iter_mut() {
        if super::is_out_of_flow(ch) {
            // display:none na abs - zero out a skip.
            if matches!(ch.display, super::super::layout::Display::None) {
                ch.rect.x = 0.0; ch.rect.y = 0.0;
                ch.rect.width = 0.0; ch.rect.height = 0.0;
                continue;
            }
            // Pri grid-row/col placement, CB v dane axe vychazi z grid track positions.
            let mut ab_cb_x = cb_x;
            let mut ab_cb_y = cb_y;
            let mut ab_cb_w = cb_w;
            let mut ab_cb_h = cb_h;
            // CSS spec: pri grid-col-start ale no end, CB konci na border-box edge.
            let has_col_end = ch.grid_column_end > 0 || ch.grid_column_span > 0;
            let has_row_end = ch.grid_row_end > 0 || ch.grid_row_span > 0;
            // Only end (no start): CB od border-edge do track end.
            if ch.grid_column_start == 0 && ch.grid_column_end > 0 {
                let c_idx = ((ch.grid_column_end - 1) as usize).min(cols.saturating_sub(1));
                let track_end = inner_x + col_positions.get(c_idx).copied().unwrap_or(0.0);
                ab_cb_x = bx.rect.x;
                ab_cb_w = (track_end - bx.rect.x).max(0.0);
            }
            if ch.grid_row_start == 0 && ch.grid_row_end > 0 {
                let r_idx = ((ch.grid_row_end - 1) as usize).min(rows.saturating_sub(1));
                let track_end = inner_y + row_positions.get(r_idx).copied().unwrap_or(0.0);
                ab_cb_y = bx.rect.y;
                ab_cb_h = (track_end - bx.rect.y).max(0.0);
            }
            if ch.grid_column_start > 0 {
                let c_idx = ((ch.grid_column_start - 1) as usize).min(cols.saturating_sub(1));
                let track_x = inner_x + col_positions.get(c_idx).copied().unwrap_or(0.0);
                ab_cb_x = track_x;
                if has_col_end {
                    let span = if ch.grid_column_span > 0 { ch.grid_column_span as usize }
                              else { ((ch.grid_column_end - ch.grid_column_start).max(1)) as usize };
                    let track_w: f32 = (0..span).map(|d| col_tracks.get(c_idx + d).copied().unwrap_or(0.0)).sum::<f32>()
                        + col_gap * (span.saturating_sub(1) as f32);
                    ab_cb_w = track_w;
                } else {
                    // Bez column-end - CB protahnut do border edge.
                    ab_cb_w = (bx.rect.x + bx.rect.width - track_x).max(0.0);
                }
            }
            if ch.grid_row_start > 0 {
                let r_idx = ((ch.grid_row_start - 1) as usize).min(rows.saturating_sub(1));
                let track_y = inner_y + row_positions.get(r_idx).copied().unwrap_or(0.0);
                ab_cb_y = track_y;
                if has_row_end {
                    let span = if ch.grid_row_span > 0 { ch.grid_row_span as usize }
                              else { ((ch.grid_row_end - ch.grid_row_start).max(1)) as usize };
                    let track_h: f32 = (0..span).map(|d| row_tracks.get(r_idx + d).copied().unwrap_or(0.0)).sum::<f32>()
                        + row_gap * (span.saturating_sub(1) as f32);
                    ab_cb_h = track_h;
                } else {
                    ab_cb_h = (bx.rect.y + bx.rect.height - track_y).max(0.0);
                }
            }
            super::layout_absolute_child(ch, ab_cb_x, ab_cb_y, ab_cb_w, ab_cb_h);
            // Override pri zadnem insetu: respektuj justify-self / align-self.
            let no_inset_x = ch.offset_left.is_none() && ch.offset_right.is_none();
            let no_inset_y = ch.offset_top.is_none() && ch.offset_bottom.is_none();
            let m_l_c = ch.margin_left.unwrap_or(ch.margin);
            let m_t_c = ch.margin_top.unwrap_or(ch.margin);
            let m_r_c = ch.margin_right.unwrap_or(ch.margin);
            let m_b_c = ch.margin_bottom.unwrap_or(ch.margin);
            let js = if !ch.justify_self.is_empty() { ch.justify_self.clone() } else { parent_justify.clone() };
            let als = if !ch.align_self.is_empty() { ch.align_self.clone() } else { parent_align.clone() };
            if no_inset_x {
                let free = (cb_w - ch.rect.width - m_l_c - m_r_c).max(0.0);
                let off = match js.as_str() {
                    "end" | "flex-end" => free,
                    "center" => free / 2.0,
                    _ => 0.0,
                };
                ch.rect.x = cb_x + m_l_c + off;
            }
            if no_inset_y {
                let free = (cb_h - ch.rect.height - m_t_c - m_b_c).max(0.0);
                let off = match als.as_str() {
                    "end" | "flex-end" => free,
                    "center" => free / 2.0,
                    _ => 0.0,
                };
                ch.rect.y = cb_y + m_t_c + off;
            }
        }
    }
}

/// Distribuce free space pro justify/align-content v gridu.
/// Vraci (start_offset, between_extra) - mezi tracky se pak prida `between_extra` (krome gap).
/// Pro negativni free, end/center muze produkovat negativni offset (overflow).
fn grid_distribute(value: &str, free: f32, count: usize) -> (f32, f32) {
    if count == 0 { return (0.0, 0.0); }
    match value.trim() {
        "end" | "flex-end" => (free, 0.0),
        "center" => (free / 2.0, 0.0),
        "space-between" => {
            if count <= 1 || free <= 0.0 { (0.0, 0.0) }
            else { (0.0, free / (count - 1) as f32) }
        }
        "space-around" => {
            if free <= 0.0 { (0.0, 0.0) }
            else { let g = free / count as f32; (g / 2.0, g) }
        }
        "space-evenly" => {
            if free <= 0.0 { (0.0, 0.0) }
            else { let g = free / (count + 1) as f32; (g, g) }
        }
        _ => (0.0, 0.0), // start (default)
    }
}

/// Resolve track sizes (px / % / fr / auto) na concrete pixel values.
/// Inspirace taffy compute/grid/track_sizing.rs (MIT licence).
/// Algorithm:
/// 1. Expand repeat(N, ...) syntax
/// 2. Filter out [line-name] tokens
/// 3. Parse kazdy token jako (typ, hodnota)
/// 4. Compute fixed (px, %) sizes - precteno
/// 5. Compute fr unit base = (free_space) / total_fr
/// 6. Auto = average remaining
pub fn resolve_tracks(s: &str, container_size: f32, gap: f32) -> Vec<f32> {
    let tokens = parse_track_tokens_sized(s, container_size, gap);
    if tokens.is_empty() { return Vec::new(); }
    let track_count = tokens.len();
    let total_gap = gap * (track_count.saturating_sub(1) as f32);

    // Klasifikuj tokens - minmax bere fixed_total z min_px (kdyz max je fr).
    let mut fixed_total = 0.0_f32;
    let mut fr_total = 0.0_f32;
    let mut auto_count = 0;
    for t in &tokens {
        match t {
            Track::Fixed(px) => fixed_total += *px,
            Track::Percent(p) => fixed_total += container_size * p / 100.0,
            Track::Fr(f) => fr_total += *f,
            Track::Auto => auto_count += 1,
            Track::MaxContent => auto_count += 1,
            Track::MinContent => auto_count += 1,
            Track::FitContent(_) => auto_count += 1,
            Track::Minmax(min, max, is_fr) => {
                let min_resolved = if min.is_nan() { 0.0 }
                                   else if (*min - (-1000.0)).abs() < 0.5 { 0.0 }
                                   else if *min < 0.0 && *min > -2.0 { container_size * (-min) }
                                   else { *min };
                if *is_fr {
                    fixed_total += min_resolved;
                    fr_total += *max;
                } else {
                    fixed_total += min_resolved;
                    auto_count += 1;
                }
            }
        }
    }

    let free = (container_size - fixed_total - total_gap).max(0.0);
    // CSS Grid spec: fr_size = leftover / max(1, sum_fr). Pri sum < 1 fr_size = leftover
    // a tracky berou jen sve fr*leftover (zbyle prostor neabsorbujou).
    let fr_base = if fr_total > 0.0 { free / fr_total.max(1.0) } else { 0.0 };
    let after_fr = (free - fr_base * fr_total).max(0.0);
    let auto_base = if auto_count > 0 { after_fr / auto_count as f32 } else { 0.0 };

    tokens.iter().map(|t| match t {
        Track::Fixed(px) => *px,
        Track::Percent(p) => container_size * p / 100.0,
        Track::Fr(f) => fr_base * f,
        Track::Auto => auto_base,
        Track::MaxContent => auto_base,
        Track::MinContent => auto_base,
        Track::FitContent(_) => auto_base,
        Track::Minmax(min, max, is_fr) => {
            let min_r = if min.is_nan() { 0.0 }
                        else if (*min - (-1000.0)).abs() < 0.5 { 0.0 }
                        else if *min < 0.0 && *min > -2.0 { container_size * (-min) }
                        else { *min };
            let max_r = if max.is_nan() { f32::INFINITY }
                        else if (*max - (-1000.0)).abs() < 0.5 { f32::INFINITY }
                        else if *max < 0.0 && *max > -2.0 { container_size * (-max) }
                        else { *max };
            if *is_fr {
                let v = min_r + fr_base * max;
                v.max(min_r)
            } else {
                let v = min_r + auto_base;
                v.clamp(min_r, max_r)
            }
        }
    }).collect()
}

#[derive(Debug, Clone, Copy)]
enum Track {
    Fixed(f32),
    Percent(f32),
    Fr(f32),
    Auto,
    /// max-content keyword (chove se jako Auto, ale pri span distribuci dostane prioritu max).
    MaxContent,
    /// min-content keyword.
    MinContent,
    /// minmax(min_px, max_px or fr) - vlastne flexible s rozsahem.
    Minmax(f32, f32, bool /* max je fr */),
    /// fit-content(<value>): clamp(min-content, max(min-content, arg), max-content).
    /// arg ulozeno: kladne = px, zaporne = -percent (0..-1).
    FitContent(f32),
}

/// Vraci (tokens, is_auto_fit_per_token).
fn parse_track_tokens_with_autofit(s: &str, container: f32, gap: f32) -> (Vec<Track>, Vec<bool>) {
    let total_fixed_outside = pre_compute_fixed(s, container);
    let s = s.trim();
    if s.is_empty() { return (Vec::new(), Vec::new()); }
    let mut tokens: Vec<Track> = Vec::new();
    let mut is_autofit: Vec<bool> = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() { chars.next(); continue; }
        if c == '[' { chars.next(); for cc in chars.by_ref() { if cc == ']' { break; } } continue; }
        let mut buf = String::new();
        let mut depth = 0i32;
        while let Some(&cc) = chars.peek() {
            if cc == '(' { depth += 1; buf.push(cc); chars.next(); continue; }
            if cc == ')' { depth -= 1; buf.push(cc); chars.next(); continue; }
            if depth == 0 && (cc.is_whitespace() || cc == '[') { break; }
            buf.push(cc); chars.next();
        }
        if buf.is_empty() { continue; }
        if let Some(rest) = buf.strip_prefix("repeat(") {
            if let Some(inner) = rest.strip_suffix(')') {
                let comma_idx = inner.find(',').unwrap_or(0);
                let count_str = inner[..comma_idx].trim();
                let inner_tracks = inner[comma_idx+1..].trim();
                let sub_tokens = parse_track_tokens(inner_tracks);
                let sub_size: f32 = sub_tokens.iter().map(|t| match t {
                    Track::Fixed(p) => *p,
                    Track::Percent(p) => container * p / 100.0,
                    _ => 0.0,
                }).sum();
                let is_af = count_str == "auto-fit";
                let count: usize = match count_str {
                    "auto-fill" | "auto-fit" => {
                        if sub_size > 0.0 {
                            let avail = (container - total_fixed_outside).max(0.0);
                            let pattern_len = sub_tokens.len();
                            if pattern_len == 0 { 1 } else {
                                let mut n = 0usize;
                                let mut used = 0.0_f32;
                                loop {
                                    let next = used + sub_size + if n > 0 { gap * pattern_len as f32 } else { 0.0 };
                                    if next > avail + 0.01 { break; }
                                    used = next; n += 1;
                                    if n > 1000 { break; }
                                }
                                n.max(1)
                            }
                        } else { 1 }
                    }
                    _ => count_str.parse().unwrap_or(1),
                };
                for _ in 0..count {
                    for t in &sub_tokens {
                        tokens.push(*t);
                        is_autofit.push(is_af);
                    }
                }
                continue;
            }
        }
        tokens.push(parse_single_track(&buf));
        is_autofit.push(false);
    }
    (tokens, is_autofit)
}

/// Tokenizace grid-template-columns/rows + expand repeat() s container-aware auto-fill count.
fn parse_track_tokens_sized(s: &str, container: f32, gap: f32) -> Vec<Track> {
    // Spocti delku non-repeat fixed tokens pro auto-fill kalkulaci.
    let total_fixed_outside = pre_compute_fixed(s, container);
    let s = s.trim();
    if s.is_empty() { return Vec::new(); }
    let mut tokens: Vec<Track> = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() { chars.next(); continue; }
        if c == '[' { chars.next(); for cc in chars.by_ref() { if cc == ']' { break; } } continue; }
        let mut buf = String::new();
        let mut depth = 0i32;
        while let Some(&cc) = chars.peek() {
            if cc == '(' { depth += 1; buf.push(cc); chars.next(); continue; }
            if cc == ')' { depth -= 1; buf.push(cc); chars.next(); continue; }
            if depth == 0 && (cc.is_whitespace() || cc == '[') { break; }
            buf.push(cc); chars.next();
        }
        if buf.is_empty() { continue; }
        if let Some(rest) = buf.strip_prefix("repeat(") {
            if let Some(inner) = rest.strip_suffix(')') {
                let comma_idx = inner.find(',').unwrap_or(0);
                let count_str = inner[..comma_idx].trim();
                let inner_tracks = inner[comma_idx+1..].trim();
                let sub_tokens = parse_track_tokens(inner_tracks);
                let sub_size: f32 = sub_tokens.iter().map(|t| match t {
                    Track::Fixed(p) => *p,
                    Track::Percent(p) => container * p / 100.0,
                    _ => 0.0,
                }).sum();
                let count: usize = match count_str {
                    "auto-fill" | "auto-fit" => {
                        if sub_size > 0.0 {
                            let avail = (container - total_fixed_outside).max(0.0);
                            // Pocet kompletnich opakovani co se vejdou (pocita gap mezi)
                            let pattern_len = sub_tokens.len();
                            if pattern_len == 0 { 1 } else {
                                let mut n = 0usize;
                                let mut used = 0.0_f32;
                                loop {
                                    let next = used + sub_size + if n > 0 { gap * pattern_len as f32 } else { 0.0 };
                                    if next > avail + 0.01 { break; }
                                    used = next; n += 1;
                                    if n > 1000 { break; }
                                }
                                n.max(1)
                            }
                        } else { 1 }
                    }
                    _ => count_str.parse().unwrap_or(1),
                };
                for _ in 0..count { tokens.extend(sub_tokens.clone()); }
                continue;
            }
        }
        tokens.push(parse_single_track(&buf));
    }
    tokens
}

/// Soucet fixed/percent tokens MIMO repeat (pro auto-fill kalkulaci).
fn pre_compute_fixed(s: &str, container: f32) -> f32 {
    let mut total = 0.0f32;
    let s = s.trim();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() { chars.next(); continue; }
        if c == '[' { chars.next(); for cc in chars.by_ref() { if cc == ']' { break; } } continue; }
        let mut buf = String::new();
        let mut depth = 0i32;
        while let Some(&cc) = chars.peek() {
            if cc == '(' { depth += 1; buf.push(cc); chars.next(); continue; }
            if cc == ')' { depth -= 1; buf.push(cc); chars.next(); continue; }
            if depth == 0 && (cc.is_whitespace() || cc == '[') { break; }
            buf.push(cc); chars.next();
        }
        if buf.is_empty() { continue; }
        if let Some(rest) = buf.strip_prefix("repeat(") {
            if let Some(inner) = rest.strip_suffix(')') {
                let comma_idx = inner.find(',').unwrap_or(0);
                let count_str = inner[..comma_idx].trim();
                let inner_tracks = inner[comma_idx+1..].trim();
                // Auto-fill/fit nemuze byt pri pre-compute (potrebujeme container size).
                if count_str == "auto-fill" || count_str == "auto-fit" { continue; }
                if let Ok(count) = count_str.parse::<usize>() {
                    let sub_tokens = parse_track_tokens(inner_tracks);
                    let sub_size: f32 = sub_tokens.iter().map(|t| match t {
                        Track::Fixed(p) => *p,
                        Track::Percent(p) => container * p / 100.0,
                        _ => 0.0,
                    }).sum();
                    total += sub_size * count as f32;
                }
            }
            continue;
        }
        match parse_single_track(&buf) {
            Track::Fixed(p) => total += p,
            Track::Percent(p) => total += container * p / 100.0,
            _ => {}
        }
    }
    total
}

/// Tokenizace grid-template-columns/rows + expand repeat() (bez container - count = 1 pro auto-fill).
fn parse_track_tokens(s: &str) -> Vec<Track> {
    let s = s.trim();
    if s.is_empty() { return Vec::new(); }
    let mut tokens: Vec<Track> = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() { chars.next(); continue; }
        // Skip [line-name]
        if c == '[' {
            chars.next();
            for cc in chars.by_ref() {
                if cc == ']' { break; }
            }
            continue;
        }
        // Posbiraj token text (do whitespace nebo '[')
        let mut buf = String::new();
        let mut depth = 0i32;
        while let Some(&cc) = chars.peek() {
            if cc == '(' { depth += 1; buf.push(cc); chars.next(); continue; }
            if cc == ')' { depth -= 1; buf.push(cc); chars.next(); continue; }
            if depth == 0 && (cc.is_whitespace() || cc == '[') { break; }
            buf.push(cc);
            chars.next();
        }
        if buf.is_empty() { continue; }
        // Expand repeat(N, ...)
        if let Some(rest) = buf.strip_prefix("repeat(") {
            if let Some(inner) = rest.strip_suffix(')') {
                let comma_idx = inner.find(',').unwrap_or(0);
                let count_str = inner[..comma_idx].trim();
                let inner_tracks = inner[comma_idx+1..].trim();
                let count: usize = match count_str {
                    "auto-fill" | "auto-fit" => 1,
                    _ => count_str.parse().unwrap_or(1),
                };
                let sub_tokens = parse_track_tokens(inner_tracks);
                for _ in 0..count {
                    tokens.extend(sub_tokens.clone());
                }
                continue;
            }
        }
        tokens.push(parse_single_track(&buf));
    }
    tokens
}

fn parse_single_track(s: &str) -> Track {
    let s = s.trim();
    if s == "auto" { return Track::Auto; }
    if s == "min-content" { return Track::MinContent; }
    if s == "max-content" { return Track::MaxContent; }
    if let Some(num) = s.strip_suffix("fr") {
        return Track::Fr(num.trim().parse().unwrap_or(1.0));
    }
    if let Some(num) = s.strip_suffix('%') {
        return Track::Percent(num.trim().parse().unwrap_or(0.0));
    }
    // minmax(min, max). Percent ulozeno jako negativni hodnota (sentinel)
    // a resolve_tracks ho prepocita podle container_size.
    if let Some(rest) = s.strip_prefix("minmax(").and_then(|x| x.strip_suffix(')')) {
        let parts: Vec<&str> = rest.split(',').collect();
        if parts.len() == 2 {
            let min_s = parts[0].trim();
            let max_s = parts[1].trim();
            // Pouzijeme negativni sentinel pro percent: -p kde p je 0..1.
            // Sentinely:
            //   f32::NAN = min-content / max-content (resolve s items)
            //   -1.0 .. 0.0 = percent (0..100%)
            //   0.0 = auto
            let parse_part = |p: &str| -> f32 {
                if p == "auto" { return 0.0; }
                // Sentinely:
                //   f32::NAN = max-content
                //   -1000.0 = min-content (mimo percent rozsah -0..-1)
                if p == "max-content" { return f32::NAN; }
                if p == "min-content" { return -1000.0; }
                if let Some(num) = p.strip_suffix('%') {
                    let v: f32 = num.trim().parse().unwrap_or(0.0);
                    return -(v / 100.0); // sentinel
                }
                super::super::layout::parse_length(p)
            };
            let min_v = parse_part(min_s);
            // Max
            if let Some(num) = max_s.strip_suffix("fr") {
                let max_fr = num.trim().parse().unwrap_or(1.0);
                return Track::Minmax(min_v, max_fr, true);
            }
            let max_v = if max_s == "auto" || max_s == "min-content" || max_s == "max-content" {
                f32::INFINITY
            } else {
                parse_part(max_s)
            };
            return Track::Minmax(min_v, max_v, false);
        }
    }
    // fit-content(<value>): parse arg.
    if let Some(rest) = s.strip_prefix("fit-content(").and_then(|x| x.strip_suffix(')')) {
        let v = rest.trim();
        if let Some(num) = v.strip_suffix('%') {
            let p: f32 = num.trim().parse().unwrap_or(0.0);
            return Track::FitContent(-(p / 100.0));
        }
        return Track::FitContent(super::super::layout::parse_length(v));
    }
    // px / em / rem / cm / in / pc / pt - parse_length
    Track::Fixed(super::super::layout::parse_length(s))
}

/// Pocet tracku z grid-template-columns / grid-template-rows.
/// Velmi zjednodusene - count whitespace-separated tokens, ignore [name].
fn parse_track_count(s: &str) -> usize {
    if s.is_empty() { return 0; }
    let mut count = 0;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' {
            // Skip [line-name]
            for cc in chars.by_ref() {
                if cc == ']' { break; }
            }
            continue;
        }
        if c.is_whitespace() { continue; }
        // Posbiraj token
        while let Some(&cc) = chars.peek() {
            if cc.is_whitespace() || cc == '[' { break; }
            chars.next();
        }
        count += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_count_basic() {
        assert_eq!(parse_track_count("1fr 1fr 1fr"), 3);
        assert_eq!(parse_track_count("100px 200px"), 2);
        assert_eq!(parse_track_count(""), 0);
    }

    #[test]
    fn track_count_with_named_lines() {
        assert_eq!(parse_track_count("[start] 1fr [middle] 2fr [end]"), 2);
    }
}
