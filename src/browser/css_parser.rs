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
    /// @keyframes: name -> Vec<(percent, declarations)>
    pub keyframes: Vec<Keyframes>,
}

#[derive(Debug, Clone)]
pub struct Keyframes {
    pub name: String,
    pub frames: Vec<(f32, Vec<Declaration>)>, // (percent 0..1, declarations)
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
    /// Pseudo-class :hover, :active, :focus, etc. (bez argumentu)
    pub pseudo_classes: Vec<String>,
    /// Funkcni pseudo-classes: :is(...), :not(...), :nth-child(...), :has(...)
    pub pseudo_funcs: Vec<PseudoFunc>,
    /// Combinator pred timto selektorem (None = root, Descendant = " ", Child = ">")
    pub combinator: Option<Combinator>,
}

/// Funkcni pseudo-classy s argumenty.
#[derive(Debug, Clone)]
pub enum PseudoFunc {
    /// `:is(<selector-list>)` - matches-any
    Is(Vec<Selector>),
    /// `:where(<selector-list>)` - jako :is ale specificita 0
    Where(Vec<Selector>),
    /// `:not(<selector-list>)` - negace
    Not(Vec<Selector>),
    /// `:has(<relative-selector-list>)` - relacni (descendant)
    Has(Vec<Selector>),
    /// `:nth-child(an+b)` / `:nth-last-child(an+b)`
    NthChild { a: i32, b: i32, of_type: bool, last: bool },
    /// Neznamy / nepodporovany - raw args pro forward-compat
    Unknown { name: String, args: String },
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

        // Funkcni pseudo: :is/:not/:has = max specificity argumentu, :where = 0
        for pf in &p.pseudo_funcs {
            match pf {
                PseudoFunc::Is(args) | PseudoFunc::Not(args) | PseudoFunc::Has(args) => {
                    let mut best = (0, 0, 0);
                    for arg_sel in args {
                        let s = specificity(arg_sel);
                        if s > best { best = s; }
                    }
                    id_count += best.0;
                    class_count += best.1;
                    type_count += best.2;
                }
                PseudoFunc::Where(_) => { /* specificita 0 */ }
                PseudoFunc::NthChild { .. } => { class_count += 1; }
                PseudoFunc::Unknown { .. } => { class_count += 1; }
            }
        }
    }
    (id_count, class_count, type_count)
}

/// Parsuje CSS stylesheet (lite parser, hand-rolled).
pub fn parse_stylesheet(source: &str) -> Stylesheet {
    let mut rules = Vec::new();
    let mut media_queries = Vec::new();
    let mut keyframes = Vec::new();
    let mut chars = source.chars().peekable();

    while chars.peek().is_some() {
        skip_whitespace_and_comments(&mut chars);
        if chars.peek().is_none() { break; }

        // Read selectors / at-rule until '{' or ';'
        let mut selectors_str = String::new();
        let mut has_block = false;
        while let Some(&c) = chars.peek() {
            if c == '{' { chars.next(); has_block = true; break; }
            if c == ';' { chars.next(); break; }
            selectors_str.push(c);
            chars.next();
        }
        let selectors_str = selectors_str.trim().to_string();
        if selectors_str.is_empty() { break; }

        // @import / @charset bez bloku - skip
        if !has_block {
            // Tato at-rule je zpracovana (@import "url" pro budoucnost, ted ignored)
            continue;
        }

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
        } else if selectors_str.starts_with("@keyframes") || selectors_str.starts_with("@-webkit-keyframes") {
            let name = selectors_str
                .trim_start_matches("@keyframes")
                .trim_start_matches("@-webkit-keyframes")
                .trim().to_string();
            let frames = parse_keyframes(&block_str);
            keyframes.push(Keyframes { name, frames });
        } else if selectors_str.starts_with('@') {
            // Ostatni at-rules ignorujeme (@import, @font-face, ...)
        } else {
            let selectors = parse_selectors(&selectors_str);
            // CSS Nesting L1: zpracuj block s ohledem na nested rulesets
            process_nested_block(&selectors, &block_str, &mut rules);
        }
    }

    Stylesheet { rules, media_queries, keyframes }
}

/// CSS Nesting: rozdeli block na (vlastni declarace, nested rulesets).
/// Pro nested rulesets vyrobi kombinovany selector (parent x nested)
/// a rekurzivne ho ulozi do rules.
fn process_nested_block(parent: &[Selector], block: &str, rules: &mut Vec<Rule>) {
    let mut own_decls = String::new();
    let mut chars = block.chars().peekable();
    while chars.peek().is_some() {
        // Sosa do prvniho `;` nebo `{`
        let mut buf = String::new();
        let mut found_brace = false;
        while let Some(&c) = chars.peek() {
            if c == ';' { chars.next(); break; }
            if c == '{' { found_brace = true; break; }
            buf.push(c);
            chars.next();
        }
        if !found_brace {
            // Pricti k vlastnim deklaracim (semicolon/EOF)
            if !buf.trim().is_empty() {
                own_decls.push_str(&buf);
                own_decls.push(';');
            }
            continue;
        }
        // Nested ruleset: buf obsahuje nested selektor, dale nasleduje `{...}`
        chars.next(); // pohlt `{`
        let nested_sel_str = buf.trim().to_string();
        let mut inner_block = String::new();
        let mut depth = 1;
        while let Some(c) = chars.next() {
            if c == '{' { depth += 1; }
            if c == '}' { depth -= 1; if depth == 0 { break; } }
            inner_block.push(c);
        }
        // Spojuj parent x nested
        let nested_selectors = parse_selectors(&nested_sel_str);
        let combined = combine_nested_selectors(parent, &nested_selectors);
        // Rekurzivni nesting
        process_nested_block(&combined, &inner_block, rules);
    }
    if !own_decls.trim().is_empty() {
        let declarations = parse_decls_str(&own_decls);
        if !declarations.is_empty() {
            rules.push(Rule { selectors: parent.to_vec(), declarations });
        }
    }
}

/// Kombinuje parent selektory s nested selektory pres CSS Nesting `&`.
/// Cartesian product: vsechny kombinace.
fn combine_nested_selectors(parent: &[Selector], nested: &[Selector]) -> Vec<Selector> {
    let mut out = Vec::new();
    for n in nested {
        for p in parent {
            out.push(substitute_ampersand(p, n));
        }
    }
    out
}

/// Provede substituci `&` v nested selektoru za parent.
/// Pravidla:
/// - Pokud `&` v nested neni, predpokladame implicit `& <nested>` (descendant).
/// - `&` v prvni casti nahradi parent ruzove parts.
/// - `&` v dalsich castech (compound s tagem/classou) - jednoduche zacleneni: pridame parent jako prefix.
fn substitute_ampersand(parent: &Selector, nested: &Selector) -> Selector {
    if nested.parts.is_empty() { return parent.clone(); }

    // Detekce zda nested zacina amp
    let first = &nested.parts[0];
    let starts_with_amp = first.tag.as_deref() == Some("&")
        || first.id.as_deref() == Some("&")
        || first.classes.iter().any(|c| c == "&");

    let mut new_parts = parent.parts.clone();
    // Bezpecnostne odstran `&` kdykoliv se vyskytuje v parts (parser ho mohl ulozit do tag)
    if starts_with_amp {
        // Sloucit prvni part nested (bez `&` markeru) s posledni part parenta
        if let Some(last) = new_parts.last_mut() {
            let mut merged = first.clone();
            // Odstran `&` markery z merged
            if merged.tag.as_deref() == Some("&") { merged.tag = None; }
            merged.id = merged.id.filter(|id| id != "&");
            merged.classes.retain(|c| c != "&");
            // Pridej z merged do last (concat)
            if merged.tag.is_some() { last.tag = merged.tag; }
            if merged.id.is_some()  { last.id = merged.id; }
            last.classes.extend(merged.classes);
            last.attributes.extend(merged.attributes);
            last.pseudo_classes.extend(merged.pseudo_classes);
            last.pseudo_funcs.extend(merged.pseudo_funcs);
        }
        // Zbytek nested.parts (>= 1) pridat s puvodnymi combinatory
        for part in nested.parts.iter().skip(1) {
            new_parts.push(part.clone());
        }
    } else {
        // Implicit descendant - pridej cely nested za parent
        for (i, part) in nested.parts.iter().enumerate() {
            let mut np = part.clone();
            // Prvni cast nested musi mit kombinator (default Descendant)
            if i == 0 && np.combinator.is_none() {
                np.combinator = Some(Combinator::Descendant);
            }
            new_parts.push(np);
        }
    }
    Selector { parts: new_parts }
}

/// Parsuje @keyframes blok: "0% { ... } 50% { ... } 100% { ... }".
fn parse_keyframes(block: &str) -> Vec<(f32, Vec<Declaration>)> {
    let mut frames = Vec::new();
    let mut chars = block.chars().peekable();
    while chars.peek().is_some() {
        skip_whitespace_and_comments(&mut chars);
        if chars.peek().is_none() { break; }
        let mut sel = String::new();
        while let Some(&c) = chars.peek() {
            if c == '{' { chars.next(); break; }
            sel.push(c);
            chars.next();
        }
        let mut block_inner = String::new();
        let mut depth = 1;
        while let Some(c) = chars.next() {
            if c == '{' { depth += 1; }
            if c == '}' { depth -= 1; if depth == 0 { break; } }
            block_inner.push(c);
        }
        // Sel: napr "0%", "50%", "from", "to"
        for part in sel.split(',') {
            let s = part.trim();
            let percent = match s {
                "from" => 0.0,
                "to"   => 1.0,
                _ => s.trim_end_matches('%').parse::<f32>().unwrap_or(0.0) / 100.0,
            };
            let decls = parse_decls_str(&block_inner);
            frames.push((percent, decls));
        }
    }
    frames.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    frames
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
    split_top_level_comma(s).into_iter()
        .map(|sel_str| parse_single_selector(sel_str.trim()))
        .collect()
}

/// Split na top-level (respektuje zavorky/uvozovky) podle char.
fn split_top_level(s: &str, sep: impl Fn(char) -> bool) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut depth_paren = 0i32;
    let mut depth_brack = 0i32;
    let mut quote: Option<char> = None;
    for ch in s.chars() {
        if let Some(q) = quote {
            cur.push(ch);
            if ch == q { quote = None; }
            continue;
        }
        match ch {
            '"' | '\'' => { quote = Some(ch); cur.push(ch); }
            '(' => { depth_paren += 1; cur.push(ch); }
            ')' => { depth_paren -= 1; cur.push(ch); }
            '[' => { depth_brack += 1; cur.push(ch); }
            ']' => { depth_brack -= 1; cur.push(ch); }
            c if depth_paren == 0 && depth_brack == 0 && sep(c) => {
                if !cur.is_empty() { tokens.push(std::mem::take(&mut cur)); }
            }
            _ => cur.push(ch),
        }
    }
    if !cur.is_empty() { tokens.push(cur); }
    tokens
}

fn split_top_level_whitespace(s: &str) -> Vec<String> {
    split_top_level(s, |c| c.is_whitespace())
}

fn split_top_level_comma(s: &str) -> Vec<String> {
    split_top_level(s, |c| c == ',')
}

fn parse_single_selector(s: &str) -> Selector {
    let mut parts = Vec::new();
    let mut current_combinator: Option<Combinator> = None;
    for token in split_top_level_whitespace(s) {
        if token == ">" { current_combinator = Some(Combinator::Child); continue; }
        if token == "+" { current_combinator = Some(Combinator::AdjacentSibling); continue; }
        if token == "~" { current_combinator = Some(Combinator::GeneralSibling); continue; }

        let mut tag: Option<String> = None;
        let mut id: Option<String> = None;
        let mut classes = Vec::new();
        let mut attributes = Vec::new();
        let mut pseudo_classes = Vec::new();
        let mut pseudo_funcs = Vec::new();

        let chars: Vec<char> = token.chars().collect();
        let mut i = 0;
        // Tag / universal / `&` (CSS Nesting) (volitelny)
        if i < chars.len() && chars[i] == '&' {
            tag = Some("&".to_string());
            i += 1;
        } else {
            let mut tag_buf = String::new();
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '*' || chars[i] == '-') {
                tag_buf.push(chars[i]);
                i += 1;
            }
            if !tag_buf.is_empty() { tag = Some(tag_buf); }
        }

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
                    let mut name = String::new();
                    while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
                        name.push(chars[i]); i += 1;
                    }
                    // Funkcni pseudo: :name(args)
                    if i < chars.len() && chars[i] == '(' {
                        i += 1;
                        let mut depth = 1;
                        let mut args = String::new();
                        while i < chars.len() {
                            let c = chars[i];
                            if c == '(' { depth += 1; }
                            if c == ')' { depth -= 1; if depth == 0 { i += 1; break; } }
                            args.push(c);
                            i += 1;
                        }
                        let pf = match name.as_str() {
                            "is" => PseudoFunc::Is(parse_selectors(&args)),
                            "where" => PseudoFunc::Where(parse_selectors(&args)),
                            "not" => PseudoFunc::Not(parse_selectors(&args)),
                            "has" => PseudoFunc::Has(parse_selectors(&args)),
                            "nth-child" => {
                                let (a, b) = parse_an_plus_b(&args);
                                PseudoFunc::NthChild { a, b, of_type: false, last: false }
                            }
                            "nth-last-child" => {
                                let (a, b) = parse_an_plus_b(&args);
                                PseudoFunc::NthChild { a, b, of_type: false, last: true }
                            }
                            "nth-of-type" => {
                                let (a, b) = parse_an_plus_b(&args);
                                PseudoFunc::NthChild { a, b, of_type: true, last: false }
                            }
                            "nth-last-of-type" => {
                                let (a, b) = parse_an_plus_b(&args);
                                PseudoFunc::NthChild { a, b, of_type: true, last: true }
                            }
                            _ => PseudoFunc::Unknown { name: name.clone(), args: args.clone() },
                        };
                        pseudo_funcs.push(pf);
                    } else {
                        pseudo_classes.push(name);
                    }
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
            tag, id, classes, attributes, pseudo_classes, pseudo_funcs, combinator,
        });
    }
    Selector { parts }
}

/// Parsuje `an+b` syntax pro :nth-* pseudo-classes.
/// Specialni: "odd" = (2, 1), "even" = (2, 0).
fn parse_an_plus_b(s: &str) -> (i32, i32) {
    let s = s.trim();
    if s == "odd" { return (2, 1); }
    if s == "even" { return (2, 0); }
    if s == "n" { return (1, 0); }
    if s == "-n" { return (-1, 0); }

    // Try plain integer "5"
    if let Ok(n) = s.parse::<i32>() { return (0, n); }

    // Pattern: "an" / "an+b" / "an-b" / "n+b"
    if let Some(n_pos) = s.find('n') {
        let a_str = &s[..n_pos];
        let a: i32 = match a_str {
            "" | "+" => 1,
            "-" => -1,
            _ => a_str.parse().unwrap_or(1),
        };
        let rest = s[n_pos + 1..].trim();
        let b: i32 = if rest.is_empty() { 0 } else { rest.replace(' ', "").parse().unwrap_or(0) };
        return (a, b);
    }
    (0, 0)
}

/// Konverze deklaraci na HashMap (property -> value).
pub fn declarations_to_map(decls: &[Declaration]) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for d in decls {
        m.insert(d.property.clone(), d.value.clone());
    }
    m
}
