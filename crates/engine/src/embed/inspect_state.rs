//! Shared inspector state mezi shell + WebViews (= page + devtools).
//!
//! Pres CDP `Overlay` domain (Chrome inspirace):
//! - Devtools tree hover -> `Overlay.highlightNode { nodeId }` -> page WV
//!   overlay_painter draws box-model highlight.
//! - Picker mode (devtools toolbar button) -> `Overlay.setInspectMode` -> shell
//!   intercepts cursor over page area -> hit-test -> emit
//!   `Overlay.inspectNodeRequested` event up to devtools.
//!
//! Architecture: shell owns `Rc<RefCell<InspectState>>` + shares pres
//! DevtoolsTarget (= devtools WV) + pres page WV via set_overlay_painter
//! callback.

use std::cell::RefCell;
use std::rc::Rc;

/// Shared inspector state. Read/write across all 3 WebViews + shell.
#[derive(Debug, Clone)]
pub struct InspectState {
    /// Node currently highlighted v page (= devtools tree hover OR picker
    /// hover). Stored as Rc::as_ptr usize (= matches LayoutBox.node id).
    /// None = no highlight.
    pub hovered_node: Option<usize>,
    /// Currently selected node v devtools elements panel. Persistent
    /// (vs hovered = transient).
    pub selected_node: Option<usize>,
    /// Picker mode active = page cursor hit-test -> highlight + click to select.
    /// Toggled pres `Overlay.setInspectMode { mode: "searchForNode" }`.
    pub picker_active: bool,
    /// Highlight rendering options - color overrides per box-model layer.
    pub highlight_options: HighlightOptions,
}

#[derive(Debug, Clone, Copy)]
pub struct HighlightOptions {
    pub content_color: [u8; 4],
    pub padding_color: [u8; 4],
    pub border_color: [u8; 4],
    pub margin_color: [u8; 4],
    pub show_info: bool,  // bbox label s element tag + dims
}

impl Default for HighlightOptions {
    fn default() -> Self {
        Self {
            // Chrome-style colors (= z chromium DevTools_Overlay defaults).
            content_color: [111, 168, 220, 102],  // blue, 40% alpha
            padding_color: [147, 196, 125, 102],  // green
            border_color: [255, 229, 153, 102],   // yellow
            margin_color: [246, 178, 107, 102],   // orange
            show_info: false,
        }
    }
}

impl InspectState {
    pub fn new() -> Self {
        Self {
            hovered_node: None,
            selected_node: None,
            picker_active: false,
            highlight_options: HighlightOptions::default(),
        }
    }

    pub fn shared() -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self::new()))
    }
}

impl Default for InspectState {
    fn default() -> Self { Self::new() }
}
