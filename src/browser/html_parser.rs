/// HTML5 parser - obal nad html5ever crate.
///
/// Parsuje HTML5 source na nas DOM strom (browser::dom::Document).
/// Pouziva markup5ever_rcdom jako interni reprezentaci a pak konvertuje.

use std::collections::HashMap;
use std::rc::Rc;
use html5ever::tendril::TendrilSink;
use html5ever::{parse_document, parse_fragment};
use html5ever::driver::ParseOpts;
use html5ever::tree_builder::TreeBuilderOpts;
use html5ever::{namespace_url, ns, local_name, QualName};
use markup5ever_rcdom::{RcDom, NodeData as RcNodeData, Handle};

use super::dom::{Document, Node, NodeData};

/// Parsuje HTML source na Document.
pub fn parse_html(source: &str, url: &str) -> Document {
    let opts = ParseOpts {
        tree_builder: TreeBuilderOpts {
            drop_doctype: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let dom = parse_document(RcDom::default(), opts)
        .from_utf8()
        .read_from(&mut source.as_bytes())
        .unwrap();

    let mut document = Document::empty(url.to_string());
    convert_handle(&dom.document, &document.root);

    // Extrakce title
    if let Some(title_el) = document.root.find(|n| n.tag_name().as_deref() == Some("title")) {
        document.title = title_el.text_content().trim().to_string();
    }

    document
}

/// Parsuje HTML fragment (bez <html><body> wrapperu).
pub fn parse_html_fragment(source: &str) -> Rc<Node> {
    let opts = ParseOpts::default();
    let context = QualName::new(None, ns!(html), local_name!("body"));
    let dom = parse_fragment(RcDom::default(), opts, context, vec![])
        .from_utf8()
        .read_from(&mut source.as_bytes())
        .unwrap();

    let root = NodeData::new_document();
    convert_handle(&dom.document, &root);
    root
}

/// Konvertuje markup5ever Handle na nas Node strom (rekurzivne).
fn convert_handle(handle: &Handle, parent: &Rc<Node>) {
    for child in handle.children.borrow().iter() {
        let node_opt: Option<Rc<Node>> = match &child.data {
            RcNodeData::Element { name, attrs, .. } => {
                let tag = name.local.to_string();
                let mut attributes = HashMap::new();
                for attr in attrs.borrow().iter() {
                    attributes.insert(attr.name.local.to_string(), attr.value.to_string());
                }
                Some(NodeData::new_element(&tag, attributes))
            }
            RcNodeData::Text { contents } => {
                let text = contents.borrow().to_string();
                if !text.is_empty() {
                    Some(NodeData::new_text(&text))
                } else {
                    None
                }
            }
            RcNodeData::Comment { contents } => {
                Some(NodeData::new_comment(&contents.to_string()))
            }
            RcNodeData::Doctype { name, .. } => {
                Some(Rc::new(NodeData {
                    kind: super::dom::NodeKind::DocType(name.to_string()),
                    attributes: std::cell::RefCell::new(HashMap::new()),
                    parent: std::cell::RefCell::new(std::rc::Weak::new()),
                    children: std::cell::RefCell::new(Vec::new()),
                    listeners: std::cell::RefCell::new(HashMap::new()),
                }))
            }
            RcNodeData::Document => {
                // Top-level document - jen recursi do children (uvnitr stacker grow).
                stacker::maybe_grow(32 * 1024, 8 * 1024 * 1024, || {
                    convert_handle(child, parent);
                });
                None
            }
            _ => None,
        };

        if let Some(node) = node_opt {
            parent.append_child(Rc::clone(&node));
            // Auto-grow stack pro deep DOM nesting.
            stacker::maybe_grow(32 * 1024, 8 * 1024 * 1024, || {
                convert_handle(child, &node);
            });
        } else if matches!(child.data, RcNodeData::Document) {
            // Document case uz volana convert_handle vyse - mit ji v stacker chain.
        }
    }
}

/// Pretty-print DOM strom (pro debugging).
pub fn dump_tree(node: &Rc<Node>, depth: usize) -> String {
    use super::dom::NodeKind;
    let indent = "  ".repeat(depth);
    let mut out = String::new();
    match &node.kind {
        NodeKind::Document => out.push_str(&format!("{indent}#document\n")),
        NodeKind::Element(tag) => {
            let attrs: Vec<String> = node.attributes.borrow().iter()
                .map(|(k, v)| format!(" {k}=\"{v}\""))
                .collect();
            out.push_str(&format!("{indent}<{tag}{}>\n", attrs.join("")));
        }
        NodeKind::Text(t) => {
            let trimmed = t.trim();
            if !trimmed.is_empty() {
                out.push_str(&format!("{indent}\"{}\"\n", trimmed));
            }
        }
        NodeKind::Comment(c) => out.push_str(&format!("{indent}<!--{c}-->\n")),
        NodeKind::Cdata(c)   => out.push_str(&format!("{indent}<![CDATA[{c}]]>\n")),
        NodeKind::DocType(n) => out.push_str(&format!("{indent}<!DOCTYPE {n}>\n")),
    }
    for ch in node.children.borrow().iter() {
        out.push_str(&dump_tree(ch, depth + 1));
    }
    out
}
