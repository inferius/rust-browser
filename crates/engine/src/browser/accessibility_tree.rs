//! Accessibility (AX) tree - parallel to DOM, exposed to AT (screen readers etc).
//!
//! Chromium reference: //ui/accessibility/ax_tree.h
//! Firefox reference: accessible/generic/Accessible.h
//!
//! Per HTML AAM (https://www.w3.org/TR/html-aam-1.0/) each HTML element maps to
//! an ARIA role + name/description/state computed via WAI-ARIA AccName 1.2.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AxRole {
    Generic,
    Button,
    Link,
    Heading,
    Paragraph,
    StaticText,
    Image,
    Textbox,
    Checkbox,
    Radio,
    Combobox,
    Listbox,
    Option,
    Menu,
    MenuItem,
    Dialog,
    Alert,
    Region,
    Landmark,
    Navigation,
    Main,
    Banner,
    ContentInfo,
    Form,
    Table,
    Row,
    Cell,
    ColumnHeader,
    RowHeader,
    Tree,
    TreeItem,
    Slider,
    Spinbutton,
    Progressbar,
    Scrollbar,
    Switch,
    Tab,
    TabList,
    TabPanel,
    Toolbar,
    Tooltip,
    Document,
    Article,
    GroupRole,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AxStateBit {
    Focused,
    Selected,
    Checked,
    Expanded,
    Disabled,
    Required,
    Invalid,
    Busy,
    Hidden,
    Multiselectable,
    Modal,
    Readonly,
}

#[derive(Debug, Clone)]
pub struct AxNode {
    pub id: u64,
    pub role: AxRole,
    pub name: String,            // computed accessible name
    pub description: String,
    pub value: Option<String>,
    pub state: u32,              // bitfield of AxStateBit
    pub level: Option<u32>,      // for headings/treeitem
    pub bounds: (f32, f32, f32, f32),
    pub children: Vec<u64>,
    pub parent: Option<u64>,
}

impl AxNode {
    pub fn new(id: u64, role: AxRole) -> Self {
        Self {
            id, role,
            name: String::new(), description: String::new(),
            value: None, state: 0, level: None,
            bounds: (0.0, 0.0, 0.0, 0.0),
            children: Vec::new(), parent: None,
        }
    }

    pub fn set(&mut self, bit: AxStateBit) {
        self.state |= 1u32 << bit as u32;
    }
    pub fn clear(&mut self, bit: AxStateBit) {
        self.state &= !(1u32 << bit as u32);
    }
    pub fn has(&self, bit: AxStateBit) -> bool {
        (self.state & (1u32 << bit as u32)) != 0
    }
}

#[derive(Default)]
pub struct AxTree {
    pub nodes: HashMap<u64, AxNode>,
    pub root: Option<u64>,
}

impl AxTree {
    pub fn new() -> Self { Self::default() }

    pub fn insert(&mut self, node: AxNode) {
        if self.root.is_none() { self.root = Some(node.id); }
        self.nodes.insert(node.id, node);
    }

    pub fn append_child(&mut self, parent: u64, child: u64) -> Result<(), String> {
        if !self.nodes.contains_key(&parent) || !self.nodes.contains_key(&child) {
            return Err("missing node".into());
        }
        self.nodes.get_mut(&parent).unwrap().children.push(child);
        self.nodes.get_mut(&child).unwrap().parent = Some(parent);
        Ok(())
    }

    /// Walk depth-first.
    pub fn traverse(&self) -> Vec<u64> {
        let mut out = Vec::new();
        if let Some(root) = self.root { self.walk(root, &mut out); }
        out
    }

    fn walk(&self, id: u64, out: &mut Vec<u64>) {
        out.push(id);
        if let Some(n) = self.nodes.get(&id) {
            for c in &n.children { self.walk(*c, out); }
        }
    }

    pub fn find_by_role(&self, role: AxRole) -> Vec<&AxNode> {
        self.nodes.values().filter(|n| n.role == role).collect()
    }
}

/// Map HTML tag to default ARIA role per HTML AAM.
pub fn html_tag_to_role(tag: &str) -> AxRole {
    match tag.to_ascii_lowercase().as_str() {
        "button" => AxRole::Button,
        "a" => AxRole::Link,
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => AxRole::Heading,
        "p" => AxRole::Paragraph,
        "img" => AxRole::Image,
        "input" => AxRole::Textbox,
        "select" => AxRole::Combobox,
        "option" => AxRole::Option,
        "dialog" => AxRole::Dialog,
        "nav" => AxRole::Navigation,
        "main" => AxRole::Main,
        "header" => AxRole::Banner,
        "footer" => AxRole::ContentInfo,
        "form" => AxRole::Form,
        "table" => AxRole::Table,
        "tr" => AxRole::Row,
        "td" => AxRole::Cell,
        "th" => AxRole::ColumnHeader,
        "article" => AxRole::Article,
        "section" => AxRole::Region,
        "ul" | "ol" => AxRole::Listbox,
        "li" => AxRole::Option,
        _ => AxRole::Generic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_mapping() {
        assert_eq!(html_tag_to_role("button"), AxRole::Button);
        assert_eq!(html_tag_to_role("h2"), AxRole::Heading);
        assert_eq!(html_tag_to_role("nav"), AxRole::Navigation);
        assert_eq!(html_tag_to_role("xyz"), AxRole::Generic);
    }

    #[test]
    fn insert_first_becomes_root() {
        let mut t = AxTree::new();
        t.insert(AxNode::new(1, AxRole::Generic));
        assert_eq!(t.root, Some(1));
    }

    #[test]
    fn parent_child_link() {
        let mut t = AxTree::new();
        t.insert(AxNode::new(1, AxRole::Document));
        t.insert(AxNode::new(2, AxRole::Heading));
        t.append_child(1, 2).unwrap();
        assert_eq!(t.nodes.get(&2).unwrap().parent, Some(1));
        assert_eq!(t.nodes.get(&1).unwrap().children, vec![2]);
    }

    #[test]
    fn state_bits() {
        let mut n = AxNode::new(1, AxRole::Checkbox);
        n.set(AxStateBit::Checked);
        n.set(AxStateBit::Focused);
        assert!(n.has(AxStateBit::Checked));
        assert!(n.has(AxStateBit::Focused));
        n.clear(AxStateBit::Checked);
        assert!(!n.has(AxStateBit::Checked));
    }

    #[test]
    fn find_by_role() {
        let mut t = AxTree::new();
        t.insert(AxNode::new(1, AxRole::Document));
        t.insert(AxNode::new(2, AxRole::Heading));
        t.insert(AxNode::new(3, AxRole::Heading));
        assert_eq!(t.find_by_role(AxRole::Heading).len(), 2);
    }

    #[test]
    fn traverse_depth_first() {
        let mut t = AxTree::new();
        t.insert(AxNode::new(1, AxRole::Document));
        t.insert(AxNode::new(2, AxRole::Heading));
        t.insert(AxNode::new(3, AxRole::Paragraph));
        t.append_child(1, 2).unwrap();
        t.append_child(1, 3).unwrap();
        let order = t.traverse();
        assert_eq!(order, vec![1, 2, 3]);
    }
}
