/// CSS parser - obal nad cssparser crate.
///
/// Parsuje CSS stylesheet na seznam pravidel (Rule).
/// Kazde pravidlo ma selektor + deklarace (property: value).
/// Pro plne CSS3 selectors by se pouzil selectors crate, zde lite parser.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
    /// @media queries: kazda obsahuje query string + nested rules
    pub media_queries: Vec<MediaQuery>,
}

#[derive(Debug, Clone)]
pub struct MediaQuery {
    pub query: String,        // "(max-width: 768px)" / "screen and (min-width: 600px)"
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
    /// Atribute selektory: [type], [type="text"], [class~="foo"]
    pub attributes: Vec<AttrSelector>,
    /// Pseudo-class :hover, :active, :focus, etc.
    pub pseudo_classes: Vec<String>,
    /// Combinator pred timto selektorem (None = root, Descendant = " ", Child = ">")
    pub combinator: Option<Combinator>,
}

#[derive(Debug, Clone)]
pub struct AttrSelector {
    pub name: String,
    pub op: AttrOp,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttrOp {
    Exists,        // [name]
    Equals,        // [name="value"]
    Contains,      // [name*="value"]
    StartsWith,    // [name^="value"]
    EndsWith,      // [name$="value"]
    WordContains,  // [name~="value"]
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
        class_count += p.attributes.len() as u32;
        class_count += p.pseudo_classes.len() as u32;
        if p.tag.is_some() && p.tag.as_deref() != Some("*") { type_count += 1; }
    }
    (id_count, class_count, type_count)
}

/// Parsuje CSS stylesheet (lite parser, hand-rolled).
pub fn parse_stylesheet(source: &str) -> Stylesheet {
    let mut rules = Vec::new();
    let mut media_queries = Vec::new();
    let mut chars = source.chars().peekable();

    while chars.peek().is_some() {
        skip_whitespace_and_comments(&mut chars);
        if chars.peek().is_none() { break; }

        // Read selectors / at-rule until '{'
        let mut selectors_str = String::new();
        while let Some(&c) = chars.peek() {
            if c == '{' { chars.next(); break; }
            selectors_str.push(c);
            chars.next();
        }
        let selectors_str = selectors_str.trim().to_string();
        if selectors_str.is_empty() { break; }

        // Read block until matching '}'
        let mut block_str = String::new();
        let mut depth = 1;
        while let Some(c) = chars.next() {
            if c == '{' { depth += 1; }
            if c == '}' { depth -= 1; if depth == 0 { break; } }
            block_str.push(c);
        }

        // Detekce @media
        if selectors_str.starts_with("@media") {
            let query = selectors_str.trim_start_matches("@media").trim().to_string();
            let nested = parse_stylesheet(&block_str);
            media_queries.push(MediaQuery { query, rules: nested.rules });
        } else if selectors_str.starts_with('@') {
            // Ostatni at-rules zatim ignorujeme (@import, @keyframes, atd.)
        } else {
            let selectors = parse_selectors(&selectors_str);
            let declarations = parse_decls_str(&block_str);
            rules.push(Rule { selectors, declarations });
        }
    }

    Stylesheet { rules, media_queries }
}

/// Vyhodnoti media query string proti viewport.
/// Podporuje: (max-width: Npx), (min-width: Npx), (orientation: ...)
pub fn evaluate_media_query(query: &str, viewport_w: f32, viewport_h: f32) -> bool {
    // Mlha: zjednoduseny - pokud "screen", "all" nebo prazdny -> true
    let q = query.trim().to_lowercase();
    // Strip type "screen"/"all" + "and"
    let q = q.replace("screen", "").replace("all", "").replace(" and ", " ");
    // Conditions oddelene zavorkami
    for cond in q.split(')').filter(|s| !s.trim().is_empty()) {
        let c = cond.trim_start_matches('(').trim();
        if c.is_empty() { continue; }
        if let Some(idx) = c.find(':') {
            let prop = c[..idx].trim();
            let val = c[idx+1..].trim();
            let num: f32 = val.trim_end_matches("px").trim().parse().unwrap_or(0.0);
            match prop {
                "max-width"  => if viewport_w > num  { return false; }
                "min-width"  => if viewport_w < num  { return false; }
                "max-height" => if viewport_h > num  { return false; }
                "min-height" => if viewport_h < num  { return false; }
                "orientation" => {
                    let landscape = viewport_w >= viewport_h;
                    if val == "landscape" && !landscape { return false; }
                    if val == "portrait" &&  landscape  { return false; }
                }
                _ => {}
            }
        }
    }
    true
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
        if token == ">" { current_combinator = Some(Combinator::Child); continue; }
        if token == "+" { current_combinator = Some(Combinator::AdjacentSibling); continue; }
        if token == "~" { current_combinator = Some(Combinator::GeneralSibling); continue; }

        let mut tag: Option<String> = None;
        let mut id: Option<String> = None;
        let mut classes = Vec::new();
        let mut attributes = Vec::new();
        let mut pseudo_classes = Vec::new();

        let chars: Vec<char> = token.chars().collect();
        let mut i = 0;
        // Tag / universal (volitelny)
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
                ':' => {
                    i += 1;
                    // Skip druhy : (::before)
                    if i < chars.len() && chars[i] == ':' { i += 1; }
                    let mut buf = String::new();
                    while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
                        buf.push(chars[i]); i += 1;
                    }
                    pseudo_classes.push(buf);
                }
                '[' => {
                    i += 1;
                    let mut name = String::new();
                    while i < chars.len() && chars[i] != ']' && chars[i] != '=' && chars[i] != '~'
                        && chars[i] != '^' && chars[i] != '$' && chars[i] != '*' && chars[i] != '|'
                    {
                        name.push(chars[i]); i += 1;
                    }
                    let name = name.trim().to_string();
                    let mut op = AttrOp::Exists;
                    let mut value: Option<String> = None;
                    if i < chars.len() && chars[i] != ']' {
                        // Detekce operatoru
                        op = match chars[i] {
                            '=' => AttrOp::Equals,
                            '~' => { i += 1; AttrOp::WordContains }
                            '^' => { i += 1; AttrOp::StartsWith }
                            '$' => { i += 1; AttrOp::EndsWith }
                            '*' => { i += 1; AttrOp::Contains }
                            _   => AttrOp::Exists,
                        };
                        if i < chars.len() && chars[i] == '=' { i += 1; }
                        // Hodnota - mozna v uvozovkach
                        if i < chars.len() && (chars[i] == '"' || chars[i] == '\'') {
                            let quote = chars[i];
                            i += 1;
                            let mut buf = String::new();
                            while i < chars.len() && chars[i] != quote {
                                buf.push(chars[i]); i += 1;
                            }
                            if i < chars.len() { i += 1; }
                            value = Some(buf);
                        } else {
                            let mut buf = String::new();
                            while i < chars.len() && chars[i] != ']' {
                                buf.push(chars[i]); i += 1;
                            }
                            if !buf.is_empty() { value = Some(buf); }
                        }
                    }
                    if i < chars.len() && chars[i] == ']' { i += 1; }
                    attributes.push(AttrSelector { name, op, value });
                }
                _ => { i += 1; }
            }
        }

        let combinator = if parts.is_empty() {
            None
        } else {
            current_combinator.take().or(Some(Combinator::Descendant))
        };

        parts.push(SimpleSelector {
            tag, id, classes, attributes, pseudo_classes, combinator,
        });
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
