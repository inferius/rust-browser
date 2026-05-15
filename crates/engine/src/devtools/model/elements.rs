//! Elements panel data model: ElementRow + tree builder + flatten s collapse stavem.

use std::rc::Rc;
use std::collections::HashSet;
use crate::browser::dom::{NodeData, NodeKind};

#[derive(Debug, Clone)]
pub enum RowKind {
    Document,
    Element {
        tag: String,
        attrs: Vec<(String, String)>,
        self_closing: bool,
        has_children: bool,
        /// Pri collapsed s pouhym text-child a kratkym textem (<INLINE_LIMIT chars)
        /// Some(text) -> render `<tag>text</tag>` inline misto `<tag>...</tag>`.
        /// Firefox-style summary preview.
        inline_text: Option<String>,
    },
    Text(String),
    Comment(String),
    DocType(String),
    Cdata(String),
    /// Closing tag radek pro expanded element (zobrazuje se za children).
    CloseTag(String),
}

/// Limit pro inline text preview pri collapsed elementu.
const INLINE_PREVIEW_LIMIT: usize = 60;

#[derive(Debug, Clone)]
pub struct ElementRow {
    pub depth: usize,
    pub node_id: usize,
    pub kind: RowKind,
}

/// Flatten DOM strom do plocheho vektoru radku, respektujici collapsed stav.
/// Pri collapsed elementu pridame jen open tag radek (close tag preskocime).
/// Pri expanded pridame open + children + close tag.
/// Text nodes se zobrazi jako citelne text radky.
pub fn build_rows(
    root: &Rc<NodeData>,
    collapsed: &HashSet<usize>,
) -> Vec<ElementRow> {
    let mut out = Vec::new();
    walk(root, 0, collapsed, &mut out);
    out
}

fn walk(
    node: &Rc<NodeData>,
    depth: usize,
    collapsed: &HashSet<usize>,
    out: &mut Vec<ElementRow>,
) {
    let id = Rc::as_ptr(node) as usize;
    match &node.kind {
        NodeKind::Document => {
            out.push(ElementRow { depth, node_id: id, kind: RowKind::Document });
            for ch in node.children.borrow().iter() {
                walk(ch, depth + 1, collapsed, out);
            }
        }
        NodeKind::Element(tag) => {
            let attrs: Vec<(String, String)> = node.attributes.borrow().iter()
                .filter(|(k, _)| !k.is_empty())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let kids = node.children.borrow();
            let has_children = !kids.is_empty();
            // Self-closing pro void elements (br/img/input/...) i kdyz NodeKind je Element.
            let self_closing = !has_children && is_void_element(tag);
            let is_collapsed = collapsed.contains(&id);
            // Inline text preview: vsichni children = text, suma <= INLINE_PREVIEW_LIMIT.
            let inline_text: Option<String> = if has_children {
                let only_text: Vec<String> = kids.iter().filter_map(|ch| match &ch.kind {
                    NodeKind::Text(t) => Some(t.trim().to_string()),
                    _ => None,
                }).collect();
                if only_text.len() == kids.len() {
                    let combined = only_text.join("");
                    if combined.chars().count() <= INLINE_PREVIEW_LIMIT && !combined.is_empty() {
                        Some(combined)
                    } else { None }
                } else { None }
            } else { None };
            out.push(ElementRow {
                depth,
                node_id: id,
                kind: RowKind::Element {
                    tag: tag.clone(),
                    attrs,
                    self_closing,
                    has_children,
                    inline_text,
                },
            });
            if has_children && !is_collapsed {
                for ch in kids.iter() {
                    walk(ch, depth + 1, collapsed, out);
                }
                out.push(ElementRow {
                    depth,
                    node_id: id,
                    kind: RowKind::CloseTag(tag.clone()),
                });
            }
        }
        NodeKind::Text(t) => {
            // Skip text nodes uvnitr <script>/<style>/<noscript>/<template>.
            // Real JS / CSS source bloky maji tisice chars, devtools elements
            // tree by je vykreslil jako one giant line co overflow do styles
            // pane (uzivatel reportoval "div overflow skript").
            let parent_tag = node.parent.borrow().upgrade()
                .and_then(|p| match &p.kind {
                    NodeKind::Element(t) => Some(t.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            if matches!(parent_tag.as_str(), "script" | "style" | "noscript" | "template") {
                return;
            }
            let trimmed = t.trim();
            if !trimmed.is_empty() {
                // Truncate na 80 chars (drive 200) - pri row width ~400 px
                // jsou cca 50 znaku.
                let truncated = if trimmed.chars().count() > 80 {
                    let s: String = trimmed.chars().take(80).collect();
                    format!("{}...", s)
                } else {
                    trimmed.to_string()
                };
                out.push(ElementRow {
                    depth,
                    node_id: id,
                    kind: RowKind::Text(truncated),
                });
            }
        }
        NodeKind::Comment(c) => {
            out.push(ElementRow {
                depth,
                node_id: id,
                kind: RowKind::Comment(c.clone()),
            });
        }
        NodeKind::DocType(n) => {
            out.push(ElementRow {
                depth,
                node_id: id,
                kind: RowKind::DocType(n.clone()),
            });
        }
        NodeKind::Cdata(c) => {
            out.push(ElementRow {
                depth,
                node_id: id,
                kind: RowKind::Cdata(c.clone()),
            });
        }
        NodeKind::DocumentFragment => {
            // DocumentFragment je transparentni - nezobrazujeme self, jen deti.
            for ch in node.children.borrow().iter() {
                walk(ch, depth, collapsed, out);
            }
        }
    }
}

fn is_void_element(tag: &str) -> bool {
    matches!(tag.to_ascii_lowercase().as_str(),
        "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | "input" |
        "link" | "meta" | "param" | "source" | "track" | "wbr")
}

/// Najdi node v DOM stromu podle ptr id. Vraci klon Rc.
pub fn find_node_by_id(root: &Rc<NodeData>, target_id: usize) -> Option<Rc<NodeData>> {
    let id = Rc::as_ptr(root) as usize;
    if id == target_id { return Some(Rc::clone(root)); }
    for ch in root.children.borrow().iter() {
        if let Some(found) = find_node_by_id(ch, target_id) {
            return Some(found);
        }
    }
    None
}
