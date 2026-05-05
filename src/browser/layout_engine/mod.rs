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

/// Layout absolute/fixed positioned child relativne k containing block (parent rect).
/// CSS Position L3: top/left/right/bottom + width/height + aspect-ratio + margins.
pub fn layout_absolute_child(child: &mut LayoutBox, parent_x: f32, parent_y: f32, parent_w: f32, parent_h: f32) {
    let cb_x = parent_x;
    let cb_y = parent_y;
    let cb_w = parent_w;
    let cb_h = parent_h;
    // Width: explicit nebo (right-left) nebo z aspect-ratio nebo 0
    let mut w = if let Some(w) = child.explicit_width {
        w
    } else if let (Some(l), Some(r)) = (child.offset_left, child.offset_right) {
        (cb_w - l - r).max(0.0)
    } else if let Some(ar) = child.aspect_ratio {
        if let Some(h) = child.explicit_height { if ar > 0.0 { h * ar } else { 0.0 } } else { 0.0 }
    } else { 0.0 };
    let mut h = if let Some(h) = child.explicit_height {
        h
    } else if let (Some(t), Some(b)) = (child.offset_top, child.offset_bottom) {
        (cb_h - t - b).max(0.0)
    } else if let Some(ar) = child.aspect_ratio {
        if ar > 0.0 { w / ar } else { 0.0 }
    } else { 0.0 };
    // Aspect ratio override pri "fill" sizing (inset bez explicit dimensi).
    // Spec: aspect-ratio override height kdyz width explicit/inset, naopak.
    // Pri obou inset bez explicit -> aspect ratio aplikuje na width (drz width z insetu, h dopocitaj).
    if let Some(ar) = child.aspect_ratio {
        if ar > 0.0 {
            let has_explicit_w = child.explicit_width.is_some();
            let has_explicit_h = child.explicit_height.is_some();
            if has_explicit_w && !has_explicit_h {
                h = w / ar;
            } else if has_explicit_h && !has_explicit_w {
                w = h * ar;
            } else if !has_explicit_w && !has_explicit_h {
                // Bez explicit: bere width z insetu (mainly), h = w/ar.
                if w > 0.0 {
                    h = w / ar;
                } else if h > 0.0 {
                    w = h * ar;
                }
            }
        }
    }
    child.rect.width = w;
    child.rect.height = h;
    let m_l = child.margin_left.unwrap_or(child.margin);
    let m_t = child.margin_top.unwrap_or(child.margin);
    let m_r = child.margin_right.unwrap_or(child.margin);
    let m_b = child.margin_bottom.unwrap_or(child.margin);
    child.rect.x = if let Some(l) = child.offset_left {
        cb_x + l + m_l
    } else if let Some(r) = child.offset_right {
        cb_x + cb_w - r - w - m_r
    } else {
        cb_x + m_l
    };
    child.rect.y = if let Some(t) = child.offset_top {
        cb_y + t + m_t
    } else if let Some(b) = child.offset_bottom {
        cb_y + cb_h - b - h - m_b
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

pub use flex::{layout_flex, FlexDirection, FlexWrap, JustifyContent, AlignItems};
pub use grid::layout_grid;
