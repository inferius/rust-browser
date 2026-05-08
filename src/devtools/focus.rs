//! Focus dispatcher - urci kdo dostane char/key events.
//!
//! Page form input + DevTools console + various overlays sdileji keyboard.
//! `FocusTarget` rozhoduje. Page form input dostane char eventy POUZE pri Page focus
//! - resi se duplicitnimu typing pri Console fokus.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    /// Page render area - form inputs, contenteditable, focused element atd.
    Page,
    /// DevTools console input.
    DevToolsConsole,
    /// Elements search bar.
    DevToolsElementsSearch,
    /// Sources editor (uprava skriptu).
    DevToolsSourcesEditor,
    /// Sources file filter.
    DevToolsSourcesFilter,
    /// Address bar (Ctrl+L).
    AddressBar,
    /// Find on page overlay (Ctrl+F).
    FindOverlay,
    /// Context menu otevren - dosta navigation events (Up/Down/Enter/Esc).
    ContextMenu,
}

impl FocusTarget {
    pub fn is_devtools(self) -> bool {
        matches!(self,
            FocusTarget::DevToolsConsole
            | FocusTarget::DevToolsElementsSearch
            | FocusTarget::DevToolsSourcesEditor
            | FocusTarget::DevToolsSourcesFilter
        )
    }

    pub fn is_text_input(self) -> bool {
        matches!(self,
            FocusTarget::DevToolsConsole
            | FocusTarget::DevToolsElementsSearch
            | FocusTarget::DevToolsSourcesEditor
            | FocusTarget::DevToolsSourcesFilter
            | FocusTarget::AddressBar
            | FocusTarget::FindOverlay
        )
    }
}
