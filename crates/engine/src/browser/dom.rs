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

/// Iterativni drop pres flat queue - default Drop by se rekurzivne zanoril pri
/// hlubokem DOMu (e.g. 5000 nestnutych <div>) a pretekl stack.
/// Princip: pred dropnutim uzlu drainujeme jeho children do queue (kdyz mame
/// jediny owner); takhle se sekvencne uvolnuji listy, az nakonec dropujeme
/// "holy" root bez children -> zadna recursive drop chain.
impl Drop for NodeData {
    fn drop(&mut self) {
        // Steal children z self - od ted jsou volne v queue, nikoliv pres self.
        let initial: Vec<Rc<Node>> = std::mem::take(&mut *self.children.borrow_mut());
        let mut queue: Vec<Rc<Node>> = initial;
        while let Some(node) = queue.pop() {
            // Pokud jsme jediny owner, vyboxuj jeho children.
            if Rc::strong_count(&node) == 1 {
                if let Ok(mut ch_ref) = node.children.try_borrow_mut() {
                    let stolen: Vec<Rc<Node>> = std::mem::take(&mut *ch_ref);
                    drop(ch_ref);
                    queue.extend(stolen);
                }
            }
            // Drop node Rc - kdyz strong_count byl 1, NodeData drop fire,
            // ale jeho children uz prazdne -> recursion ends here.
            drop(node);
        }
    }
}

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

    /// PERF: allocation-free varianta tag_name(). Vraci &str borrow primo z node.
    /// Pouzij v hot paths (cascade matches_simple).
    #[inline]
    pub fn tag_name_ref(&self) -> Option<&str> {
        if let NodeKind::Element(tag) = &self.kind {
            Some(tag.as_str())
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
            stacker::maybe_grow(32 * 1024, 8 * 1024 * 1024, || {
                ch.collect_text(out);
            });
        }
    }

    /// Walk preorder - vola cb pro kazdy uzel.
    /// Auto-grow stacku pres stacker (red zone 32 KB, chunk 8 MB) - pokryva
    /// libovolne hluboke DOMy bez stack overflow.
    pub fn walk(self: &Rc<Self>, cb: &mut dyn FnMut(&Rc<Node>)) {
        stacker::maybe_grow(32 * 1024, 8 * 1024 * 1024, || {
            cb(self);
            for ch in self.children.borrow().iter() {
                ch.walk(cb);
            }
        });
    }

    /// Najde prvni element ktery vyhovuje predikatu.
    /// Pouziva &dyn Fn aby se vyhnulo nekonecne monomorfizaci.
    pub fn find<F: Fn(&Rc<Node>) -> bool>(self: &Rc<Self>, pred: F) -> Option<Rc<Node>> {
        self.find_inner(&pred)
    }

    fn find_inner(self: &Rc<Self>, pred: &dyn Fn(&Rc<Node>) -> bool) -> Option<Rc<Node>> {
        if pred(self) { return Some(Rc::clone(self)); }
        for ch in self.children.borrow().iter() {
            let r = stacker::maybe_grow(32 * 1024, 8 * 1024 * 1024, || ch.find_inner(pred));
            if r.is_some() { return r; }
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
    /// Document-level selection state - text input cursors, page selection
    /// rangesy. Foundation pro W3C Selection API + page text-run selection.
    pub selection: RefCell<super::selection::SelectionRegistry>,
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
            selection: RefCell::new(super::selection::SelectionRegistry::new()),
        }
    }

    /// Vytvori prazdny dokument bez html/head/body (pro testy parseru).
    pub fn empty(url: String) -> Self {
        Document {
            root: NodeData::new_document(),
            url,
            title: String::new(),
            selection: RefCell::new(super::selection::SelectionRegistry::new()),
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
