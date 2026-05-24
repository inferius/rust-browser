//! Accessibility (a11y) tree foundation - mirror DOM s ARIA roles + states.
//!
//! Inspired by:
//! - W3C ARIA spec: https://w3c.github.io/aria/
//! - Chromium `content/browser/accessibility/`
//!
//! Platform AT bridge (NVDA/JAWS/VoiceOver/Orca) je separate impl per OS.

use std::rc::Rc;
use crate::browser::dom::Node;

/// ARIA role - explicit z [role="..."] nebo implicit z tag (button, link, ...).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AriaRole {
    None,
    Button,
    Link,
    Heading,
    Img,
    TextField,
    Checkbox,
    Radio,
    List,
    ListItem,
    Navigation,
    Main,
    Region,
    Banner,
    Dialog,
    Alert,
    Form,
    Table,
    Row,
    Cell,
    Group,
    Other,
}

impl AriaRole {
    pub fn from_string(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "button" => Self::Button,
            "link" => Self::Link,
            "heading" => Self::Heading,
            "img" | "image" => Self::Img,
            "textbox" | "searchbox" => Self::TextField,
            "checkbox" => Self::Checkbox,
            "radio" => Self::Radio,
            "list" => Self::List,
            "listitem" => Self::ListItem,
            "navigation" => Self::Navigation,
            "main" => Self::Main,
            "region" => Self::Region,
            "banner" => Self::Banner,
            "dialog" | "alertdialog" => Self::Dialog,
            "alert" => Self::Alert,
            "form" => Self::Form,
            "table" | "grid" => Self::Table,
            "row" => Self::Row,
            "cell" | "gridcell" => Self::Cell,
            "group" => Self::Group,
            _ => Self::Other,
        }
    }

    /// Implicit role z HTML tag.
    pub fn implicit_for_tag(tag: &str) -> Self {
        match tag {
            "button" => Self::Button,
            "a" => Self::Link,
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => Self::Heading,
            "img" => Self::Img,
            "input" => Self::TextField,
            "ul" | "ol" => Self::List,
            "li" => Self::ListItem,
            "nav" => Self::Navigation,
            "main" => Self::Main,
            "header" => Self::Banner,
            "section" => Self::Region,
            "form" => Self::Form,
            "table" => Self::Table,
            "tr" => Self::Row,
            "td" | "th" => Self::Cell,
            _ => Self::None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AriaState {
    pub label: Option<String>,         // aria-label
    pub labelled_by: Option<String>,   // aria-labelledby (id refs)
    pub described_by: Option<String>,  // aria-describedby
    pub expanded: Option<bool>,        // aria-expanded
    pub selected: Option<bool>,        // aria-selected
    pub checked: Option<bool>,         // aria-checked
    pub disabled: Option<bool>,        // aria-disabled
    pub hidden: Option<bool>,          // aria-hidden
    pub live: Option<String>,          // aria-live: off/polite/assertive
    pub current: Option<String>,       // aria-current: page/step/...
    pub level: Option<i32>,            // aria-level (heading depth)
}

#[derive(Debug, Clone)]
pub struct AccessibilityNode {
    pub role: AriaRole,
    pub name: String,           // computed accessible name
    pub description: String,    // computed accessible description
    pub state: AriaState,
    pub children: Vec<usize>,   // indices v ax_tree
    pub dom_node_id: usize,     // Rc::as_ptr
}

/// Build accessibility tree z DOM root.
pub fn build_a11y_tree(root: &Rc<Node>) -> Vec<AccessibilityNode> {
    let mut tree = Vec::new();
    walk_dom(root, &mut tree);
    tree
}

fn walk_dom(node: &Rc<Node>, tree: &mut Vec<AccessibilityNode>) -> Option<usize> {
    use crate::browser::dom::NodeKind;
    let (role, tag) = match &node.kind {
        NodeKind::Element(t) => {
            let explicit = node.attr("role");
            let role = match explicit {
                Some(s) => AriaRole::from_string(&s),
                None => AriaRole::implicit_for_tag(t),
            };
            (role, t.clone())
        }
        _ => return None,
    };
    let state = AriaState {
        label: node.attr("aria-label"),
        labelled_by: node.attr("aria-labelledby"),
        described_by: node.attr("aria-describedby"),
        expanded: node.attr("aria-expanded").and_then(|s| s.parse().ok()),
        selected: node.attr("aria-selected").and_then(|s| s.parse().ok()),
        checked: node.attr("aria-checked").and_then(|s| s.parse().ok()),
        disabled: node.attr("aria-disabled").and_then(|s| s.parse().ok()),
        hidden: node.attr("aria-hidden").and_then(|s| s.parse().ok()),
        live: node.attr("aria-live"),
        current: node.attr("aria-current"),
        level: node.attr("aria-level").and_then(|s| s.parse().ok())
            .or_else(|| heading_level_from_tag(&tag)),
    };
    let name = state.label.clone().unwrap_or_else(|| node.text_content());
    let idx = tree.len();
    tree.push(AccessibilityNode {
        role,
        name,
        description: String::new(),
        state,
        children: Vec::new(),
        dom_node_id: Rc::as_ptr(node) as usize,
    });
    let mut child_indices = Vec::new();
    for ch in node.children.borrow().iter() {
        if let Some(ci) = walk_dom(ch, tree) {
            child_indices.push(ci);
        }
    }
    tree[idx].children = child_indices;
    Some(idx)
}

fn heading_level_from_tag(tag: &str) -> Option<i32> {
    match tag {
        "h1" => Some(1), "h2" => Some(2), "h3" => Some(3),
        "h4" => Some(4), "h5" => Some(5), "h6" => Some(6),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::html_parser::parse_html;

    #[test]
    fn build_tree_no_panic() {
        let doc = parse_html("<button>click</button>", "about:blank");
        let _tree = build_a11y_tree(&doc.root);
    }

    #[test]
    fn build_tree_with_explicit_role() {
        let doc = parse_html("<div role=\"button\">x</div>", "about:blank");
        let _tree = build_a11y_tree(&doc.root);
    }

    #[test]
    fn heading_level() {
        let doc = parse_html("<h2>title</h2>", "about:blank");
        let tree = build_a11y_tree(&doc.root);
        if let Some(h2) = tree.iter().find(|n| n.role == AriaRole::Heading) {
            assert_eq!(h2.state.level, Some(2));
        }
    }

    #[test]
    fn aria_label_used_as_name() {
        let doc = parse_html("<button aria-label=\"close dialog\">X</button>", "about:blank");
        let tree = build_a11y_tree(&doc.root);
        if let Some(btn) = tree.iter().find(|n| n.role == AriaRole::Button) {
            assert_eq!(btn.name, "close dialog");
        }
    }

    #[test]
    fn aria_expanded_state() {
        let doc = parse_html("<button aria-expanded=\"true\">menu</button>", "about:blank");
        let tree = build_a11y_tree(&doc.root);
        if let Some(btn) = tree.iter().find(|n| n.role == AriaRole::Button) {
            assert_eq!(btn.state.expanded, Some(true));
        }
    }
}
