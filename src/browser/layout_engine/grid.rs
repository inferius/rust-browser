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
    if col_tracks.is_empty() { col_tracks = vec![inner_w]; }
    let cols = col_tracks.len();

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
        let fallback_h = if any_explicit { 50.0 } else if rows > 0 { (inner_h / rows as f32).max(0.0) } else { inner_h.max(0.0) };
        for r in 0..rows {
            let mut h = fallback_h;
            let mut row_has_explicit = false;
            for c in 0..cols {
                let idx = r * cols + c;
                if let Some(child) = bx.children.get(idx) {
                    if let Some(eh) = child.explicit_height {
                        h = h.max(eh);
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

    // Total tracks delky
    let total_col: f32 = col_tracks.iter().sum::<f32>() + col_gap * (cols.saturating_sub(1) as f32);
    let total_row: f32 = row_tracks.iter().sum::<f32>() + row_gap * (rows.saturating_sub(1) as f32);
    // justify-content (inline = column axis) + align-content (block = row axis)
    let (jc_start, jc_between) = grid_distribute(&bx.justify_content, inner_w - total_col, cols);
    let (ac_start, ac_between) = grid_distribute(&bx.align_content, inner_h - total_row, rows);

    // Compute x positions per col + y positions per row
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

    // Place items - explicit (grid-row-start/grid-column-start) i auto-flow row major.
    // Track occupied cells.
    let mut occupied: Vec<bool> = vec![false; rows.max(1) * cols.max(1)];
    let mut auto_cursor = 0usize;
    for &real_idx in in_flow.iter() {
        let child = &bx.children[real_idx];
        // Resolve placement: 1-based start lines -> 0-based cell index
        let explicit_col = if child.grid_column_start > 0 { Some((child.grid_column_start - 1) as usize) } else { None };
        let explicit_row = if child.grid_row_start > 0 { Some((child.grid_row_start - 1) as usize) } else { None };
        let span_col = if child.grid_column_span > 0 { child.grid_column_span as usize }
                       else if child.grid_column_end > 0 && child.grid_column_start > 0 { (child.grid_column_end - child.grid_column_start).max(1) as usize }
                       else { 1 };
        let span_row = if child.grid_row_span > 0 { child.grid_row_span as usize }
                       else if child.grid_row_end > 0 && child.grid_row_start > 0 { (child.grid_row_end - child.grid_row_start).max(1) as usize }
                       else { 1 };
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
        // Apply min/max
        let cw_min = super::super::layout::parse_length(&child.min_width_v);
        let cw_max = if child.max_width_v.is_empty() { f32::INFINITY } else { super::super::layout::parse_length(&child.max_width_v) };
        let ch_min = super::super::layout::parse_length(&child.min_height_v);
        let ch_max = if child.max_height_v.is_empty() { f32::INFINITY } else { super::super::layout::parse_length(&child.max_height_v) };
        final_w = final_w.min(cw_max);
        if cw_min > 0.0 { final_w = final_w.max(cw_min); }
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
        child.rect.x = inner_x + cx + m_l + off_x;
        child.rect.y = inner_y + cy + m_t + off_y;
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
            super::layout_absolute_child(ch, cb_x, cb_y, cb_w, cb_h);
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

    // Klasifikuj tokens
    let mut fixed_total = 0.0_f32;
    let mut fr_total = 0.0_f32;
    let mut auto_count = 0;
    for t in &tokens {
        match t {
            Track::Fixed(px) => fixed_total += *px,
            Track::Percent(p) => fixed_total += container_size * p / 100.0,
            Track::Fr(f) => fr_total += *f,
            Track::Auto => auto_count += 1,
        }
    }

    let free = (container_size - fixed_total - total_gap).max(0.0);
    // fr base: kdyz fr_total < 1, divisor je 1 (CSS spec).
    let fr_base = if fr_total > 0.0 { free / fr_total.max(1.0) } else { 0.0 };
    // auto base (po fr distribuci)
    let after_fr = (free - fr_base * fr_total).max(0.0);
    let auto_base = if auto_count > 0 { after_fr / auto_count as f32 } else { 0.0 };

    tokens.iter().map(|t| match t {
        Track::Fixed(px) => *px,
        Track::Percent(p) => container_size * p / 100.0,
        Track::Fr(f) => fr_base * f,
        Track::Auto => auto_base,
    }).collect()
}

#[derive(Debug, Clone, Copy)]
enum Track {
    Fixed(f32),
    Percent(f32),
    Fr(f32),
    Auto,
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
        if buf.starts_with("repeat(") { continue; }
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
    if let Some(num) = s.strip_suffix("fr") {
        return Track::Fr(num.trim().parse().unwrap_or(1.0));
    }
    if let Some(num) = s.strip_suffix('%') {
        return Track::Percent(num.trim().parse().unwrap_or(0.0));
    }
    // minmax(min, max) - vrati max
    if let Some(rest) = s.strip_prefix("minmax(").and_then(|x| x.strip_suffix(')')) {
        let parts: Vec<&str> = rest.split(',').collect();
        if let Some(max) = parts.get(1) {
            return parse_single_track(max.trim());
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
