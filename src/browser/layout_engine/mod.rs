/// Vlastni layout engine - flex / grid.
///
/// Inspirovano `taffy` crate (MIT licence, https://github.com/DioxusLabs/taffy).
/// Zacatek byl wrapper pres taffy, postupne nahrazujeme vlastni implementaci aby
/// sme meli plnou kontrolu nad layout chovanim a mohli pridat custom features
/// (subgrid, real shape-outside, atd.) ktere taffy nepodporuje.
///
/// Kompletni flex spec je velky - implementujeme JEN co realne v rendrovanych
/// strankach pouzivame: row/column direction, wrap, justify-content,
/// align-items, gap, basic flex-grow/shrink.

pub mod flex;
pub mod grid;

use crate::browser::layout::{LayoutBox, Position, Display};

/// Layout absolute/fixed positioned child relativne k containing block.
/// CB = padding-box parenta = border-box minus border (CSS Position L3).
pub fn layout_absolute_child_with_parent(child: &mut LayoutBox, parent: &LayoutBox) {
    let bw_l = parent.border_left_width.unwrap_or(parent.border_width);
    let bw_r = parent.border_right_width.unwrap_or(parent.border_width);
    let bw_t = parent.border_top_width.unwrap_or(parent.border_width);
    let bw_b = parent.border_bottom_width.unwrap_or(parent.border_width);
    // Scrollbar zmenuje containing block pro abs items.
    let sb_w = if parent.scrollbar_size > 0.0 && (parent.overflow_y == "scroll" || parent.overflow_y == "auto") { parent.scrollbar_size } else { 0.0 };
    let sb_h = if parent.scrollbar_size > 0.0 && (parent.overflow_x == "scroll" || parent.overflow_x == "auto") { parent.scrollbar_size } else { 0.0 };
    let cb_x = parent.rect.x + bw_l;
    let cb_y = parent.rect.y + bw_t;
    let cb_w = (parent.rect.width - bw_l - bw_r - sb_w).max(0.0);
    let cb_h = (parent.rect.height - bw_t - bw_b - sb_h).max(0.0);
    layout_absolute_child_inner(child, cb_x, cb_y, cb_w, cb_h);
}

/// Backward compat - parent_x/y/w/h vetsinou rect (bez border odecet).
pub fn layout_absolute_child(child: &mut LayoutBox, parent_x: f32, parent_y: f32, parent_w: f32, parent_h: f32) {
    layout_absolute_child_inner(child, parent_x, parent_y, parent_w, parent_h);
}

fn layout_absolute_child_inner(child: &mut LayoutBox, parent_x: f32, parent_y: f32, parent_w: f32, parent_h: f32) {
    let cb_x = parent_x;
    let cb_y = parent_y;
    let cb_w = parent_w;
    let cb_h = parent_h;
    // Pre-pass intrinsic: pri abs item bez explicit size (a bez plne sady inset) + has children.
    let needs_w = child.explicit_width.is_none() && !(child.offset_left.is_some() && child.offset_right.is_some());
    let needs_h = child.explicit_height.is_none() && !(child.offset_top.is_some() && child.offset_bottom.is_some());
    if (needs_w || needs_h) && !child.children.is_empty() {
        let saved = child.rect.clone();
        child.rect.x = 0.0; child.rect.y = 0.0;
        if needs_w { child.rect.width = 0.0; }
        if needs_h { child.rect.height = 0.0; }
        // Pre-pass intrinsic mode marker: rect = 0 v constrained axis.
        // Pri Flex: preserve item basis (no shrink).
        let saved_intrinsic_mode = std::mem::replace(&mut child.taffy_intrinsic_mode, true);
        match child.display {
            Display::Flex => flex::layout_flex(child),
            Display::Grid => grid::layout_grid(child),
            Display::Block => {
                // Block intrinsic: max child explicit_width pro w, sum heights+margins pro h.
                // Pri parent flex-direction (column): sum heights z flex-basis nebo height.
                let parent_is_flex_col = !child.flex_direction.is_empty() && child.flex_direction.contains("column");
                let mut max_w = 0.0_f32;
                let mut total_h = 0.0_f32;
                let mut prev_m_b = 0.0_f32;
                for gc in &child.children {
                    if matches!(gc.position, Position::Absolute | Position::Fixed) { continue; }
                    if matches!(gc.display, Display::None) { continue; }
                    let gw = gc.explicit_width.unwrap_or(0.0);
                    if gw > max_w { max_w = gw; }
                    // Height from explicit OR flex-basis (parent column flex).
                    let basis_h = if parent_is_flex_col {
                        let bv = gc.flex_basis.trim();
                        if let Some(num) = bv.strip_suffix("px") {
                            num.parse::<f32>().unwrap_or(0.0)
                        } else { 0.0 }
                    } else { 0.0 };
                    let gh = gc.explicit_height.unwrap_or(basis_h);
                    let gm_t = gc.margin_top.unwrap_or(gc.margin);
                    let gm_b = gc.margin_bottom.unwrap_or(gc.margin);
                    let collapsed = if prev_m_b >= 0.0 && gm_t >= 0.0 { prev_m_b.max(gm_t) } else { prev_m_b + gm_t };
                    total_h += collapsed + gh;
                    prev_m_b = gm_b;
                }
                total_h += prev_m_b;
                if needs_w { child.rect.width = max_w; }
                if needs_h { child.rect.height = total_h; }
            }
            _ => {}
        }
        let intrinsic_w = if needs_w { child.rect.width } else { saved.width };
        let intrinsic_h = if needs_h { child.rect.height } else { saved.height };
        child.rect = saved;
        child.taffy_intrinsic_mode = saved_intrinsic_mode;
        if needs_w && intrinsic_w > 0.0 { child.rect.width = intrinsic_w; }
        if needs_h && intrinsic_h > 0.0 { child.rect.height = intrinsic_h; }
    }
    // Width: explicit nebo (right-left) nebo intrinsic z pre-pass nebo aspect-ratio nebo 0
    // Margin pre-load pro inset-based size calc.
    let m_l_pre = child.margin_left.unwrap_or(child.margin);
    let m_r_pre = child.margin_right.unwrap_or(child.margin);
    let m_t_pre = child.margin_top.unwrap_or(child.margin);
    let m_b_pre = child.margin_bottom.unwrap_or(child.margin);
    // Text intrinsic v taffy_mode pro abs.
    let (text_w_int, text_h_int) = if child.taffy_mode {
        if let Some(t) = &child.text {
            let w = t.chars().filter(|c| !matches!(*c, '\u{200B}' | ' ' | '\n' | '\t')).count() as f32 * 10.0;
            (w, 10.0)
        } else { (0.0, 0.0) }
    } else { (0.0, 0.0) };
    let mut w = if let Some(w) = child.explicit_width {
        w
    } else if let (Some(l), Some(r)) = (child.offset_left, child.offset_right) {
        (cb_w - l - r - m_l_pre - m_r_pre).max(0.0)
    } else if let Some(ar) = child.aspect_ratio {
        if let Some(h) = child.explicit_height { if ar > 0.0 { h * ar } else { 0.0 } } else { 0.0 }
    } else if child.rect.width > 0.0 { child.rect.width }
    else if text_w_int > 0.0 { text_w_int }
    else { 0.0 };
    let mut h = if let Some(h) = child.explicit_height {
        h
    } else if let (Some(t), Some(b)) = (child.offset_top, child.offset_bottom) {
        (cb_h - t - b - m_t_pre - m_b_pre).max(0.0)
    } else if let Some(ar) = child.aspect_ratio {
        if ar > 0.0 { w / ar } else { 0.0 }
    } else if child.rect.height > 0.0 { child.rect.height }
    else if text_h_int > 0.0 { text_h_int }
    else { 0.0 };
    // Aspect ratio override pri "fill" sizing (inset bez explicit dimensi).
    if let Some(ar) = child.aspect_ratio {
        if ar > 0.0 {
            let has_explicit_w = child.explicit_width.is_some();
            let has_explicit_h = child.explicit_height.is_some();
            // Min/max jako preferred pri zadnem w/h.
            if w == 0.0 && !has_explicit_w {
                let cw_max_p = if child.max_width_v.is_empty() { 0.0 } else { crate::browser::layout::parse_length(&child.max_width_v) };
                let cw_min_p = crate::browser::layout::parse_length(&child.min_width_v);
                if cw_min_p > 0.0 { w = cw_min_p; }
                else if cw_max_p > 0.0 { w = cw_max_p; }
            }
            if h == 0.0 && !has_explicit_h {
                let ch_max_p = if child.max_height_v.is_empty() { 0.0 } else { crate::browser::layout::parse_length(&child.max_height_v) };
                let ch_min_p = crate::browser::layout::parse_length(&child.min_height_v);
                if ch_min_p > 0.0 { h = ch_min_p; }
                else if ch_max_p > 0.0 { h = ch_max_p; }
            }
            if has_explicit_w && !has_explicit_h {
                h = w / ar;
            } else if has_explicit_h && !has_explicit_w {
                w = h * ar;
            } else if !has_explicit_w && !has_explicit_h {
                if w > 0.0 {
                    h = w / ar;
                } else if h > 0.0 {
                    w = h * ar;
                }
            }
        }
    }
    // Apply min/max width/height + dopocet drugeho rozmeru z aspect-ratio kdyz min/max active
    let cw_min = crate::browser::layout::parse_length(&child.min_width_v);
    let cw_max = if child.max_width_v.is_empty() { f32::INFINITY } else { crate::browser::layout::parse_length(&child.max_width_v) };
    let ch_min = crate::browser::layout::parse_length(&child.min_height_v);
    let ch_max = if child.max_height_v.is_empty() { f32::INFINITY } else { crate::browser::layout::parse_length(&child.max_height_v) };
    // Apply max first then min, takze min wins kdyz min > max (CSS spec).
    w = w.min(cw_max);
    h = h.min(ch_max);
    if cw_min > 0.0 { w = w.max(cw_min); }
    if ch_min > 0.0 { h = h.max(ch_min); }
    // Padding+border floor.
    let pb_l = child.padding_left.unwrap_or(child.padding) + child.border_left_width.unwrap_or(child.border_width);
    let pb_r = child.padding_right.unwrap_or(child.padding) + child.border_right_width.unwrap_or(child.border_width);
    let pb_t = child.padding_top.unwrap_or(child.padding) + child.border_top_width.unwrap_or(child.border_width);
    let pb_b = child.padding_bottom.unwrap_or(child.padding) + child.border_bottom_width.unwrap_or(child.border_width);
    w = w.max(pb_l + pb_r);
    h = h.max(pb_t + pb_b);
    // Pokud ar set a jen jeden rozmer ma min/max-effect, dopocet drugeho.
    if let Some(ar) = child.aspect_ratio {
        if ar > 0.0 {
            let has_explicit_w = child.explicit_width.is_some();
            let has_explicit_h = child.explicit_height.is_some();
            if !has_explicit_w && w == 0.0 && h > 0.0 { w = h * ar; }
            if !has_explicit_h && h == 0.0 && w > 0.0 { h = w / ar; }
        }
    }
    // Druhe kolo (po ar) - opet max, pak min, pak pad+border floor.
    w = w.min(cw_max);
    h = h.min(ch_max);
    w = w.max(pb_l + pb_r);
    h = h.max(pb_t + pb_b);
    if cw_min > 0.0 { w = w.max(cw_min); }
    if ch_min > 0.0 { h = h.max(ch_min); }
    child.rect.width = w;
    child.rect.height = h;
    let m_l = child.margin_left.unwrap_or(child.margin);
    let m_t = child.margin_top.unwrap_or(child.margin);
    let m_r = child.margin_right.unwrap_or(child.margin);
    let m_b = child.margin_bottom.unwrap_or(child.margin);
    let auto_l = child.margin_left_auto;
    let auto_r = child.margin_right_auto;
    let auto_t = child.margin_top_auto;
    let auto_b = child.margin_bottom_auto;
    // Auto margin pro abs s oboustrannym insetem rozdeli free space (i negativni).
    // CSS spec: pri over-constrained (free<0) v LTR margin-left=0, margin-right absorbs.
    let (extra_l, extra_r) = if let (Some(l), Some(r)) = (child.offset_left, child.offset_right) {
        let free = cb_w - w - l - r - m_l - m_r;
        if auto_l && auto_r {
            if free >= 0.0 { (free / 2.0, free / 2.0) }
            else { (0.0, free) } // LTR: left=0, right absorb
        }
        else if auto_l { (free, 0.0) }
        else if auto_r { (0.0, free) }
        else { (0.0, 0.0) }
    } else { (0.0, 0.0) };
    let (extra_t, extra_b) = if let (Some(t), Some(b)) = (child.offset_top, child.offset_bottom) {
        let free = cb_h - h - t - b - m_t - m_b;
        if auto_t && auto_b {
            if free >= 0.0 { (free / 2.0, free / 2.0) }
            else { (0.0, free) }
        }
        else if auto_t { (free, 0.0) }
        else if auto_b { (0.0, free) }
        else { (0.0, 0.0) }
    } else { (0.0, 0.0) };
    child.rect.x = if let Some(l) = child.offset_left {
        cb_x + l + m_l + extra_l
    } else if let Some(r) = child.offset_right {
        cb_x + cb_w - r - m_r - extra_r - w
    } else {
        cb_x + m_l
    };
    child.rect.y = if let Some(t) = child.offset_top {
        cb_y + t + m_t + extra_t
    } else if let Some(b) = child.offset_bottom {
        cb_y + cb_h - b - m_b - extra_b - h
    } else {
        cb_y + m_t
    };
    match child.display {
        Display::Flex => flex::layout_flex(child),
        Display::Grid => grid::layout_grid(child),
        _ => {}
    }
}

/// Vraci true kdyz position je out-of-flow (abs/fixed).
pub fn is_out_of_flow(bx: &LayoutBox) -> bool {
    matches!(bx.position, Position::Absolute | Position::Fixed)
}

#[cfg(test)]
mod flex_tests;
#[cfg(test)]
mod grid_tests;
#[cfg(test)]
mod flex_spec_tests;
#[cfg(test)]
mod grid_spec_tests;
#[cfg(test)]
mod taffy_compliance;

