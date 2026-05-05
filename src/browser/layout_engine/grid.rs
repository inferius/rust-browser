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
    let pad_l = bx.padding_left.unwrap_or(bx.padding) + bx.border_width;
    let pad_r = bx.padding_right.unwrap_or(bx.padding) + bx.border_width;
    let pad_t = bx.padding_top.unwrap_or(bx.padding) + bx.border_width;
    let pad_b = bx.padding_bottom.unwrap_or(bx.padding) + bx.border_width;
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

    let item_count = bx.children.len();
    let rows_explicit_str = bx.grid_template_rows.clone();
    let rows = if !rows_explicit_str.is_empty() {
        super::super::layout::parse_length(&rows_explicit_str).max(0.0) as usize;
        let count = parse_track_count(&rows_explicit_str);
        if count > 0 { count } else { item_count.div_ceil(cols) }
    } else {
        item_count.div_ceil(cols)
    };

    // Resolve row tracks (s default_row_h pro auto)
    let default_row_h = 50.0_f32;
    let row_tracks: Vec<f32> = if !rows_explicit_str.is_empty() {
        let resolved = resolve_tracks(&rows_explicit_str, inner_h, row_gap);
        if resolved.is_empty() { vec![default_row_h; rows] }
        else { resolved }
    } else {
        // Per-row: prevezit explicit_height z prvni dite v row, jinak default
        let mut out = Vec::with_capacity(rows);
        for r in 0..rows {
            let mut h = default_row_h;
            for c in 0..cols {
                let idx = r * cols + c;
                if let Some(child) = bx.children.get(idx) {
                    if let Some(eh) = child.explicit_height {
                        h = h.max(eh);
                    }
                }
            }
            out.push(h);
        }
        out
    };

    // Total tracks delky
    let total_col: f32 = col_tracks.iter().sum::<f32>() + col_gap * (cols.saturating_sub(1) as f32);
    let total_row: f32 = row_tracks.iter().sum::<f32>() + row_gap * (rows.saturating_sub(1) as f32);
    // justify-content (inline = column axis) + align-content (block = row axis)
    let (jc_start, jc_between) = grid_distribute(&bx.justify_content, (inner_w - total_col).max(0.0), cols);
    let (ac_start, ac_between) = grid_distribute(&bx.align_content, (inner_h - total_row).max(0.0), rows);

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

    // In-flow indices (skip abs/fixed)
    let in_flow: Vec<usize> = bx.children.iter().enumerate()
        .filter(|(_, c)| !super::is_out_of_flow(c))
        .map(|(i, _)| i)
        .collect();

    // Place items v auto-flow row order (jen in-flow)
    for (k, &real_idx) in in_flow.iter().enumerate() {
        let row = k / cols;
        let col = k % cols;
        if row >= rows { break; }
        let cw = col_tracks.get(col).copied().unwrap_or(default_row_h);
        let ch_h = row_tracks.get(row).copied().unwrap_or(default_row_h);
        let child = &mut bx.children[real_idx];
        child.rect.x = inner_x + col_positions[col];
        child.rect.y = inner_y + row_positions[row];
        child.rect.width = child.explicit_width.unwrap_or(cw);
        child.rect.height = child.explicit_height.unwrap_or(ch_h);
        super::super::layout::layout_block(child);
    }

    // Update parent height
    let total_h = y_cursor + pad_t + pad_b;
    if bx.rect.height < total_h {
        bx.rect.height = total_h;
    }

    // Position absolute/fixed children
    let parent_x = bx.rect.x;
    let parent_y = bx.rect.y;
    let parent_w = bx.rect.width;
    let parent_h = bx.rect.height;
    for ch in bx.children.iter_mut() {
        if super::is_out_of_flow(ch) {
            super::layout_absolute_child(ch, parent_x, parent_y, parent_w, parent_h);
        }
    }
}

/// Distribuce free space pro justify/align-content v gridu.
/// Vraci (start_offset, between_extra) - mezi tracky se pak prida `between_extra` (krome gap).
fn grid_distribute(value: &str, free: f32, count: usize) -> (f32, f32) {
    if free <= 0.0 || count == 0 { return (0.0, 0.0); }
    match value.trim() {
        "end" | "flex-end" => (free, 0.0),
        "center" => (free / 2.0, 0.0),
        "space-between" => {
            if count <= 1 { (0.0, 0.0) }
            else { (0.0, free / (count - 1) as f32) }
        }
        "space-around" => {
            let g = free / count as f32;
            (g / 2.0, g)
        }
        "space-evenly" => {
            let g = free / (count + 1) as f32;
            (g, g)
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
    let tokens = parse_track_tokens(s);
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
    // fr base
    let fr_base = if fr_total > 0.0 { free / fr_total } else { 0.0 };
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

/// Tokenizace grid-template-columns/rows + expand repeat().
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
