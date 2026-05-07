//! HTML serialization (innerHTML / outerHTML).
//! Extrahovano z mod.rs (Iter 267 refactor).

use std::rc::Rc;

pub(crate) fn serialize_inner_html(node: &Rc<crate::browser::dom::NodeData>) -> String {
    use crate::browser::dom::NodeKind;
    let mut out = String::new();
    for child in node.children.borrow().iter() {
        match &child.kind {
            NodeKind::Element(_) => out.push_str(&serialize_outer_html(child)),
            NodeKind::Text(t) => out.push_str(t),
            NodeKind::Comment(c) => { out.push_str("<!--"); out.push_str(c); out.push_str("-->"); }
            _ => {}
        }
    }
    out
}

pub(crate) fn serialize_outer_html(node: &Rc<crate::browser::dom::NodeData>) -> String {
    use crate::browser::dom::NodeKind;
    match &node.kind {
        NodeKind::Element(_) => {
            let tag = node.tag_name().unwrap_or_default();
            let mut out = format!("<{tag}");
            for (k, v) in node.attributes.borrow().iter() {
                out.push_str(&format!(" {k}=\"{v}\""));
            }
            out.push('>');
            out.push_str(&serialize_inner_html(node));
            // Self-closing tagy bez end tag
            if !matches!(tag.as_str(),
                "br" | "img" | "input" | "hr" | "meta" | "link" | "area" | "base"
                | "col" | "embed" | "source" | "track" | "wbr") {
                out.push_str(&format!("</{tag}>"));
            }
            out
        }
        NodeKind::Text(t) => t.clone(),
        _ => String::new(),
    }
}
