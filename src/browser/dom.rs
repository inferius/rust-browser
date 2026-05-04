/// DOM tree pro browser engine.
///
/// Strom uzlu kde kazdy uzel ma typ + parent + children.
/// Pouziva `Rc<RefCell<NodeData>>` pro sdilene mutable references.
/// Parent je Weak aby nedoslo k cyklum (children > parent).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

/// Typ DOM uzlu.
#[derive(Debug, Clone)]
pub enum NodeKind {
    /// Document - korenovy uzel
    Document,
    /// Element s tagem a atributy: <div id="x">
    Element {
        tag: String,
        attributes: HashMap<String, String>,
    },
    /// Textovy uzel (mezi tagy)
    Text(String),
    /// Komentar: <!-- ... -->
    Comment(String),
    /// CDATA section (XHTML/XML)
    Cdata(String),
    /// DOCTYPE deklarace
    DocType(String),
}

/// DOM uzel - public node ID a data.
#[derive(Debug)]
pub struct NodeData {
    pub kind: NodeKind,
    pub parent: RefCell<Weak<Node>>,
    pub children: RefCell<Vec<Rc<Node>>>,
}

pub type Node = NodeData;

impl NodeData {
    pub fn new_document() -> Rc<Self> {
        Rc::new(NodeData {
            kind: NodeKind::Document,
            parent: RefCell::new(Weak::new()),
            children: RefCell::new(Vec::new()),
        })
    }

    pub fn new_element(tag: &str, attributes: HashMap<String, String>) -> Rc<Self> {
        Rc::new(NodeData {
            kind: NodeKind::Element {
                tag: tag.to_lowercase(),
                attributes,
            },
            parent: RefCell::new(Weak::new()),
            children: RefCell::new(Vec::new()),
        })
    }

    pub fn new_text(content: &str) -> Rc<Self> {
        Rc::new(NodeData {
            kind: NodeKind::Text(content.to_string()),
            parent: RefCell::new(Weak::new()),
            children: RefCell::new(Vec::new()),
        })
    }

    pub fn new_comment(content: &str) -> Rc<Self> {
        Rc::new(NodeData {
            kind: NodeKind::Comment(content.to_string()),
            parent: RefCell::new(Weak::new()),
            children: RefCell::new(Vec::new()),
        })
    }

    /// Pripoj dite jako posledni a nastav parent.
    pub fn append_child(self: &Rc<Self>, child: Rc<Node>) {
        *child.parent.borrow_mut() = Rc::downgrade(self);
        self.children.borrow_mut().push(child);
    }

    /// Vrati tag (lowercase) pokud je element.
    pub fn tag_name(&self) -> Option<String> {
        if let NodeKind::Element { tag, .. } = &self.kind {
            Some(tag.clone())
        } else {
            None
        }
    }

    /// Vrati hodnotu atributu (pokud existuje).
    pub fn attr(&self, name: &str) -> Option<String> {
        if let NodeKind::Element { attributes, .. } = &self.kind {
            attributes.get(name).cloned()
        } else {
            None
        }
    }

    /// Pretvori DOM podstrom na text content (jen Text uzly).
    pub fn text_content(&self) -> String {
        let mut out = String::new();
        self.collect_text(&mut out);
        out
    }

    fn collect_text(&self, out: &mut String) {
        if let NodeKind::Text(t) = &self.kind {
            out.push_str(t);
        }
        for ch in self.children.borrow().iter() {
            ch.collect_text(out);
        }
    }

    /// Walk preorder - vola cb pro kazdy uzel.
    pub fn walk(self: &Rc<Self>, cb: &mut dyn FnMut(&Rc<Node>)) {
        cb(self);
        for ch in self.children.borrow().iter() {
            ch.walk(cb);
        }
    }

    /// Najde prvni element ktery vyhovuje predikatu.
    /// Pouziva &dyn Fn aby se vyhnulo nekonecne monomorfizaci.
    pub fn find<F: Fn(&Rc<Node>) -> bool>(self: &Rc<Self>, pred: F) -> Option<Rc<Node>> {
        self.find_inner(&pred)
    }

    fn find_inner(self: &Rc<Self>, pred: &dyn Fn(&Rc<Node>) -> bool) -> Option<Rc<Node>> {
        if pred(self) { return Some(Rc::clone(self)); }
        for ch in self.children.borrow().iter() {
            if let Some(found) = ch.find_inner(pred) { return Some(found); }
        }
        None
    }

    /// getElementById - hledej v podstrome
    pub fn get_element_by_id(self: &Rc<Self>, id: &str) -> Option<Rc<Node>> {
        self.find(|n| n.attr("id").as_deref() == Some(id))
    }

    /// getElementsByTagName
    pub fn get_elements_by_tag(self: &Rc<Self>, tag: &str) -> Vec<Rc<Node>> {
        let tag_lower = tag.to_lowercase();
        let mut out = Vec::new();
        let collect = |n: &Rc<Node>, out: &mut Vec<Rc<Node>>| {
            if n.tag_name().as_deref() == Some(&tag_lower) {
                out.push(Rc::clone(n));
            }
        };
        let mut accumulator = Vec::new();
        self.walk(&mut |n| collect(n, &mut accumulator));
        out.extend(accumulator);
        out
    }

    /// getElementsByClassName
    pub fn get_elements_by_class(self: &Rc<Self>, class: &str) -> Vec<Rc<Node>> {
        let mut accumulator = Vec::new();
        self.walk(&mut |n| {
            if let Some(cls) = n.attr("class") {
                if cls.split_whitespace().any(|c| c == class) {
                    accumulator.push(Rc::clone(n));
                }
            }
        });
        accumulator
    }
}

/// Document - korenovy DOM container.
pub struct Document {
    pub root: Rc<Node>,
    pub url: String,
    pub title: String,
}

impl Document {
    pub fn new(url: String) -> Self {
        Document {
            root: NodeData::new_document(),
            url,
            title: String::new(),
        }
    }

    /// Vrati html element (prvni <html>).
    pub fn html_element(&self) -> Option<Rc<Node>> {
        self.root.find(|n| n.tag_name().as_deref() == Some("html"))
    }

    pub fn body(&self) -> Option<Rc<Node>> {
        self.root.find(|n| n.tag_name().as_deref() == Some("body"))
    }

    pub fn head(&self) -> Option<Rc<Node>> {
        self.root.find(|n| n.tag_name().as_deref() == Some("head"))
    }
}
