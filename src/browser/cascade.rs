/// CSS cascade - aplikace stylesheets na DOM strom.
///
/// Vrati StyleMap: pro kazdy element computed styles (HashMap<String, String>).
/// Specificita rozhoduje pri kolizi.

use std::collections::HashMap;
use std::rc::Rc;
use super::dom::{Node, NodeKind};
use super::css_parser::{Stylesheet, Selector, SimpleSelector, Combinator, specificity};

/// Expanduje CSS shorthand props (margin/padding/border) do longhand.
/// Napr. "margin: 10px 20px;" -> margin-top:10, margin-right:20, margin-bottom:10, margin-left:20.
/// "border: 1px solid red;" -> border-width:1, border-style:solid, border-color:red.
pub fn expand_shorthand(prop: &str, value: &str, out: &mut HashMap<String, String>) {
    match prop {
        "margin" | "padding" => {
            let parts: Vec<&str> = value.split_whitespace().collect();
            let (t, r, b, l) = match parts.len() {
                1 => (parts[0], parts[0], parts[0], parts[0]),
                2 => (parts[0], parts[1], parts[0], parts[1]),
                3 => (parts[0], parts[1], parts[2], parts[1]),
                4 => (parts[0], parts[1], parts[2], parts[3]),
                _ => return,
            };
            out.insert(format!("{prop}-top"),    t.into());
            out.insert(format!("{prop}-right"),  r.into());
            out.insert(format!("{prop}-bottom"), b.into());
            out.insert(format!("{prop}-left"),   l.into());
            out.insert(prop.into(), value.into()); // shorthand zachovan pro existing read
        }
        "border" => {
            // "1px solid red" - parse postupne
            let parts: Vec<&str> = value.split_whitespace().collect();
            for p in &parts {
                if p.ends_with("px") || p.ends_with("em") || p.ends_with("rem") {
                    out.insert("border-width".into(), p.to_string());
                } else if matches!(*p, "solid" | "dashed" | "dotted" | "double" | "none") {
                    out.insert("border-style".into(), p.to_string());
                } else if super::layout::parse_color(p).is_some() {
                    out.insert("border-color".into(), p.to_string());
                }
            }
            out.insert("border".into(), value.into());
        }
        "background" => {
            // Zjednoduseno: pokud je color, ulozit jako background-color
            if super::layout::parse_color(value).is_some() {
                out.insert("background-color".into(), value.into());
            }
            out.insert("background".into(), value.into());
        }
        "font" => {
            // "16px Arial" / "bold 14px Verdana" - parse size + family
            for p in value.split_whitespace() {
                if p.ends_with("px") || p.ends_with("em") || p.ends_with("rem") {
                    out.insert("font-size".into(), p.into());
                } else if p == "bold" {
                    out.insert("font-weight".into(), "bold".into());
                } else if p == "italic" {
                    out.insert("font-style".into(), "italic".into());
                }
            }
            out.insert("font".into(), value.into());
        }
        _ => {
            out.insert(prop.into(), value.into());
        }
    }
}

/// Mapa: pointer na Node -> computed styles.
pub type StyleMap = HashMap<usize, HashMap<String, String>>;

/// Pomocnik: vrati pointer hodnotu Rc<Node> jako klic.
fn node_id(node: &Rc<Node>) -> usize {
    Rc::as_ptr(node) as usize
}

/// Aplikuje stylesheet na DOM strom, vrati StyleMap.
pub fn cascade(root: &Rc<Node>, stylesheets: &[Stylesheet]) -> StyleMap {
    let mut style_map: StyleMap = HashMap::new();

    // Prochazime DOM, pro kazdy element zkontrolujeme vsechny rules
    root.walk(&mut |node| {
        if !matches!(node.kind, NodeKind::Element { .. }) { return; }

        let mut matched_decls: Vec<((u32, u32, u32, usize), &super::css_parser::Declaration)> = Vec::new();
        let mut order = 0;

        for sheet in stylesheets {
            for rule in &sheet.rules {
                for sel in &rule.selectors {
                    if matches_selector(node, sel) {
                        let spec = specificity(sel);
                        for decl in &rule.declarations {
                            let key = (
                                if decl.important { 1 } else { 0 },
                                spec.0,
                                spec.1 + spec.2,
                                order,
                            );
                            matched_decls.push((
                                (key.0, key.1, key.2, key.3),
                                decl,
                            ));
                            order += 1;
                        }
                    }
                }
            }
        }

        // Sort podle (important, id_count, class+type, order) - vyssi kombinace vyhrava
        matched_decls.sort_by(|a, b| a.0.cmp(&b.0));

        let mut styles = HashMap::new();
        for (_, decl) in matched_decls {
            expand_shorthand(&decl.property, &decl.value, &mut styles);
        }

        // Inline styly z attributu "style" maji nejvyssi prioritu (mimo !important rules)
        if let Some(inline) = node.attr("style") {
            for pair in inline.split(';') {
                if let Some(colon) = pair.find(':') {
                    let prop = pair[..colon].trim().to_string();
                    let val = pair[colon+1..].trim().to_string();
                    if !prop.is_empty() && !val.is_empty() {
                        expand_shorthand(&prop, &val, &mut styles);
                    }
                }
            }
        }

        style_map.insert(node_id(node), styles);
    });

    style_map
}

/// Kontrola jestli selektor matchuje uzel.
/// Pro multi-part selektory chodime parents.
pub fn matches_selector(node: &Rc<Node>, sel: &Selector) -> bool {
    if sel.parts.is_empty() { return false; }
    // Posledni cast musi matchovat node
    let last = &sel.parts[sel.parts.len() - 1];
    if !matches_simple(node, last) { return false; }

    // Pokud jen jedna cast, hotovo
    if sel.parts.len() == 1 { return true; }

    // Vice casti - chodime po parents
    let mut current_part = sel.parts.len() - 2;
    let mut current_node = node.parent.borrow().upgrade();

    loop {
        let part = &sel.parts[current_part];
        let combinator = sel.parts[current_part + 1].combinator.clone()
            .unwrap_or(Combinator::Descendant);

        match combinator {
            Combinator::Child => {
                let p_clone = current_node.clone();
                if let Some(p) = p_clone {
                    if !matches_simple(&p, part) { return false; }
                    if current_part == 0 { return true; }
                    current_part -= 1;
                    let next = p.parent.borrow().upgrade();
                    current_node = next;
                } else { return false; }
            }
            Combinator::Descendant => {
                let mut found = false;
                loop {
                    let p_clone = current_node.clone();
                    let p = match p_clone { Some(p) => p, None => break };
                    if matches_simple(&p, part) {
                        if current_part == 0 { return true; }
                        current_part -= 1;
                        let next = p.parent.borrow().upgrade();
                        current_node = next;
                        found = true;
                        break;
                    }
                    let next = p.parent.borrow().upgrade();
                    current_node = next;
                }
                if !found { return false; }
            }
            // Sibling combinators - zatim nepodporujeme spravne
            _ => return false,
        }
    }
}

/// Kontroluje simple selector proti uzlu.
pub fn matches_simple(node: &Rc<Node>, sel: &SimpleSelector) -> bool {
    use super::css_parser::AttrOp;

    let tag = match node.tag_name() {
        Some(t) => t,
        None => return false,
    };

    if let Some(want_tag) = &sel.tag {
        if want_tag != "*" && want_tag.to_lowercase() != tag {
            return false;
        }
    }

    if let Some(want_id) = &sel.id {
        if node.attr("id").as_deref() != Some(want_id.as_str()) {
            return false;
        }
    }

    if !sel.classes.is_empty() {
        let class_attr = node.attr("class").unwrap_or_default();
        let classes: Vec<&str> = class_attr.split_whitespace().collect();
        for required in &sel.classes {
            if !classes.contains(&required.as_str()) {
                return false;
            }
        }
    }

    // Atribute selektory
    for attr_sel in &sel.attributes {
        let actual = node.attr(&attr_sel.name);
        match (&attr_sel.op, &attr_sel.value, &actual) {
            (AttrOp::Exists, _, None) => return false,
            (AttrOp::Exists, _, Some(_)) => {}
            (_, _, None) => return false,
            (AttrOp::Equals, Some(want), Some(got)) => {
                if want != got { return false; }
            }
            (AttrOp::Contains, Some(want), Some(got)) => {
                if !got.contains(want.as_str()) { return false; }
            }
            (AttrOp::StartsWith, Some(want), Some(got)) => {
                if !got.starts_with(want.as_str()) { return false; }
            }
            (AttrOp::EndsWith, Some(want), Some(got)) => {
                if !got.ends_with(want.as_str()) { return false; }
            }
            (AttrOp::WordContains, Some(want), Some(got)) => {
                if !got.split_whitespace().any(|w| w == want) { return false; }
            }
            _ => {}
        }
    }

    // Pseudo-classes :hover/:active/:focus zatim ignorujeme (vyzaduji runtime stav)
    // Skip - vraci match aby se pravidlo neaplikovalo zbytecne
    for pc in &sel.pseudo_classes {
        match pc.as_str() {
            "root" => {
                // :root match jen html element
                if tag != "html" { return false; }
            }
            "first-child" => {
                let parent = node.parent.borrow().upgrade();
                if let Some(p) = parent {
                    let children = p.children.borrow();
                    let first_el = children.iter().find(|c| matches!(c.kind, NodeKind::Element(_)));
                    if first_el.map(|f| !std::rc::Rc::ptr_eq(f, node)).unwrap_or(true) {
                        return false;
                    }
                }
            }
            "last-child" => {
                let parent = node.parent.borrow().upgrade();
                if let Some(p) = parent {
                    let children = p.children.borrow();
                    let last_el = children.iter().rev().find(|c| matches!(c.kind, NodeKind::Element(_)));
                    if last_el.map(|f| !std::rc::Rc::ptr_eq(f, node)).unwrap_or(true) {
                        return false;
                    }
                }
            }
            // hover/active/focus - bez runtime nemuzu - skip (pravidlo se NEaplikuje)
            "hover" | "active" | "focus" | "visited" | "link" => return false,
            _ => {}
        }
    }

    true
}

/// Vrati computed styles pro dany uzel (z StyleMap).
pub fn get_styles<'a>(map: &'a StyleMap, node: &Rc<Node>) -> Option<&'a HashMap<String, String>> {
    map.get(&node_id(node))
}
