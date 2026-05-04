/// DOM tree pro browser engine.
///
/// Strom uzlu kde kazdy uzel ma typ + parent + children.
/// Pouziva `Rc<RefCell<NodeData>>` pro sdilene mutable references.
/// Parent je Weak aby nedoslo k cyklum (children > parent).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

/// Typ DOM uzlu.
/// Attributes jsou na NodeData (RefCell) aby byly mutable.
#[derive(Debug, Clone)]
pub enum NodeKind {
    /// Document - korenovy uzel
    Document,
    /// Element s tagem (attributes na NodeData)
    Element(String),
    /// Textovy uzel
    Text(String),
    /// Komentar
    Comment(String),
    /// CDATA section
    Cdata(String),
    /// DOCTYPE deklarace
    DocType(String),
}

/// DOM uzel.
#[derive(Debug)]
pub struct NodeData {
    pub kind: NodeKind,
    pub attributes: RefCell<HashMap<String, String>>,
    pub parent: RefCell<Weak<Node>>,
    pub children: RefCell<Vec<Rc<Node>>>,
    /// Listeners: event_type -> Vec<callback> (callback je opaque pres usize id)
    pub listeners: RefCell<HashMap<String, Vec<usize>>>,
}

pub type Node = NodeData;

impl NodeData {
    pub fn new_document() -> Rc<Self> {
        Rc::new(NodeData {
            kind: NodeKind::Document,
            attributes: RefCell::new(HashMap::new()),
            parent: RefCell::new(Weak::new()),
            children: RefCell::new(Vec::new()),
            listeners: RefCell::new(HashMap::new()),
        })
    }

    pub fn new_element(tag: &str, attributes: HashMap<String, String>) -> Rc<Self> {
        Rc::new(NodeData {
            kind: NodeKind::Element(tag.to_lowercase()),
            attributes: RefCell::new(attributes),
            parent: RefCell::new(Weak::new()),
            children: RefCell::new(Vec::new()),
            listeners: RefCell::new(HashMap::new()),
        })
    }

    pub fn new_text(content: &str) -> Rc<Self> {
        Rc::new(NodeData {
            kind: NodeKind::Text(content.to_string()),
            attributes: RefCell::new(HashMap::new()),
            parent: RefCell::new(Weak::new()),
            children: RefCell::new(Vec::new()),
            listeners: RefCell::new(HashMap::new()),
        })
    }

    pub fn new_comment(content: &str) -> Rc<Self> {
        Rc::new(NodeData {
            kind: NodeKind::Comment(content.to_string()),
            attributes: RefCell::new(HashMap::new()),
            parent: RefCell::new(Weak::new()),
            children: RefCell::new(Vec::new()),
            listeners: RefCell::new(HashMap::new()),
        })
    }

    /// Pripoj dite jako posledni a nastav parent.
    pub fn append_child(self: &Rc<Self>, child: Rc<Node>) {
        *child.parent.borrow_mut() = Rc::downgrade(self);
        self.children.borrow_mut().push(child);
    }

    /// Vrati tag (lowercase) pokud je element.
    pub fn tag_name(&self) -> Option<String> {
        if let NodeKind::Element(tag) = &self.kind {
            Some(tag.clone())
        } else {
            None
        }
    }

    /// Vrati hodnotu atributu (pokud existuje).
    pub fn attr(&self, name: &str) -> Option<String> {
        self.attributes.borrow().get(name).cloned()
    }

    /// Nastavi atribut.
    pub fn set_attr(&self, name: &str, value: &str) {
        self.attributes.borrow_mut().insert(name.to_string(), value.to_string());
    }

    /// Smaze atribut.
    pub fn remove_attr(&self, name: &str) {
        self.attributes.borrow_mut().remove(name);
    }

    /// Kontroluje pritomnost atributu.
    pub fn has_attr(&self, name: &str) -> bool {
        self.attributes.borrow().contains_key(name)
    }

    /// Nastavi text content - smazne deti, vlozi jeden Text node.
    pub fn set_text_content(self: &Rc<Self>, text: &str) {
        self.children.borrow_mut().clear();
        if !text.is_empty() {
            self.append_child(NodeData::new_text(text));
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
        // Default: <document> -> <html> -> <head>, <body>
        let root = NodeData::new_document();
        let html = NodeData::new_element("html", HashMap::new());
        let head = NodeData::new_element("head", HashMap::new());
        let body = NodeData::new_element("body", HashMap::new());
        html.append_child(head);
        html.append_child(body);
        root.append_child(html);
        Document {
            root,
            url,
            title: String::new(),
        }
    }

    /// Vytvori prazdny dokument bez html/head/body (pro testy parseru).
    pub fn empty(url: String) -> Self {
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
