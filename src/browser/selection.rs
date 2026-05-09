//! Document-level SelectionRegistry - centralni misto pro vsechen "current state"
//! text/input selection na strance.
//!
//! Drzi:
//! - `input_states`: per-input element cursor + anchor (lazy, jen kdyz user
//!   klikl/typoval). Klic = NodeId (Rc<NodeData> ptr cast as usize).
//! - `active_input`: currently focused input/textarea NodeId. Pri set focus
//!   commit_back current cache + create new.
//! - `page_selection`: rect-drag selection nad page contentem. Phase 6 ho
//!   rozsiri na (run_idx, byte_offset) tuples pro proper text-run selection.
//!
//! Cleanup: lazy GC pri pristupu - kdyz Rc::upgrade selze (DOM removed),
//! odstrani ze state map. Bez Weak refs neni jednoducha cesta detekce removal.
//! V praxi inputs neumiraji casto, mapa zustava mala (~1-10 entries).
//!
//! W3C Selection API foundation: budouci `document.getSelection()` JS bridge
//! cte stejny registry, vraci aktivni range. Multi-range (Ctrl+drag) jde
//! pridat extension.

use std::collections::HashMap;
use std::ops::Range;

pub type NodeId = usize;

/// Per-element text input state (input/textarea).
#[derive(Debug, Clone, Default)]
pub struct InputState {
    pub cursor: usize,
    pub anchor: Option<usize>,
}

/// Page-level selection. Aktualne rect-drag, phase 6 -> text-run.
#[derive(Debug, Clone)]
pub struct PageSelection {
    /// Anchor v document logical px (kde mouse pressed).
    pub anchor: (f32, f32),
    /// Current v document logical px (kde mouse je / kde released).
    pub current: (f32, f32),
    /// Aktivni drag (mouse down)?
    pub dragging: bool,
}

#[derive(Debug, Default)]
pub struct SelectionRegistry {
    pub input_states: HashMap<NodeId, InputState>,
    pub active_input: Option<NodeId>,
    pub page_selection: Option<PageSelection>,
}

impl SelectionRegistry {
    pub fn new() -> Self { Self::default() }

    /// Lazy access - vytvori InputState kdyz neni.
    pub fn input_state_mut(&mut self, node_id: NodeId) -> &mut InputState {
        self.input_states.entry(node_id).or_default()
    }

    pub fn input_state(&self, node_id: NodeId) -> Option<&InputState> {
        self.input_states.get(&node_id)
    }

    /// Pri DOM removal nebo focus blur - zachovat state pres drift sessions
    /// nebo smazat? Default = zachovat (Tab away + back resumes cursor).
    /// Volat cleanup() pri navigation / page reload.
    pub fn forget_input(&mut self, node_id: NodeId) {
        self.input_states.remove(&node_id);
        if self.active_input == Some(node_id) {
            self.active_input = None;
        }
    }

    pub fn clear_all(&mut self) {
        self.input_states.clear();
        self.active_input = None;
        self.page_selection = None;
    }

    /// Set selection range na konkretni input (klik + drag, programaticky).
    pub fn set_input_selection(&mut self, node_id: NodeId, range: Range<usize>) {
        let st = self.input_state_mut(node_id);
        st.anchor = Some(range.start);
        st.cursor = range.end;
    }

    /// Set page selection (page text drag).
    pub fn begin_page_selection(&mut self, anchor: (f32, f32)) {
        self.page_selection = Some(PageSelection {
            anchor,
            current: anchor,
            dragging: true,
        });
    }

    pub fn update_page_selection(&mut self, current: (f32, f32)) {
        if let Some(s) = &mut self.page_selection {
            if s.dragging { s.current = current; }
        }
    }

    pub fn end_page_selection(&mut self) {
        if let Some(s) = &mut self.page_selection {
            s.dragging = false;
            // Drop pri zero-extent selection.
            if (s.current.0 - s.anchor.0).abs() < 3.0 && (s.current.1 - s.anchor.1).abs() < 3.0 {
                self.page_selection = None;
            }
        }
    }

    pub fn clear_page_selection(&mut self) {
        self.page_selection = None;
    }
}
