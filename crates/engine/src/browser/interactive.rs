//! Klasifikace interactive HTML elementu pro centralni dispatch click/hover/key.
//!
//! Navrh phase 7 z planu unifikace: misto ad-hoc match na `tag` v 6+ mistech
//! (handle_click, compute_cursor_icon, focus dispatch, form submit, ...) se
//! kazdy element prevede na `InteractiveElement` enum a kazdy handler dispatchuje
//! per varianta. Aktualne pouziva `compute_cursor_icon`; handle_click migrace
//! je inkrementalni (next session).

use std::rc::Rc;
use crate::browser::dom::NodeData;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractiveKind {
    Link,            // <a href>
    Button,          // <button>, <input type=submit/reset/button>
    Checkbox,        // <input type=checkbox>
    Radio,           // <input type=radio>
    TextInput,       // <input type=text/...>, <textarea>, <input> bez type
    Select,          // <select>
    Option,          // <option>
    Label,           // <label>
    Summary,         // <details>/<summary>
    None,
}

pub fn classify(node: &Rc<NodeData>) -> InteractiveKind {
    let tag = node.tag_name();
    let tag_str = tag.as_deref().unwrap_or("");
    match tag_str {
        "a" => {
            if node.attr("href").is_some() { InteractiveKind::Link }
            else { InteractiveKind::None }
        }
        "button" => InteractiveKind::Button,
        "input" => {
            let t = node.attr("type").unwrap_or_else(|| "text".to_string());
            match t.to_lowercase().as_str() {
                "submit" | "reset" | "button" | "image" => InteractiveKind::Button,
                "checkbox" => InteractiveKind::Checkbox,
                "radio" => InteractiveKind::Radio,
                "text" | "password" | "email" | "url" | "tel" | "search"
                    | "number" | "date" | "datetime-local" | "month" | "week"
                    | "time" | "color" | "" => InteractiveKind::TextInput,
                _ => InteractiveKind::TextInput,
            }
        }
        "textarea" => InteractiveKind::TextInput,
        "select" => InteractiveKind::Select,
        "option" => InteractiveKind::Option,
        "label" => InteractiveKind::Label,
        "summary" => InteractiveKind::Summary,
        _ => InteractiveKind::None,
    }
}

impl InteractiveKind {
    /// Cursor icon pro hover nad timto kindem. Mirror brower-typical UX.
    pub fn cursor_icon(self) -> winit::window::CursorIcon {
        use winit::window::CursorIcon;
        match self {
            InteractiveKind::Link | InteractiveKind::Button | InteractiveKind::Checkbox
                | InteractiveKind::Radio | InteractiveKind::Select | InteractiveKind::Option
                | InteractiveKind::Label | InteractiveKind::Summary => CursorIcon::Pointer,
            InteractiveKind::TextInput => CursorIcon::Text,
            InteractiveKind::None => CursorIcon::Default,
        }
    }
    pub fn is_focusable(self) -> bool {
        !matches!(self, InteractiveKind::None | InteractiveKind::Option | InteractiveKind::Label)
    }
    pub fn accepts_text(self) -> bool {
        matches!(self, InteractiveKind::TextInput)
    }
}
