/// CSS parser - obal nad cssparser crate.
///
/// Parsuje CSS stylesheet na seznam pravidel (Rule).
/// Kazde pravidlo ma selektor + deklarace (property: value).
/// Pro plne CSS3 selectors by se pouzil selectors crate, zde lite parser.

#[derive(Debug, Clone)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
    /// @media queries: kazda obsahuje query string + nested rules
    pub media_queries: Vec<MediaQuery>,
    /// @keyframes: name -> Vec<(percent, declarations)>
    pub keyframes: Vec<Keyframes>,
    /// @container queries: name (volitelny) + condition + rules
    pub container_queries: Vec<ContainerQuery>,
    /// @font-face declarations - kazdy ma family + src + dalsi properties.
    pub font_faces: Vec<FontFace>,
    /// @layer declarace order (jmeno -> priorita; pozdejsi = vyssi priorita).
    pub layer_order: Vec<String>,
    /// Rules patrici do layer: layer_name -> Vec<Rule>.
    /// Layered rules maji nizsi prio nez unlayered (per CSS Cascade Layers spec L5).
    pub layered_rules: Vec<(String, Vec<Rule>)>,
    /// @property --name registrace - meta info pro custom properties.
    pub registered_properties: Vec<RegisteredProperty>,
    /// @scope (root) [to (limit)] { rules } - scoped rules.
    pub scopes: Vec<ScopeRule>,
    /// @starting-style { rules } - styly aplikovane na zacatku transition.
    pub starting_style_rules: Vec<Rule>,
    /// @font-palette-values --name { font-family / base-palette / override-colors }.
    pub font_palettes: Vec<FontPalette>,
    /// @counter-style name { ... } - custom counter.
    pub counter_styles: Vec<CounterStyle>,
    /// @view-transition { navigation: auto } - global config.
    pub view_transition_navigation: Option<String>,
    /// @page rules - per-page declarations pro print.
    pub page_rules: Vec<Rule>,
    /// @function --name(<args>) <returns> { ... } - user-defined CSS functions (Functions L1).
    pub functions: Vec<CssFunction>,
}

/// CSS Functions L1 - user-defined function.
#[derive(Debug, Clone, Default)]
pub struct CssFunction {
    pub name: String,
    pub args: Vec<String>,
    pub returns: String,
    pub body: String,
}

#[derive(Debug, Clone, Default)]
pub struct FontPalette {
    pub name: String,
    pub font_family: String,
    pub base_palette: String,
    pub override_colors: Vec<(u32, String)>,
}

#[derive(Debug, Clone, Default)]
pub struct CounterStyle {
    pub name: String,
    pub system: String,
    pub symbols: String,
    pub suffix: String,
    pub prefix: String,
    pub range: String,
    pub pad: String,
    pub fallback: String,
    pub negative: String,
}

/// CSS @scope at-rule - root + optional limit.
/// `@scope (.card) to (.divider) { ... }`
#[derive(Debug, Clone, Default)]
pub struct ScopeRule {
    /// Selector pro root - element musi byt potomkem.
    pub root_selector: String,
    /// Optional limit selector - scope konci na techto elementech.
    pub limit_selector: Option<String>,
    /// Rules platici uvnitr scope.
    pub rules: Vec<Rule>,
}

/// CSS @property registrace pro custom properties (var(--foo)).
#[derive(Debug, Clone, Default)]
pub struct RegisteredProperty {
    /// Jmeno vc. -- prefix, napr. "--my-color".
    pub name: String,
    /// syntax descriptor (jen storage): "<color>", "<length>", "*"
    pub syntax: String,
    /// Inherits flag.
    pub inherits: bool,
    /// initial-value (volitelny).
    pub initial_value: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct FontFace {
    pub family: String,
    pub src: String, // url() / local() string raw
    pub weight: String,
    pub style: String,
    pub display: String,
}

#[derive(Debug, Clone)]
pub struct ContainerQuery {
    /// Volitelny container name: "@container card (min-width: 400px)" -> "card".
    /// Bez nazvu (`@container (min-width: ...)`) -> empty string -> match nejblizsiho ancestor s container-type.
    pub name: String,
    /// Surovy condition string napr. "(min-width: 400px)".
    pub condition: String,
    pub rules: Vec<Rule>,
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
    /// Pseudo-element ::before / ::after / ::marker / ::placeholder
    pub pseudo_element: Option<String>,
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
    /// `:lang(<language>)` - matches kdyz element nebo ancestor ma lang="<language>"
    /// (BCP 47 language tag prefix match: `:lang(en)` matches "en", "en-US", "en-GB").
    Lang(String),
    /// `:dir(ltr|rtl)` - matches pri direction attribute / computed direction.
    Dir(String),
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
    WordContains,  // [name~="value"] - whitespace separated
    DashMatch,     // [name|="value"] - exact or starts s "value-"
}

#[derive(Debug, Clone)]
pub enum Combinator {
    Descendant,
    Child,
    AdjacentSibling,
    GeneralSibling,
}

impl std::fmt::Display for SimpleSelector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(t) = &self.tag {
            f.write_str(t)?;
        } else if self.id.is_none() && self.classes.is_empty()
            && self.attributes.is_empty() && self.pseudo_classes.is_empty()
            && self.pseudo_funcs.is_empty() && self.pseudo_element.is_none() {
            f.write_str("*")?;
        }
        if let Some(id) = &self.id { write!(f, "#{}", id)?; }
        for c in &self.classes { write!(f, ".{}", c)?; }
        for a in &self.attributes {
            f.write_str("[")?;
            f.write_str(&a.name)?;
            match a.op {
                AttrOp::Exists => {}
                AttrOp::Equals => write!(f, "=\"{}\"", a.value.as_deref().unwrap_or(""))?,
                AttrOp::Contains => write!(f, "*=\"{}\"", a.value.as_deref().unwrap_or(""))?,
                AttrOp::StartsWith => write!(f, "^=\"{}\"", a.value.as_deref().unwrap_or(""))?,
                AttrOp::EndsWith => write!(f, "$=\"{}\"", a.value.as_deref().unwrap_or(""))?,
                AttrOp::WordContains => write!(f, "~=\"{}\"", a.value.as_deref().unwrap_or(""))?,
                AttrOp::DashMatch => write!(f, "|=\"{}\"", a.value.as_deref().unwrap_or(""))?,
            }
            f.write_str("]")?;
        }
        for p in &self.pseudo_classes { write!(f, ":{}", p)?; }
        for p in &self.pseudo_funcs {
            match p {
                PseudoFunc::Is(list) => write!(f, ":is({})", join_sels(list))?,
                PseudoFunc::Where(list) => write!(f, ":where({})", join_sels(list))?,
                PseudoFunc::Not(list) => write!(f, ":not({})", join_sels(list))?,
                PseudoFunc::Has(list) => write!(f, ":has({})", join_sels(list))?,
                PseudoFunc::NthChild { a, b, of_type, last } => {
                    let name = match (last, of_type) {
                        (true, true) => "nth-last-of-type",
                        (true, false) => "nth-last-child",
                        (false, true) => "nth-of-type",
                        (false, false) => "nth-child",
                    };
                    if *a == 0 { write!(f, ":{}({})", name, b)?; }
                    else if *b == 0 { write!(f, ":{}({}n)", name, a)?; }
                    else if *b > 0 { write!(f, ":{}({}n+{})", name, a, b)?; }
                    else { write!(f, ":{}({}n{})", name, a, b)?; }
                }
                PseudoFunc::Lang(l) => write!(f, ":lang({})", l)?,
                PseudoFunc::Dir(d) => write!(f, ":dir({})", d)?,
                PseudoFunc::Unknown { name, args } => write!(f, ":{}({})", name, args)?,
            }
        }
        if let Some(pe) = &self.pseudo_element { write!(f, "::{}", pe)?; }
        Ok(())
    }
}

fn join_sels(list: &[Selector]) -> String {
    list.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(", ")
}

impl std::fmt::Display for Selector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, part) in self.parts.iter().enumerate() {
            if i > 0 {
                let comb = match part.combinator {
                    Some(Combinator::Child) => " > ",
                    Some(Combinator::AdjacentSibling) => " + ",
                    Some(Combinator::GeneralSibling) => " ~ ",
                    _ => " ",
                };
                f.write_str(comb)?;
            }
            std::fmt::Display::fmt(part, f)?;
        }
        Ok(())
    }
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
                PseudoFunc::Lang(_) => { class_count += 1; }
                PseudoFunc::Dir(_) => { class_count += 1; }
                PseudoFunc::Unknown { .. } => { class_count += 1; }
            }
        }
    }
    (id_count, class_count, type_count)
}

/// CSS @supports condition evaluation (Conditional Rules L4).
/// Podporuje: (prop: value), selector(...), font-tech(...), font-format(...),
/// not (...), and / or operatory.
pub fn evaluate_supports(condition: &str) -> bool {
    let c = condition.trim();
    // Cely condition v zavorkach -> unwrap
    if let Some(inner) = strip_outer_parens(c) {
        return evaluate_supports(inner);
    }
    // not <cond>
    if let Some(rest) = c.strip_prefix("not ") {
        return !evaluate_supports(rest.trim());
    }
    // and / or - hleda top-level operator
    if let Some((left, right, is_and)) = split_supports_op(c) {
        return if is_and {
            evaluate_supports(left.trim()) && evaluate_supports(right.trim())
        } else {
            evaluate_supports(left.trim()) || evaluate_supports(right.trim())
        };
    }
    // selector(<sel>) - zda umime parse selector. Aktualne assume yes.
    if let Some(_inner) = c.strip_prefix("selector(").and_then(|s| s.strip_suffix(')')) {
        return true;
    }
    // font-tech(<keyword>) - color-COLRv1, color-CBDT, ...
    if let Some(inner) = c.strip_prefix("font-tech(").and_then(|s| s.strip_suffix(')')) {
        let tech = inner.trim();
        // Vetsina moderni: variations, palettes, color-COLRv1, features-aat, features-graphite
        return matches!(tech,
            "variations" | "palettes" | "color-COLRv0" | "color-COLRv1" |
            "color-SVG" | "color-sbix" | "color-CBDT" |
            "features-opentype" | "features-aat" | "features-graphite" |
            "incremental"
        );
    }
    // font-format(<keyword>) - opentype, woff2, woff, truetype, ...
    if let Some(inner) = c.strip_prefix("font-format(").and_then(|s| s.strip_suffix(')')) {
        let fmt = inner.trim().trim_matches('"').trim_matches('\'');
        return matches!(fmt,
            "opentype" | "truetype" | "woff" | "woff2" | "embedded-opentype" |
            "svg" | "collection"
        );
    }
    // (prop: value) - assume yes (vsechny props parsujem)
    if let Some(inner) = c.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
        if inner.contains(':') {
            return true;
        }
    }
    // Default - assume yes
    true
}

fn strip_outer_parens(s: &str) -> Option<&str> {
    let s = s.trim();
    if !s.starts_with('(') || !s.ends_with(')') { return None; }
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate() {
        match b { b'(' => depth += 1, b')' => depth -= 1, _ => {} }
        if depth == 0 && i < bytes.len() - 1 { return None; }
    }
    Some(&s[1..s.len()-1])
}

fn split_supports_op(s: &str) -> Option<(&str, &str, bool)> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            // Hledame " and " nebo " or " na top-level
            if i + 4 < bytes.len() && bytes[i] == b' '
                && &bytes[i+1..i+4] == b"and" && bytes[i+4] == b' ' {
                return Some((&s[..i], &s[i+5..], true));
            }
            if i + 3 < bytes.len() && bytes[i] == b' '
                && &bytes[i+1..i+3] == b"or" && bytes[i+3] == b' ' {
                return Some((&s[..i], &s[i+4..], false));
            }
        }
        i += 1;
    }
    None
}

/// Parsuje @scope header: "(root_sel) [to (limit_sel)]" -> (root, optional limit).
pub fn parse_scope_header(header: &str) -> (String, Option<String>) {
    let h = header.trim();
    // Najdi prvni (...) blok pro root, pak optional " to (...)" pro limit.
    let bytes = h.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i] != b'(' { i += 1; }
    if i >= bytes.len() { return (String::new(), None); }
    let mut depth = 1i32; let start = i + 1;
    let mut j = start;
    while j < bytes.len() && depth > 0 {
        match bytes[j] { b'(' => depth += 1, b')' => depth -= 1, _ => {} }
        if depth == 0 { break; }
        j += 1;
    }
    let root = h[start..j].trim().to_string();
    let rest = if j + 1 < bytes.len() { h[j+1..].trim() } else { "" };
    let limit = if let Some(rest) = rest.strip_prefix("to") {
        let r = rest.trim();
        if let Some(open) = r.find('(') {
            let mut d = 1i32; let st = open + 1;
            let mut e = st; let rb = r.as_bytes();
            while e < rb.len() && d > 0 {
                match rb[e] { b'(' => d += 1, b')' => d -= 1, _ => {} }
                if d == 0 { break; }
                e += 1;
            }
            Some(r[st..e].trim().to_string())
        } else { None }
    } else { None };
    (root, limit)
}

/// Parsuje CSS stylesheet (lite parser, hand-rolled).
pub fn parse_stylesheet(source: &str) -> Stylesheet {
    let mut rules = Vec::new();
    let mut media_queries = Vec::new();
    let mut keyframes = Vec::new();
    let mut container_queries = Vec::new();
    let mut font_faces = Vec::new();
    let mut layer_order: Vec<String> = Vec::new();
    let mut layered_rules: Vec<(String, Vec<Rule>)> = Vec::new();
    let mut registered_properties: Vec<RegisteredProperty> = Vec::new();
    let mut scopes: Vec<ScopeRule> = Vec::new();
    let mut starting_style_rules: Vec<Rule> = Vec::new();
    let mut font_palettes: Vec<FontPalette> = Vec::new();
    let mut counter_styles: Vec<CounterStyle> = Vec::new();
    let mut view_transition_navigation: Option<String> = None;
    let mut page_rules: Vec<Rule> = Vec::new();
    let mut functions: Vec<CssFunction> = Vec::new();
    let mut chars = source.chars().peekable();

    while chars.peek().is_some() {
        skip_whitespace_and_comments(&mut chars);
        if chars.peek().is_none() { break; }

        // Read selectors / at-rule until '{' or ';'. Respekt quote/paren tak ze
        // `;` v url("a;b") / `@import url("data:text/css;base64,...");` nezarezne
        // selector predcasne.
        let mut selectors_str = String::new();
        let mut has_block = false;
        let mut sel_quote: Option<char> = None;
        let mut sel_paren = 0i32;
        let mut sel_brack = 0i32;
        let mut sel_prev = ' ';
        while let Some(&c) = chars.peek() {
            if let Some(q) = sel_quote {
                selectors_str.push(c);
                chars.next();
                if sel_prev != '\\' && c == q { sel_quote = None; }
                sel_prev = c;
                continue;
            }
            match c {
                '"' | '\'' => { sel_quote = Some(c); selectors_str.push(c); chars.next(); }
                '(' => { sel_paren += 1; selectors_str.push(c); chars.next(); }
                ')' => { sel_paren -= 1; selectors_str.push(c); chars.next(); }
                '[' => { sel_brack += 1; selectors_str.push(c); chars.next(); }
                ']' => { sel_brack -= 1; selectors_str.push(c); chars.next(); }
                '{' if sel_paren == 0 && sel_brack == 0 => {
                    chars.next(); has_block = true; break;
                }
                ';' if sel_paren == 0 && sel_brack == 0 => {
                    chars.next(); break;
                }
                _ => { selectors_str.push(c); chars.next(); }
            }
            sel_prev = c;
        }
        let selectors_str = selectors_str.trim().to_string();
        if selectors_str.is_empty() { break; }

        // @import / @charset / @layer (bez bloku - jen order declaration) - handle pred general skip
        if !has_block {
            if selectors_str.starts_with("@layer") {
                let rest = selectors_str.trim_start_matches("@layer").trim();
                for name in rest.split(',') {
                    let n = name.trim().to_string();
                    if !n.is_empty() && !layer_order.contains(&n) { layer_order.push(n); }
                }
            } else if selectors_str.starts_with("@import") {
                // @import "url" [layer(name)] [supports(...)] [media];
                // Aktualne nenacitame externi soubory (TODO HTTP fetch + recursive parse).
                // Aspon detect layer() pro registraci layer name.
                let rest = selectors_str.trim_start_matches("@import").trim();
                if let Some(start) = rest.find("layer(") {
                    let after = &rest[start + 6..];
                    if let Some(end) = after.find(')') {
                        let name = after[..end].trim().to_string();
                        if !name.is_empty() && !layer_order.contains(&name) { layer_order.push(name); }
                    }
                }
            } else if selectors_str.starts_with("@charset")
                || selectors_str.starts_with("@namespace") {
                // No-op (pre-parser pro ECMA / vendor prefixes)
            }
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
        } else if selectors_str.starts_with("@layer") {
            // @layer name1, name2, name3;  - declarace order bez bloku
            // @layer name { rules } - rules v layeru
            let rest = selectors_str.trim_start_matches("@layer").trim();
            if !has_block {
                // Order declaration
                for name in rest.split(',') {
                    let n = name.trim().to_string();
                    if !n.is_empty() && !layer_order.contains(&n) { layer_order.push(n); }
                }
                continue;
            }
            // S blokem: rules patrici do layer
            let layer_name = rest.split(',').next().unwrap_or("").trim().to_string();
            if !layer_name.is_empty() && !layer_order.contains(&layer_name) {
                layer_order.push(layer_name.clone());
            }
            let nested = parse_stylesheet(&block_str);
            layered_rules.push((layer_name, nested.rules));
        } else if selectors_str.starts_with("@supports") {
            // @supports <condition> { rules }
            // Conditions: (prop: value), selector(...), font-tech(...), font-format(...),
            //             not (...), (...) and (...), (...) or (...).
            let condition = selectors_str.trim_start_matches("@supports").trim();
            if evaluate_supports(condition) {
                let nested = parse_stylesheet(&block_str);
                rules.extend(nested.rules);
            }
        } else if selectors_str.starts_with("@scope") {
            // @scope (root_selector) [to (limit_selector)] { rules }
            let header = selectors_str.trim_start_matches("@scope").trim();
            let (root_sel, limit_sel) = parse_scope_header(header);
            let nested = parse_stylesheet(&block_str);
            scopes.push(ScopeRule {
                root_selector: root_sel,
                limit_selector: limit_sel,
                rules: nested.rules,
            });
        } else if selectors_str.starts_with("@starting-style") {
            // @starting-style { rules } - styly pro transition start state
            let nested = parse_stylesheet(&block_str);
            starting_style_rules.extend(nested.rules);
        } else if selectors_str.starts_with("@document") {
            // Mozilla @document - rules unwrap (treat as if matched)
            let nested = parse_stylesheet(&block_str);
            rules.extend(nested.rules);
        } else if selectors_str.starts_with("@view-transition") {
            // @view-transition { navigation: auto }
            for d in parse_decls_str(&block_str) {
                if d.property == "navigation" {
                    view_transition_navigation = Some(d.value.trim().to_string());
                }
            }
        } else if selectors_str.starts_with("@counter-style") {
            let name = selectors_str.trim_start_matches("@counter-style").trim().to_string();
            let mut cs = CounterStyle { name, ..Default::default() };
            for d in parse_decls_str(&block_str) {
                match d.property.as_str() {
                    "system" => cs.system = d.value.trim().to_string(),
                    "symbols" => cs.symbols = d.value.trim().to_string(),
                    "suffix" => cs.suffix = d.value.trim().to_string(),
                    "prefix" => cs.prefix = d.value.trim().to_string(),
                    "range" => cs.range = d.value.trim().to_string(),
                    "pad" => cs.pad = d.value.trim().to_string(),
                    "fallback" => cs.fallback = d.value.trim().to_string(),
                    "negative" => cs.negative = d.value.trim().to_string(),
                    _ => {}
                }
            }
            counter_styles.push(cs);
        } else if selectors_str.starts_with("@font-palette-values") {
            // @font-palette-values --name { font-family / base-palette / override-colors }
            let name = selectors_str.trim_start_matches("@font-palette-values").trim().to_string();
            let mut fp = FontPalette { name, ..Default::default() };
            for d in parse_decls_str(&block_str) {
                match d.property.as_str() {
                    "font-family" => fp.font_family = d.value.trim().to_string(),
                    "base-palette" => fp.base_palette = d.value.trim().to_string(),
                    "override-colors" => {
                        // "0 red, 1 blue, 2 green" -> parsuje pairs
                        for part in d.value.split(',') {
                            let part = part.trim();
                            if let Some(idx) = part.find(' ') {
                                let (n, c) = part.split_at(idx);
                                if let Ok(num) = n.parse::<u32>() {
                                    fp.override_colors.push((num, c.trim().to_string()));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            font_palettes.push(fp);
        } else if selectors_str.starts_with("@page") {
            let decls = parse_decls_str(&block_str);
            page_rules.push(Rule {
                selectors: vec![],
                declarations: decls,
            });
        } else if selectors_str.starts_with("@function") {
            // @function --name(<arg1>, <arg2>) returns <type> { result: <expr>; }
            let header = selectors_str.trim_start_matches("@function").trim();
            let mut func = CssFunction::default();
            // Parse name(args) returns Type
            if let Some(open) = header.find('(') {
                func.name = header[..open].trim().to_string();
                if let Some(close) = header[open..].find(')') {
                    let args_str = &header[open+1..open+close];
                    func.args = args_str.split(',').map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty()).collect();
                    let after = &header[open+close+1..].trim();
                    if let Some(rest) = after.strip_prefix("returns") {
                        func.returns = rest.trim().to_string();
                    }
                }
            }
            func.body = block_str.clone();
            functions.push(func);
        } else if selectors_str.starts_with("@font-feature-values") {
            // @font-feature-values FontName { @styleset {} ... } - parse only
            let _ = block_str;
        } else if selectors_str.starts_with("@property") {
            // @property --name { syntax/inherits/initial-value }
            let name = selectors_str.trim_start_matches("@property").trim().to_string();
            let mut prop = RegisteredProperty { name, ..Default::default() };
            for d in parse_decls_str(&block_str) {
                match d.property.as_str() {
                    "syntax" => prop.syntax = d.value.trim().trim_matches('"').trim_matches('\'').to_string(),
                    "inherits" => prop.inherits = matches!(d.value.trim(), "true"),
                    "initial-value" => prop.initial_value = Some(d.value.clone()),
                    _ => {}
                }
            }
            registered_properties.push(prop);
        } else if selectors_str.starts_with("@font-face") {
            let mut ff = FontFace::default();
            let decls = parse_decls_str(&block_str);
            for d in decls {
                match d.property.as_str() {
                    "font-family" => ff.family = d.value.trim_matches('"').trim_matches('\'').to_string(),
                    "src"         => ff.src = d.value.clone(),
                    "font-weight" => ff.weight = d.value.clone(),
                    "font-style"  => ff.style = d.value.clone(),
                    "font-display" => ff.display = d.value.clone(),
                    _ => {}
                }
            }
            if !ff.family.is_empty() { font_faces.push(ff); }
        } else if selectors_str.starts_with("@container") {
            // Format: @container [name] (condition) { rules }
            let rest = selectors_str.trim_start_matches("@container").trim();
            let (name, condition) = parse_container_header(rest);
            let nested = parse_stylesheet(&block_str);
            container_queries.push(ContainerQuery { name, condition, rules: nested.rules });
        } else if selectors_str.starts_with('@') {
            // Ostatni at-rules ignorujeme (@import, @font-face, ...)
        } else {
            let selectors = parse_selectors(&selectors_str);
            // CSS Nesting L1: zpracuj block s ohledem na nested rulesets
            process_nested_block(&selectors, &block_str, &mut rules);
        }
    }

    Stylesheet {
        rules, media_queries, keyframes, container_queries, font_faces,
        layer_order, layered_rules, registered_properties,
        scopes, starting_style_rules,
        font_palettes, counter_styles, view_transition_navigation, page_rules,
        functions,
    }
}

/// Range syntax @media L4: `400px <= width <= 800px`, `width < 600px`, `400px < width`.
/// Vrati Some(true/false) pri detekci range, None kdyz to neni range query.
fn eval_range_query(c: &str, vw: f32, vh: f32) -> Option<bool> {
    // Pokud neobsahuje '<' ani '>', neni range
    if !c.contains('<') && !c.contains('>') { return None; }
    // Tokenize na (a, op1, b, op2, c) nebo (a, op, b)
    // Replace <= / >= / < / > whitespace-padded
    let s = c.replace("<=", " <= ").replace(">=", " >= ")
             .replace('<', " < ").replace('>', " > ");
    let toks: Vec<&str> = s.split_whitespace().collect();
    let parse_val = |t: &str| -> Option<f32> {
        if t == "width" { return Some(vw); }
        if t == "height" { return Some(vh); }
        let n = t.trim_end_matches("px").trim();
        n.parse().ok()
    };
    let cmp = |a: f32, op: &str, b: f32| -> bool {
        match op {
            "<"  => a <  b,
            "<=" => a <= b,
            ">"  => a >  b,
            ">=" => a >= b,
            _    => true,
        }
    };
    if toks.len() == 5 {
        let (a, op1, b, op2, cc) = (toks[0], toks[1], toks[2], toks[3], toks[4]);
        let av = parse_val(a)?;
        let bv = parse_val(b)?;
        let cv = parse_val(cc)?;
        return Some(cmp(av, op1, bv) && cmp(bv, op2, cv));
    }
    if toks.len() == 3 {
        let av = parse_val(toks[0])?;
        let bv = parse_val(toks[2])?;
        return Some(cmp(av, toks[1], bv));
    }
    None
}

/// Vytahne URL z @font-face src deklarace: `src: url("foo.woff2") format("woff2")` -> "foo.woff2".
pub fn extract_font_url(src: &str) -> Option<String> {
    let s = src.trim();
    let url_idx = s.find("url(")?;
    let after = &s[url_idx + 4..];
    let close = after.find(')')?;
    let raw = &after[..close];
    Some(raw.trim().trim_matches('"').trim_matches('\'').to_string())
}

/// Rozpojuje "@container [name] (condition)" header na (name, condition).
/// `@container card (min-width: 400px)` -> ("card", "(min-width: 400px)")
/// `@container (min-width: 400px)`      -> ("", "(min-width: 400px)")
fn parse_container_header(s: &str) -> (String, String) {
    let s = s.trim();
    if let Some(paren_idx) = s.find('(') {
        let before = s[..paren_idx].trim();
        let cond = s[paren_idx..].trim().to_string();
        return (before.to_string(), cond);
    }
    (String::new(), s.to_string())
}

/// Vyhodnoti @container query proti (container_w, container_h).
/// Same syntax as @media (min-width / max-width / orientation / ...).
pub fn evaluate_container_query(condition: &str, container_w: f32, container_h: f32) -> bool {
    evaluate_media_query(condition, container_w, container_h)
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
    let q = query.trim().to_lowercase();
    // Print media disable: stranky ne pri print neaplikuji print rules.
    // Real browser dela switch screen/print mode. Default = screen.
    if q == "print" || q.starts_with("print ") || q.contains(" print ") || q.ends_with(" print") {
        return false;
    }
    // Strip type "screen"/"all" + "and"
    let q = q.replace("screen", "").replace("all", "").replace(" and ", " ");
    // Range syntax L4: "(400px <= width <= 800px)" / "(width < 600px)" / "(width >= 800px)"
    // Try detect <= / >= / < / >
    for cond_raw in q.split(')').filter(|s| !s.trim().is_empty()) {
        let c = cond_raw.trim_start_matches('(').trim();
        if c.is_empty() { continue; }
        // Range: a OP1 b OP2 c
        if let Some(eval) = eval_range_query(c, viewport_w, viewport_h) {
            if !eval { return false; }
            continue;
        }
    }
    // Conditions oddelene zavorkami
    for cond in q.split(')').filter(|s| !s.trim().is_empty()) {
        let c = cond.trim().trim_start_matches('(').trim();
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
                // User preferences - default rozumne (light theme, no reduced motion, hover/pointer dostupny)
                "prefers-color-scheme" => {
                    let dark = std::env::var("RUST_WEB_ENGINE_DARK").is_ok();
                    if val == "dark" && !dark { return false; }
                    if val == "light" && dark { return false; }
                }
                "prefers-reduced-motion" => {
                    let reduced = std::env::var("RUST_WEB_ENGINE_REDUCED_MOTION").is_ok();
                    if val == "reduce" && !reduced { return false; }
                    if val == "no-preference" && reduced { return false; }
                }
                "hover" => {
                    // Default: hover dostupny
                    if val == "none" { return false; }
                }
                "pointer" => {
                    // Default: fine pointer (mouse)
                    if val == "coarse" { return false; }
                    if val == "none" { return false; }
                }
                "any-hover" | "any-pointer" => { /* match default fine/hover */ }
                "display-mode" => {
                    // Default: browser
                    if val != "browser" && val != "fullscreen" { return false; }
                }
                "forced-colors" => {
                    // Default: none
                    if val == "active" { return false; }
                }
                "color" => {
                    // Default: 8 (256 colors per channel)
                    if val == "0" { return false; }
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
    // Top-level split na `;` ale respektuj ("..."), ('...'), (..), [..], /*..*/.
    // Bez tohoto minified CSS s `;` uvnitr url("data:..;base64,..") nebo
    // `background-image: url(http://a/b?q=1;p=2)` rozsekalo declarace nesmyslne.
    let mut stmts: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut depth_paren = 0i32;
    let mut depth_brack = 0i32;
    let mut quote: Option<char> = None;
    let mut in_block_comment = false;
    let mut prev_ch: char = ' ';
    for ch in block.chars() {
        if in_block_comment {
            if prev_ch == '*' && ch == '/' { in_block_comment = false; }
            prev_ch = ch;
            continue;
        }
        if let Some(q) = quote {
            cur.push(ch);
            // Escape \" / \' uvnitr stringu (CSS spec: backslash escape).
            if prev_ch != '\\' && ch == q { quote = None; }
            prev_ch = ch;
            continue;
        }
        if prev_ch == '/' && ch == '*' {
            // Strip otevirany `/` z cur.
            cur.pop();
            in_block_comment = true;
            prev_ch = ch;
            continue;
        }
        match ch {
            '"' | '\'' => { quote = Some(ch); cur.push(ch); }
            '(' => { depth_paren += 1; cur.push(ch); }
            ')' => { depth_paren -= 1; cur.push(ch); }
            '[' => { depth_brack += 1; cur.push(ch); }
            ']' => { depth_brack -= 1; cur.push(ch); }
            ';' if depth_paren == 0 && depth_brack == 0 => {
                if !cur.trim().is_empty() { stmts.push(std::mem::take(&mut cur)); }
                else { cur.clear(); }
            }
            _ => cur.push(ch),
        }
        prev_ch = ch;
    }
    if !cur.trim().is_empty() { stmts.push(cur); }

    for stmt in stmts {
        let stmt = stmt.trim();
        if stmt.is_empty() { continue; }
        // Najit prvni top-level `:` (mezi prop a value). `:` v hodnote (url("a:b"),
        // pseudo-attr value) nepocita - quote/paren aware scan.
        let mut colon: Option<usize> = None;
        let mut dp = 0i32;
        let mut db = 0i32;
        let mut q: Option<char> = None;
        let mut prev: char = ' ';
        for (i, ch) in stmt.char_indices() {
            if let Some(qc) = q {
                if prev != '\\' && ch == qc { q = None; }
                prev = ch;
                continue;
            }
            match ch {
                '"' | '\'' => { q = Some(ch); }
                '(' => dp += 1,
                ')' => dp -= 1,
                '[' => db += 1,
                ']' => db -= 1,
                ':' if dp == 0 && db == 0 => { colon = Some(i); break; }
                _ => {}
            }
            prev = ch;
        }
        if let Some(colon) = colon {
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

pub fn parse_selectors(s: &str) -> Vec<Selector> {
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

/// Vlozi whitespace okolo top-level combinator znaku (`>`, `+`, `~`) tak aby
/// nasledny whitespace-split ten symbol oddelil. Bez tohoto `.a>.b`, `.a+.b`
/// padly do split jako jeden token a parser je necetl jako Combinator -> child
/// combinator selectory ignoroval (nektere styly se neaplikovaly, viz
/// mileneckaseznamka.cz `.min-box > .text-box`).
/// Pozor: `~`, `+`, `>` uvnitr `[attr~="..."]` / `[attr+="..."]` NE-rozsekat -
/// tracking depth_brack > 0.
fn pad_combinators(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    let mut depth_paren = 0i32;
    let mut depth_brack = 0i32;
    let mut quote: Option<char> = None;
    for ch in s.chars() {
        if let Some(q) = quote {
            out.push(ch);
            if ch == q { quote = None; }
            continue;
        }
        match ch {
            '"' | '\'' => { quote = Some(ch); out.push(ch); }
            '(' => { depth_paren += 1; out.push(ch); }
            ')' => { depth_paren -= 1; out.push(ch); }
            '[' => { depth_brack += 1; out.push(ch); }
            ']' => { depth_brack -= 1; out.push(ch); }
            '>' | '+' | '~' if depth_paren == 0 && depth_brack == 0 => {
                out.push(' ');
                out.push(ch);
                out.push(' ');
            }
            _ => out.push(ch),
        }
    }
    out
}

fn parse_single_selector(s: &str) -> Selector {
    let s_padded = pad_combinators(s);
    let s = s_padded.as_str();
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
        let mut pseudo_element: Option<String> = None;

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
            // PERF: lowercase pri PARSE time (jednou per selector) misto pri kazdem
            // matches_simple call (drive `want_tag.to_lowercase()` alocovaný String
            // milion× per second pri cascade).
            if !tag_buf.is_empty() { tag = Some(tag_buf.to_lowercase()); }
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
                    // Detekce pseudo-elementu pres `::` nebo legacy `:before`/`:after`
                    let mut is_pseudo_element_syntax = false;
                    if i < chars.len() && chars[i] == ':' { i += 1; is_pseudo_element_syntax = true; }
                    let mut name = String::new();
                    while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
                        name.push(chars[i]); i += 1;
                    }
                    let is_pe_name = matches!(name.as_str(),
                        "before" | "after" | "first-line" | "first-letter"
                        | "marker" | "placeholder" | "backdrop" | "selection");
                    if is_pseudo_element_syntax || is_pe_name {
                        // Pseudo-element: ulozit do pseudo_element pole
                        if pseudo_element.is_none() {
                            pseudo_element = Some(name);
                        }
                        continue;
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
                            "lang" => PseudoFunc::Lang(args.trim().trim_matches('"').trim_matches('\'').to_string()),
                            "dir" => PseudoFunc::Dir(args.trim().to_lowercase()),
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
                        && !chars[i].is_whitespace()
                    {
                        name.push(chars[i]); i += 1;
                    }
                    // Skip whitespace mezi name a operator/].
                    while i < chars.len() && chars[i].is_whitespace() { i += 1; }
                    let name = name.trim().to_string();
                    let mut op = AttrOp::Exists;
                    let mut value: Option<String> = None;
                    if i < chars.len() && chars[i] != ']' {
                        // Detekce operatoru (vc. `|=` = DashMatch - lang|=en match `en`/`en-US`).
                        op = match chars[i] {
                            '=' => AttrOp::Equals,
                            '~' => { i += 1; AttrOp::WordContains }
                            '^' => { i += 1; AttrOp::StartsWith }
                            '$' => { i += 1; AttrOp::EndsWith }
                            '*' => { i += 1; AttrOp::Contains }
                            '|' => { i += 1; AttrOp::DashMatch }
                            _   => AttrOp::Exists,
                        };
                        if i < chars.len() && chars[i] == '=' { i += 1; }
                        while i < chars.len() && chars[i].is_whitespace() { i += 1; }
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
            tag, id, classes, attributes, pseudo_classes, pseudo_funcs,
            pseudo_element, combinator,
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

