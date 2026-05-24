//! Focus management - tab order + sequential focus traversal.
//!
//! Spec: https://html.spec.whatwg.org/multipage/interaction.html#focus

#[derive(Debug, Clone)]
pub struct FocusableElement {
    pub id: u64,
    pub tab_index: i32,
    pub disabled: bool,
    pub hidden: bool,
    pub in_dom_order: u32,            // document order index
    pub auto_focus: bool,
}

impl FocusableElement {
    pub fn is_focusable(&self) -> bool {
        !self.disabled && !self.hidden && self.tab_index >= 0
    }
}

#[derive(Default)]
pub struct FocusManager {
    pub elements: Vec<FocusableElement>,
    pub current_focus_id: Option<u64>,
    pub focus_ring_visible: bool,
}

impl FocusManager {
    pub fn new() -> Self { Self::default() }

    pub fn register(&mut self, el: FocusableElement) {
        self.elements.push(el);
    }

    pub fn unregister(&mut self, id: u64) {
        self.elements.retain(|e| e.id != id);
        if self.current_focus_id == Some(id) {
            self.current_focus_id = None;
        }
    }

    pub fn focus(&mut self, id: u64) -> bool {
        if self.elements.iter().any(|e| e.id == id && e.is_focusable()) {
            self.current_focus_id = Some(id);
            return true;
        }
        false
    }

    /// Spec: focus the first auto-focused candidate, then return false if none.
    pub fn process_autofocus(&mut self) -> Option<u64> {
        let id = self.elements.iter().find(|e| e.auto_focus && e.is_focusable()).map(|e| e.id)?;
        self.focus(id);
        Some(id)
    }

    /// Tab order: positive tabindex first (ascending), then 0/default (DOM order).
    fn order(&self) -> Vec<&FocusableElement> {
        let mut out: Vec<&FocusableElement> = self.elements.iter().filter(|e| e.is_focusable()).collect();
        out.sort_by(|a, b| {
            let ka = if a.tab_index > 0 { (0, a.tab_index, a.in_dom_order) }
                     else { (1, 0, a.in_dom_order) };
            let kb = if b.tab_index > 0 { (0, b.tab_index, b.in_dom_order) }
                     else { (1, 0, b.in_dom_order) };
            ka.cmp(&kb)
        });
        out
    }

    pub fn focus_next(&mut self) -> Option<u64> {
        let order = self.order();
        if order.is_empty() { return None; }
        let idx = match self.current_focus_id {
            Some(id) => order.iter().position(|e| e.id == id).map(|i| (i + 1) % order.len()).unwrap_or(0),
            None => 0,
        };
        let next_id = order[idx].id;
        self.focus(next_id);
        Some(next_id)
    }

    pub fn focus_prev(&mut self) -> Option<u64> {
        let order = self.order();
        if order.is_empty() { return None; }
        let idx = match self.current_focus_id {
            Some(id) => order.iter().position(|e| e.id == id)
                .map(|i| if i == 0 { order.len() - 1 } else { i - 1 })
                .unwrap_or(order.len() - 1),
            None => order.len() - 1,
        };
        let prev_id = order[idx].id;
        self.focus(prev_id);
        Some(prev_id)
    }

    pub fn blur(&mut self) {
        self.current_focus_id = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(id: u64, ti: i32, dom_order: u32) -> FocusableElement {
        FocusableElement {
            id, tab_index: ti, disabled: false, hidden: false,
            in_dom_order: dom_order, auto_focus: false,
        }
    }

    #[test]
    fn focus_focusable() {
        let mut m = FocusManager::new();
        m.register(el(1, 0, 0));
        assert!(m.focus(1));
        assert_eq!(m.current_focus_id, Some(1));
    }

    #[test]
    fn cannot_focus_disabled() {
        let mut m = FocusManager::new();
        let mut e = el(1, 0, 0);
        e.disabled = true;
        m.register(e);
        assert!(!m.focus(1));
    }

    #[test]
    fn next_cycles_through() {
        let mut m = FocusManager::new();
        m.register(el(1, 0, 0));
        m.register(el(2, 0, 1));
        m.register(el(3, 0, 2));
        m.focus(1);
        m.focus_next();
        assert_eq!(m.current_focus_id, Some(2));
        m.focus_next();
        assert_eq!(m.current_focus_id, Some(3));
        m.focus_next();
        assert_eq!(m.current_focus_id, Some(1));
    }

    #[test]
    fn positive_tabindex_first() {
        let mut m = FocusManager::new();
        m.register(el(1, 0, 0));
        m.register(el(2, 1, 1));
        m.register(el(3, 5, 2));
        m.focus_next();
        assert_eq!(m.current_focus_id, Some(2)); // tabindex 1 < 5
    }

    #[test]
    fn autofocus_selects() {
        let mut m = FocusManager::new();
        let mut e = el(2, 0, 1);
        e.auto_focus = true;
        m.register(el(1, 0, 0));
        m.register(e);
        let id = m.process_autofocus();
        assert_eq!(id, Some(2));
    }

    #[test]
    fn negative_tabindex_not_in_sequence() {
        let mut m = FocusManager::new();
        m.register(el(1, -1, 0));
        m.register(el(2, 0, 1));
        m.focus_next();
        assert_eq!(m.current_focus_id, Some(2));
    }
}
