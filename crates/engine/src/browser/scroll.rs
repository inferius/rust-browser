//! Scrollable abstrakce - unifikovany API pro viewport + per-element scroll.
//!
//! Trait `Scrollable` exponuje read-only state (offset, viewport_size,
//! content_size). Helpery `max_scroll`, `thumb_y/x`, `needs_scrollbar_*`
//! tezi z default impls.
//!
//! Trait `ScrollableMut` pridava mutation - `set_scroll_offset` + default
//! `scroll_by` (clamps to max).
//!
//! Impl pro `LayoutBox` cita `bx.rect.width/height` jako viewport,
//! `bx.inner_content_w/h` jako content, `bx.scroll_offset_x/y` jako offset.
//!
//! WebView viewport scroll pres helper struct `ViewportScroll` ktery drzi
//! refs na viewport rect + total layout rect + scroll fields.
//!
//! Pouziti:
//! - Paint: `emit_inner_scrollbars` jen vola trait methody na &LayoutBox
//! - find_scroll_target: walk path, check `needs_scrollbar_*` + `has_room_*`
//! - Wheel handler: `scroll_by` na Scrollable& target
//! - Scrollbar drag (futurni): track click + thumb -> set_scroll_offset

use super::layout::LayoutBox;

/// Read-only scroll state access. Default impls vypocitaj derived hodnoty.
pub trait Scrollable {
    /// Aktualni scroll offset v px (x, y).
    fn scroll_offset(&self) -> (f32, f32);
    /// Viewport (= visible area) v px.
    fn viewport_size(&self) -> (f32, f32);
    /// Total content extent v px.
    fn content_size(&self) -> (f32, f32);

    /// Max scroll offset = content - viewport, clamped >= 0.
    fn max_scroll(&self) -> (f32, f32) {
        let (vw, vh) = self.viewport_size();
        let (cw, ch) = self.content_size();
        ((cw - vw).max(0.0), (ch - vh).max(0.0))
    }

    /// Content > viewport v dany ose.
    fn needs_scrollbar_y(&self) -> bool {
        let (_, vh) = self.viewport_size();
        let (_, ch) = self.content_size();
        ch > vh + 0.5
    }
    fn needs_scrollbar_x(&self) -> bool {
        let (vw, _) = self.viewport_size();
        let (cw, _) = self.content_size();
        cw > vw + 0.5
    }

    /// Vrati (thumb_y, thumb_h) pro vertical scrollbar v rozsahu track_h px.
    /// None pokud scrollbar nepotreba NEBO neni realne co scrollovat
    /// (max_scroll < 2px = vlasovy overflow z metrickeho rozdilu vs Chrome -
    /// thumb by vyplnil cely track = "zlute linky" pri scrollbar-color; Chrome
    /// v tom stavu kresli track bez draggeru).
    fn thumb_y(&self, track_h: f32) -> Option<(f32, f32)> {
        if !self.needs_scrollbar_y() { return None; }
        let (_, my) = self.max_scroll();
        if my < 2.0 { return None; }
        let (_, vh) = self.viewport_size();
        let (_, ch) = self.content_size();
        let (_, sy) = self.scroll_offset();
        // .min(track_h): pri overflow:scroll + obsah se vejde (vh>=ch) by thumb
        // presahl track - clamp aby vyplnil presne track (nic ke scrollovani).
        let thumb_h = (track_h * vh / ch).max(30.0).min(track_h);
        let usable = (track_h - thumb_h).max(0.0);
        let thumb_y = (sy / my).clamp(0.0, 1.0) * usable;
        Some((thumb_y, thumb_h))
    }
    fn thumb_x(&self, track_w: f32) -> Option<(f32, f32)> {
        if !self.needs_scrollbar_x() { return None; }
        let (mx, _) = self.max_scroll();
        if mx < 2.0 { return None; }
        let (vw, _) = self.viewport_size();
        let (cw, _) = self.content_size();
        let (sx, _) = self.scroll_offset();
        let thumb_w = (track_w * vw / cw).max(30.0).min(track_w);
        let usable = (track_w - thumb_w).max(0.0);
        let thumb_x = (sx / mx).clamp(0.0, 1.0) * usable;
        Some((thumb_x, thumb_w))
    }

    /// Room v scroll smeru (positive dy = scroll down, dx = scroll right).
    fn has_room(&self, dx: f32, dy: f32) -> bool {
        let (sx, sy) = self.scroll_offset();
        let (mx, my) = self.max_scroll();
        let room_y = (dy > 0.0 && sy < my) || (dy < 0.0 && sy > 0.0);
        let room_x = (dx > 0.0 && sx < mx) || (dx < 0.0 && sx > 0.0);
        (dy != 0.0 && room_y) || (dx != 0.0 && room_x)
    }
}

/// Mutable scroll state. Default `scroll_by` cte+set s clamp.
pub trait ScrollableMut: Scrollable {
    fn set_scroll_offset(&mut self, x: f32, y: f32);

    fn scroll_by(&mut self, dx: f32, dy: f32) {
        let (sx, sy) = self.scroll_offset();
        let (mx, my) = self.max_scroll();
        let nx = (sx + dx).clamp(0.0, mx);
        let ny = (sy + dy).clamp(0.0, my);
        self.set_scroll_offset(nx, ny);
    }
}

// LayoutBox read impl - autoritativni pres pre-paint walk (apply_element_scroll
// nastavuje bx.scroll_offset_*).
impl Scrollable for LayoutBox {
    fn scroll_offset(&self) -> (f32, f32) {
        (self.scroll_offset_x, self.scroll_offset_y)
    }
    fn viewport_size(&self) -> (f32, f32) {
        (self.rect.width, self.rect.height)
    }
    fn content_size(&self) -> (f32, f32) {
        (self.inner_content_w, self.inner_content_h)
    }
    // needs_scrollbar_y/x override - musi byt overflow:auto/scroll (jinak
    // content_h > rect.height moze byt overflow:visible = no scrollbar).
    fn needs_scrollbar_y(&self) -> bool {
        // Pozn: overflow:scroll per spec ukazuje scrollbar vzdy, ALE custom
        // scrollbar-color (zluta) pak svitil "zlute linky" i kdyz neni co
        // scrollovat (docx2 sekce 11). User chce bez draggeru kdyz se vejde ->
        // chovame se jako auto (scrollbar jen pri preteceni). always_shows()
        // ponechano pro pripadne budouci subtle-disabled rendering.
        self.overflow_y.scrollable() && self.inner_content_h > self.rect.height + 0.5
    }
    fn needs_scrollbar_x(&self) -> bool {
        self.overflow_x.scrollable() && self.inner_content_w > self.rect.width + 0.5
    }
}

/// Viewport scroll handle - WebView root level scroll. Drzi mutable refs +
/// viewport/content dimensions.
pub struct ViewportScroll<'a> {
    pub scroll_x: &'a mut f32,
    pub scroll_y: &'a mut f32,
    pub viewport_w: f32,
    pub viewport_h: f32,
    pub content_w: f32,
    pub content_h: f32,
}

impl<'a> Scrollable for ViewportScroll<'a> {
    fn scroll_offset(&self) -> (f32, f32) {
        (*self.scroll_x, *self.scroll_y)
    }
    fn viewport_size(&self) -> (f32, f32) {
        (self.viewport_w, self.viewport_h)
    }
    fn content_size(&self) -> (f32, f32) {
        (self.content_w, self.content_h)
    }
}

impl<'a> ScrollableMut for ViewportScroll<'a> {
    fn set_scroll_offset(&mut self, x: f32, y: f32) {
        *self.scroll_x = x;
        *self.scroll_y = y;
    }
}

/// Element scroll handle - WebView element_scroll map + LayoutBox metrics.
pub struct ElementScroll<'a> {
    pub map: &'a mut std::collections::HashMap<usize, (f32, f32)>,
    pub node_id: usize,
    pub rect_w: f32,
    pub rect_h: f32,
    pub content_w: f32,
    pub content_h: f32,
}

impl<'a> Scrollable for ElementScroll<'a> {
    fn scroll_offset(&self) -> (f32, f32) {
        self.map.get(&self.node_id).copied().unwrap_or((0.0, 0.0))
    }
    fn viewport_size(&self) -> (f32, f32) {
        (self.rect_w, self.rect_h)
    }
    fn content_size(&self) -> (f32, f32) {
        (self.content_w, self.content_h)
    }
}

impl<'a> ScrollableMut for ElementScroll<'a> {
    fn set_scroll_offset(&mut self, x: f32, y: f32) {
        self.map.insert(self.node_id, (x, y));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockScroll { off: (f32, f32), vp: (f32, f32), ct: (f32, f32) }
    impl Scrollable for MockScroll {
        fn scroll_offset(&self) -> (f32, f32) { self.off }
        fn viewport_size(&self) -> (f32, f32) { self.vp }
        fn content_size(&self) -> (f32, f32) { self.ct }
    }
    impl ScrollableMut for MockScroll {
        fn set_scroll_offset(&mut self, x: f32, y: f32) { self.off = (x, y); }
    }

    #[test]
    fn max_scroll_basic() {
        let m = MockScroll { off: (0.0, 0.0), vp: (100.0, 100.0), ct: (200.0, 300.0) };
        assert_eq!(m.max_scroll(), (100.0, 200.0));
    }

    #[test]
    fn max_scroll_no_overflow() {
        let m = MockScroll { off: (0.0, 0.0), vp: (200.0, 200.0), ct: (100.0, 100.0) };
        assert_eq!(m.max_scroll(), (0.0, 0.0));
    }

    #[test]
    fn needs_scrollbar_logic() {
        let m = MockScroll { off: (0.0, 0.0), vp: (100.0, 100.0), ct: (200.0, 50.0) };
        assert!(m.needs_scrollbar_x());
        assert!(!m.needs_scrollbar_y());
    }

    #[test]
    fn thumb_y_at_top() {
        let m = MockScroll { off: (0.0, 0.0), vp: (100.0, 100.0), ct: (100.0, 400.0) };
        let (ty, th) = m.thumb_y(100.0).unwrap();
        assert!((ty - 0.0).abs() < 0.5);
        assert!(th >= 25.0);
    }

    #[test]
    fn thumb_y_at_bottom() {
        let m = MockScroll { off: (0.0, 300.0), vp: (100.0, 100.0), ct: (100.0, 400.0) };
        let (ty, th) = m.thumb_y(100.0).unwrap();
        assert!((ty + th - 100.0).abs() < 1.0);
    }

    #[test]
    fn scroll_by_clamps() {
        let mut m = MockScroll { off: (0.0, 50.0), vp: (100.0, 100.0), ct: (100.0, 200.0) };
        m.scroll_by(0.0, 200.0);
        assert_eq!(m.scroll_offset(), (0.0, 100.0));
        m.scroll_by(0.0, -200.0);
        assert_eq!(m.scroll_offset(), (0.0, 0.0));
    }

    #[test]
    fn has_room_directions() {
        let m = MockScroll { off: (0.0, 0.0), vp: (100.0, 100.0), ct: (100.0, 200.0) };
        assert!(m.has_room(0.0, 1.0));
        assert!(!m.has_room(0.0, -1.0));
        let m2 = MockScroll { off: (0.0, 100.0), vp: (100.0, 100.0), ct: (100.0, 200.0) };
        assert!(!m2.has_room(0.0, 1.0));
        assert!(m2.has_room(0.0, -1.0));
    }
}
