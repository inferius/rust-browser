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
    // Scrollbar takes space.
    let scrollbar_size = bx.scrollbar_size;
    let scrollbar_w = if scrollbar_size > 0.0 && (bx.overflow_y == "scroll" || bx.overflow_y == "auto") { scrollbar_size } else { 0.0 };
    let scrollbar_h = if scrollbar_size > 0.0 && (bx.overflow_x == "scroll" || bx.overflow_x == "auto") { scrollbar_size } else { 0.0 };
    let inner_w = (bx.rect.width - pad_l - pad_r - 2.0 * bx.margin - scrollbar_w).max(0.0);
    let inner_h = (bx.rect.height - pad_t - pad_b - 2.0 * bx.margin - scrollbar_h).max(0.0);

    if bx.children.is_empty() { return; }

    // Re-resolve gap pct proti inner content dimension.
    let row_gap = if let Some(p) = bx.row_gap_pct {
        if bx.explicit_height.is_none() { 0.0 } else { inner_h * p }
    } else { bx.row_gap };
    let col_gap = if let Some(p) = bx.column_gap_pct {
        inner_w * p
    } else { bx.column_gap };

    // grid-auto-flow detect early.
    let auto_flow_str = bx.grid_auto_flow.trim();
    let column_flow = auto_flow_str.contains("column");
    let dense_flow = auto_flow_str.contains("dense");
    // Parse + resolve column tracks
    let mut col_tracks = resolve_tracks(&bx.grid_template_columns, inner_w, col_gap);
    let (mut col_token_kinds, mut col_is_autofit) = parse_track_tokens_with_autofit(&bx.grid_template_columns, inner_w, col_gap);
    if std::env::var("GRID_TRACE").is_ok() {
        let cls = bx.node.as_ref().and_then(|n| n.attr("class")).unwrap_or_default();
        if cls.contains("transform-grid") {
            eprintln!("[grid_call] cls={} rect.x={} y={} w={} h={}",
                cls, bx.rect.x, bx.rect.y, bx.rect.width, bx.rect.height);
            for (i, c) in bx.children.iter().enumerate().take(2) {
                eprintln!("  child[{}].rect.x={} y={} w={} h={}", i, c.rect.x, c.rect.y, c.rect.width, c.rect.height);
            }
        }
    }
    if std::env::var("GRID_DEBUG").is_ok() {
        let n = bx.node.as_ref()
            .and_then(|n| n.attr("class"))
            .unwrap_or_else(|| "?".to_string());
        eprintln!("[grid_debug] class={:?} gtc={:?} inner_w={} cols={} children={} rect.h_in={}",
            n, bx.grid_template_columns, inner_w, col_tracks.len(), bx.children.len(), bx.rect.height);
    }
    if col_tracks.is_empty() { col_tracks = vec![inner_w]; }
    let mut cols_explicit = col_tracks.len();
    // Detekce negativnich grid-column-start beyond -cols-1: musi pridat implicit cols PRED explicit grid.
    let mut col_prepend: usize = 0;
    for ch in bx.children.iter() {
        if super::is_out_of_flow(ch) || matches!(ch.display, super::super::layout::Display::None) { continue; }
        if ch.grid_column_start < 0 {
            let k = (-ch.grid_column_start) as usize;
            if k > cols_explicit + 1 {
                let need = k - cols_explicit - 1;
                if need > col_prepend { col_prepend = need; }
            }
        }
        if ch.grid_column_end < 0 {
            let k = (-ch.grid_column_end) as usize;
            if k > cols_explicit + 1 {
                let need = k - cols_explicit - 1;
                if need > col_prepend { col_prepend = need; }
            }
        }
    }
    if col_prepend > 0 {
        // Prepend implicit cols (size auto). grid-auto-columns cycle reverze.
        let auto_col_resolved: Vec<f32> = if !bx.grid_auto_columns.is_empty() {
            resolve_tracks(&bx.grid_auto_columns, inner_w, col_gap)
        } else { Vec::new() };
        let cycle_len = auto_col_resolved.len();
        for i in 0..col_prepend {
            let (h, kind) = if cycle_len > 0 {
                let v = auto_col_resolved[(cycle_len + i + cycle_len - col_prepend) % cycle_len];
                (v, Track::Fixed(v))
            } else { (0.0, Track::Auto) };
            col_tracks.insert(0, h);
            col_token_kinds.insert(0, kind);
            col_is_autofit.insert(0, false);
        }
        cols_explicit += col_prepend;
    }
    // Pri column-flow: extend col_tracks pro implicit cols po explicit (cycle z grid-auto-columns).
    if column_flow {
        let row_count = if bx.grid_template_rows.is_empty() { 1 } else { parse_track_count(&bx.grid_template_rows).max(1) };
        // Simulace placement column-flow.
        let cols_n = col_tracks.len().max(1);
        let _ = cols_n;
        let mut occupied: Vec<Vec<bool>> = Vec::new();
        // occupied[col][row]. Resize as needed.
        let mut needed_cols: usize = col_tracks.len();
        // Pass 1: definite (skip - not relevant for col extension here)
        // Use simulate placement column-major.
        let mut cur_col = 0usize;
        let mut cur_row = 0usize;
        for ch in bx.children.iter() {
            if super::is_out_of_flow(ch) || matches!(ch.display, super::super::layout::Display::None) { continue; }
            let exp_col = if ch.grid_column_start > 0 { Some(((ch.grid_column_start - 1) as usize) + col_prepend) }
                          else if ch.grid_column_start < 0 { let k = (-ch.grid_column_start) as usize; let total_lines = col_tracks.len() + 1; if k <= total_lines { Some(total_lines - k) } else { Some(0) } }
                          else { None };
            let exp_row = if ch.grid_row_start > 0 { Some((ch.grid_row_start - 1) as usize) } else { None };
            let span_col = if ch.grid_column_span > 0 { ch.grid_column_span as usize }
                           else if ch.grid_column_end > 0 && ch.grid_column_start > 0 { (ch.grid_column_end - ch.grid_column_start).max(1) as usize }
                           else { 1 };
            let span_row = if ch.grid_row_span > 0 { ch.grid_row_span as usize }
                           else if ch.grid_row_end > 0 && ch.grid_row_start > 0 { (ch.grid_row_end - ch.grid_row_start).max(1) as usize }
                           else { 1 };
            let (col_p, row_p) = match (exp_col, exp_row) {
                (Some(c), Some(r)) => (c, r),
                (Some(c), None) => {
                    // Column-flow + col definite: row auto. First free row in col.
                    let mut r = 0;
                    while {
                        while occupied.len() <= c + span_col { occupied.push(Vec::new()); }
                        let mut blocked = false;
                        for dc in 0..span_col {
                            while occupied[c + dc].len() <= r + span_row { occupied[c + dc].push(false); }
                            for dr in 0..span_row {
                                if occupied[c + dc][r + dr] { blocked = true; break; }
                            }
                            if blocked { break; }
                        }
                        blocked
                    } { r += 1; if r > 1000 { break; } }
                    (c, r)
                }
                (None, Some(r)) => {
                    // Column-flow + row definite: find first col with row free.
                    let mut c = cur_col;
                    while {
                        while occupied.len() <= c + span_col { occupied.push(Vec::new()); }
                        let mut blocked = false;
                        for dc in 0..span_col {
                            while occupied[c + dc].len() <= r + span_row { occupied[c + dc].push(false); }
                            for dr in 0..span_row {
                                if occupied[c + dc][r + dr] { blocked = true; break; }
                            }
                            if blocked { break; }
                        }
                        blocked
                    } { c += 1; if c > 1000 { break; } }
                    (c, r)
                }
                (None, None) => {
                    // Auto - column-major: advance row first, then col.
                    let mut c = cur_col;
                    let mut r = cur_row;
                    loop {
                        if r + span_row > row_count {
                            r = 0;
                            c += 1;
                            continue;
                        }
                        while occupied.len() <= c + span_col { occupied.push(Vec::new()); }
                        let mut blocked = false;
                        for dc in 0..span_col {
                            while occupied[c + dc].len() <= r + span_row { occupied[c + dc].push(false); }
                            for dr in 0..span_row {
                                if occupied[c + dc][r + dr] { blocked = true; break; }
                            }
                            if blocked { break; }
                        }
                        if !blocked { break; }
                        r += 1;
                    }
                    cur_col = c;
                    cur_row = r + span_row;
                    if cur_row >= row_count { cur_col = c + 1; cur_row = 0; }
                    (c, r)
                }
            };
            // Mark
            for dc in 0..span_col {
                while occupied.len() <= col_p + dc { occupied.push(Vec::new()); }
                while occupied[col_p + dc].len() <= row_p + span_row { occupied[col_p + dc].push(false); }
                for dr in 0..span_row {
                    occupied[col_p + dc][row_p + dr] = true;
                }
            }
            if col_p + span_col > needed_cols { needed_cols = col_p + span_col; }
        }
        // Extend col_tracks pomoci grid-auto-columns cycle.
        if needed_cols > col_tracks.len() {
            let auto_col_resolved: Vec<f32> = if !bx.grid_auto_columns.is_empty() {
                resolve_tracks(&bx.grid_auto_columns, inner_w, col_gap)
            } else { Vec::new() };
            let cycle_len = auto_col_resolved.len();
            while col_tracks.len() < needed_cols {
                let i = col_tracks.len() - cols_explicit;
                let (h, kind) = if cycle_len > 0 {
                    let v = auto_col_resolved[i % cycle_len];
                    (v, Track::Fixed(v))
                } else { (0.0, Track::Auto) };
                col_tracks.push(h);
                col_token_kinds.push(kind);
                col_is_autofit.push(false);
            }
        }
    }
    let _ = (column_flow, dense_flow);
    let cols = col_tracks.len();
    // Helper: resolve grid-column-start s prepend offsetem.
    let _resolve_col_start = |start: i32| -> Option<usize> {
        if start > 0 {
            // Positive: 1-based explicit -> 0-based + prepend.
            Some(((start - 1) as usize) + col_prepend)
        } else if start < 0 {
            // Negative: -1 = last line = cols+1. -k = line cols+2-k.
            let k = (-start) as usize;
            if k <= cols + 1 {
                Some(cols + 1 - k) // line idx, 0-based
            } else { Some(0) } // shouldn't happen po prepend
        } else {
            None
        }
    };
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

    let _ = (column_flow, dense_flow);
    // In-flow item count (abs/fixed/display:none vyradit pri vypoctu rows).
    let _in_flow_count = bx.children.iter()
        .filter(|c| !super::is_out_of_flow(c) && !matches!(c.display, super::super::layout::Display::None))
        .count();
    let rows_explicit_str = bx.grid_template_rows.clone();
    // Row prepend: stejna logika jako col_prepend pro negative grid-row-start.
    let row_count_explicit = if rows_explicit_str.is_empty() { 0 } else { parse_track_count(&rows_explicit_str) };
    let mut row_prepend: usize = 0;
    for ch in bx.children.iter() {
        if super::is_out_of_flow(ch) || matches!(ch.display, super::super::layout::Display::None) { continue; }
        if ch.grid_row_start < 0 {
            let k = (-ch.grid_row_start) as usize;
            if k > row_count_explicit + 1 {
                let need = k - row_count_explicit - 1;
                if need > row_prepend { row_prepend = need; }
            }
        }
        if ch.grid_row_end < 0 {
            let k = (-ch.grid_row_end) as usize;
            if k > row_count_explicit + 1 {
                let need = k - row_count_explicit - 1;
                if need > row_prepend { row_prepend = need; }
            }
        }
    }
    // Spocti needed_rows simulaci placement (vc. spans) - drive jen ceil(items/cols).
    let needed_from_placement = {
        let cols_n = cols.max(1);
        let mut occupied: Vec<bool> = Vec::new();
        let mut max_row_used: usize = 0;
        let mut auto_cur = 0usize;
        for child in bx.children.iter() {
            if super::is_out_of_flow(child) || matches!(child.display, super::super::layout::Display::None) { continue; }
            let exp_col = if child.grid_column_start > 0 { Some(((child.grid_column_start - 1) as usize) + col_prepend) }
                          else if child.grid_column_start < 0 { let k = (-child.grid_column_start) as usize; if k <= cols_explicit + 1 { Some(cols_explicit + 1 - k) } else { Some(0) } }
                          else { None };
            let exp_row = if child.grid_row_start > 0 { Some(((child.grid_row_start - 1) as usize) + row_prepend) }
                          else if child.grid_row_start < 0 { let k = (-child.grid_row_start) as usize; let total_explicit_lines = row_count_explicit + row_prepend + 1; if k <= total_explicit_lines { Some(total_explicit_lines - k) } else { Some(0) } }
                          else { None };
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
        let count = parse_track_count(&rows_explicit_str) + row_prepend;
        count.max(needed_from_placement)
    } else {
        needed_from_placement.max(1) + row_prepend
    };

    // Resolve row tracks
    let mut row_tracks: Vec<f32> = if !rows_explicit_str.is_empty() {
        let mut resolved = resolve_tracks(&rows_explicit_str, inner_h, row_gap);
        if resolved.is_empty() { resolved = vec![inner_h.max(0.0).max(50.0)]; }
        // Prepend implicit rows pred explicit (negative grid-row-start).
        if row_prepend > 0 {
            let auto_row_resolved_pre: Vec<f32> = if !bx.grid_auto_rows.is_empty() {
                resolve_tracks(&bx.grid_auto_rows, inner_h, row_gap)
            } else { Vec::new() };
            let cycle_len = auto_row_resolved_pre.len();
            let mut prepended: Vec<f32> = Vec::with_capacity(row_prepend);
            for i in 0..row_prepend {
                let h = if cycle_len > 0 {
                    auto_row_resolved_pre[(cycle_len + i + cycle_len - row_prepend) % cycle_len]
                } else { 0.0 };
                prepended.push(h);
            }
            prepended.extend(resolved);
            resolved = prepended;
        }
        resolved
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
            // r index account for row_prepend offset (impl-before rows take cycle slots).
            let implicit_idx = r.saturating_sub(explicit_count + row_prepend);
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
            let explicit_col = if child.grid_column_start > 0 { Some(((child.grid_column_start - 1) as usize) + col_prepend) }
                              else if child.grid_column_start < 0 { let k = (-child.grid_column_start) as usize; if k <= cols_explicit + 1 { Some(cols_explicit + 1 - k) } else { Some(0) } }
                              else { None };
            let explicit_row = if child.grid_row_start > 0 { Some(((child.grid_row_start - 1) as usize) + row_prepend) }
                              else if child.grid_row_start < 0 { let k = (-child.grid_row_start) as usize; let total_explicit_lines = row_count_explicit + row_prepend + 1; if k <= total_explicit_lines { Some(total_explicit_lines - k) } else { Some(0) } }
                              else { None };
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
                    // CSS spec: pri overflow != visible v inline axis, auto min-size = 0.
                    let inline_overflow_blocks = matches!(item.overflow_x.as_str(), "hidden" | "scroll" | "auto" | "clip");
                    let item_max = item.explicit_width.unwrap_or(item.rect.width).max(text_max).max(pb_l + pb_r).max(cw_min_p);
                    let _ = inline_overflow_blocks;
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
                    let item_min = if inline_overflow_blocks {
                        // CSS overflow != visible v inline axis: auto-min-content = 0,
                        // pouze padding/border floor + explicit min-width.
                        (pb_l + pb_r).max(cw_min_p)
                    } else { item_min_base.max(pb_l + pb_r).max(cw_min_p) };
                    // Margins (jen NON-percent, fixed) prispivaji do track contribution.
                    let has_pct_margin = item.margin_left_pct.is_some() || item.margin_right_pct.is_some();
                    let m_l_g = if has_pct_margin { 0.0 } else { item.margin_left.unwrap_or(item.margin) };
                    let m_r_g = if has_pct_margin { 0.0 } else { item.margin_right.unwrap_or(item.margin) };
                    let item_min_with_m = if cw_min_p > 0.0 || item.explicit_width.is_some() {
                        item_min + m_l_g + m_r_g
                    } else { item_min };
                    if item_max > max_content { max_content = item_max; }
                    if item_min_with_m > min_content { min_content = item_min_with_m; }
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
                else if *min < 0.0 && *min > -2.0 {
                    // percent min
                    (inner_w * (-min)).max(min_content)
                }
                else {
                    // fixed px min - track base size at least the fixed value.
                    min.max(min_content)
                }
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
            let inline_overflow_blocks_span = matches!(item.overflow_x.as_str(), "hidden" | "scroll" | "auto" | "clip");
            let item_min = if inline_overflow_blocks_span { 0.0 } else { text_min };
            let item_max = text_max;
            if item_max <= 0.0 && item_min <= 0.0 { continue; }
            let span_indices: Vec<usize> = (col..(col+span_col)).collect();
            let total_gap_s = col_gap * (span_col.saturating_sub(1) as f32);
            let cur_sum: f32 = span_indices.iter().map(|&c| col_tracks[c]).sum::<f32>() + total_gap_s;
            // Helper: track has intrinsic min sizing function?
            let is_intrinsic_min = |c: usize| -> bool {
                match col_token_kinds.get(c) {
                    Some(Track::Auto) | Some(Track::MaxContent) | Some(Track::MinContent) | Some(Track::FitContent(_)) => true,
                    Some(Track::Minmax(min_v, _, _)) => {
                        // -1000 = min-content, NaN = max-content, 0 = auto.
                        min_v.is_nan() || (*min_v - (-1000.0)).abs() < 0.5 || *min_v == 0.0
                    }
                    _ => false,
                }
            };
            // Helper: track has Mc (max-content) min sizing function?
            let is_mc_min = |c: usize| -> bool {
                match col_token_kinds.get(c) {
                    Some(Track::MaxContent) => true,
                    Some(Track::Minmax(min_v, _, _)) => min_v.is_nan(),
                    _ => false,
                }
            };
            if cur_sum < item_min {
                // Step 1: distribute do all intrinsic-min tracks (vc. minmax s intrinsic min).
                let mut recipients: Vec<usize> = span_indices.iter().copied().filter(|&c| is_intrinsic_min(c)).collect();
                // Pri overflow:hidden span: fc a auto excluded z intrinsic-min count.
                if inline_overflow_blocks_span {
                    recipients.retain(|&c| !matches!(col_token_kinds.get(c), Some(Track::FitContent(_)) | Some(Track::Auto)));
                    recipients.retain(|&c| !matches!(col_token_kinds.get(c), Some(Track::Minmax(min_v, _, _)) if *min_v == 0.0));
                }
                if !recipients.is_empty() {
                    let deficit = item_min - cur_sum;
                    let share = deficit / recipients.len() as f32;
                    for &c in &recipients { col_tracks[c] += share; }
                }
            }
            let cur_sum2: f32 = span_indices.iter().map(|&c| col_tracks[c]).sum::<f32>() + total_gap_s;
            if cur_sum2 < item_max {
                // Step 2: distribute (item_max - item_min) to Mc-min tracks (max-content
                // min sizing function: plain Mc OR minmax(Mc, X)).
                let mc_min_tracks: Vec<usize> = span_indices.iter().copied().filter(|&c| is_mc_min(c)).collect();
                let tier1: Vec<usize> = span_indices.iter().copied().filter(|&c| matches!(col_token_kinds.get(c), Some(Track::MaxContent))).collect();
                let tier2: Vec<usize> = span_indices.iter().copied().filter(|&c| matches!(col_token_kinds.get(c), Some(Track::FitContent(_)))).collect();
                let tier3: Vec<usize> = span_indices.iter().copied().filter(|&c| matches!(col_token_kinds.get(c), Some(Track::Auto))).collect();
                let tier_all: Vec<usize> = span_indices.iter().copied().filter(|&c| matches!(col_token_kinds.get(c),
                    Some(Track::Auto) | Some(Track::MaxContent) | Some(Track::MinContent) | Some(Track::FitContent(_)))).collect();
                // Pri presence Mc-min tracks (incl minmax(Mc,X)): step 2 = Mc-min only.
                let recipients = if !mc_min_tracks.is_empty() {
                    mc_min_tracks
                } else if inline_overflow_blocks_span && tier1.is_empty() && !tier_all.is_empty() {
                    tier_all
                } else if !tier1.is_empty() {
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
                // Pri overflow:hidden span: aplikuj algoritmus distribution proti final size.
                // Intrinsic tracks (vc. minmax-with-intrinsic-min) ale exclud fc, auto, minmax(auto,X).
                // X = (free - text_min) / intrinsic_count.
                // Mc-min: X + text_min/Mc_count. Other intrinsic: X.
                if inline_overflow_blocks_span && text_min > 0.0 {
                    let intrinsic_indices: Vec<usize> = span_indices.iter().copied().filter(|&c| {
                        match col_token_kinds.get(c) {
                            Some(Track::MinContent) | Some(Track::MaxContent) => true,
                            Some(Track::Minmax(min_v, _, _)) => {
                                min_v.is_nan() || (*min_v - (-1000.0)).abs() < 0.5
                            }
                            _ => false,
                        }
                    }).collect();
                    let mcc_indices: Vec<usize> = span_indices.iter().copied().filter(|&c| {
                        match col_token_kinds.get(c) {
                            Some(Track::MaxContent) => true,
                            Some(Track::Minmax(min_v, _, _)) => min_v.is_nan(),
                            _ => false,
                        }
                    }).collect();
                    if !intrinsic_indices.is_empty() && !mcc_indices.is_empty() {
                        // Sum of fixed (non-intrinsic) span tracks.
                        let intrinsic_set: std::collections::HashSet<usize> = intrinsic_indices.iter().copied().collect();
                        let fixed_sum: f32 = span_indices.iter().copied().filter(|c| !intrinsic_set.contains(c))
                            .map(|c| col_tracks[c]).sum();
                        let free = (item_max - fixed_sum).max(0.0);
                        let n = intrinsic_indices.len() as f32;
                        let mcc_n = mcc_indices.len() as f32;
                        let x = ((free - text_min) / n).max(0.0);
                        let mc_extra = if mcc_n > 0.0 { text_min / mcc_n } else { 0.0 };
                        for &c in &intrinsic_indices {
                            col_tracks[c] = x;
                        }
                        for &c in &mcc_indices {
                            col_tracks[c] = x + mc_extra;
                        }
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
                let explicit_col = if child.grid_column_start > 0 { Some(((child.grid_column_start - 1) as usize) + col_prepend) }
                              else if child.grid_column_start < 0 { let k = (-child.grid_column_start) as usize; if k <= cols_explicit + 1 { Some(cols_explicit + 1 - k) } else { Some(0) } }
                              else { None };
                let explicit_row = if child.grid_row_start > 0 { Some(((child.grid_row_start - 1) as usize) + row_prepend) }
                              else if child.grid_row_start < 0 { let k = (-child.grid_row_start) as usize; let total_explicit_lines = row_count_explicit + row_prepend + 1; if k <= total_explicit_lines { Some(total_explicit_lines - k) } else { Some(0) } }
                              else { None };
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
        // Fr tracks have min sizing = auto -> intrinsic content min applies.
        Some(Track::Fr(_)) => true,
        _ => rows_explicit_str.is_empty(),
    }).collect();
    // Implicit rows (= rows nad explicit count) jsou auto.
    let explicit_row_count = if rows_explicit_str.is_empty() { 0 } else { parse_track_count(&rows_explicit_str) };
    let any_auto_row = rows_explicit_str.is_empty() || row_is_auto.iter().any(|&b| b) || rows > explicit_row_count;
    if any_auto_row && !in_flow.is_empty() {
        // Pro radky bez template, dej max item explicit_height (uz jsme to delali). Ted jeste rect.height.
        let mut by_row: std::collections::HashMap<usize, f32> = std::collections::HashMap::new();
        let mut occupied_d: Vec<bool> = vec![false; rows.max(1) * cols.max(1)];
        let mut auto_cursor_d = 0usize;
        for &real_idx in in_flow.iter() {
            let child = &bx.children[real_idx];
            let explicit_col = if child.grid_column_start > 0 { Some(((child.grid_column_start - 1) as usize) + col_prepend) }
                              else if child.grid_column_start < 0 { let k = (-child.grid_column_start) as usize; if k <= cols_explicit + 1 { Some(cols_explicit + 1 - k) } else { Some(0) } }
                              else { None };
            let explicit_row = if child.grid_row_start > 0 { Some(((child.grid_row_start - 1) as usize) + row_prepend) }
                              else if child.grid_row_start < 0 { let k = (-child.grid_row_start) as usize; let total_explicit_lines = row_count_explicit + row_prepend + 1; if k <= total_explicit_lines { Some(total_explicit_lines - k) } else { Some(0) } }
                              else { None };
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
                // Intrinsic h measurement pass pro items bez explicit_height ale s children
                // (deep nested percent layouts). Clone, layout, capture rect.height.
                let needs_measure = {
                    let it = &bx.children[real_idx];
                    it.explicit_height.is_none() && !it.children.is_empty()
                };
                if needs_measure {
                    // Track width: sum spanned col_tracks + gaps - margins.
                    let m_l = bx.children[real_idx].margin_left.unwrap_or(bx.children[real_idx].margin);
                    let m_r = bx.children[real_idx].margin_right.unwrap_or(bx.children[real_idx].margin);
                    let track_w: f32 = (col..(col+span_col)).filter_map(|c| col_tracks.get(c).copied()).sum::<f32>()
                        + col_gap * (span_col.saturating_sub(1) as f32);
                    let item_w = (track_w - m_l - m_r).max(0.0);
                    let mut measured = bx.children[real_idx].clone();
                    measured.rect.x = 0.0;
                    measured.rect.y = 0.0;
                    measured.rect.width = if measured.explicit_width.is_some() { measured.explicit_width.unwrap() } else { item_w };
                    measured.rect.height = 0.0;
                    measured.taffy_intrinsic_mode = true;
                    match measured.display {
                        super::super::layout::Display::Flex => super::flex::layout_flex(&mut measured),
                        super::super::layout::Display::Grid => super::grid::layout_grid(&mut measured),
                        super::super::layout::Display::Block | super::super::layout::Display::None => super::super::layout::layout_block(&mut measured),
                        _ => {}
                    }
                    let intrinsic_h = measured.rect.height;
                    if intrinsic_h > 0.0 {
                        bx.children[real_idx].rect.height = intrinsic_h;
                    }
                }
                let item = &bx.children[real_idx];
                let mut h = item.explicit_height.unwrap_or(item.rect.height);
                // Text intrinsic height v taffy_mode = 10 per visible line (estimate by ZWS/whitespace breaks not done).
                if item.taffy_mode && item.text.is_some() && h == 0.0 {
                    h = 10.0;
                }
                // Wrap-aware text height: pri text > col_track width spocti pocet linek.
                // Triggers JEN kdyz je explicit grid template-cols a text > track_w STRICTLY.
                let cols_str = bx.grid_template_columns.trim().to_lowercase();
                let is_vertical_text_item = matches!(item.writing_mode.as_str(),
                    "vertical-lr" | "vertical-rl");
                if item.taffy_mode && item.text.is_some() && item.explicit_height.is_none()
                    && !cols_str.is_empty() && !cols_str.contains("auto")
                    && !cols_str.contains("max-content")
                    && !cols_str.contains("fit-content")
                    && !is_vertical_text_item {
                    if let Some(t) = &item.text {
                        let track_w: f32 = (col..(col+span_col)).filter_map(|c| col_tracks.get(c).copied()).sum::<f32>()
                            + col_gap * (span_col.saturating_sub(1) as f32);
                        let m_l = item.margin_left.unwrap_or(item.margin);
                        let m_r = item.margin_right.unwrap_or(item.margin);
                        let avail_w = (track_w - m_l - m_r).max(0.0);
                        let total_text_w = t.chars().filter(|c| !matches!(*c, '\u{200B}' | ' ' | '\n' | '\t')).count() as f32 * 10.0;
                        if avail_w > 0.0 && total_text_w > avail_w + 0.5 {
                            let mut lines = 1usize;
                            let mut cur_w = 0.0_f32;
                            let mut seg_w = 0.0_f32;
                            for c in t.chars() {
                                if matches!(c, '\u{200B}' | ' ' | '\n' | '\t') {
                                    if cur_w + seg_w <= avail_w + 0.01 {
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
                                if cur_w + seg_w > avail_w + 0.01 { lines += 1; }
                            }
                            let wh = lines as f32 * 10.0;
                            if wh > h { h = wh; }
                        }
                    }
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
        // Snapshot items_intrinsic per row (z by_row) PRED merge do row_tracks.
        let mut items_intrinsic: Vec<f32> = vec![0.0; rows];
        for (r, h) in &by_row {
            if *r < items_intrinsic.len() { items_intrinsic[*r] = *h; }
        }
        for (r, h) in by_row {
            if r < row_tracks.len() && row_tracks[r] < h { row_tracks[r] = h; }
        }
        // Iterativni fr re-resolution: pri intrinsic-min > fr*share, freeze track at
        // intrinsic_min, redistribute zbytek mezi unfrozen fr (CSS Grid 12.7).
        if bx.explicit_height.is_some() && !rows_explicit_str.is_empty() && rows_explicit_str.contains("fr") {
            let fr_factors: Vec<f32> = (0..rows).map(|i| match row_token_kinds.get(i) {
                Some(Track::Fr(f)) => *f,
                _ => 0.0,
            }).collect();
            // Intrinsic min pro fr tracks = items_intrinsic (NEbere initial fr resolution).
            // Pro non-fr tracks, current row_tracks (fixed/percent).
            let intrinsic_min: Vec<f32> = (0..rows).map(|i| {
                if fr_factors[i] > 0.0 {
                    items_intrinsic.get(i).copied().unwrap_or(0.0)
                } else {
                    row_tracks[i]
                }
            }).collect();
            if fr_factors.iter().any(|&f| f > 0.0) {
                let mut frozen: Vec<bool> = (0..rows).map(|i| fr_factors[i] <= 0.0).collect();
                let total_gap = row_gap * (rows.saturating_sub(1) as f32);
                let mut new_sizes: Vec<f32> = row_tracks.clone();
                loop {
                    let frozen_sum: f32 = (0..rows).filter(|&i| frozen[i]).map(|i| new_sizes[i]).sum();
                    let unfrozen_factor: f32 = (0..rows).filter(|&i| !frozen[i]).map(|i| fr_factors[i]).sum();
                    if unfrozen_factor <= 0.0 { break; }
                    let leftover = (inner_h - frozen_sum - total_gap).max(0.0);
                    let fr_size = leftover / unfrozen_factor;
                    let mut newly_frozen = false;
                    // Za prvni: rozhodnout, ktery fr track ma intrinsic > fr_size * factor.
                    // Tyto frozen na intrinsic. Ostatni dostanou fr_size * factor.
                    let mut min_ratio = f32::INFINITY;
                    let mut min_idx: Option<usize> = None;
                    for i in 0..rows {
                        if frozen[i] { continue; }
                        // Pri fr=1, intrinsic=51 - "ratio" intrinsic/factor = 51.
                        // Vyssi ratio = priorita freeze pri vyssim intrinsic.
                        let ratio = intrinsic_min[i] / fr_factors[i].max(0.0001);
                        if ratio > fr_size {
                            // Tento track potrebuje freeze.
                            // Kdyz vic, freeze ten s nejvyssim ratio first.
                            if ratio < min_ratio {
                                // chceme NEJVYSSI ratio first
                            }
                            if min_idx.is_none() || ratio > intrinsic_min[min_idx.unwrap()] / fr_factors[min_idx.unwrap()].max(0.0001) {
                                min_idx = Some(i);
                                min_ratio = ratio;
                            }
                            newly_frozen = true;
                        }
                    }
                    if newly_frozen {
                        let i = min_idx.unwrap();
                        frozen[i] = true;
                        new_sizes[i] = intrinsic_min[i];
                    } else {
                        // Vsechny unfrozen fr take fr_size.
                        for i in 0..rows {
                            if !frozen[i] {
                                new_sizes[i] = fr_size * fr_factors[i];
                            }
                        }
                        break;
                    }
                }
                row_tracks = new_sizes;
            }
        }
        // Span items rows distribute (CSS §11.5.5): pri item span_row > 1 expand row tracks.
        let mut occupied_d2: Vec<bool> = vec![false; rows.max(1) * cols.max(1)];
        let mut auto_cursor_d2 = 0usize;
        for &real_idx in in_flow.iter() {
            let child = &bx.children[real_idx];
            let explicit_col = if child.grid_column_start > 0 { Some(((child.grid_column_start - 1) as usize) + col_prepend) }
                              else if child.grid_column_start < 0 { let k = (-child.grid_column_start) as usize; if k <= cols_explicit + 1 { Some(cols_explicit + 1 - k) } else { Some(0) } }
                              else { None };
            let explicit_row = if child.grid_row_start > 0 { Some(((child.grid_row_start - 1) as usize) + row_prepend) }
                              else if child.grid_row_start < 0 { let k = (-child.grid_row_start) as usize; let total_explicit_lines = row_count_explicit + row_prepend + 1; if k <= total_explicit_lines { Some(total_explicit_lines - k) } else { Some(0) } }
                              else { None };
            let span_col = if child.grid_column_span > 0 { child.grid_column_span as usize }
                           else if child.grid_column_end > 0 && child.grid_column_start > 0 { (child.grid_column_end - child.grid_column_start).max(1) as usize }
                           else { 1 };
            let span_row = if child.grid_row_span > 0 { child.grid_row_span as usize }
                           else if child.grid_row_end > 0 && child.grid_row_start > 0 { (child.grid_row_end - child.grid_row_start).max(1) as usize }
                           else { 1 };
            let (row, col) = if let (Some(r), Some(c)) = (explicit_row, explicit_col) { (r, c) }
                else if let Some(c) = explicit_col { let mut r = 0; while r * cols + c < occupied_d2.len() && occupied_d2[r * cols + c] { r += 1; } (r, c) }
                else if let Some(r) = explicit_row { let mut c = 0; while r * cols + c < occupied_d2.len() && occupied_d2[r * cols + c] { c += 1; } (r, c) }
                else { let mut idx = auto_cursor_d2; while idx < occupied_d2.len() && occupied_d2[idx] { idx += 1; } auto_cursor_d2 = idx + 1; (idx / cols.max(1), idx % cols.max(1)) };
            for dr in 0..span_row {
                for dc in 0..span_col {
                    let idx = (row + dr) * cols + (col + dc);
                    if idx < occupied_d2.len() { occupied_d2[idx] = true; }
                }
            }
            if span_row <= 1 { continue; }
            // Compute item h (vc. margins).
            let item = &bx.children[real_idx];
            let mut h = item.explicit_height.unwrap_or(item.rect.height);
            if item.taffy_mode && item.text.is_some() && h == 0.0 { h = 10.0; }
            let m_t = item.margin_top.unwrap_or(item.margin);
            let m_b = item.margin_bottom.unwrap_or(item.margin);
            h += m_t + m_b;
            // Sum aktualne spanned row tracks + gaps.
            let total_row_gap = row_gap * (span_row.saturating_sub(1) as f32);
            let span_indices: Vec<usize> = (row..(row+span_row)).filter(|&r| r < row_tracks.len()).collect();
            let cur_sum: f32 = span_indices.iter().map(|&r| row_tracks[r]).sum::<f32>() + total_row_gap;
            if cur_sum >= h { continue; }
            // Distribute deficit do auto-class spanned rows. Pokud zadne auto, skip.
            let auto_recipients: Vec<usize> = span_indices.iter().copied().filter(|&r| {
                row_token_kinds.get(r).map(|t| matches!(t, Track::Auto | Track::MaxContent | Track::MinContent | Track::FitContent(_))).unwrap_or(rows_explicit_str.is_empty() || r >= explicit_row_count)
            }).collect();
            if auto_recipients.is_empty() { continue; }
            // Distribute to FIRST auto recipient (taffy behavior - one row absorbs).
            // CSS spec ambiguous, taffy testy ocekavaji prvni track.
            let deficit = h - cur_sum;
            let first = auto_recipients[0];
            row_tracks[first] += deficit;
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
            let explicit_col = if child.grid_column_start > 0 { Some(((child.grid_column_start - 1) as usize) + col_prepend) }
                              else if child.grid_column_start < 0 { let k = (-child.grid_column_start) as usize; if k <= cols_explicit + 1 { Some(cols_explicit + 1 - k) } else { Some(0) } }
                              else { None };
            let explicit_row = if child.grid_row_start > 0 { Some(((child.grid_row_start - 1) as usize) + row_prepend) }
                              else if child.grid_row_start < 0 { let k = (-child.grid_row_start) as usize; let total_explicit_lines = row_count_explicit + row_prepend + 1; if k <= total_explicit_lines { Some(total_explicit_lines - k) } else { Some(0) } }
                              else { None };
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

    // Place items - multi-pass podle CSS Grid §8.5 auto-placement algoritmu:
    // Pass 1: items s definite row + col (oba explicitni)
    // Pass 2: items s definite row, auto col (row-locked)
    // Pass 3: items s definite col, auto row (col-locked)
    // Pass 4: full auto items
    let mut occupied: Vec<bool> = vec![false; rows.max(1) * cols.max(1)];
    let mut auto_cursor = 0usize;
    let mut item_row_info: Vec<(usize, usize, f32)> = Vec::new();
    // Precompute placements: pre kazdy real_idx (row, col, span_row, span_col).
    let mut placements: std::collections::HashMap<usize, (usize, usize, usize, usize)> = std::collections::HashMap::new();
    let resolve_end_fn = |start: i32, end: i32, span: i32, count: usize| -> usize {
        if span > 0 { return span as usize; }
        if end < 0 && start > 0 {
            let end_line = (count as i32 + 1 + end + 1).max(start + 1);
            ((end_line - start).max(1)) as usize
        } else if end > 0 && start > 0 {
            ((end - start).max(1)) as usize
        } else { 1 }
    };
    // Klasifikace items per CSS Grid §8.5:
    //   pass 1: oba osy definite
    //   pass 2: row-locked (only row definite, doc order)
    //   pass 3: col-locked + auto (doc order combined)
    let mut both_definite: Vec<usize> = Vec::new();
    let mut row_locked: Vec<usize> = Vec::new();
    let mut col_or_auto: Vec<usize> = Vec::new(); // col-locked + plne auto, v doc order
    for &real_idx in in_flow.iter() {
        let child = &bx.children[real_idx];
        let has_col = child.grid_column_start != 0;
        let has_row = child.grid_row_start != 0;
        if has_col && has_row { both_definite.push(real_idx); }
        else if has_row { row_locked.push(real_idx); }
        else { col_or_auto.push(real_idx); }
    }
    let resolve_col = |start: i32| -> Option<usize> {
        if start > 0 { Some(((start - 1) as usize) + col_prepend) }
        else if start < 0 { let k = (-start) as usize; if k <= cols_explicit + 1 { Some(cols_explicit + 1 - k) } else { Some(0) } }
        else { None }
    };
    let mark_occupied = |occupied: &mut Vec<bool>, row: usize, col: usize, span_row: usize, span_col: usize| {
        for dr in 0..span_row {
            for dc in 0..span_col {
                let idx = (row + dr) * cols + (col + dc);
                if idx < occupied.len() { occupied[idx] = true; }
            }
        }
    };
    // Pass 1: both definite.
    for &real_idx in &both_definite {
        let child = &bx.children[real_idx];
        let col = resolve_col(child.grid_column_start).unwrap_or(0);
        let row = if child.grid_row_start > 0 { ((child.grid_row_start - 1) as usize) + row_prepend }
                  else if child.grid_row_start < 0 {
                      let k = (-child.grid_row_start) as usize;
                      let total_explicit_lines = row_count_explicit + row_prepend + 1;
                      if k <= total_explicit_lines { total_explicit_lines - k } else { 0 }
                  } else { 0 };
        let span_col = resolve_end_fn(child.grid_column_start, child.grid_column_end, child.grid_column_span, cols);
        let span_row = resolve_end_fn(child.grid_row_start, child.grid_row_end, child.grid_row_span, rows);
        mark_occupied(&mut occupied, row, col, span_row, span_col);
        placements.insert(real_idx, (row, col, span_row, span_col));
    }
    // Pass 2: row-locked.
    let resolve_row = |start: i32| -> usize {
        if start > 0 { ((start - 1) as usize) + row_prepend }
        else if start < 0 {
            let k = (-start) as usize;
            let total_explicit_lines = row_count_explicit + row_prepend + 1;
            if k <= total_explicit_lines { total_explicit_lines - k } else { 0 }
        } else { 0 }
    };
    for &real_idx in &row_locked {
        let child = &bx.children[real_idx];
        let row = resolve_row(child.grid_row_start);
        let span_col = resolve_end_fn(child.grid_column_start, child.grid_column_end, child.grid_column_span, cols);
        let span_row = resolve_end_fn(child.grid_row_start, child.grid_row_end, child.grid_row_span, rows);
        let mut c = 0;
        loop {
            if c + span_col > cols { break; }
            let mut blocked = false;
            for dc in 0..span_col {
                let idx = row * cols + (c + dc);
                if idx < occupied.len() && occupied[idx] { blocked = true; break; }
            }
            if !blocked { break; }
            c += 1;
        }
        mark_occupied(&mut occupied, row, c, span_row, span_col);
        placements.insert(real_idx, (row, c, span_row, span_col));
    }
    // Pass 3+4: col-locked + auto v doc order.
    for &real_idx in &col_or_auto {
        let child = &bx.children[real_idx];
        let span_col = resolve_end_fn(child.grid_column_start, child.grid_column_end, child.grid_column_span, cols);
        let span_row = resolve_end_fn(child.grid_row_start, child.grid_row_end, child.grid_row_span, rows);
        let has_col = child.grid_column_start != 0;
        let (r, c) = if has_col {
            // Col-locked: find first free row v fixed col.
            let col = resolve_col(child.grid_column_start).unwrap_or(0);
            let mut rr = 0;
            loop {
                let mut blocked = false;
                for dr in 0..span_row {
                    for dc in 0..span_col {
                        let idx = (rr + dr) * cols + (col + dc);
                        if idx < occupied.len() && occupied[idx] { blocked = true; break; }
                    }
                    if blocked { break; }
                }
                if !blocked { break; }
                rr += 1;
                if rr > rows + 100 { break; }
            }
            (rr, col)
        } else if column_flow {
            // Column-flow auto: advance row-first, then col.
            let mut idx = auto_cursor;
            let result;
            loop {
                let cc = idx / rows.max(1);
                let rr = idx % rows.max(1);
                if rr + span_row > rows { idx = (cc + 1) * rows.max(1); continue; }
                let mut blocked = false;
                for dr in 0..span_row {
                    for dc in 0..span_col {
                        let i = (rr + dr) * cols + (cc + dc);
                        if i < occupied.len() && occupied[i] { blocked = true; break; }
                    }
                    if blocked { break; }
                }
                if !blocked {
                    auto_cursor = idx + 1;
                    result = (rr, cc);
                    break;
                }
                idx += 1;
                if idx > occupied.len() + 100 { result = (0, 0); break; }
            }
            result
        } else {
            // Auto: posun cursorem (row-major).
            let mut idx = auto_cursor;
            let result;
            loop {
                let rr = idx / cols.max(1);
                let cc = idx % cols.max(1);
                if cc + span_col > cols { idx = (rr + 1) * cols.max(1); continue; }
                let mut blocked = false;
                for dr in 0..span_row {
                    for dc in 0..span_col {
                        let i = (rr + dr) * cols + (cc + dc);
                        if i < occupied.len() && occupied[i] { blocked = true; break; }
                    }
                    if blocked { break; }
                }
                if !blocked {
                    auto_cursor = idx + 1;
                    result = (rr, cc);
                    break;
                }
                idx += 1;
                if idx > occupied.len() + 100 { result = (0, 0); break; }
            }
            result
        };
        mark_occupied(&mut occupied, r, c, span_row, span_col);
        placements.insert(real_idx, (r, c, span_row, span_col));
    }
    // Apply placements v document order.
    for &real_idx in in_flow.iter() {
        let (row, col, span_row, span_col) = match placements.get(&real_idx) {
            Some(&p) => p,
            None => continue,
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
        // Pri auto margin v ose: pouzij intrinsic (0 nebo text_h), ne ch_avail (ten by stretchnul item).
        let item_h = child.explicit_height.unwrap_or_else(|| if any_auto_y { text_h } else { ch_avail });
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
        // Percent values resolvujem proti grid container inner_w/inner_h.
        // Bez tohoto by max-width: 100% z parse_length vracelo 16 (default
        // parent_size) a item zustaval clampnuty na 16 px.
        fn pct_or_px(v: &str, parent: f32) -> f32 {
            if let Some(pct_str) = v.trim().strip_suffix('%') {
                if let Ok(p) = pct_str.parse::<f32>() {
                    return parent * p / 100.0;
                }
            }
            super::super::layout::parse_length(v)
        }
        let cw_min = if child.min_width_v.is_empty() || child.min_width_v == "none" { 0.0 } else { pct_or_px(&child.min_width_v, cw_avail) };
        let cw_max = if child.max_width_v.is_empty() || child.max_width_v == "none" { f32::INFINITY } else { pct_or_px(&child.max_width_v, cw_avail) };
        let ch_min = if child.min_height_v.is_empty() || child.min_height_v == "none" { 0.0 } else { pct_or_px(&child.min_height_v, ch_avail) };
        let ch_max = if child.max_height_v.is_empty() || child.max_height_v == "none" { f32::INFINITY } else { pct_or_px(&child.max_height_v, ch_avail) };
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
        // CSS positioning offset - top/left/bottom/right se aplikuje JEN pri
        // position: relative. Sticky se vola apply_sticky() zvlast pri scroll.
        // Static + sticky pri layout-time = bez offset (sticky shift dela
        // apply_sticky podle scroll_y).
        if matches!(child.position, super::super::layout::Position::Relative) {
            if let Some(l) = child.offset_left { child.rect.x += l; }
            else if let Some(r) = child.offset_right { child.rect.x -= r; }
            if let Some(t) = child.offset_top { child.rect.y += t; }
            else if let Some(b) = child.offset_bottom { child.rect.y -= b; }
        }
        child.rect.width = final_w;
        child.rect.height = final_h;
        // Subgrid (CSS Grid L2): pri grid-template-rows/columns = "subgrid",
        // misto vlastnich tracku pouzij parent's tracks v ramci grid area item.
        // Substituce pred recursive layout_grid.
        if matches!(child.display, super::super::layout::Display::Grid)
            && (child.grid_template_rows.trim() == "subgrid" || child.grid_template_columns.trim() == "subgrid")
        {
            let (row, col, span_row, span_col) = (row, col, span_row, span_col);
            // Inner padding+border na child - subgrid tracky se aplikuji do inner.
            let pb_l_c = child.padding_left.unwrap_or(child.padding) + child.border_left_width.unwrap_or(child.border_width);
            let pb_r_c = child.padding_right.unwrap_or(child.padding) + child.border_right_width.unwrap_or(child.border_width);
            let pb_t_c = child.padding_top.unwrap_or(child.padding) + child.border_top_width.unwrap_or(child.border_width);
            let pb_b_c = child.padding_bottom.unwrap_or(child.padding) + child.border_bottom_width.unwrap_or(child.border_width);
            // Dostupne inner_w/h pro subgrid tracky.
            let _ = (pb_l_c, pb_r_c, pb_t_c, pb_b_c);
            if child.grid_template_columns.trim() == "subgrid" {
                let mut tracks_str = String::new();
                for d in 0..span_col {
                    let t = col_tracks.get(col + d).copied().unwrap_or(0.0);
                    if d > 0 { tracks_str.push(' '); }
                    tracks_str.push_str(&format!("{}px", t));
                }
                child.grid_template_columns = tracks_str;
            }
            if child.grid_template_rows.trim() == "subgrid" {
                let mut tracks_str = String::new();
                for d in 0..span_row {
                    let t = row_tracks.get(row + d).copied().unwrap_or(0.0);
                    if d > 0 { tracks_str.push(' '); }
                    tracks_str.push_str(&format!("{}px", t));
                }
                child.grid_template_rows = tracks_str;
            }
        }
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
        // Pri percent rows v indefinite container: nepritahuj container na sum
        // tracks - container = intrinsic (max item content height).
        let has_percent_row = !rows_explicit_str.is_empty() && rows_explicit_str.contains('%');
        if has_percent_row {
            // Najdi max item explicit_height (intrinsic).
            let mut max_item_h = 0.0_f32;
            for ch in bx.children.iter() {
                if super::is_out_of_flow(ch) || matches!(ch.display, super::super::layout::Display::None) { continue; }
                if let Some(h) = ch.explicit_height {
                    if h > max_item_h { max_item_h = h; }
                }
            }
            let intrinsic_h = max_item_h + pad_t + pad_b;
            if bx.rect.height < intrinsic_h {
                bx.rect.height = intrinsic_h;
            }
        } else {
            // Vzdy override - bez tohoto pri pre-pass (inner_w=0, rows_pocet=children)
            // se rect.height nastavila na 8*row_h, a nasledny correct pass (cols=20,
            // 1 row) ji nedokazal zmensit, takze container zustal multiple row tall.
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
                // Pro auto-fill/auto-fit: minmax(min_px, X) pouzije min_px jako
                // intrinsic min (per CSS Grid §7.2.2.1). Bez toho by minmax(120,1fr)
                // pri auto-fill = sub_size 0 -> count=1 -> 1 sloupec, items
                // collapse na ~0 px sirky misto N sloupcu po 120 px.
                let sub_size: f32 = sub_tokens.iter().map(|t| match t {
                    Track::Fixed(p) => *p,
                    Track::Percent(p) => container * p / 100.0,
                    Track::Minmax(min_v, _, _) => {
                        if min_v.is_nan() || *min_v <= -999.0 { 0.0 }
                        else if *min_v < 0.0 && *min_v >= -1.0 { container * (-min_v) }
                        else { min_v.max(0.0) }
                    }
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
                // Pro auto-fill/auto-fit: minmax(min_px, X) pouzije min_px jako
                // intrinsic min (per CSS Grid §7.2.2.1). Bez toho by minmax(120,1fr)
                // pri auto-fill = sub_size 0 -> count=1 -> 1 sloupec, items
                // collapse na ~0 px sirky misto N sloupcu po 120 px.
                let sub_size: f32 = sub_tokens.iter().map(|t| match t {
                    Track::Fixed(p) => *p,
                    Track::Percent(p) => container * p / 100.0,
                    Track::Minmax(min_v, _, _) => {
                        if min_v.is_nan() || *min_v <= -999.0 { 0.0 }
                        else if *min_v < 0.0 && *min_v >= -1.0 { container * (-min_v) }
                        else { min_v.max(0.0) }
                    }
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
