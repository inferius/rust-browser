/// CSS parser - obal nad cssparser crate.
///
/// Parsuje CSS stylesheet na seznam pravidel (Rule).
/// Kazde pravidlo ma selektor + deklarace (property: value).
/// Pro plne CSS3 selectors by se pouzil selectors crate, zde lite parser.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
}

#[derive(Debug, Clone)]
pub struct Declaration {
    pub property: String,
    pub value: String,
    pub important: bool,
}

/// Selector - lite verze (typ, id, class, descendant kombinace).
#[derive(Debug, Clone)]
pub struct Selector {
    pub parts: Vec<SimpleSelector>,
}

#[derive(Debug, Clone)]
pub struct SimpleSelector {
    /// Tag name nebo "*" pro universal
    pub tag: Option<String>,
    /// IDs
    pub id: Option<String>,
    /// Classes
    pub classes: Vec<String>,
    /// Combinator pred timto selektorem (None = root, Descendant = " ", Child = ">")
    pub combinator: Option<Combinator>,
}

#[derive(Debug, Clone)]
pub enum Combinator {
    Descendant,
    Child,
    AdjacentSibling,
    GeneralSibling,
}

/// Specificita selektoru: (id_count, class_count, type_count)
pub fn specificity(sel: &Selector) -> (u32, u32, u32) {
    let mut id_count = 0;
    let mut class_count = 0;
    let mut type_count = 0;
    for p in &sel.parts {
        if p.id.is_some() { id_count += 1; }
        class_count += p.classes.len() as u32;
        if p.tag.is_some() && p.tag.as_deref() != Some("*") { type_count += 1; }
    }
    (id_count, class_count, type_count)
}

/// Parsuje CSS stylesheet (lite parser, hand-rolled).
pub fn parse_stylesheet(source: &str) -> Stylesheet {
    let mut rules = Vec::new();
    let mut chars = source.chars().peekable();

    while chars.peek().is_some() {
        // Skip whitespace + comments
        skip_whitespace_and_comments(&mut chars);
        if chars.peek().is_none() { break; }

        // Read selectors until '{'
        let mut selectors_str = String::new();
        while let Some(&c) = chars.peek() {
            if c == '{' { chars.next(); break; }
            selectors_str.push(c);
            chars.next();
        }
        let selectors_str = selectors_str.trim().to_string();
        if selectors_str.is_empty() { break; }

        // Read declarations until '}'
        let mut block_str = String::new();
        let mut depth = 1;
        while let Some(c) = chars.next() {
            if c == '{' { depth += 1; }
            if c == '}' { depth -= 1; if depth == 0 { break; } }
            block_str.push(c);
        }

        let selectors = parse_selectors(&selectors_str);
        let declarations = parse_decls_str(&block_str);
        rules.push(Rule { selectors, declarations });
    }

    Stylesheet { rules }
}

fn skip_whitespace_and_comments<I: Iterator<Item=char>>(chars: &mut std::iter::Peekable<I>) {
    loop {
        while matches!(chars.peek(), Some(c) if c.is_whitespace()) { chars.next(); }
        // Skip /* ... */ - peek '/' a pak musime peeknou dalsi znak.
        // Bez Clone iteratoru musime advance a pak restore. Misto toho:
        // jen kontrolujeme '/' a pak '*' samostatne.
        if matches!(chars.peek(), Some('/')) {
            // Spotrebuj '/'
            chars.next();
            if matches!(chars.peek(), Some('*')) {
                chars.next();
                while let Some(c) = chars.next() {
                    if c == '*' && matches!(chars.peek(), Some('/')) {
                        chars.next();
                        break;
                    }
                }
                continue;
            } else {
                // Neni komentar - vratit '/' nemuzeme, ale CSS tam '/' nepatri (jen v values)
                // Pro zjednodusene parsing toto nesmysl ignorujeme
                break;
            }
        }
        break;
    }
}

fn parse_decls_str(block: &str) -> Vec<Declaration> {
    let mut decls = Vec::new();
    for stmt in block.split(';') {
        let stmt = stmt.trim();
        if stmt.is_empty() { continue; }
        if let Some(colon) = stmt.find(':') {
            let property = stmt[..colon].trim().to_string();
            let mut value = stmt[colon+1..].trim().to_string();
            let mut important = false;
            if let Some(idx) = value.to_lowercase().find("!important") {
                important = true;
                value.truncate(idx);
                value = value.trim().to_string();
            }
            decls.push(Declaration { property, value, important });
        }
    }
    decls
}

fn parse_selectors(s: &str) -> Vec<Selector> {
    s.split(',').map(|sel_str| parse_single_selector(sel_str.trim())).collect()
}

fn parse_single_selector(s: &str) -> Selector {
    let mut parts = Vec::new();
    let mut current_combinator: Option<Combinator> = None;
    for token in s.split_whitespace() {
        // Detekce combinatoru
        if token == ">" { current_combinator = Some(Combinator::Child); continue; }
        if token == "+" { current_combinator = Some(Combinator::AdjacentSibling); continue; }
        if token == "~" { current_combinator = Some(Combinator::GeneralSibling); continue; }

        let mut tag: Option<String> = None;
        let mut id: Option<String> = None;
        let mut classes = Vec::new();

        let chars: Vec<char> = token.chars().collect();
        let mut i = 0;
        // Tag (volitelny, na zacatku)
        let mut tag_buf = String::new();
        while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '*' || chars[i] == '-') {
            tag_buf.push(chars[i]);
            i += 1;
        }
        if !tag_buf.is_empty() { tag = Some(tag_buf); }

        while i < chars.len() {
            match chars[i] {
                '#' => {
                    i += 1;
                    let mut buf = String::new();
                    while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
                        buf.push(chars[i]); i += 1;
                    }
                    id = Some(buf);
                }
                '.' => {
                    i += 1;
                    let mut buf = String::new();
                    while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
                        buf.push(chars[i]); i += 1;
                    }
                    classes.push(buf);
                }
                _ => { i += 1; }
            }
        }

        let combinator = if parts.is_empty() {
            None
        } else {
            current_combinator.take().or(Some(Combinator::Descendant))
        };

        parts.push(SimpleSelector { tag, id, classes, combinator });
    }
    Selector { parts }
}

/// Konverze deklaraci na HashMap (property -> value).
pub fn declarations_to_map(decls: &[Declaration]) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for d in decls {
        m.insert(d.property.clone(), d.value.clone());
    }
    m
}
