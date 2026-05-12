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

use super::super::layout::{LayoutBox, FlexDirection, FlexWrap, JustifyContent, AlignItems};

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
    // Pri taffy_intrinsic_mode + rect.width=0 (pre-pass) pouzij min-width jako floor
    // pro container width - jinak by se items wrapovaly do nul-sirky.
    // ResolveCtx pro self: parent_size 0 (zatim neznamy - tyto values typicky px
    // nebo neset). Pri % by tahle hodnota byla 0 = no min - acceptable degenerate.
    let self_ctx_w = super::super::layout::ResolveCtx { parent_size: 0.0, font_size: bx.font_size, ..Default::default() };
    let bx_min_w = bx.min_width.resolve(&self_ctx_w);
    let bx_min_h = bx.min_height.resolve(&self_ctx_w);
    let effective_w = if bx.rect.width == 0.0 && bx_min_w > 0.0 { bx_min_w } else { bx.rect.width };
    let effective_h = if bx.rect.height == 0.0 && bx_min_h > 0.0 { bx_min_h } else { bx.rect.height };
    // Scrollbar takes space: overflow-y scroll/auto -> right scrollbar reduces inner_w.
    // overflow-x scroll/auto -> bottom scrollbar reduces inner_h.
    let (scrollbar_w, scrollbar_h) = super::scrollbar_takes(bx);
    let inner_w = (effective_w - pad_l - pad_r - 2.0 * bx.margin - scrollbar_w).max(0.0);

    // CSS flex props - uz typed v cascade. Per-frame parse zmizel.
    let direction = bx.flex_direction;
    let wrap = bx.flex_wrap;
    let justify = bx.justify_content;
    let align = bx.align_items;
    // Re-resolve gap pct proti inner_w/inner_h (po vypoctu pad+border).
    let inner_h_for_gap = (bx.rect.height - pad_t - pad_b - 2.0 * bx.margin - scrollbar_h).max(0.0);
    let (row_gap, col_gap) = super::resolve_gaps(bx, inner_w, inner_h_for_gap);

    if bx.children.is_empty() { return; }

    // 0. Collect in-flow indices (abs/fixed jdou mimo flex flow, display:none vyradit zcela)
    let in_flow: Vec<usize> = bx.children.iter().enumerate()
        .filter(|(_, c)| !super::is_out_of_flow(c) && !matches!(c.display, super::super::layout::Display::None))
        .map(|(i, _)| i)
        .collect();
    // display:none -> 0x0 vc. descendants
    fn zero_out(bx: &mut LayoutBox) {
        bx.rect.x = 0.0; bx.rect.y = 0.0; bx.rect.width = 0.0; bx.rect.height = 0.0;
        for c in bx.children.iter_mut() { zero_out(c); }
    }
    for ch in bx.children.iter_mut() {
        if matches!(ch.display, super::super::layout::Display::None) {
            zero_out(ch);
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
        // Pri explicit set rect na explicit hodnotu, jinak 0.
        // Pri width_pct/height_pct: re-resolve proti bx.rect (current container).
        // Pri intrinsic parent + width_pct: 0 (indefinite).
        let parent_intrinsic = bx.taffy_intrinsic_mode;
        ch.rect.width = if let Some(p) = ch.width_pct {
            if parent_intrinsic { 0.0 } else if bx.rect.width > 0.0 {
                // Pri content-box ch + width_pct: explicit_width uz inflated. Pouzij ho.
                if !ch.box_sizing.is_border_box() {
                    ch.explicit_width.unwrap_or_else(|| {
                        let inner_w_pct = (bx.rect.width - pad_l - pad_r - 2.0 * bx.margin).max(0.0);
                        inner_w_pct * p
                    })
                } else {
                    let inner_w_pct = (bx.rect.width - pad_l - pad_r - 2.0 * bx.margin).max(0.0);
                    inner_w_pct * p
                }
            } else { 0.0 }
        } else { ch.explicit_width.unwrap_or(0.0) };
        ch.rect.height = if let Some(p) = ch.height_pct {
            if parent_intrinsic { 0.0 } else if bx.rect.height > 0.0 {
                let inner_h_pct = (bx.rect.height - pad_t - pad_b - 2.0 * bx.margin).max(0.0);
                inner_h_pct * p
            } else { 0.0 }
        } else { ch.explicit_height.unwrap_or(0.0) };
        let saved_intrinsic = std::mem::replace(&mut ch.taffy_intrinsic_mode, true);
        // Pri block s non-default flex-direction nebo justify-content: treat as
        // flex pro pre-pass intrinsic. (Drive empty String detect; ted typed
        // enums - non-default value indicates user-set.)
        let has_flex_dir = !matches!(ch.flex_direction, FlexDirection::Row);
        let has_justify = !matches!(ch.justify_content, JustifyContent::FlexStart);
        let pre_pass_as_flex = matches!(ch.display, super::super::layout::Display::Flex)
            || (matches!(ch.display, super::super::layout::Display::Block) && (has_flex_dir || has_justify));
        // Recursivni layout: nemenime explicit values, jen rect.
        if pre_pass_as_flex {
            super::flex::layout_flex(ch);
        } else { match ch.display {
            super::super::layout::Display::Grid => super::grid::layout_grid(ch),
            _ => {
                // Block-like: aproximace - max child explicit_width, sum explicit_heights.
                // Pri flex/grid grandchild: recursive intrinsic, vezme rect po layoutu.
                let mut max_w = 0.0_f32;
                let mut sum_h = 0.0_f32;
                for gc in ch.children.iter_mut() {
                    if matches!(gc.position, super::super::layout::Position::Absolute | super::super::layout::Position::Fixed) { continue; }
                    if matches!(gc.display, super::super::layout::Display::None) { continue; }
                    // Skip percent-derived widths/heights (nepropagovat jako parent intrinsic).
                    let gc_m_l = gc.margin_left.unwrap_or(gc.margin);
                    let gc_m_r = gc.margin_right.unwrap_or(gc.margin);
                    let gc_m_t = gc.margin_top.unwrap_or(gc.margin);
                    let gc_m_b = gc.margin_bottom.unwrap_or(gc.margin);
                    let mut gc_w = if gc.width_pct.is_some() { 0.0 }
                                   else { gc.explicit_width.unwrap_or(0.0) };
                    let mut gc_h = if gc.height_pct.is_some() { 0.0 }
                                   else { gc.explicit_height.unwrap_or(0.0) };
                    // Text intrinsic v taffy_mode: 10/char.
                    if gc.taffy_mode {
                        if let Some(t) = &gc.text {
                            let tw = t.chars().filter(|c| !matches!(*c, '\u{200B}' | ' ' | '\n' | '\t')).count() as f32 * 10.0;
                            if gc_w == 0.0 { gc_w = tw; }
                            if gc_h == 0.0 { gc_h = 10.0; }
                        }
                    }
                    // Leaf gc: include own padding+border do intrinsic.
                    if gc.children.is_empty() {
                        let gc_pl = gc.padding_left.unwrap_or(gc.padding) + gc.border_left_width.unwrap_or(gc.border_width);
                        let gc_pr = gc.padding_right.unwrap_or(gc.padding) + gc.border_right_width.unwrap_or(gc.border_width);
                        let gc_pt = gc.padding_top.unwrap_or(gc.padding) + gc.border_top_width.unwrap_or(gc.border_width);
                        let gc_pb = gc.padding_bottom.unwrap_or(gc.padding) + gc.border_bottom_width.unwrap_or(gc.border_width);
                        if gc_w < gc_pl + gc_pr { gc_w = gc_pl + gc_pr; }
                        if gc_h < gc_pt + gc_pb { gc_h = gc_pt + gc_pb; }
                    }
                    // Pri grandchild bez explicit, recursive intrinsic (flex/grid recursive layout,
                    // block sum z ggchild).
                    if (gc_w == 0.0 || gc_h == 0.0) && !gc.children.is_empty() {
                        match gc.display {
                            super::super::layout::Display::Flex | super::super::layout::Display::Grid => {
                                let saved_gc_rect = gc.rect.clone();
                                gc.rect.x = 0.0; gc.rect.y = 0.0;
                                if gc.explicit_width.is_none() { gc.rect.width = 0.0; }
                                if gc.explicit_height.is_none() { gc.rect.height = 0.0; }
                                match gc.display {
                                    super::super::layout::Display::Flex => super::flex::layout_flex(gc),
                                    super::super::layout::Display::Grid => super::grid::layout_grid(gc),
                                    _ => {}
                                }
                                if gc.explicit_width.is_none() { gc_w = gc.rect.width; }
                                if gc.explicit_height.is_none() { gc_h = gc.rect.height; }
                                // Po pre-pass restoraci NE jen gc.rect, ale i deti -
                                // pre-pass zapsala child rect.height z run s gc.rect.width=0
                                // (1 col, N rows). Bez deep reset by transform-grid
                                // tf-box rect.h zustaly stale a real-pass je nemusi
                                // overridovat (block layout grow-only).
                                gc.rect = saved_gc_rect;
                                fn deep_reset_rect(b: &mut super::super::layout::LayoutBox) {
                                    if b.explicit_width.is_none() { b.rect.width = 0.0; }
                                    if b.explicit_height.is_none() { b.rect.height = 0.0; }
                                    b.rect.x = 0.0;
                                    b.rect.y = 0.0;
                                    for c in b.children.iter_mut() { deep_reset_rect(c); }
                                }
                                for c in gc.children.iter_mut() { deep_reset_rect(c); }
                            }
                            _ => {
                                // Block grandchild: sum ggchild explicit heights, max ggchild widths.
                                let mut ggw = 0.0_f32;
                                let mut ggh = 0.0_f32;
                                for ggc in &gc.children {
                                    if matches!(ggc.position, super::super::layout::Position::Absolute | super::super::layout::Position::Fixed) { continue; }
                                    if matches!(ggc.display, super::super::layout::Display::None) { continue; }
                                    if let Some(w) = ggc.explicit_width { if w > ggw { ggw = w; } }
                                    if let Some(h) = ggc.explicit_height { ggh += h; }
                                }
                                if gc_w == 0.0 { gc_w = ggw; }
                                if gc_h == 0.0 { gc_h = ggh; }
                            }
                        }
                    }
                    // Include margins do sum/max kdyz gc neni nulove.
                    if gc_w > 0.0 { gc_w += gc_m_l + gc_m_r; }
                    if gc_h > 0.0 { gc_h += gc_m_t + gc_m_b; }
                    max_w = max_w.max(gc_w);
                    sum_h += gc_h;
                }
                // Vlastni padding+border pripocist do intrinsic rect (content+pad = total).
                let own_pl = ch.padding_left.unwrap_or(ch.padding) + ch.border_left_width.unwrap_or(ch.border_width);
                let own_pr = ch.padding_right.unwrap_or(ch.padding) + ch.border_right_width.unwrap_or(ch.border_width);
                let own_pt = ch.padding_top.unwrap_or(ch.padding) + ch.border_top_width.unwrap_or(ch.border_width);
                let own_pb = ch.padding_bottom.unwrap_or(ch.padding) + ch.border_bottom_width.unwrap_or(ch.border_width);
                if ch.explicit_width.is_none() && max_w > 0.0 { ch.rect.width = max_w + own_pl + own_pr; }
                if ch.explicit_height.is_none() && sum_h > 0.0 { ch.rect.height = sum_h + own_pt + own_pb; }
            }
        }}
        ch.rect.x = saved_rect.x;
        ch.rect.y = saved_rect.y;
        ch.taffy_intrinsic_mode = saved_intrinsic;
    }

    let mut items: Vec<FlexItem> = Vec::with_capacity(in_flow.len());
    for &i in &in_flow {
        let ch = &bx.children[i];
        // Pri intrinsic mode parenta (taffy_intrinsic_mode): percent-derived size = 0,
        // child shrink-to-content. (CSS: percent na auto-size parent = indefinite).
        let intrinsic_parent = bx.taffy_intrinsic_mode;
        let pct_w_skip = intrinsic_parent && ch.width_pct.is_some();
        let pct_h_skip = intrinsic_parent && ch.height_pct.is_some();
        let pct_w_indefinite = !intrinsic_parent && ch.width_pct.is_some() && bx.rect.width == 0.0;
        let pct_h_indefinite = !intrinsic_parent && ch.height_pct.is_some() && bx.rect.height == 0.0;
        let mut est_w = if pct_w_skip || pct_w_indefinite {
            // Pouzij intrinsic z rect.width nebo content
            if let Some(t) = &ch.text {
                if ch.taffy_mode {
                    t.chars().filter(|c| !matches!(*c, '\u{200B}' | ' ' | '\n' | '\t')).count() as f32 * 10.0
                } else {
                    super::super::layout::measure_text_width_full(t, ch.font_size, ch.bold, ch.italic, &ch.font_family)
                }
            } else if ch.rect.width > 0.0 { ch.rect.width } else {
                // Recursive content width pres descendants (text nodes uvnitr).
                // Pri flex item bez explicit width + bez vlastniho textu, ale
                // s child text (napr. <button>Primary</button>) potrebujeme
                // sirku obsahu + own padding.
                intrinsic_content_width(ch)
            }
        } else { ch.explicit_width.unwrap_or_else(|| {
            if let Some(t) = &ch.text {
                if ch.taffy_mode {
                    // Taffy fixtures: 10px per visible char (excl. ZWS).
                    t.chars().filter(|c| !matches!(*c, '\u{200B}' | ' ' | '\n' | '\t')).count() as f32 * 10.0
                } else {
                    super::super::layout::measure_text_width_full(t, ch.font_size, ch.bold, ch.italic, &ch.font_family)
                }
            } else if ch.rect.width > 0.0 { ch.rect.width } else {
                intrinsic_content_width(ch)
            }
        }) };
        // Pri inline elementu s text-only children (span s textem) - intrinsic
        // h = font_size * line_height. Drive: span.text=None, rect.h=0, est_h=0.
        // Pak flex item h se clamp na pb_h (= padding+border) -> 27 px misto 14.
        fn intrinsic_text_h(ch: &super::super::layout::LayoutBox) -> f32 {
            if ch.text.is_some() {
                return if ch.taffy_mode { 10.0 } else { ch.font_size * 1.4 };
            }
            if !ch.children.is_empty() && ch.children.iter().all(|c| c.text.is_some() || (c.tag.is_none() && c.children.is_empty())) {
                let lh = if ch.line_height > 0.0 { ch.line_height } else { 1.2 };
                return ch.font_size * lh;
            }
            0.0
        }
        let mut est_h = if pct_h_skip || pct_h_indefinite {
            if ch.text.is_some() {
                if ch.taffy_mode { 10.0 } else { ch.font_size * 1.4 }
            } else if ch.rect.height > 0.0 { ch.rect.height }
            else { intrinsic_text_h(ch) }
        } else { ch.explicit_height.unwrap_or_else(|| {
            if ch.text.is_some() {
                if ch.taffy_mode { 10.0 } else { ch.font_size * 1.4 }
            } else if ch.rect.height > 0.0 { ch.rect.height }
            else { intrinsic_text_h(ch) }
        }) };
        // writing-mode: vertical-lr/rl - osy textu se prohodi.
        // Inline axis (delka textu) je vertikalni, block axis je horizontalni.
        // Pro intrinsic sizing text bloku to znamena swap est_w <-> est_h.
        let ch_vertical_text = ch.taffy_mode && ch.text.is_some()
            && matches!(ch.writing_mode.as_str(), "vertical-lr" | "vertical-rl");
        if ch_vertical_text && ch.explicit_width.is_none() && ch.explicit_height.is_none() {
            std::mem::swap(&mut est_w, &mut est_h);
        }
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
                // Content-box: basis je content size, pricti padding+border do main basis.
                let pb_main_for_basis = if direction.is_row() {
                    ch.padding_left.unwrap_or(ch.padding) + ch.padding_right.unwrap_or(ch.padding)
                        + ch.border_left_width.unwrap_or(ch.border_width) + ch.border_right_width.unwrap_or(ch.border_width)
                } else {
                    ch.padding_top.unwrap_or(ch.padding) + ch.padding_bottom.unwrap_or(ch.padding)
                        + ch.border_top_width.unwrap_or(ch.border_width) + ch.border_bottom_width.unwrap_or(ch.border_width)
                };
                let basis_final = if !ch.box_sizing.is_border_box() {
                    b + pb_main_for_basis
                } else { b };
                if direction.is_row() { est_w = basis_final; } else { est_h = basis_final; }
            }
        }
        // Apply min-w/h pred aspect ratio dopoctem
        let ctx_w = super::super::layout::ResolveCtx { parent_size: inner_w, font_size: ch.font_size, ..Default::default() };
        let ctx_h = super::super::layout::ResolveCtx { parent_size: inner_h_for_gap, font_size: ch.font_size, ..Default::default() };
        let min_w_pre = ch.min_width.resolve(&ctx_w);
        let min_h_pre = ch.min_height.resolve(&ctx_h);
        let max_w_pre = ch.max_width.resolve_max(&ctx_w);
        let max_h_pre = ch.max_height.resolve_max(&ctx_h);
        // Min PRED aspect dopoctem - jen pro aspect-ratio kontext, NE pro est_w/est_h
        // (base size pro flex algo). Min se aplikuje az v resolve step.
        let _ = (min_w_pre, min_h_pre); // suppress warning
        // Pri zadnem est_w/h + max/min finite, preferuj jako velikost pro aspect dopocet.
        if est_w == 0.0 && ch.aspect_ratio.is_some() {
            if min_w_pre > 0.0 { est_w = min_w_pre; }
            else if max_w_pre.is_finite() { est_w = max_w_pre; }
        }
        if est_h == 0.0 && ch.aspect_ratio.is_some() {
            if min_h_pre > 0.0 { est_h = min_h_pre; }
            else if max_h_pre.is_finite() { est_h = max_h_pre; }
        }
        // Pri aspect-ratio + text item: max-h/w wins over text intrinsic.
        if ch.aspect_ratio.is_some() && ch.text.is_some() {
            if max_w_pre.is_finite() && max_w_pre > 0.0 { est_w = max_w_pre; }
            if max_h_pre.is_finite() && max_h_pre > 0.0 { est_h = max_h_pre; }
        }
        // Pri aspect-ratio: clamp est_w/h pred aspect dopoctem (max wins).
        if ch.aspect_ratio.is_some() {
            est_w = est_w.min(max_w_pre);
            est_h = est_h.min(max_h_pre);
        }
        // Padding+border floor
        let ch_pb_l = ch.padding_left.unwrap_or(ch.padding) + ch.border_left_width.unwrap_or(ch.border_width);
        let ch_pb_r = ch.padding_right.unwrap_or(ch.padding) + ch.border_right_width.unwrap_or(ch.border_width);
        let ch_pb_t = ch.padding_top.unwrap_or(ch.padding) + ch.border_top_width.unwrap_or(ch.border_width);
        let ch_pb_b = ch.padding_bottom.unwrap_or(ch.padding) + ch.border_bottom_width.unwrap_or(ch.border_width);
        est_w = est_w.max(ch_pb_l + ch_pb_r);
        est_h = est_h.max(ch_pb_t + ch_pb_b);
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
        // Re-resolve percent margins proti inner_w (CSS spec: pct margin v flex resolvuje
        // proti inline-size CB = flex container width).
        let ch_mut = &mut bx.children[i];
        if let Some(p) = ch_mut.margin_left_pct { ch_mut.margin_left = Some(inner_w * p); }
        if let Some(p) = ch_mut.margin_right_pct { ch_mut.margin_right = Some(inner_w * p); }
        if let Some(p) = ch_mut.margin_top_pct { ch_mut.margin_top = Some(inner_w * p); }
        if let Some(p) = ch_mut.margin_bottom_pct { ch_mut.margin_bottom = Some(inner_w * p); }
        let ch = &bx.children[i];
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
    let inner_h = (effective_h - pad_t - pad_b - 2.0 * bx.margin - scrollbar_h).max(0.0);
    let container_main = if direction.is_row() { inner_w } else { inner_h };

    // Apply min/max width/height na items - ulozit pro resolve_flexible_lengths.
    for (i, &real_idx) in in_flow.iter().enumerate() {
        let ch = &bx.children[real_idx];
        // Percent values resolvujem proti flex container inner dimensions.
        let cw_ctx = super::super::layout::ResolveCtx { parent_size: inner_w, font_size: ch.font_size, ..Default::default() };
        let ch_ctx = super::super::layout::ResolveCtx { parent_size: inner_h_for_gap, font_size: ch.font_size, ..Default::default() };
        let cw_min = ch.min_width.resolve(&cw_ctx);
        let cw_max = ch.max_width.resolve_max(&cw_ctx);
        let ch_min = ch.min_height.resolve(&ch_ctx);
        let ch_max = ch.max_height.resolve_max(&ch_ctx);
        let (min_m, max_m, min_c, max_c) = if direction.is_row() {
            (cw_min, cw_max, ch_min, ch_max)
        } else {
            (ch_min, ch_max, cw_min, cw_max)
        };
        // Min-main floor: max(specified min, intrinsic content z pre-pass jen kdyz no explicit, pad+border).
        // Pri text item v taffy_mode: min-content text intrinsic = nejdelsi nelomitelny
        // fragment * 10 (mezi ZWS / mezerou / newline jsou break opportunities).
        let text_min_content = if ch.taffy_mode {
            if let Some(t) = &ch.text {
                if direction.is_row() {
                    let mut max_segment = 0usize;
                    let mut cur = 0usize;
                    for c in t.chars() {
                        if matches!(c, '\u{200B}' | ' ' | '\n' | '\t') {
                            if cur > max_segment { max_segment = cur; }
                            cur = 0;
                        } else {
                            cur += 1;
                        }
                    }
                    if cur > max_segment { max_segment = cur; }
                    max_segment as f32 * 10.0
                } else { 10.0 }
            } else { 0.0 }
        } else { 0.0 };
        // Descendant max-width prispiva do min_main pri row (CSS auto-min-content):
        // Pouze pri overflow scenariich (parent width < child width). CSS spec: definite
        // size = min 0, ale taffy ma special case "shrink-to-content" pri padding/baseline.
        // Heuristika: jen kdyz total items > container (overflow), apply descendant_min.
        let descendant_min_main = if direction.is_row() && !ch.children.is_empty()
            && ch.flex_grow == 0.0 {
            let mut max_dc_w = 0.0_f32;
            let parent_w = ch.explicit_width.unwrap_or(f32::INFINITY);
            for dc in &ch.children {
                if matches!(dc.position, super::super::layout::Position::Absolute | super::super::layout::Position::Fixed) { continue; }
                if matches!(dc.display, super::super::layout::Display::None) { continue; }
                if dc.width_pct.is_some() { continue; }
                if let Some(w) = dc.explicit_width {
                    // Pri child width > parent explicit, NEPROPAGOVAT (overflow OK).
                    if w > parent_w { continue; }
                    if w > max_dc_w { max_dc_w = w; }
                }
            }
            max_dc_w
        } else { 0.0 };
        // CSS Flex L1 §4.5: pri overflow != visible v main axis je auto-min-content = 0.
        // Pri flex-basis = 0 (definite) + min-height set: auto-min jen padding+min-h, ne content.
        let main_overflow = if direction.is_row() { ch.overflow_x } else { ch.overflow_y };
        let main_overflow_blocks = main_overflow.clips();
        let basis_v_check = ch.flex_basis.trim();
        let basis_zero = basis_v_check == "0" || basis_v_check == "0px";
        let main_min_cl = if direction.is_row() { &ch.min_width } else { &ch.min_height };
        let has_main_min = main_min_cl.is_specified();
        let intrinsic_main = if main_overflow_blocks { 0.0 }
                            else if basis_zero && has_main_min { 0.0 }
                            else if ch.explicit_width.is_some() && direction.is_row() { descendant_min_main }
                            else if ch.explicit_height.is_some() && !direction.is_row() { 0.0 }
                            else if direction.is_row() { ch.rect.width.max(text_min_content).max(descendant_min_main) } else { ch.rect.height.max(text_min_content) };
        let pb_main = if direction.is_row() {
            ch.padding_left.unwrap_or(ch.padding) + ch.padding_right.unwrap_or(ch.padding)
                + ch.border_left_width.unwrap_or(ch.border_width) + ch.border_right_width.unwrap_or(ch.border_width)
        } else {
            ch.padding_top.unwrap_or(ch.padding) + ch.padding_bottom.unwrap_or(ch.padding)
                + ch.border_top_width.unwrap_or(ch.border_width) + ch.border_bottom_width.unwrap_or(ch.border_width)
        };
        // Pri box-sizing=content-box: descendant_min + pb_main (auto-min vc. padding).
        // Pri border-box: descendant_min (padding uz v explicit_width).
        let min_m_with_intrinsic = if !ch.box_sizing.is_border_box() && descendant_min_main > 0.0 && pb_main > 0.0 && ch.explicit_width.is_some() && direction.is_row() {
            min_m.max(intrinsic_main + pb_main)
        } else {
            min_m.max(intrinsic_main).max(pb_main)
        };
        items[i].min_main = min_m_with_intrinsic;
        items[i].max_main = max_m;
        // Cross floor: pad+border + intrinsic.
        // Intrinsic se musi clamp max_c - element s explicit > max nesmi propagovat
        // explicit jako "natural", musi jen max.
        let raw_intrinsic_cross = if direction.is_row() { ch.rect.height } else { ch.rect.width };
        let intrinsic_cross = raw_intrinsic_cross.min(max_c);
        let pb_cross = if direction.is_row() {
            ch.padding_top.unwrap_or(ch.padding) + ch.padding_bottom.unwrap_or(ch.padding)
                + ch.border_top_width.unwrap_or(ch.border_width) + ch.border_bottom_width.unwrap_or(ch.border_width)
        } else {
            ch.padding_left.unwrap_or(ch.padding) + ch.padding_right.unwrap_or(ch.padding)
                + ch.border_left_width.unwrap_or(ch.border_width) + ch.border_right_width.unwrap_or(ch.border_width)
        };
        let min_c_total = min_c.max(intrinsic_cross).max(pb_cross);
        if min_c_total > 0.0 { items[i].cross_size = items[i].cross_size.max(min_c_total); }
        items[i].cross_size = items[i].cross_size.min(max_c);
        // Re-apply min po max - min wins.
        if min_c_total > 0.0 { items[i].cross_size = items[i].cross_size.max(min_c_total); }
        items[i].main_size = items[i].main_size.min(max_m);
        // Pri specified min > basis a wrap container: hypothetical = min (CSS Flex L1 §9.3.4).
        // V wrap mode min wins (forces wrap), v nowrap zachovat shrink kompatibilitu.
        let specified_min = if direction.is_row() {
            ch.min_width.resolve(&cw_ctx)
        } else {
            ch.min_height.resolve(&ch_ctx)
        };
        if !matches!(wrap, FlexWrap::NoWrap) && specified_min > items[i].main_size {
            items[i].main_size = specified_min;
        }
    }

    // 3. Collect lines (wrap)
    let lines = collect_lines(&items, container_main, wrap, if direction.is_row() { col_gap } else { row_gap });

    // 4. Resolve flexible lengths per line. V intrinsic_mode pouzij max-content (no shrink).
    // PERF: pre-alloc capacity (typicky 1 line nowrap, vetsinou < 10 pri wrap).
    let mut resolved_lines: Vec<ResolvedLine> = Vec::with_capacity(lines.len());
    for line_indices in &lines {
        let effective_container_main = if bx.taffy_intrinsic_mode {
            let total: f32 = line_indices.iter().map(|&i| items[i].main_size).sum();
            let gaps = (line_indices.len().saturating_sub(1) as f32) * if direction.is_row() { col_gap } else { row_gap };
            total + gaps
        } else { container_main };
        let resolved = resolve_flexible_lengths(&items, line_indices, effective_container_main,
            if direction.is_row() { col_gap } else { row_gap });
        resolved_lines.push(resolved);
    }

    // 5. Compute total cross size
    let line_gap = if direction.is_row() { row_gap } else { col_gap };
    let container_cross = if direction.is_row() { (bx.rect.height - pad_t - pad_b - 2.0 * bx.margin - scrollbar_h).max(0.0) } else { inner_w };
    let nline = resolved_lines.len();
    let total_gap_cross = line_gap * nline.saturating_sub(1) as f32;
    let lines_natural_total: f32 = resolved_lines.iter().map(|l| l.cross_size).sum::<f32>() + total_gap_cross;
    // align-content: kdyz neset (Normal) nebo Stretch -> rozdel container cross rovnomerne mezi lines.
    let stretch_lines = bx.align_content.is_normal_or_stretch();
    if container_cross > 0.0 && stretch_lines && lines_natural_total < container_cross && nline > 0 {
        let extra_per_line = (container_cross - lines_natural_total) / nline as f32;
        for line in &mut resolved_lines {
            line.cross_size += extra_per_line;
        }
    } else if matches!(wrap, FlexWrap::NoWrap) && nline == 1 && container_cross > 0.0 {
        // Single line nowrap: line zabira container cross.
        // Pri explicit/max-bound cross je line PRESNE container (items overflow).
        let has_bound_cross = if direction.is_row() {
            bx.explicit_height.is_some() || bx.max_height.is_specified()
        } else {
            bx.explicit_width.is_some() || bx.max_width.is_specified()
        };
        if has_bound_cross {
            resolved_lines[0].cross_size = container_cross;
        } else {
            resolved_lines[0].cross_size = resolved_lines[0].cross_size.max(container_cross);
        }
    }
    // Pre-spocti per-line baseline (max_above + max_below) - rozsirit cross_size pri
    // baseline-aligned items v row direction.
    if direction.is_row() {
        for (line_idx, line_indices) in lines.iter().enumerate() {
            let mut max_above: f32 = 0.0;
            let mut max_below: f32 = 0.0;
            let mut has_baseline = false;
            // Spocti baseline kazdeho item (synth nebo first-child).
            for &item_idx in line_indices.iter() {
                let real_idx_b = in_flow[item_idx];
                let item_align_b = bx.children[real_idx_b].align_self.resolve(align);
                if !matches!(item_align_b, AlignItems::Baseline) { continue; }
                has_baseline = true;
                let it_b = items[item_idx];
                let item_box = &bx.children[real_idx_b];
                let synth = it_b.cross_size + it_b.margin_cross_start;
                let is_flex_or_grid = matches!(item_box.display,
                    super::super::layout::Display::Flex | super::super::layout::Display::Grid);
                let has_flex_attr = matches!(item_box.display, super::super::layout::Display::Flex);
                let item_has_children = item_box.children.iter().any(|c|
                    !matches!(c.position, super::super::layout::Position::Absolute | super::super::layout::Position::Fixed)
                    && !matches!(c.display, super::super::layout::Display::None));
                let parent_is_real_flex = !bx.pseudo_flex;
                let use_first_child = is_flex_or_grid || has_flex_attr
                    || (item_has_children && parent_is_real_flex);
                let own_baseline = if !use_first_child {
                    synth
                } else {
                    fn child_baseline(c: &super::super::layout::LayoutBox) -> f32 {
                        let c_h = c.explicit_height.unwrap_or(c.rect.height);
                        let is_flex_or_grid = matches!(c.display,
                            super::super::layout::Display::Flex | super::super::layout::Display::Grid);
                        let has_flex_attr = matches!(c.display, super::super::layout::Display::Flex);
                        if is_flex_or_grid || has_flex_attr {
                            let baseline_first = c.children.iter().find(|x|
                                !matches!(x.position, super::super::layout::Position::Absolute | super::super::layout::Position::Fixed)
                                && !matches!(x.display, super::super::layout::Display::None)
                                && matches!(x.align_self, super::super::layout::AlignSelf::Baseline));
                            let gc_opt = baseline_first.or_else(|| c.children.iter().find(|x|
                                !matches!(x.position, super::super::layout::Position::Absolute | super::super::layout::Position::Fixed)
                                && !matches!(x.display, super::super::layout::Display::None)));
                            if let Some(gc) = gc_opt {
                                let gc_m_t = gc.margin_top.unwrap_or(gc.margin);
                                let gc_pad_t = c.padding_top.unwrap_or(c.padding) + c.border_top_width.unwrap_or(c.border_width);
                                return gc_pad_t + gc_m_t + child_baseline(gc);
                            }
                        }
                        c_h
                    }
                    let first_child_baseline = item_box.children.iter().find(|c|
                        !matches!(c.position, super::super::layout::Position::Absolute | super::super::layout::Position::Fixed)
                        && !matches!(c.display, super::super::layout::Display::None)
                        && matches!(c.align_self, super::super::layout::AlignSelf::Baseline));
                    let first_child = first_child_baseline.or_else(|| item_box.children.iter().find(|c|
                        !matches!(c.position, super::super::layout::Position::Absolute | super::super::layout::Position::Fixed)
                        && !matches!(c.display, super::super::layout::Display::None)));
                    match first_child {
                        Some(c) => {
                            let c_m_t = c.margin_top.unwrap_or(c.margin);
                            let pad_t = item_box.padding_top.unwrap_or(item_box.padding) + item_box.border_top_width.unwrap_or(item_box.border_width);
                            pad_t + c_m_t + child_baseline(c) + it_b.margin_cross_start
                        }
                        None => synth,
                    }
                };
                let item_full_cross = it_b.cross_size + it_b.margin_cross_start + it_b.margin_cross_end;
                let above = own_baseline;
                let below = (item_full_cross - own_baseline).max(0.0);
                if above > max_above { max_above = above; }
                if below > max_below { max_below = below; }
            }
            if has_baseline {
                let baseline_cross = max_above + max_below;
                if baseline_cross > resolved_lines[line_idx].cross_size {
                    resolved_lines[line_idx].cross_size = baseline_cross;
                    resolved_lines[line_idx].natural_cross =
                        resolved_lines[line_idx].natural_cross.max(baseline_cross);
                }
            }
        }
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
        use super::super::layout::AlignContent as AC;
        match bx.align_content {
            AC::FlexEnd | AC::End => (free, 0.0),
            AC::Center => (free / 2.0, 0.0),
            AC::SpaceBetween => {
                if nline <= 1 { (0.0, 0.0) }
                else { (0.0, free / (nline - 1) as f32) }
            }
            AC::SpaceAround => {
                let g = free / nline.max(1) as f32;
                (g / 2.0, g)
            }
            AC::SpaceEvenly => {
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
        let free_main = container_main - used_main; // muze byt negativni (overflow)
        // Spocti auto margin slots v main axis - kazdy dostane equal share free.
        let auto_main_count: usize = line_indices.iter()
            .map(|&i| (items[i].auto_main_start as usize) + (items[i].auto_main_end as usize))
            .sum();
        let auto_main_share = if auto_main_count > 0 { free_main / auto_main_count as f32 } else { 0.0 };
        let effective_free = if auto_main_count > 0 { 0.0 } else { free_main };
        let (mut start_main, between_main) = compute_justify_offsets(justify, effective_free, resolved.main_sizes.len(), main_gap);

        let main_iter: Box<dyn Iterator<Item = (usize, &usize)>> = if direction.is_reverse() {
            Box::new(line_indices.iter().enumerate().rev())
        } else {
            Box::new(line_indices.iter().enumerate())
        };
        // Pri reverse direction, FlexStart/FlexEnd flipnou polohu (main-axis reverse).
        // Start/End jsou writing-mode aware (NEZmeni se v reverse).
        if direction.is_reverse() {
            match justify {
                JustifyContent::FlexStart => { start_main = container_main - used_main; }
                JustifyContent::FlexEnd => { start_main = 0.0; }
                JustifyContent::Start => { start_main = 0.0; }
                JustifyContent::End => { start_main = container_main - used_main; }
                _ => { /* center/space-* zustanou */ }
            }
        }

        // Baseline policy: pokud VSECHNY items s align=baseline maji children, pouzijeme
        // first-child baseline (CSS spec). Jinak synth = item bottom margin edge.
        // Pri flex/grid display nebo flex-direction: vzdy first-child.
        let baseline_items_idx: Vec<usize> = line_indices.iter().enumerate().filter(|&(_, &item_idx)| {
            let real_idx = in_flow[item_idx];
            let item_align = bx.children[real_idx].align_self.resolve(align);
            matches!(item_align, AlignItems::Baseline)
        }).map(|(k, _)| k).collect();
        let _all_have_children = !baseline_items_idx.is_empty() && baseline_items_idx.iter().all(|&k| {
            let real_idx = in_flow[line_indices[k]];
            bx.children[real_idx].children.iter().any(|c|
                !matches!(c.position, super::super::layout::Position::Absolute | super::super::layout::Position::Fixed)
                && !matches!(c.display, super::super::layout::Display::None))
        });
        // Detekce pseudo-flex (block s align-items=baseline heuristika): vsechny items jsou
        // plain block bez flex-direction. Pri tom synth baseline pro vsechny.
        let _any_real_flex_item = baseline_items_idx.iter().any(|&k| {
            let real_idx = in_flow[line_indices[k]];
            let item = &bx.children[real_idx];
            matches!(item.display, super::super::layout::Display::Flex | super::super::layout::Display::Grid)
                || matches!(item.display, super::super::layout::Display::Flex)
        });
        let item_baselines: Vec<f32> = line_indices.iter().map(|&item_idx| {
            let it_b = items[item_idx];
            let real_idx_b = in_flow[item_idx];
            let item_box = &bx.children[real_idx_b];
            let synth = it_b.cross_size + it_b.margin_cross_start;
            let is_flex_or_grid = matches!(item_box.display,
                super::super::layout::Display::Flex | super::super::layout::Display::Grid);
            let has_flex_attr = matches!(item_box.display, super::super::layout::Display::Flex);
            let item_has_children = item_box.children.iter().any(|c|
                !matches!(c.position, super::super::layout::Position::Absolute | super::super::layout::Position::Fixed)
                && !matches!(c.display, super::super::layout::Display::None));
            // First-child baseline pri: flex/grid item, flex-direction set, NEBO
            // (parent je real flex (NE pseudo) AND item ma children).
            let parent_is_real_flex = !bx.pseudo_flex;
            let use_first_child = is_flex_or_grid || has_flex_attr
                || (item_has_children && parent_is_real_flex);
            if !use_first_child {
                return synth;
            }
            // Recursivne walk first-child chain pri flex/grid/flex-direction items.
            // CSS: first-baseline = top + walk first-child baseline. Pri multi-line
            // flex container: prefer first child with align-self=baseline; jinak first.
            fn child_baseline(c: &super::super::layout::LayoutBox) -> f32 {
                let c_h = c.explicit_height.unwrap_or(c.rect.height);
                let is_flex_or_grid = matches!(c.display,
                    super::super::layout::Display::Flex | super::super::layout::Display::Grid);
                let has_flex_attr = matches!(c.display, super::super::layout::Display::Flex);
                if is_flex_or_grid || has_flex_attr {
                    // Najdi first in-flow child WITH align-self=baseline; fallback first child.
                    let baseline_first = c.children.iter().find(|x|
                        !matches!(x.position, super::super::layout::Position::Absolute | super::super::layout::Position::Fixed)
                        && !matches!(x.display, super::super::layout::Display::None)
                        && matches!(x.align_self, super::super::layout::AlignSelf::Baseline));
                    let gc_opt = baseline_first.or_else(|| c.children.iter().find(|x|
                        !matches!(x.position, super::super::layout::Position::Absolute | super::super::layout::Position::Fixed)
                        && !matches!(x.display, super::super::layout::Display::None)));
                    if let Some(gc) = gc_opt {
                        let gc_m_t = gc.margin_top.unwrap_or(gc.margin);
                        let gc_pad_t = c.padding_top.unwrap_or(c.padding) + c.border_top_width.unwrap_or(c.border_width);
                        return gc_pad_t + gc_m_t + child_baseline(gc);
                    }
                }
                c_h
            }
            // Prefer first child with align-self=baseline V LINE 1 (CSS spec: container
            // baseline = first item participating in baseline alignment v first line).
            // Greedy line 1 detection: items pridavame dokud sum_main_size <= container_main.
            let item_box_inner_main = if matches!(item_box.display, super::super::layout::Display::Flex) && !item_box.flex_direction.is_row() {
                // Column - main = height. Drive ne aplikujeme line detection.
                f32::INFINITY
            } else {
                let pad_l_b = item_box.padding_left.unwrap_or(item_box.padding) + item_box.border_left_width.unwrap_or(item_box.border_width);
                let pad_r_b = item_box.padding_right.unwrap_or(item_box.padding) + item_box.border_right_width.unwrap_or(item_box.border_width);
                let item_w = item_box.explicit_width.unwrap_or(it_b.cross_size);
                (item_w - pad_l_b - pad_r_b).max(0.0)
            };
            let item_has_wrap = !matches!(item_box.flex_wrap, FlexWrap::NoWrap);
            let mut line1_indices: Vec<usize> = Vec::new();
            let mut used = 0.0_f32;
            for (gi, gc) in item_box.children.iter().enumerate() {
                if matches!(gc.position, super::super::layout::Position::Absolute | super::super::layout::Position::Fixed) { continue; }
                if matches!(gc.display, super::super::layout::Display::None) { continue; }
                let gc_m_l = gc.margin_left.unwrap_or(gc.margin);
                let gc_m_r = gc.margin_right.unwrap_or(gc.margin);
                let gc_w = gc.explicit_width.unwrap_or(0.0) + gc_m_l + gc_m_r;
                if item_has_wrap && !line1_indices.is_empty() && used + gc_w > item_box_inner_main + 0.01 {
                    break;
                }
                line1_indices.push(gi);
                used += gc_w;
            }
            let first_baseline_child = line1_indices.iter().find_map(|&gi| {
                let c = &item_box.children[gi];
                if matches!(c.align_self, super::super::layout::AlignSelf::Baseline) { Some(c) } else { None }
            });
            let first_child = first_baseline_child.or_else(|| item_box.children.iter().find(|c|
                !matches!(c.position, super::super::layout::Position::Absolute | super::super::layout::Position::Fixed)
                && !matches!(c.display, super::super::layout::Display::None)));
            match first_child {
                Some(c) => {
                    let c_m_t = c.margin_top.unwrap_or(c.margin);
                    let pad_t = item_box.padding_top.unwrap_or(item_box.padding) + item_box.border_top_width.unwrap_or(item_box.border_width);
                    pad_t + c_m_t + child_baseline(c) + it_b.margin_cross_start
                }
                None => synth,
            }
        }).collect();
        // Pre-spocti max baseline napric items na line aligned by baseline.
        let mut line_max_baseline: f32 = 0.0;
        for (k, &item_idx) in line_indices.iter().enumerate() {
            let real_idx_b = in_flow[item_idx];
            let item_align_b = bx.children[real_idx_b].align_self.resolve(align);
            if matches!(item_align_b, AlignItems::Baseline) {
                if item_baselines[k] > line_max_baseline { line_max_baseline = item_baselines[k]; }
            }
        }
        // Pri row direction + baseline aligned items: line cross_size se musi rozsirit
        // o (max_baseline - own_baseline) extension above baseline + extent below.
        // Linka cross = max(own_baseline_above) + max(item_size - own_baseline_below).
        if direction.is_row() {
            let mut max_above: f32 = 0.0;
            let mut max_below: f32 = 0.0;
            let mut has_baseline = false;
            for (k, &item_idx) in line_indices.iter().enumerate() {
                let real_idx_b = in_flow[item_idx];
                let item_align_b = bx.children[real_idx_b].align_self.resolve(align);
                if matches!(item_align_b, AlignItems::Baseline) {
                    has_baseline = true;
                    let it_full = items[item_idx];
                    let item_full_cross = it_full.cross_size + it_full.margin_cross_start + it_full.margin_cross_end;
                    let above = item_baselines[k];
                    let below = (item_full_cross - above).max(0.0);
                    if above > max_above { max_above = above; }
                    if below > max_below { max_below = below; }
                } else {
                    let it_full = items[item_idx];
                    let item_full_cross = it_full.cross_size + it_full.margin_cross_start + it_full.margin_cross_end;
                    if item_full_cross > max_below + max_above {
                        // Non-baseline item dictate via item full size
                    }
                }
            }
            let _ = (has_baseline, max_above, max_below); // Pre-pass nepouziva
        }

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
            let item_align = bx.children[real_idx_for_align].align_self.resolve(align);
            // Baseline alignment v column direction = fallback na start (CSS Flex L1
            // §8.3: "baseline alignment is supported only in row containers; in
            // columns, treat as start").
            let item_align = if matches!(item_align, AlignItems::Baseline) && !direction.is_row() {
                AlignItems::FlexStart
            } else { item_align };
            // Pro baseline pouzij natural cross (max bez stretch), ne cross_size kte
            // muze byt stretchnut na container.
            let align_box = if matches!(item_align, AlignItems::Baseline) {
                resolved.natural_cross
            } else {
                cross_size
            };
            // Auto cross margin override align: pri auto cross margin se align-items ignoruje.
            let auto_cross_count = (it.auto_cross_start as usize) + (it.auto_cross_end as usize);
            let cross_free = (cross_size - item_cross_size - it.margin_cross_start - it.margin_cross_end).max(0.0);
            let mut cross_offset;
            if auto_cross_count > 0 {
                let share = cross_free / auto_cross_count as f32;
                cross_offset = it.margin_cross_start;
                if it.auto_cross_start { cross_offset += share; }
                // auto_cross_end neovlivni offset, jen zabere zbylou plochu
            } else if matches!(item_align, AlignItems::Baseline) {
                // Baseline alignment: own_baseline z item_baselines (first child bottom or synth).
                let own_baseline = item_baselines[i_in_line];
                cross_offset = line_max_baseline - own_baseline + it.margin_cross_start;
            } else {
                // Pri item s flex-wrap: stretch cross => cross_offset = 0 (item zabira plnou cross).
                let real_idx_off = in_flow[item_idx];
                let item_has_wrap_off = !matches!(bx.children[real_idx_off].flex_wrap, FlexWrap::NoWrap);
                let effective_item_cross = if item_has_wrap_off {
                    cross_size - it.margin_cross_start - it.margin_cross_end
                } else {
                    item_cross_size
                };
                let cross_offset_align = compute_align_offset(item_align, align_box, effective_item_cross + it.margin_cross_start + it.margin_cross_end);
                cross_offset = cross_offset_align + it.margin_cross_start;
                // Pri wrap-reverse: cross axis se prevracije, takze align FlexStart/FlexEnd
                // tahaji item z opacne strany line. flip cross_offset.
                if matches!(wrap, FlexWrap::WrapReverse) {
                    let item_total = item_cross_size + it.margin_cross_start + it.margin_cross_end;
                    let from_end_align = compute_align_offset(item_align, align_box, item_total);
                    let flipped = align_box - item_total - from_end_align;
                    cross_offset = flipped + it.margin_cross_start;
                }
            }

            // Apply to child (item_idx je do in_flow, prevest na real index)
            let real_idx = in_flow[item_idx];
            let child = &mut bx.children[real_idx];
            // Pre-load child max/min cross + pad/border floor.
            // Percent values resolvuji se proti container's inner_w / inner_h_for_gap -
            // bez teto cesty `parse_length("100%")` davalo default 16 a clamp w=16
            // shrinknul kazdou flex-column item na 16 px wide.
            let cw_ctx_c = super::super::layout::ResolveCtx { parent_size: inner_w, font_size: child.font_size, ..Default::default() };
            let ch_ctx_c = super::super::layout::ResolveCtx { parent_size: inner_h_for_gap, font_size: child.font_size, ..Default::default() };
            let cw_max_c = child.max_width.resolve_max(&cw_ctx_c);
            let ch_max_c = child.max_height.resolve_max(&ch_ctx_c);
            let cw_min_c = child.min_width.resolve(&cw_ctx_c);
            let ch_min_c = child.min_height.resolve(&ch_ctx_c);
            let pb_w = child.padding_left.unwrap_or(child.padding) + child.padding_right.unwrap_or(child.padding)
                + child.border_left_width.unwrap_or(child.border_width) + child.border_right_width.unwrap_or(child.border_width);
            let pb_h = child.padding_top.unwrap_or(child.padding) + child.padding_bottom.unwrap_or(child.padding)
                + child.border_top_width.unwrap_or(child.border_width) + child.border_bottom_width.unwrap_or(child.border_width);
            if direction.is_row() {
                child.rect.x = inner_x + main_cursor;
                child.rect.y = inner_y + cross_cursor + cross_offset;
                let mut w = main_size;
                // Pri item s flex-wrap: stretch cross axis (taffy behavior) - ALE ne pri
                // baseline-aligned itemu (CSS: baseline neclamp).
                let item_has_wrap = !matches!(child.flex_wrap, FlexWrap::NoWrap);
                let stretch_cross = (matches!(item_align, AlignItems::Stretch) || item_has_wrap)
                    && !matches!(item_align, AlignItems::Baseline);
                let mut h = if stretch_cross && child.explicit_height.is_none() {
                    (cross_size - it.margin_cross_start - it.margin_cross_end).max(0.0)
                } else { item_cross_size };
                // Clamp h max/min PRED aspect dopoctem.
                h = h.min(ch_max_c);
                if ch_min_c > 0.0 { h = h.max(ch_min_c); }
                // Pri aspect-ratio + stretch row: w dopocti z (clamped) h.
                if let Some(ar) = child.aspect_ratio {
                    if ar > 0.0 && matches!(item_align, AlignItems::Stretch) && child.explicit_width.is_none() && child.explicit_height.is_none() {
                        if h > 0.0 { w = h * ar; }
                    }
                }
                w = w.min(cw_max_c);
                if cw_min_c > 0.0 { w = w.max(cw_min_c); }
                w = w.max(pb_w);
                let h_final = h.max(pb_h);
                child.rect.width = w;
                child.rect.height = h_final;
            } else {
                child.rect.x = inner_x + cross_cursor + cross_offset;
                child.rect.y = inner_y + main_cursor;
                let mut h = main_size;
                let item_has_wrap = !matches!(child.flex_wrap, FlexWrap::NoWrap);
                let stretch_cross = matches!(item_align, AlignItems::Stretch) || item_has_wrap;
                let mut w = if stretch_cross && child.explicit_width.is_none() {
                    (cross_size - it.margin_cross_start - it.margin_cross_end).max(0.0)
                } else { item_cross_size };
                // Clamp w max/min PRED aspect dopoctem.
                w = w.min(cw_max_c);
                if cw_min_c > 0.0 { w = w.max(cw_min_c); }
                // Pri aspect-ratio + stretch column: h dopocti z (clamped) w pak clamp max-h.
                if let Some(ar) = child.aspect_ratio {
                    if ar > 0.0 && matches!(item_align, AlignItems::Stretch) && child.explicit_height.is_none() && child.explicit_width.is_none() {
                        if w > 0.0 { h = w / ar; }
                    }
                }
                // Text wrap: pri text item + final w < text natural, dopocti pocet linek.
                if child.taffy_mode && child.text.is_some() && child.explicit_height.is_none() && child.aspect_ratio.is_none() {
                    if let Some(t) = &child.text {
                        let total_text_w = t.chars().filter(|c| !matches!(*c, '\u{200B}' | ' ' | '\n' | '\t')).count() as f32 * 10.0;
                        if w > 0.0 && total_text_w > w {
                            let mut lines = 1usize;
                            let mut cur_w = 0.0_f32;
                            let mut seg_w = 0.0_f32;
                            for c in t.chars() {
                                if matches!(c, '\u{200B}' | ' ' | '\n' | '\t') {
                                    if cur_w + seg_w <= w + 0.01 {
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
                            if seg_w > 0.0 && cur_w + seg_w > w + 0.01 { lines += 1; }
                            h = (lines as f32 * 10.0).max(h);
                        }
                    }
                }
                h = h.min(ch_max_c);
                if ch_min_c > 0.0 { h = h.max(ch_min_c); }
                child.rect.height = h.max(pb_h);
                child.rect.width = w.max(pb_w);
            }

            main_cursor += main_size + it.margin_main_end;
            if it.auto_main_end { main_cursor += auto_main_share; }
        }

        cross_cursor += resolved.cross_size + line_gap + ac_between;
    }

    // 7. Update parent width jen kdyz neni explicit + rect.width = 0 (pre-pass intrinsic).
    // Pri intrinsic mode + width_pct: ignore explicit_width (came from percent pre-resolve).
    let width_is_pct_intrinsic = bx.taffy_intrinsic_mode && bx.width_pct.is_some();
    if (bx.explicit_width.is_none() || width_is_pct_intrinsic) && bx.rect.width == 0.0 {
        let needed_w = if direction.is_row() {
            // Per line: suma main_sizes + per-item margin start/end + gaps mezi.
            // Pozn: items[real_idx] kde real_idx = lines[li][k]; ne items[k]
            // (k je pozice v line, ne globalni items index).
            let mut max_main: f32 = 0.0;
            for (li, line) in resolved_lines.iter().enumerate() {
                let line_indices = &lines[li];
                let mut sum: f32 = 0.0;
                for (k, s) in line.main_sizes.iter().enumerate() {
                    let it = &items[line_indices[k]];
                    sum += s + it.margin_main_start + it.margin_main_end;
                }
                sum += main_gap * (line.main_sizes.len().saturating_sub(1) as f32);
                if sum > max_main { max_main = sum; }
            }
            max_main + pad_l + pad_r
        } else {
            // Column: cross axis = width
            total_cross + pad_l + pad_r
        };
        bx.rect.width = needed_w;
    }
    // 7. Update parent height jen kdyz neni explicit set.
    let height_is_pct_intrinsic = bx.taffy_intrinsic_mode && bx.height_pct.is_some();
    if bx.explicit_height.is_none() || height_is_pct_intrinsic {
        let needed = if direction.is_row() {
            total_cross + pad_t + pad_b
        } else {
            // Column main = height. Include item margin_main_start+end (= margin-top+bottom).
            // Pre-pass intrinsic potrebuje znat realny obsah vc. margin pro nadrazene flexy.
            // Use ACTUAL child.rect.height po sub-layout (text wrap muze h zvetsit).
            let mut main_used_max: f32 = 0.0;
            for (li, line_indices) in lines.iter().enumerate() {
                let resolved = &resolved_lines[li];
                let mut line_actual_sum: f32 = 0.0;
                for (k, &item_idx) in line_indices.iter().enumerate() {
                    let real_idx = in_flow[item_idx];
                    let actual_h = bx.children[real_idx].rect.height;
                    let assigned = resolved.main_sizes.get(k).copied().unwrap_or(0.0);
                    line_actual_sum += actual_h.max(assigned);
                }
                let line_gap_sum = main_gap * (resolved.main_sizes.len().saturating_sub(1) as f32);
                main_used_max = main_used_max.max(line_actual_sum + line_gap_sum);
            }
            let item_margins: f32 = items.iter().map(|it| it.margin_main_start + it.margin_main_end).sum();
            main_used_max + item_margins + pad_t + pad_b
        };
        // V intrinsic mode (pre-pass) override vzdy. V normal mode pri row
        // direction (cross=height): rect.height = needed (= total_cross), aby
        // wrap container po stretch na sirku spravne shrinkl na content height.
        // Pri column direction expand jen. Pri overflow non-visible v main axis (= column = height):
        // bx zustane na rect.height (drive set), neexpanduj na content (overflow clip).
        let main_overflow_blocks_self = if direction.is_row() {
            bx.overflow_x.clips()
        } else {
            bx.overflow_y.clips()
        };
        if bx.taffy_intrinsic_mode {
            bx.rect.height = needed;
        } else if direction.is_row() {
            // Row direction: needed = total_cross. Pri overflow-y (cross) blocks - skip override.
            let cross_overflow = bx.overflow_y.clips();
            if !cross_overflow { bx.rect.height = needed; }
            else if bx.rect.height < needed { /* keep */ }
        } else if bx.rect.height < needed && !main_overflow_blocks_self {
            // Pri rect.height >= min-height (parent uz set spravnou hodnotu), neexpanduj.
            let self_ctx_h = super::super::layout::ResolveCtx { parent_size: bx.rect.height, font_size: bx.font_size, ..Default::default() };
            let mnh_self = bx.min_height.resolve(&self_ctx_h);
            if mnh_self > 0.0 && bx.rect.height >= mnh_self {
                // Keep rect.height (parent sized respecting min-height).
            } else {
                bx.rect.height = needed;
            }
        }
        // Apply max/min-height clamp na container kdyz auto.
        let self_ctx_h = super::super::layout::ResolveCtx { parent_size: bx.rect.height, font_size: bx.font_size, ..Default::default() };
        if bx.max_height.is_specified() {
            let mh = bx.max_height.resolve(&self_ctx_h);
            if mh > 0.0 && bx.rect.height > mh { bx.rect.height = mh; }
        }
        let mnh = bx.min_height.resolve(&self_ctx_h);
        if mnh > 0.0 && bx.rect.height < mnh { bx.rect.height = mnh; }
    }

    // 8. Position absolute/fixed children (CB = padding-box parenta; fixed = viewport)
    let cb_x = bx.rect.x + bw_l;
    let cb_y = bx.rect.y + bw_t;
    let cb_w = (bx.rect.width - bw_l - bw_r).max(0.0);
    let cb_h = (bx.rect.height - bw_t - bw_b).max(0.0);
    for ch in bx.children.iter_mut() {
        if super::is_out_of_flow(ch) {
            // display:none na abs - zero out a skip.
            if matches!(ch.display, super::super::layout::Display::None) {
                ch.rect.x = 0.0; ch.rect.y = 0.0;
                ch.rect.width = 0.0; ch.rect.height = 0.0;
                continue;
            }
            // Fixed: CB = initial CB (viewport), ne parent.
            let is_fixed = matches!(ch.position, super::super::layout::Position::Fixed);
            let (vw, vh) = super::super::cascade::MATH_VIEWPORT.with(|c| *c.borrow());
            let (fcb_x, fcb_y, fcb_w, fcb_h) = if is_fixed && vw > 0.0 && vh > 0.0 {
                (0.0, 0.0, vw, vh)
            } else { (cb_x, cb_y, cb_w, cb_h) };
            // Pre-layout: pokud abs nema inset v dane ose, pouzij flex-container
            // alignment (justify-content / align-items) pro static position.
            super::layout_absolute_child(ch, fcb_x, fcb_y, fcb_w, fcb_h);
            // Override pri zadnem insetu: respektuj justify-content / align-items / align-self.
            let no_inset_x = ch.offset_left.is_none() && ch.offset_right.is_none();
            let no_inset_y = ch.offset_top.is_none() && ch.offset_bottom.is_none();
            if no_inset_x || no_inset_y {
                let m_l_c = ch.margin_left.unwrap_or(ch.margin);
                let m_t_c = ch.margin_top.unwrap_or(ch.margin);
                let m_r_c = ch.margin_right.unwrap_or(ch.margin);
                let m_b_c = ch.margin_bottom.unwrap_or(ch.margin);
                let self_align = ch.align_self.resolve(align);
                let is_wrap_reverse = matches!(wrap, FlexWrap::WrapReverse);
                if direction.is_row() {
                    if no_inset_x {
                        let free = (fcb_w - ch.rect.width - m_l_c - m_r_c).max(0.0);
                        let off = match justify {
                            JustifyContent::FlexEnd => free,
                            JustifyContent::Center => free / 2.0,
                            _ => 0.0,
                        };
                        ch.rect.x = fcb_x + m_l_c + off;
                    }
                    if no_inset_y {
                        let mut use_align = if !ch.align_self.is_auto() { self_align } else { align };
                        // Pro abs s explicit cross size, Stretch nedava smysl - default FlexStart.
                        if matches!(use_align, AlignItems::Stretch) { use_align = AlignItems::FlexStart; }
                        let free = (fcb_h - ch.rect.height - m_t_c - m_b_c).max(0.0);
                        // Wrap-reverse flips cross start/end.
                        let effective_align = if is_wrap_reverse {
                            match use_align {
                                AlignItems::FlexStart => AlignItems::FlexEnd,
                                AlignItems::FlexEnd => AlignItems::FlexStart,
                                a => a,
                            }
                        } else { use_align };
                        let off = match effective_align {
                            AlignItems::FlexEnd => free,
                            AlignItems::Center => free / 2.0,
                            _ => 0.0,
                        };
                        ch.rect.y = fcb_y + m_t_c + off;
                    }
                } else {
                    if no_inset_y {
                        let free = (fcb_h - ch.rect.height - m_t_c - m_b_c).max(0.0);
                        let off = match justify {
                            JustifyContent::FlexEnd => free,
                            JustifyContent::Center => free / 2.0,
                            _ => 0.0,
                        };
                        ch.rect.y = fcb_y + m_t_c + off;
                    }
                    if no_inset_x {
                        let mut use_align = if !ch.align_self.is_auto() { self_align } else { align };
                        if matches!(use_align, AlignItems::Stretch) { use_align = AlignItems::FlexStart; }
                        let free = (fcb_w - ch.rect.width - m_l_c - m_r_c).max(0.0);
                        let effective_align = if is_wrap_reverse {
                            match use_align {
                                AlignItems::FlexStart => AlignItems::FlexEnd,
                                AlignItems::FlexEnd => AlignItems::FlexStart,
                                a => a,
                            }
                        } else { use_align };
                        let off = match effective_align {
                            AlignItems::FlexEnd => free,
                            AlignItems::Center => free / 2.0,
                            _ => 0.0,
                        };
                        ch.rect.x = fcb_x + m_l_c + off;
                    }
                }
            }
        }
    }

    // 9. Recursive layout uvnitr child boxu (jen non-abs - abs uz layoutnut)
    for ch in bx.children.iter_mut() {
        if super::is_out_of_flow(ch) { continue; }
        if matches!(ch.display, super::super::layout::Display::None) { continue; }
        // CSS positioning: top/left/bottom/right ovlivnuje JEN Position::Relative.
        // Sticky offset aplikuje apply_sticky() pri scroll.
        if matches!(ch.position, super::super::layout::Position::Relative) {
            let off_x = if let Some(l) = ch.offset_left { l }
                        else if let Some(r) = ch.offset_right { -r }
                        else { 0.0 };
            let off_y = if let Some(t) = ch.offset_top { t }
                        else if let Some(b) = ch.offset_bottom { -b }
                        else { 0.0 };
            ch.rect.x += off_x;
            ch.rect.y += off_y;
        }
        // Non-default flex props na block elementu = treat jako flex (drive
        // String non-empty detect; ted typed - check non-default values).
        let has_flex_attr = !matches!(ch.flex_direction, FlexDirection::Row)
            || !matches!(ch.justify_content, JustifyContent::FlexStart);
        if matches!(ch.display, super::super::layout::Display::Block) && has_flex_attr {
            super::flex::layout_flex(ch);
        } else {
            super::dispatch_layout(ch, true);
        }
    }

    // 10. POST-LAYOUT REFLOW: aplikuje JEN pri column auto-height +
    // vsech items auto-height (zadny explicit_height + flex_grow=0). Bez teto
    // podminky kazi justify-content + column-reverse + fixed-height stack.
    // Ucel: opravit auto-height items prekryv pri column main_size pre-pass=0.
    if !direction.is_row() && bx.explicit_height.is_none()
       && matches!(justify, JustifyContent::FlexStart)
       && !matches!(direction, FlexDirection::ColumnReverse)
       && bx.children.iter().all(|c| c.explicit_height.is_none() && c.flex_grow == 0.0)
    {
        let pad_t = bx.padding_top.unwrap_or(bx.padding);
        let inner_y_local = bx.rect.y + pad_t + bx.border_width;
        let mut cursor = inner_y_local;
        let gap = bx.row_gap;
        let n = bx.children.len();
        let mut first = true;
        for i in 0..n {
            let ch = &mut bx.children[i];
            if super::is_out_of_flow(ch) { continue; }
            if matches!(ch.display, super::super::layout::Display::None) { continue; }
            let m_t = ch.margin_top.unwrap_or(ch.margin);
            let m_b = ch.margin_bottom.unwrap_or(ch.margin);
            if !first { cursor += gap; }
            cursor += m_t;
            let dy = cursor - ch.rect.y;
            if dy.abs() > 0.5 {
                super::super::layout::shift_subtree(ch, 0.0, dy);
            }
            cursor += ch.rect.height + m_b;
            first = false;
        }
        let pad_b = bx.padding_bottom.unwrap_or(bx.padding);
        let new_h = cursor - bx.rect.y + pad_b + bx.border_width;
        if new_h > bx.rect.height {
            bx.rect.height = new_h;
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
/// Recursive intrinsic content width pres LayoutBox subtree.
/// Pri flex item bez explicit width spocte sirku z descendant text + own
/// padding/margin/border. Pro <button>Primary</button> = "Primary" width
/// + button.padding * 2 + border * 2.
fn intrinsic_content_width(bx: &LayoutBox) -> f32 {
    use super::super::layout::Display;
    let pad_l = bx.padding_left.unwrap_or(bx.padding);
    let pad_r = bx.padding_right.unwrap_or(bx.padding);
    let chrome = pad_l + pad_r + 2.0 * bx.border_width;
    if let Some(t) = &bx.text {
        return super::super::layout::measure_text_width_full(t, bx.font_size, bx.bold, bx.italic, &bx.font_family) + chrome;
    }
    // CSS spec max-content:
    // - Inline flow (text nodes + inline children): SUM child widths (vsechna
    //   na stejne lince).
    // - Block flow (block children): MAX child width (kazdy na vlastni lince).
    // Detekce: pokud aspon 1 child neni Block/Flex/Grid -> inline flow.
    let any_inline = bx.children.iter().any(|c| matches!(c.display,
        Display::Inline | Display::InlineBlock | Display::InlineFlex | Display::InlineGrid)
        || c.tag.is_none() // text node
    );
    let aggregate: f32 = if any_inline {
        // Sum text + inline children, max nad block siblings (rare).
        bx.children.iter()
            .map(|c| intrinsic_content_width(c))
            .sum::<f32>()
    } else {
        bx.children.iter()
            .map(|c| intrinsic_content_width(c))
            .fold(0.0_f32, f32::max)
    };
    if aggregate > 0.0 { aggregate + chrome } else { chrome }
}

fn collect_lines(items: &[FlexItem], container_main: f32, wrap: FlexWrap, gap: f32) -> Vec<Vec<usize>> {
    if matches!(wrap, FlexWrap::NoWrap) {
        return vec![(0..items.len()).collect()];
    }
    // Pri container_main = 0 (parent rect.width pre-pass not set) - all items
    // do jedne line. Bez teto pojistky: kazdy item vlastni line (used > 0 always),
    // container.h = sum(item.h) misto max -> 5x roznasobeny height.
    if container_main <= 0.0 {
        return vec![(0..items.len()).collect()];
    }
    // PERF: pre-alloc capacity (typicky nowrap = 1 line, wrap rarely > items/2).
    let mut lines: Vec<Vec<usize>> = Vec::with_capacity(2);
    let mut current: Vec<usize> = Vec::with_capacity(items.len());
    let mut used = 0.0_f32;
    for (i, item) in items.iter().enumerate() {
        let item_total = item.main_size + item.margin_main_start + item.margin_main_end;
        let with_gap = if current.is_empty() { item_total } else { item_total + gap };
        if !current.is_empty() && used + with_gap > container_main {
            lines.push(current);
            current = Vec::with_capacity(items.len() / 2);
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

        // Distribute free. CSS spec: sum_grow < 1 -> divisor = 1 (leftover free zustane).
        let divisor = if growing { total_factor.max(1.0) } else { total_factor };
        for (k, &i) in indices.iter().enumerate() {
            if frozen[k] { continue; }
            let factor = if growing { items[i].flex_grow } else { items[i].flex_shrink * items[i].main_size };
            sizes[k] = items[i].main_size + free * (factor / divisor);
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
        JustifyContent::FlexStart | JustifyContent::Start => (0.0, 0.0),
        JustifyContent::FlexEnd | JustifyContent::End => (free, 0.0),
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

// PERF: parse_flex_direction/wrap/justify_content/align_items NIK pres
// free fn - LayoutBox fields uz typed (cascade parsuje JEDNOU). Hot loop
// reads bx.flex_direction directly. Stari testy v `tests` mod jeste pouzily
// volnou variantu; preznacuj se na FlexDirection::parse.


#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn parse_direction_basic() {
        assert_eq!(FlexDirection::parse("row"), FlexDirection::Row);
        assert_eq!(FlexDirection::parse("row-reverse"), FlexDirection::RowReverse);
        assert_eq!(FlexDirection::parse("column"), FlexDirection::Column);
        assert_eq!(FlexDirection::parse("column-reverse"), FlexDirection::ColumnReverse);
    }
    
    #[test]
    fn parse_wrap_basic() {
        assert_eq!(FlexWrap::parse("wrap"), FlexWrap::Wrap);
        assert_eq!(FlexWrap::parse("nowrap"), FlexWrap::NoWrap);
        assert_eq!(FlexWrap::parse("wrap-reverse"), FlexWrap::WrapReverse);
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
