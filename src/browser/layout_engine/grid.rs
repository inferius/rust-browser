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

    let row_gap = bx.row_gap;
    let col_gap = bx.column_gap;

    // Parse + resolve column tracks
    let mut col_tracks = resolve_tracks(&bx.grid_template_columns, inner_w, col_gap);
    let col_token_kinds = parse_track_tokens_sized(&bx.grid_template_columns, inner_w, col_gap);
    if col_tracks.is_empty() { col_tracks = vec![inner_w]; }
    let cols = col_tracks.len();
    let col_is_auto: Vec<bool> = (0..cols).map(|i| match col_token_kinds.get(i) {
        Some(Track::Auto) => true,
        Some(Track::Minmax(_, max, false)) if !max.is_finite() => true,
        _ => false,
    }).collect();

    // In-flow item count (abs/fixed/display:none vyradit pri vypoctu rows).
    let in_flow_count = bx.children.iter()
        .filter(|c| !super::is_out_of_flow(c) && !matches!(c.display, super::super::layout::Display::None))
        .count();
    let rows_explicit_str = bx.grid_template_rows.clone();
    let rows = if !rows_explicit_str.is_empty() {
        let count = parse_track_count(&rows_explicit_str);
        let needed = in_flow_count.div_ceil(cols.max(1));
        count.max(needed)
    } else {
        in_flow_count.div_ceil(cols.max(1))
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
            if !row_has_explicit && any_explicit { h = 50.0; }
            out.push(h);
        }
        out
    };
    // Implicitni rows: pokud potreba vic nez explicit, doplnit z child explicit_height (jinak 0).
    while row_tracks.len() < rows {
        let r = row_tracks.len();
        let mut h = 0.0_f32;
        for c in 0..cols {
            let idx = r * cols + c;
            if let Some(child) = bx.children.get(idx) {
                if let Some(eh) = child.explicit_height {
                    h = h.max(eh);
                }
            }
        }
        row_tracks.push(h);
    }

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
        // Pro kazdy auto col, najdi max intrinsic width z items (min-content).
        // Reset auto cols na min-content (z items) misto auto_base.
        for c_idx in 0..cols {
            if !col_is_auto[c_idx] { continue; }
            let mut max_w = 0.0_f32;
            for (i, &real_idx) in in_flow.iter().enumerate() {
                let (_, col, _, span_col) = item_placement[i];
                if col == c_idx && span_col == 1 {
                    let item = &bx.children[real_idx];
                    let intrinsic = item.explicit_width.unwrap_or(item.rect.width);
                    let pb_l = item.padding_left.unwrap_or(item.padding) + item.border_left_width.unwrap_or(item.border_width);
                    let pb_r = item.padding_right.unwrap_or(item.padding) + item.border_right_width.unwrap_or(item.border_width);
                    let cw_min_p = super::super::layout::parse_length(&item.min_width_v);
                    let real_intrinsic = intrinsic.max(pb_l + pb_r).max(cw_min_p);
                    if real_intrinsic > max_w { max_w = real_intrinsic; }
                }
            }
            col_tracks[c_idx] = max_w;
        }
        // Distribute leftover free space rovnomerne mezi auto cols.
        if inner_w > 0.0 {
            let total_used: f32 = col_tracks.iter().sum::<f32>() + col_gap * (cols.saturating_sub(1) as f32);
            let leftover = inner_w - total_used;
            let auto_count = col_is_auto.iter().filter(|&&b| b).count();
            if leftover > 0.0 && auto_count > 0 {
                let share = leftover / auto_count as f32;
                for c_idx in 0..cols {
                    if col_is_auto[c_idx] { col_tracks[c_idx] += share; }
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
            // Pro kazdy item span 1 col, kdyz fr a explicit_width > current track, expand.
            // Iterate vicekrat, protoze expansion v jednom col snizi zbyle pro ostatni fr.
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
                            if let Some(w) = bx.children[real_idx].explicit_width {
                                if w > req { req = w; }
                            }
                        }
                    }
                    if req > col_tracks[c_idx] {
                        col_tracks[c_idx] = req;
                        available_for_fr -= req;
                        active_fr_total -= fr_factors[c_idx];
                        changed = true;
                    }
                }
                // Multi-span items: kdyz item span > 1 a explicit_width > suma tracku v spanu,
                // distribute extra mezi fr tracky v spanu (proporc. fr factoru, equal pri sum=0).
                for (i, &real_idx) in in_flow.iter().enumerate() {
                    let (_, col, _, span_col) = item_placements[i];
                    if span_col <= 1 { continue; }
                    if let Some(w) = bx.children[real_idx].explicit_width {
                        let span_sum: f32 = (col..(col+span_col)).map(|c| col_tracks.get(c).copied().unwrap_or(0.0)).sum();
                        let span_gap = col_gap * (span_col.saturating_sub(1) as f32);
                        let needed = w - span_sum - span_gap;
                        if needed > 0.0 {
                            // Najdi fr tracky v spanu.
                            let fr_in_span: Vec<usize> = (col..(col+span_col)).filter(|&c| is_fr_track.get(c).copied().unwrap_or(false)).collect();
                            if !fr_in_span.is_empty() {
                                let fr_sum_span: f32 = fr_in_span.iter().map(|&c| fr_factors[c]).sum();
                                if fr_sum_span > 0.0 {
                                    for &c in &fr_in_span {
                                        col_tracks[c] += needed * fr_factors[c] / fr_sum_span;
                                    }
                                } else {
                                    // 0fr - rozdel rovnomerne.
                                    let share = needed / fr_in_span.len() as f32;
                                    for &c in &fr_in_span { col_tracks[c] += share; }
                                }
                                changed = true;
                            }
                        }
                    }
                }
                // Redistribute zbytek mezi fr tracky, ktere jeste nejsou expanded nad ratio.
                if active_fr_total > 0.0 && available_for_fr >= 0.0 {
                    let fr_size = available_for_fr / active_fr_total;
                    for c_idx in 0..cols {
                        let f = fr_factors[c_idx];
                        if f > 0.0 {
                            let new_size = fr_size * f;
                            // Jen nastav, pokud item-driven expansion neni vetsi.
                            if (col_tracks[c_idx] - new_size).abs() > 0.01 && new_size > col_tracks[c_idx] {
                                col_tracks[c_idx] = new_size;
                                changed = true;
                            } else if !changed && (col_tracks[c_idx] - new_size).abs() > 0.01 {
                                col_tracks[c_idx] = new_size;
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
                let h = item.explicit_height.unwrap_or(item.rect.height);
                let entry = by_row.entry(row).or_insert(0.0);
                if h > *entry { *entry = h; }
            }
        }
        for (r, h) in by_row {
            if r < row_tracks.len() && row_tracks[r] < h { row_tracks[r] = h; }
        }
    }

    // Total tracks delky (po auto track sizing)
    let total_col: f32 = col_tracks.iter().sum::<f32>() + col_gap * (cols.saturating_sub(1) as f32);
    let total_row: f32 = row_tracks.iter().sum::<f32>() + row_gap * (rows.saturating_sub(1) as f32);
    let (jc_start, jc_between) = grid_distribute(&bx.justify_content, inner_w - total_col, cols);
    let (ac_start, ac_between) = grid_distribute(&bx.align_content, inner_h - total_row, rows);
    let mut col_positions: Vec<f32> = Vec::with_capacity(cols);
    let mut x_cursor = jc_start;
    for (i, w) in col_tracks.iter().enumerate() {
        col_positions.push(x_cursor);
        x_cursor += *w;
        if i + 1 < cols { x_cursor += col_gap + jc_between; }
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
        // Resolve item size + alignment v grid area
        let parent_align_items = bx.align_items.clone();
        let parent_justify_items = bx.justify_items.clone();
        let child = &mut bx.children[real_idx];
        let m_l = child.margin_left.unwrap_or(child.margin);
        let m_r = child.margin_right.unwrap_or(child.margin);
        let m_t = child.margin_top.unwrap_or(child.margin);
        let m_b = child.margin_bottom.unwrap_or(child.margin);
        let cw_avail = (cw - m_l - m_r).max(0.0);
        let ch_avail = (ch_h - m_t - m_b).max(0.0);
        let has_w = child.explicit_width.is_some();
        let has_h = child.explicit_height.is_some();
        let item_w = child.explicit_width.unwrap_or(cw_avail);
        let item_h = child.explicit_height.unwrap_or(ch_avail);
        // justify-self na inline (cols), align-self na block (rows). Default = stretch.
        let js = if !child.justify_self.is_empty() { child.justify_self.clone() } else { parent_justify_items };
        let als = if !child.align_self.is_empty() { child.align_self.clone() } else { parent_align_items };
        let stretch_w = !has_w && (js.is_empty() || js == "stretch" || js == "normal");
        let stretch_h = !has_h && (als.is_empty() || als == "stretch" || als == "normal");
        let mut final_w = if stretch_w { cw_avail } else { item_w };
        let mut final_h = if stretch_h { ch_avail } else { item_h };
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
            Track::Minmax(min, max, is_fr) => {
                let min_resolved = if *min < 0.0 { container_size * (-min) } else { *min };
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
        Track::Minmax(min, max, is_fr) => {
            let min_r = if *min < 0.0 { container_size * (-min) } else { *min };
            let max_r = if *max < 0.0 { container_size * (-max) } else { *max };
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
    /// minmax(min_px, max_px or fr) - vlastne flexible s rozsahem.
    Minmax(f32, f32, bool /* max je fr */),
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
    if s == "auto" || s == "min-content" || s == "max-content" { return Track::Auto; }
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
            // Pri resolve to detekuje a vynasobi container.
            let parse_part = |p: &str| -> f32 {
                if p == "auto" || p == "min-content" || p == "max-content" { return 0.0; }
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
    // fit-content(<value>) - jako auto
    if s.starts_with("fit-content(") { return Track::Auto; }
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
