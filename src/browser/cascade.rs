/// CSS cascade - aplikace stylesheets na DOM strom.
///
/// Vrati StyleMap: pro kazdy element computed styles (HashMap<String, String>).
/// Specificita rozhoduje pri kolizi.

use std::collections::HashMap;
use std::rc::Rc;
use super::dom::{Node, NodeKind};
use super::css_parser::{Stylesheet, Selector, SimpleSelector, Combinator, Rule, specificity};
use super::computed_style::{
    AlignContent as CsAlignContent, AlignItems as CsAlignItems,
    AlignSelf as CsAlignSelf, BorderStyle as CsBorderStyle,
    BoxSizing as CsBoxSizing, CascadeOutput, CascadeDecl,
    Clear as CsClear, Color, ComputedStyle, ComputedStyleMap, Cursor,
    DeclarationsMap, Direction as CsDirection, Display as CsDisplay,
    FlexBasis as CsFlexBasis, FlexDirection as CsFlexDirection,
    FlexWrap as CsFlexWrap, Float as CsFloat, FontFamily, GenericFamily,
    JustifyContent as CsJustifyContent, Length, LineHeight,
    Overflow as CsOverflow, OverflowWrap, PointerEvents as CsPointerEvents,
    PositionKind, PropertyId, CascadeOrigin, Specificity as CsSpec,
    AnimationDirection as CsAnimationDirection,
    AnimationFillMode as CsAnimationFillMode,
    AnimationPlayState as CsAnimationPlayState,
    BlendMode as CsBlendMode, GridAutoFlow as CsGridAutoFlow, GridLine as CsGridLine,
    Appearance as CsAppearance, BackgroundAttachment as CsBackgroundAttachment,
    BackgroundClip as CsBackgroundClip,
    ColumnCount as CsColumnCount, ColumnFill as CsColumnFill, ColumnSpan as CsColumnSpan,
    OverscrollBehavior as CsOverscrollBehavior, ScrollSnapAlign as CsScrollSnapAlign,
    Isolation as CsIsolation, JustifyItems as CsJustifyItems,
    JustifySelf as CsJustifySelf, ScrollBehavior as CsScrollBehavior,
    BorderCollapse as CsBorderCollapse, CaptionSide as CsCaptionSide,
    ListStyleImage as CsListStyleImage, ListStylePosition as CsListStylePosition,
    ListStyleType as CsListStyleType, ObjectFit as CsObjectFit, Resize as CsResize,
    TableLayout as CsTableLayout,
    TextAlign as CsTextAlign, TextDecorationLine as CsTextDecorationLine,
    TextDecorationStyle as CsTextDecorationStyle, TextOverflow as CsTextOverflow,
    TextTransform as CsTextTransform, VerticalAlign as CsVerticalAlign,
    Visibility, WhiteSpace, WordBreak, WritingMode as CsWritingMode, ZIndex,
};

// Runtime UI state pres thread-local. Nastavuje render loop pred kazdym
// cascade pass; matches_selector cte pro :hover / :active / :focus / :focus-within.
thread_local! {
    static HOVERED_NODE: std::cell::RefCell<Option<usize>> = std::cell::RefCell::new(None);
    static ACTIVE_NODE: std::cell::RefCell<Option<usize>> = std::cell::RefCell::new(None);
    static FOCUSED_NODE: std::cell::RefCell<Option<usize>> = std::cell::RefCell::new(None);
}

/// Set hovered element (= node id z Rc::as_ptr cast as usize). None = zadny.
pub fn set_hovered_node(id: Option<usize>) { HOVERED_NODE.with(|c| *c.borrow_mut() = id); }
pub fn get_hovered_node() -> Option<usize> { HOVERED_NODE.with(|c| *c.borrow()) }
pub fn set_active_node(id: Option<usize>) { ACTIVE_NODE.with(|c| *c.borrow_mut() = id); }
pub fn set_focused_node(id: Option<usize>) { FOCUSED_NODE.with(|c| *c.borrow_mut() = id); }
pub fn get_focused_node() -> Option<usize> { FOCUSED_NODE.with(|c| *c.borrow()) }

fn current_node_id(node: &Rc<Node>) -> usize { Rc::as_ptr(node) as usize }
fn is_node_match(node: &Rc<Node>, cell: &'static std::thread::LocalKey<std::cell::RefCell<Option<usize>>>) -> bool {
    let id = current_node_id(node);
    cell.with(|c| c.borrow().map(|x| x == id).unwrap_or(false))
}
fn is_node_or_ancestor_match(node: &Rc<Node>, cell: &'static std::thread::LocalKey<std::cell::RefCell<Option<usize>>>) -> bool {
    let target = cell.with(|c| *c.borrow());
    let target = match target { Some(t) => t, None => return false };
    let mut cur: Option<Rc<Node>> = Some(Rc::clone(node));
    while let Some(n) = cur {
        if current_node_id(&n) == target { return true; }
        cur = n.parent.borrow().upgrade();
    }
    false
}

/// Expanduje CSS shorthand props (margin/padding/border) do longhand.
/// Napr. "margin: 10px 20px;" -> margin-top:10, margin-right:20, margin-bottom:10, margin-left:20.
/// "border: 1px solid red;" -> border-width:1, border-style:solid, border-color:red.
pub fn expand_shorthand(prop: &str, value: &str, out: &mut HashMap<String, String>) {
    // CSS Logical Properties L1 - mapping na fyzicke (predpokladam LTR + horizontal-tb)
    if let Some(physical) = logical_to_physical(prop) {
        out.insert(physical.into(), value.into());
        out.insert(prop.into(), value.into()); // zachovat puvodni jmeno
        return;
    }
    // Logical shorthand (margin-block, margin-inline, inset)
    if let Some((p1, p2)) = logical_shorthand_pair(prop) {
        let parts: Vec<&str> = value.split_whitespace().collect();
        let (a, b) = match parts.len() {
            1 => (parts[0], parts[0]),
            2 => (parts[0], parts[1]),
            _ => (parts[0], parts.get(1).copied().unwrap_or(parts[0])),
        };
        out.insert(p1.into(), a.into());
        out.insert(p2.into(), b.into());
        out.insert(prop.into(), value.into());
        return;
    }
    // place-content / place-items / place-self shorthandy: <align> <justify>
    if matches!(prop, "place-content" | "place-items" | "place-self") {
        let parts: Vec<&str> = value.split_whitespace().collect();
        let (align, justify) = match parts.len() {
            1 => (parts[0], parts[0]),
            _ => (parts[0], parts[1]),
        };
        let (align_prop, justify_prop) = match prop {
            "place-content" => ("align-content", "justify-content"),
            "place-items"   => ("align-items", "justify-items"),
            "place-self"    => ("align-self", "justify-self"),
            _ => unreachable!(),
        };
        out.insert(align_prop.into(), align.into());
        out.insert(justify_prop.into(), justify.into());
        out.insert(prop.into(), value.into());
        return;
    }
    // gap shorthand: <row-gap> <column-gap>
    if prop == "gap" {
        let parts: Vec<&str> = value.split_whitespace().collect();
        let (row, col) = match parts.len() {
            1 => (parts[0], parts[0]),
            _ => (parts[0], parts[1]),
        };
        out.insert("row-gap".into(), row.into());
        out.insert("column-gap".into(), col.into());
        out.insert("gap".into(), value.into());
        return;
    }
    // border-radius shorthand: 1-4 values -> 4 corners.
    // 1 -> all, 2 -> tl/br=v1, tr/bl=v2, 3 -> tl=v1, tr/bl=v2, br=v3,
    // 4 -> tl tr br bl. Ignore "/" syntax pro elliptical radii.
    if prop == "border-radius" {
        let main = value.split('/').next().unwrap_or(value);
        let parts: Vec<&str> = main.split_whitespace().collect();
        if parts.is_empty() { return; }
        let (tl, tr, br, bl) = match parts.len() {
            1 => (parts[0], parts[0], parts[0], parts[0]),
            2 => (parts[0], parts[1], parts[0], parts[1]),
            3 => (parts[0], parts[1], parts[2], parts[1]),
            _ => (parts[0], parts[1], parts[2], parts[3]),
        };
        out.insert("border-top-left-radius".into(), tl.into());
        out.insert("border-top-right-radius".into(), tr.into());
        out.insert("border-bottom-right-radius".into(), br.into());
        out.insert("border-bottom-left-radius".into(), bl.into());
        out.insert(prop.into(), value.into());
        return;
    }
    // border-width / border-style / border-color: 1-4 parts -> per-side.
    if matches!(prop, "border-width" | "border-style" | "border-color") {
        let parts: Vec<&str> = value.split_whitespace().collect();
        if parts.is_empty() { return; }
        let (t, r, b, l) = match parts.len() {
            1 => (parts[0], parts[0], parts[0], parts[0]),
            2 => (parts[0], parts[1], parts[0], parts[1]),
            3 => (parts[0], parts[1], parts[2], parts[1]),
            _ => (parts[0], parts[1], parts[2], parts[3]),
        };
        let suffix = match prop {
            "border-width" => "width",
            "border-style" => "style",
            "border-color" => "color",
            _ => unreachable!(),
        };
        out.insert(format!("border-top-{suffix}"), t.into());
        out.insert(format!("border-right-{suffix}"), r.into());
        out.insert(format!("border-bottom-{suffix}"), b.into());
        out.insert(format!("border-left-{suffix}"), l.into());
        out.insert(prop.into(), value.into());
        return;
    }
    if prop == "overflow" {
        // overflow: <val> -> -x + -y. overflow: <x> <y> -> rozdelit.
        let parts: Vec<&str> = value.split_whitespace().collect();
        let (x, y) = match parts.len() {
            1 => (parts[0], parts[0]),
            _ => (parts[0], parts[1]),
        };
        out.insert("overflow-x".into(), x.into());
        out.insert("overflow-y".into(), y.into());
        out.insert("overflow".into(), value.into());
        return;
    }
    if prop == "inset" {
        // inset = top right bottom left (analog margin)
        let parts: Vec<&str> = value.split_whitespace().collect();
        let (t, r, b, l) = match parts.len() {
            1 => (parts[0], parts[0], parts[0], parts[0]),
            2 => (parts[0], parts[1], parts[0], parts[1]),
            3 => (parts[0], parts[1], parts[2], parts[1]),
            4 => (parts[0], parts[1], parts[2], parts[3]),
            _ => return,
        };
        out.insert("top".into(), t.into());
        out.insert("right".into(), r.into());
        out.insert("bottom".into(), b.into());
        out.insert("left".into(), l.into());
        out.insert("inset".into(), value.into());
        return;
    }
    // scroll-margin / scroll-padding shorthand expand do 4 sides.
    if matches!(prop, "scroll-margin" | "scroll-padding") {
        let parts: Vec<&str> = value.split_whitespace().collect();
        if parts.is_empty() { return; }
        let (t, r, b, l) = match parts.len() {
            1 => (parts[0], parts[0], parts[0], parts[0]),
            2 => (parts[0], parts[1], parts[0], parts[1]),
            3 => (parts[0], parts[1], parts[2], parts[1]),
            _ => (parts[0], parts[1], parts[2], parts[3]),
        };
        out.insert(format!("{prop}-top"), t.into());
        out.insert(format!("{prop}-right"), r.into());
        out.insert(format!("{prop}-bottom"), b.into());
        out.insert(format!("{prop}-left"), l.into());
        out.insert(prop.into(), value.into());
        return;
    }
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
        "border" | "outline" => {
            // "1px solid red" - parse postupne
            let parts: Vec<&str> = value.split_whitespace().collect();
            let prefix = prop;
            for p in &parts {
                if p.ends_with("px") || p.ends_with("em") || p.ends_with("rem") {
                    out.insert(format!("{prefix}-width"), p.to_string());
                } else if matches!(*p, "solid" | "dashed" | "dotted" | "double" | "none" | "groove" | "ridge" | "inset" | "outset") {
                    out.insert(format!("{prefix}-style"), p.to_string());
                } else if super::layout::parse_color(p).is_some() {
                    out.insert(format!("{prefix}-color"), p.to_string());
                }
            }
            out.insert(prop.into(), value.into());
        }
        "background" => {
            // Background shorthand resetuje vsechny longhandy na initial
            // (CSS spec: shorthand override). Color value -> set bg-color,
            // gradient/image -> set bg-image + reset bg-color na transparent
            // aby kaskadou prijata barva neprosakla skrze gradient.
            if super::layout::parse_color(value).is_some() {
                out.insert("background-color".into(), value.into());
                out.insert("background-image".into(), "none".into());
            } else if value.contains("linear-gradient(")
                || value.contains("radial-gradient(")
                || value.contains("conic-gradient(")
                || value.contains("url(") {
                out.insert("background-color".into(), "transparent".into());
                out.insert("background-image".into(), value.into());
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

/// Mapuje CSS Logical Property na fyzickou (LTR + horizontal-tb).
/// Vrati None kdyz prop neni logicka.
pub fn logical_to_physical(prop: &str) -> Option<&'static str> {
    Some(match prop {
        // Margin
        "margin-block-start"  => "margin-top",
        "margin-block-end"    => "margin-bottom",
        "margin-inline-start" => "margin-left",
        "margin-inline-end"   => "margin-right",
        // Padding
        "padding-block-start"  => "padding-top",
        "padding-block-end"    => "padding-bottom",
        "padding-inline-start" => "padding-left",
        "padding-inline-end"   => "padding-right",
        // Border width
        "border-block-start-width"  => "border-top-width",
        "border-block-end-width"    => "border-bottom-width",
        "border-inline-start-width" => "border-left-width",
        "border-inline-end-width"   => "border-right-width",
        // Border style
        "border-block-start-style"  => "border-top-style",
        "border-block-end-style"    => "border-bottom-style",
        "border-inline-start-style" => "border-left-style",
        "border-inline-end-style"   => "border-right-style",
        // Border color
        "border-block-start-color"  => "border-top-color",
        "border-block-end-color"    => "border-bottom-color",
        "border-inline-start-color" => "border-left-color",
        "border-inline-end-color"   => "border-right-color",
        // Border radius (logicke rohy)
        "border-start-start-radius" => "border-top-left-radius",
        "border-start-end-radius"   => "border-top-right-radius",
        "border-end-start-radius"   => "border-bottom-left-radius",
        "border-end-end-radius"     => "border-bottom-right-radius",
        // Inset
        "inset-block-start"  => "top",
        "inset-block-end"    => "bottom",
        "inset-inline-start" => "left",
        "inset-inline-end"   => "right",
        // Size
        "block-size"      => "height",
        "inline-size"     => "width",
        "min-block-size"  => "min-height",
        "min-inline-size" => "min-width",
        "max-block-size"  => "max-height",
        "max-inline-size" => "max-width",
        _ => return None,
    })
}

/// Logicka shorthand -> par fyzickych properties.
fn logical_shorthand_pair(prop: &str) -> Option<(&'static str, &'static str)> {
    Some(match prop {
        "margin-block"   => ("margin-top", "margin-bottom"),
        "margin-inline"  => ("margin-left", "margin-right"),
        "padding-block"  => ("padding-top", "padding-bottom"),
        "padding-inline" => ("padding-left", "padding-right"),
        "inset-block"    => ("top", "bottom"),
        "inset-inline"   => ("left", "right"),
        _ => return None,
    })
}

/// Mapa: pointer na Node -> computed styles.
pub type StyleMap = HashMap<usize, HashMap<String, String>>;

/// Mapa: (node_id, pseudo-element-name) -> computed styles.
/// Napr. ((0xabcd, "before"), {"content": "\"->\"", "color": "red"})
pub type PseudoStyleMap = HashMap<(usize, String), HashMap<String, String>>;

/// Pomocnik: vrati pointer hodnotu Rc<Node> jako klic.
fn node_id(node: &Rc<Node>) -> usize {
    Rc::as_ptr(node) as usize
}

/// Resolvuje CSS var(--name), env(), calc(), min(), max(), clamp() expressions.
/// Pri var(--x, fallback): pokud --x v variables, pouzij ho, jinak fallback.
pub fn resolve_value(value: &str, variables: &HashMap<String, String>) -> String {
    resolve_value_with_funcs(value, variables, &HashMap::new())
}

/// Resolvuje CSS hodnoty + uzivatelske @function volani.
pub fn resolve_value_with_funcs(
    value: &str,
    variables: &HashMap<String, String>,
    functions: &HashMap<String, super::css_parser::CssFunction>,
) -> String {
    let mut out = value.to_string();
    // @function calls: --name(arg1, arg2) -> evaluate body s arg substitution
    if !functions.is_empty() && out.contains("--") && out.contains('(') {
        out = resolve_user_functions(&out, variables, functions);
    }
    let mut out2 = out.clone();
    let _ = out;
    out2 = inner_resolve(&out2, variables);
    out2
}

fn inner_resolve(value: &str, variables: &HashMap<String, String>) -> String {
    let mut out = value.to_string();
    // Iterativne resolvujem do fixed pointu (max 10 prochodu).
    // var() muze obsahovat calc(), calc() muze obsahovat min(), atd.
    for _ in 0..10 {
        let before = out.clone();
        if out.contains("var(") {
            out = replace_var_once(&out, variables);
        }
        if out.contains("env(") {
            out = resolve_env(&out);
        }
        if out.contains("if(") {
            out = resolve_if_function(&out);
        }
        if out.contains("min(") || out.contains("max(") || out.contains("clamp(")
            || out.contains("abs(") || out.contains("sign(") || out.contains("sqrt(")
            || out.contains("round(") || out.contains("floor(") || out.contains("ceil(")
            || out.contains("exp(") || out.contains("log(") || out.contains("pow(")
            || out.contains("hypot(") || out.contains("mod(") || out.contains("rem(")
            || out.contains("sin(") || out.contains("cos(") || out.contains("tan(")
            || out.contains("asin(") || out.contains("acos(") || out.contains("atan(")
        {
            out = resolve_math_func(&out);
        }
        if out.contains("calc(") {
            out = resolve_calc(&out);
        }
        if out == before { break; }
    }
    out
}

/// CSS Functions L1 - resolve user-defined @function calls.
/// Format volani: `--name(arg1, arg2)`. Body funkce: `result: <expr>;`.
/// Args dosadime jako `var(--argname)` -> arg_value v body resolution.
fn resolve_user_functions(
    s: &str,
    variables: &HashMap<String, String>,
    functions: &HashMap<String, super::css_parser::CssFunction>,
) -> String {
    let bytes = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        // Detekce `--`
        if i + 1 < bytes.len() && bytes[i] == b'-' && bytes[i+1] == b'-' {
            // Najdeme jmeno (-- + ident)
            let mut j = i + 2;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'-' || bytes[j] == b'_') {
                j += 1;
            }
            // Pokud nasleduje '(', je to call
            if j < bytes.len() && bytes[j] == b'(' {
                let name = &s[i..j];
                if let Some(func) = functions.get(name) {
                    // Najit matching ')'
                    let mut depth = 1i32; let mut k = j + 1;
                    while k < bytes.len() && depth > 0 {
                        match bytes[k] { b'(' => depth += 1, b')' => depth -= 1, _ => {} }
                        if depth == 0 { break; }
                        k += 1;
                    }
                    let args_str = &s[j+1..k];
                    let arg_vals: Vec<String> = split_top_level_commas(args_str)
                        .into_iter().map(|a| a.trim().to_string()).collect();
                    // Build local vars: arg name -> arg value
                    let mut local_vars = variables.clone();
                    for (idx, arg_name) in func.args.iter().enumerate() {
                        if let Some(val) = arg_vals.get(idx) {
                            local_vars.insert(format!("--{}", arg_name), val.clone());
                        }
                    }
                    // Najit `result: ... ;` v body
                    let body = &func.body;
                    if let Some(result_idx) = body.find("result:") {
                        let after = &body[result_idx + 7..];
                        let end = after.find(';').unwrap_or(after.len());
                        let expr = after[..end].trim();
                        // Resolve expr s local vars
                        let resolved = inner_resolve(expr, &local_vars);
                        out.push_str(&resolved);
                        i = k + 1;
                        continue;
                    }
                }
            }
            // Neni call - emituj raw `--name`
            out.push_str(&s[i..j]);
            i = j;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// CSS Values L5 - if(<test>, <if-true>, <if-false>).
/// Test je literal: true/false/yes/no/1/0. Pokud match -> if-true, jinak if-false.
/// (Plna spec: test je media query / supports - to nas vyzaduje runtime kontext.
/// Implementuju literal-only verzi.)
fn resolve_if_function(s: &str) -> String {
    let mut out = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 3 < bytes.len() && &bytes[i..i+3] == b"if(" {
            let mut depth = 1i32; let mut j = i + 3;
            while j < bytes.len() && depth > 0 {
                match bytes[j] { b'(' => depth += 1, b')' => depth -= 1, _ => {} }
                if depth == 0 { break; }
                j += 1;
            }
            let inner = &s[i+3..j];
            let parts = split_top_level_commas(inner);
            if parts.len() >= 2 {
                let test = parts[0].trim();
                let truthy = matches!(test, "true" | "yes" | "1");
                let result = if truthy { parts[1].trim() }
                             else if parts.len() >= 3 { parts[2].trim() }
                             else { "" };
                out.push_str(result);
            }
            i = j + 1;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

use super::layout::split_top_level_commas;

/// env(safe-area-inset-top, fallback) - bez safe-area kontextu vrati fallback nebo 0px.
fn resolve_env(s: &str) -> String {
    let mut out = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 4 <= bytes.len() && &bytes[i..i+4] == b"env(" {
            let mut depth = 1;
            let mut j = i + 4;
            while j < bytes.len() && depth > 0 {
                match bytes[j] {
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                if depth == 0 { break; }
                j += 1;
            }
            let inner = &s[i+4..j];
            // Format: "name" nebo "name, fallback"
            let fallback = inner.find(',').map(|idx| inner[idx+1..].trim().to_string());
            let val = fallback.unwrap_or_else(|| "0px".to_string());
            out.push_str(&val);
            i = j + 1;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Resolvuje attr(name) / attr(name <type>) / attr(name, fallback) v CSS hodnote.
/// Vyzaduje DOM node pro cteni atributu elementu.
/// type muze byt CSS jednotka (px/em/%) nebo "string"/"number"/"color".
pub fn resolve_attr_in_value(value: &str, node: &Rc<Node>) -> String {
    if !value.contains("attr(") { return value.to_string(); }
    let bytes = value.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"attr(") {
            // Najdi odpovidajici uzaviraci zavorku
            let mut depth = 1usize;
            let mut j = i + 5;
            while j < bytes.len() && depth > 0 {
                match bytes[j] { b'(' => depth += 1, b')' => { depth -= 1; } _ => {} }
                if depth == 0 { break; }
                j += 1;
            }
            let inner = &value[i + 5..j];
            out.push_str(&attr_inner_resolve(inner, node));
            i = j + 1;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// Zpracuje obsah attr(...): "name", "name type", "name, fallback", "name type, fallback".
fn attr_inner_resolve(inner: &str, node: &Rc<Node>) -> String {
    // Oddelit fallback (prvni carka mimo zavorky)
    let (name_type, fallback) = split_at_comma_depth0(inner);
    let name_type = name_type.trim();
    // Oddelit name od type (prvni mezera)
    let (attr_name, attr_type) = match name_type.find(char::is_whitespace) {
        Some(sp) => (&name_type[..sp], Some(name_type[sp+1..].trim())),
        None => (name_type, None),
    };
    match node.attr(attr_name) {
        Some(val) => match attr_type {
            Some(t) if is_css_length_unit(t) => format!("{}{}", val.trim(), t),
            _ => val,
        },
        None => fallback.map(|f| f.trim().to_string()).unwrap_or_default(),
    }
}

/// Vrati true pro CSS delkove/casove/uhlove jednotky.
fn is_css_length_unit(s: &str) -> bool {
    matches!(s, "px"|"em"|"rem"|"%"|"vw"|"vh"|"vmin"|"vmax"
             |"pt"|"pc"|"in"|"cm"|"mm"|"ch"|"ex"|"lh"|"rlh"
             |"deg"|"rad"|"grad"|"turn"|"s"|"ms"|"hz"|"khz")
}

/// Rozdeli retezec na (cast pred prvni carkou na depth=0, Option<zbytek>).
fn split_at_comma_depth0(s: &str) -> (&str, Option<&str>) {
    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        match c { '(' => depth += 1, ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return (&s[..i], Some(&s[i+1..])),
            _ => {}
        }
    }
    (s, None)
}

// Viewport context pro min()/max()/clamp() unit konverzi (vw/vh -> px).
// Set pri cascade_with_viewport, cteno v eval_math_func. Default 0,0 ->
// fallback na old behavior bez konverze (testy bez viewport).
thread_local! {
    pub(crate) static MATH_VIEWPORT: std::cell::RefCell<(f32, f32)> = std::cell::RefCell::new((0.0, 0.0));
}

/// Resolvuje min(a, b, ...), max(a, b, ...), clamp(min, val, max).
/// Najde nejvnitrnejsi vyskyt (zaden child neni mezi argumenty), pak iterativne.
fn resolve_math_func(s: &str) -> String {
    let names = [
        "min(", "max(", "clamp(",
        "abs(", "sign(", "sqrt(", "round(", "floor(", "ceil(",
        "exp(", "log(", "pow(", "hypot(", "mod(", "rem(",
        "sin(", "cos(", "tan(", "asin(", "acos(", "atan(", "atan2(",
    ];
    let mut out = s.to_string();
    loop {
        let bytes: Vec<u8> = out.as_bytes().to_vec();
        let mut found: Option<(usize, usize, &str)> = None;
        // Najdi nejvnitrnejsi (nejlevejsi po procesu, kde uvnitr neni dalsi math func)
        'outer: for (idx, _) in bytes.iter().enumerate() {
            for &name in &names {
                let nb = name.as_bytes();
                if idx + nb.len() <= bytes.len() && &bytes[idx..idx + nb.len()] == nb {
                    // Word-boundary check: predchozi byte nesmi byt alphanumeric/_
                    // jinak by `max(` matchovalo uvnitr `minmax(...)`, vyrobilo mezivysledek
                    // `min<num>` a rozbilo CSS Grid minmax().
                    if idx > 0 {
                        let prev = bytes[idx - 1];
                        if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'-' {
                            continue;
                        }
                    }
                    // Najdi matching )
                    let mut depth = 1;
                    let mut j = idx + nb.len();
                    while j < bytes.len() && depth > 0 {
                        match bytes[j] {
                            b'(' => depth += 1,
                            b')' => depth -= 1,
                            _ => {}
                        }
                        if depth == 0 { break; }
                        j += 1;
                    }
                    if j >= bytes.len() { break 'outer; }
                    // Zkontroluj ze argumenty NEobsahuji dalsi math func (kromě calc)
                    let inner = &out[idx + nb.len()..j];
                    let has_inner = names.iter().any(|n| inner.contains(*n));
                    if !has_inner {
                        found = Some((idx, j, name.trim_end_matches('(')));
                        break 'outer;
                    }
                }
            }
        }
        let (start, end, fname) = match found { Some(t) => t, None => break };
        let nb_len = fname.len() + 1; // +1 pro '('
        let inner = out[start + nb_len..end].to_string();
        let result = eval_math_func(fname, &inner);
        out.replace_range(start..end + 1, &result);
    }
    out
}

fn eval_math_func(name: &str, args: &str) -> String {
    let parts: Vec<&str> = args.split(',').map(|s| s.trim()).collect();
    if parts.is_empty() { return args.to_string(); }

    // Parsuj kazdy argument: vrati (number, unit_string).
    let parsed: Vec<(f32, String)> = parts.iter().map(|p| parse_value_with_unit(p)).collect();
    if parsed.is_empty() { return args.to_string(); }

    // Konvertuj vsechny argumenty do px pomoci viewport contextu z thread-local.
    // Bez tohoto by min(68vw, 450px) jen porovnal cisla 68 vs 450 ignorujic
    // jednotky -> spatny result na realnych strankach (modal sirky atd.).
    let (vw_px, vh_px) = MATH_VIEWPORT.with(|c| *c.borrow());
    let to_px = |n: f32, unit: &str| -> f32 {
        match unit {
            "px" | "" => n,
            "vw"  => n * vw_px / 100.0,
            "vh"  => n * vh_px / 100.0,
            "vmin" => n * vw_px.min(vh_px) / 100.0,
            "vmax" => n * vw_px.max(vh_px) / 100.0,
            "em" | "rem" | "ch" | "ex" | "lh" | "rlh" => n * 16.0,
            "pt"  => n * 1.333_333,
            "%"   => n, // nelze resolvovat bez parent kontextu - ponech jako %.
            _     => n,
        }
    };
    // Pokud vsechny argumenty jsou ve stejne jednotce, ponech ji.
    // Pokud mix nebo neco s viewport jednotkou, vystup px.
    let first_unit = parsed[0].1.clone();
    let all_same_unit = parsed.iter().all(|(_, u)| *u == first_unit);
    let needs_conv = parsed.iter().any(|(_, u)|
        matches!(u.as_str(), "vw" | "vh" | "vmin" | "vmax" | "em" | "rem" | "ch" | "ex" | "lh" | "rlh"));
    let (nums, unit): (Vec<f32>, String) = if all_same_unit && !needs_conv {
        // Same unit - eval na raw cislech, vystup ve stejne jednotce.
        (parsed.iter().map(|(n, _)| *n).collect(), first_unit.clone())
    } else if vw_px > 0.0 || vh_px > 0.0 {
        // Convert vse na px.
        (parsed.iter().map(|(n, u)| to_px(*n, u)).collect(), "px".to_string())
    } else {
        // Bez viewport contextu = fallback na drivejsi behavior.
        (parsed.iter().map(|(n, _)| *n).collect(), first_unit.clone())
    };

    let result = match name {
        "min" => nums.iter().cloned().fold(f32::INFINITY, f32::min),
        "max" => nums.iter().cloned().fold(f32::NEG_INFINITY, f32::max),
        "clamp" if nums.len() >= 3 => {
            let lo = nums[0]; let val = nums[1]; let hi = nums[2];
            val.max(lo).min(hi)
        }
        // Math funkce L4 - vsechny pracuji v jednotkach prvniho argumentu
        "abs"   => nums[0].abs(),
        "sign"  => nums[0].signum(),
        "sqrt"  => nums[0].sqrt(),
        "round" => nums[0].round(),
        "floor" => nums[0].floor(),
        "ceil"  => nums[0].ceil(),
        "exp"   => nums[0].exp(),
        "log"   if nums.len() == 1 => nums[0].ln(),
        "log"   if nums.len() == 2 => nums[0].log(nums[1]),
        "pow"   if nums.len() == 2 => nums[0].powf(nums[1]),
        "hypot" => nums.iter().map(|x| x * x).sum::<f32>().sqrt(),
        "mod"   if nums.len() == 2 => nums[0].rem_euclid(nums[1]),
        "rem"   if nums.len() == 2 => nums[0] % nums[1],
        "sin"   => nums[0].to_radians().sin(),
        "cos"   => nums[0].to_radians().cos(),
        "tan"   => nums[0].to_radians().tan(),
        "asin"  => nums[0].asin().to_degrees(),
        "acos"  => nums[0].acos().to_degrees(),
        "atan"  => nums[0].atan().to_degrees(),
        "atan2" if nums.len() == 2 => nums[0].atan2(nums[1]).to_degrees(),
        _ => return args.to_string(),
    };
    // Trigonometrie sin/cos/tan + sqrt + exp + log + sign: vraci ciste cislo.
    // asin/acos/atan/atan2: vraci stupne (deg).
    let unitless = matches!(name,
        "sqrt" | "exp" | "log" | "sign" | "pow" | "hypot"
        | "sin" | "cos" | "tan");
    let angle = matches!(name, "asin" | "acos" | "atan" | "atan2");

    if unitless {
        format!("{result}")
    } else if angle {
        format!("{result}deg")
    } else if unit.is_empty() {
        format!("{result}")
    } else {
        format!("{result}{unit}")
    }
}

/// Parsuje hodnotu typu "12.5px", "100%", "2em", "42" -> (number, "px").
fn parse_value_with_unit(s: &str) -> (f32, String) {
    let s = s.trim();
    let units = ["px", "em", "rem", "vw", "vh", "vmin", "vmax", "pt", "%",
                 "ch", "ex", "lh", "rlh", "cqw", "cqh", "cqi", "cqb",
                 "deg", "rad", "turn", "ms", "s"];
    for u in &units {
        if let Some(num_part) = s.strip_suffix(u) {
            if let Ok(n) = num_part.trim().parse::<f32>() {
                return (n, u.to_string());
            }
        }
    }
    if let Ok(n) = s.parse::<f32>() {
        return (n, String::new());
    }
    (0.0, String::new())
}

fn replace_var_once(s: &str, variables: &HashMap<String, String>) -> String {
    let mut out = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 4 < bytes.len() && &bytes[i..i+4] == b"var(" {
            // Najdi matching )
            let mut depth = 1;
            let mut j = i + 4;
            while j < bytes.len() && depth > 0 {
                match bytes[j] {
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                if depth == 0 { break; }
                j += 1;
            }
            let inner = &s[i+4..j];
            // Split na name + fallback
            let (name, fallback) = match inner.find(',') {
                Some(idx) => (inner[..idx].trim(), Some(inner[idx+1..].trim())),
                None      => (inner.trim(), None),
            };
            let resolved = variables.get(name).cloned()
                .or_else(|| fallback.map(|f| f.to_string()))
                .unwrap_or_default();
            out.push_str(&resolved);
            i = j + 1;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn resolve_calc(s: &str) -> String {
    let mut out = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 5 < bytes.len() && &bytes[i..i+5] == b"calc(" {
            let mut depth = 1;
            let mut j = i + 5;
            while j < bytes.len() && depth > 0 {
                match bytes[j] {
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                if depth == 0 { break; }
                j += 1;
            }
            let expr = &s[i+5..j];
            let result = eval_calc_expr(expr);
            out.push_str(&result);
            i = j + 1;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Zjednoduseny calc evaluator - vstupy "Npx + Npx", "Nem * 2",
/// "10px + 2 * 3px" (= 16px, ne 36px - * ma vyssi precedence).
/// parse_length convertuje em/rem/vw/vh na px - takze acc je vzdy v px.
/// Vystup MUSI byt v px aby neproslo dalsim em-resolve (38em != 38px).
/// Vyjimka: vsechny operandy maji "%" suffix -> output %.
///
/// Two-pass: prvni prochazi */-operatory zleva doprava (kombinuje pary
/// na 1 vysledek), pak druha pass dela +- nad vyslednym seznamem.
fn eval_calc_expr(expr: &str) -> String {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() < 3 {
        return expr.trim().to_string();
    }

    // Pure-percent: vsechny numericke operandy konci na %.
    let all_pct = parts.iter().enumerate()
        .filter(|(i, _)| i % 2 == 0)
        .all(|(_, p)| p.ends_with('%'));
    let unit = if all_pct { "%" } else { "px" };

    let parse_val = |p: &str| -> f32 {
        if all_pct {
            p.trim_end_matches('%').parse::<f32>().unwrap_or(0.0)
        } else {
            super::layout::parse_length(p)
        }
    };

    // Pass 1: resolve * / left-to-right, build [(val, next_op_or_none)] seq.
    // Pri "a * b + c": [a, "*", b, "+", c] -> [(a*b), ("+"), (c)].
    let mut vals: Vec<f32> = vec![parse_val(parts[0])];
    let mut ops: Vec<&str> = Vec::new();
    let mut i = 1;
    while i + 1 < parts.len() {
        let op = parts[i];
        let val = parse_val(parts[i + 1]);
        match op {
            "*" => { let last = vals.last_mut().unwrap(); *last *= val; }
            "/" => { let last = vals.last_mut().unwrap(); if val != 0.0 { *last /= val; } }
            "+" | "-" => { ops.push(op); vals.push(val); }
            _ => break,
        }
        i += 2;
    }

    // Pass 2: resolve + / - left-to-right.
    let mut acc = vals[0];
    for (k, op) in ops.iter().enumerate() {
        match *op {
            "+" => acc += vals[k + 1],
            "-" => acc -= vals[k + 1],
            _ => {}
        }
    }
    format!("{}{}", acc, unit)
}

/// Cascade s viewport pro @media queries + @container queries.
/// Pro @container: zatim aproximace - container size je root viewport. Pro
/// presnou implementaci by se musel evaluovat per-element po layout pass
/// (kruhova zavislost s layoutem).
pub fn cascade_with_viewport(root: &Rc<Node>, stylesheets: &[Stylesheet],
                              viewport_w: f32, viewport_h: f32) -> StyleMap {
    // Set viewport pro thread-local pouzity v eval_math_func k konverzi
    // vw/vh argumentu min()/max()/clamp() na px.
    MATH_VIEWPORT.with(|c| *c.borrow_mut() = (viewport_w, viewport_h));
    // Sjednotit rules + matching media query + matching container query rules
    let mut effective: Vec<Stylesheet> = Vec::new();
    for sheet in stylesheets {
        let mut combined = sheet.clone();
        // Pre-resolve vh/vw v decl values na px hodnoty z viewport.
        for rule in &mut combined.rules {
            for d in &mut rule.declarations {
                d.value = resolve_viewport_units(&d.value, viewport_w, viewport_h);
            }
        }
        // Aplikuj jen vyhovujici media queries
        for mq in &sheet.media_queries {
            if super::css_parser::evaluate_media_query(&mq.query, viewport_w, viewport_h) {
                let mut rules = mq.rules.clone();
                for rule in &mut rules {
                    for d in &mut rule.declarations {
                        d.value = resolve_viewport_units(&d.value, viewport_w, viewport_h);
                    }
                }
                combined.rules.extend(rules);
            }
        }
        for cq in &sheet.container_queries {
            if super::css_parser::evaluate_container_query(&cq.condition, viewport_w, viewport_h) {
                let mut rules = cq.rules.clone();
                for rule in &mut rules {
                    for d in &mut rule.declarations {
                        d.value = resolve_viewport_units(&d.value, viewport_w, viewport_h);
                    }
                }
                combined.rules.extend(rules);
            }
        }
        combined.media_queries.clear();
        combined.container_queries.clear();
        effective.push(combined);
    }
    cascade(root, &effective)
}

/// Replace "Nvh" / "Nvw" / "Nvmin" / "Nvmax" v retezci na "Mpx" hodnoty
/// dle viewport_w/h. Aplikuje se pred resolve_calc (ktery pak ma px values).
fn resolve_viewport_units(s: &str, vw: f32, vh: f32) -> String {
    // Quick path: pokud retezec neobsahuje "vh"/"vw"/"vmin"/"vmax", nic nedelej.
    if !(s.contains("vh") || s.contains("vw") || s.contains("vmin") || s.contains("vmax")) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Find numeric followed by viewport unit. Must check unit BEFORE
        // consuming digits (jinak number bez vh/vw zere a vyhozeno).
        let is_digit_start = bytes[i].is_ascii_digit() || (bytes[i] == b'.' && i+1 < bytes.len() && bytes[i+1].is_ascii_digit());
        if is_digit_start {
            let start = i;
            // Allow leading -/+ already handled outside (we just push as-is).
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            let num_str = &s[start..i];
            let rest = &s[i..];
            // Check unit. Must distinguish "vmin"/"vmax" before "v" prefix.
            let (replaced, advance) = if rest.starts_with("vmin") {
                let n: f32 = num_str.parse().unwrap_or(0.0);
                (Some(n * vw.min(vh) / 100.0), 4)
            } else if rest.starts_with("vmax") {
                let n: f32 = num_str.parse().unwrap_or(0.0);
                (Some(n * vw.max(vh) / 100.0), 4)
            } else if rest.starts_with("vh") && !rest.starts_with("vhx") {
                // "vh" must be followed by non-letter (separator/end).
                let next = rest.as_bytes().get(2).copied().unwrap_or(b' ');
                if !next.is_ascii_alphabetic() {
                    let n: f32 = num_str.parse().unwrap_or(0.0);
                    (Some(n * vh / 100.0), 2)
                } else { (None, 0) }
            } else if rest.starts_with("vw") {
                let next = rest.as_bytes().get(2).copied().unwrap_or(b' ');
                if !next.is_ascii_alphabetic() {
                    let n: f32 = num_str.parse().unwrap_or(0.0);
                    (Some(n * vw / 100.0), 2)
                } else { (None, 0) }
            } else { (None, 0) };
            if let Some(px) = replaced {
                out.push_str(&format!("{}px", px));
                i += advance;
            } else {
                out.push_str(num_str);
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// L5 dual-write cascade variant. Vraci CascadeOutput { style_map, computed,
/// declarations } - bundle pro postupnou migraci layout/paint na typed
/// ComputedStyle.
///
/// Stage 2c: style_map vyplnen z legacy cascade. computed prazdny
/// (Default::initial). declarations konvertovany z style_map jako "valid"
/// stub bez specificity tracking. Stage 3 zlepsi populace.
pub fn cascade_with_viewport_typed(
    root: &Rc<Node>,
    stylesheets: &[Stylesheet],
    viewport_w: f32, viewport_h: f32,
) -> CascadeOutput {
    let style_map = cascade_with_viewport(root, stylesheets, viewport_w, viewport_h);
    let mut computed: ComputedStyleMap = HashMap::new();
    let mut declarations: DeclarationsMap = HashMap::new();
    for (node_id, props) in &style_map {
        let mut cs = ComputedStyle::initial();
        // L5 step 4 Phase G: mark vsechny CSS-deklarovane property names jako
        // explicitne set. build_box_inner pak cte cs.is_set(prop) misto
        // s.contains_key(key) - dovoli dropnout style_map dependency v layout.
        for key in props.keys() {
            let pid = PropertyId::parse(key);
            if pid != PropertyId::Unknown {
                cs.mark_set(pid);
            }
        }
        // Stage 3 batch 1: populace color/opacity/visibility/cursor.
        // Pri invalid value -> keep initial (CSS spec discard invalid).
        if let Some(v) = props.get("color") {
            if let Some(c) = Color::parse(v) { cs.color = c; }
        }
        if let Some(v) = props.get("opacity") {
            if let Ok(n) = v.trim().parse::<f32>() {
                cs.opacity = n.clamp(0.0, 1.0);
            }
        }
        if let Some(v) = props.get("visibility") {
            if let Some(vis) = Visibility::parse(v) { cs.visibility = vis; }
        }
        if let Some(v) = props.get("cursor") {
            cs.cursor = Cursor::parse(v);
        }
        // Batch 2: display/position/z_index.
        if let Some(v) = props.get("display") {
            if let Some(d) = CsDisplay::parse(v) { cs.display = d; }
        }
        if let Some(v) = props.get("position") {
            if let Some(p) = PositionKind::parse(v) { cs.position = p; }
        }
        if let Some(v) = props.get("z-index") {
            if let Some(z) = ZIndex::parse(v) { cs.z_index = z; }
        }
        // Batch 3: font_size + font_weight + font_style + line_height.
        if let Some(v) = props.get("font-size") {
            if let Some(l) = Length::parse(v) { cs.font_size = l; }
        }
        if let Some(v) = props.get("font-weight") {
            cs.font_weight = parse_font_weight(v);
        }
        if let Some(v) = props.get("font-style") {
            // italic | oblique | normal. Engine treats italic+oblique=true.
            let lv = v.trim().to_lowercase();
            cs.font_style_italic = lv == "italic" || lv.starts_with("oblique");
        }
        if let Some(v) = props.get("line-height") {
            cs.line_height = parse_line_height(v);
        }
        // Batch 4: margin-top/right/bottom/left. Shorthand `margin` rozkladame
        // pres expand_shorthand drive cascade (samostatne entries), takze tady
        // jen longhandy. `auto` -> Length::Auto.
        if let Some(v) = props.get("margin-top") {
            if let Some(l) = Length::parse(v) { cs.margin_top = l; }
        }
        if let Some(v) = props.get("margin-right") {
            if let Some(l) = Length::parse(v) { cs.margin_right = l; }
        }
        if let Some(v) = props.get("margin-bottom") {
            if let Some(l) = Length::parse(v) { cs.margin_bottom = l; }
        }
        if let Some(v) = props.get("margin-left") {
            if let Some(l) = Length::parse(v) { cs.margin_left = l; }
        }
        // Batch 5: padding-top/right/bottom/left.
        if let Some(v) = props.get("padding-top") {
            if let Some(l) = Length::parse(v) { cs.padding_top = l; }
        }
        if let Some(v) = props.get("padding-right") {
            if let Some(l) = Length::parse(v) { cs.padding_right = l; }
        }
        if let Some(v) = props.get("padding-bottom") {
            if let Some(l) = Length::parse(v) { cs.padding_bottom = l; }
        }
        if let Some(v) = props.get("padding-left") {
            if let Some(l) = Length::parse(v) { cs.padding_left = l; }
        }
        // Batch 6: width/height/min-width/min-height.
        if let Some(v) = props.get("width") {
            if let Some(l) = Length::parse(v) { cs.width = l; }
        }
        if let Some(v) = props.get("height") {
            if let Some(l) = Length::parse(v) { cs.height = l; }
        }
        if let Some(v) = props.get("min-width") {
            if let Some(l) = Length::parse(v) { cs.min_width = l; }
        }
        if let Some(v) = props.get("min-height") {
            if let Some(l) = Length::parse(v) { cs.min_height = l; }
        }
        // Batch 7: max-width/max-height + top/right offsety.
        if let Some(v) = props.get("max-width") {
            if let Some(l) = Length::parse(v) { cs.max_width = l; }
        }
        if let Some(v) = props.get("max-height") {
            if let Some(l) = Length::parse(v) { cs.max_height = l; }
        }
        if let Some(v) = props.get("top") {
            if let Some(l) = Length::parse(v) { cs.top = l; }
        }
        if let Some(v) = props.get("right") {
            if let Some(l) = Length::parse(v) { cs.right = l; }
        }
        // Batch 8: bottom/left offsety + background_color + font_family.
        if let Some(v) = props.get("bottom") {
            if let Some(l) = Length::parse(v) { cs.bottom = l; }
        }
        if let Some(v) = props.get("left") {
            if let Some(l) = Length::parse(v) { cs.left = l; }
        }
        if let Some(v) = props.get("background-color") {
            if let Some(c) = Color::parse(v) { cs.background_color = c; }
        }
        if let Some(v) = props.get("font-family") {
            cs.font_family = parse_font_family(v);
        }
        // Batch 9: text-align/white-space/word-break/overflow-wrap.
        if let Some(v) = props.get("text-align") {
            if let Some(t) = CsTextAlign::parse(v) { cs.text_align = t; }
        }
        if let Some(v) = props.get("white-space") {
            if let Some(w) = WhiteSpace::parse(v) { cs.white_space = w; }
        }
        if let Some(v) = props.get("word-break") {
            if let Some(w) = WordBreak::parse(v) { cs.word_break = w; }
        }
        if let Some(v) = props.get("overflow-wrap").or_else(|| props.get("word-wrap")) {
            if let Some(o) = OverflowWrap::parse(v) { cs.overflow_wrap = o; }
        }
        // Batch 10: writing-mode/direction/box-sizing/pointer-events.
        if let Some(v) = props.get("writing-mode") {
            if let Some(w) = CsWritingMode::parse(v) { cs.writing_mode = w; }
        }
        if let Some(v) = props.get("direction") {
            if let Some(d) = CsDirection::parse(v) { cs.direction = d; }
        }
        if let Some(v) = props.get("box-sizing") {
            if let Some(b) = CsBoxSizing::parse(v) { cs.box_sizing = b; }
        }
        if let Some(v) = props.get("pointer-events") {
            if let Some(p) = CsPointerEvents::parse(v) { cs.pointer_events = p; }
        }
        // Batch 11: overflow + overflow-x/y + float + clear. CSS spec:
        // `overflow: hidden` rozkladame na -x/-y v expand_shorthand drive.
        // Tady cti longhandy.
        if let Some(v) = props.get("overflow-x") {
            if let Some(o) = CsOverflow::parse(v) { cs.overflow_x = o; }
        }
        if let Some(v) = props.get("overflow-y") {
            if let Some(o) = CsOverflow::parse(v) { cs.overflow_y = o; }
        }
        if let Some(v) = props.get("float") {
            if let Some(f) = CsFloat::parse(v) { cs.float = f; }
        }
        if let Some(v) = props.get("clear") {
            if let Some(c) = CsClear::parse(v) { cs.clear = c; }
        }
        // Batch 12: flex_direction/flex_wrap/flex_grow/flex_shrink.
        if let Some(v) = props.get("flex-direction") {
            if let Some(f) = CsFlexDirection::parse(v) { cs.flex_direction = f; }
        }
        if let Some(v) = props.get("flex-wrap") {
            if let Some(f) = CsFlexWrap::parse(v) { cs.flex_wrap = f; }
        }
        if let Some(v) = props.get("flex-grow") {
            if let Ok(n) = v.trim().parse::<f32>() { cs.flex_grow = n.max(0.0); }
        }
        if let Some(v) = props.get("flex-shrink") {
            if let Ok(n) = v.trim().parse::<f32>() { cs.flex_shrink = n.max(0.0); }
        }
        // Batch 13: justify-content/align-items/align-content/align-self.
        if let Some(v) = props.get("justify-content") {
            if let Some(j) = CsJustifyContent::parse(v) { cs.justify_content = j; }
        }
        if let Some(v) = props.get("align-items") {
            if let Some(a) = CsAlignItems::parse(v) { cs.align_items = a; }
        }
        if let Some(v) = props.get("align-content") {
            if let Some(a) = CsAlignContent::parse(v) { cs.align_content = a; }
        }
        if let Some(v) = props.get("align-self") {
            if let Some(a) = CsAlignSelf::parse(v) { cs.align_self = a; }
        }
        // Batch 14: flex_basis/order/row_gap/column_gap.
        if let Some(v) = props.get("flex-basis") {
            if let Some(b) = CsFlexBasis::parse(v) { cs.flex_basis = b; }
        }
        if let Some(v) = props.get("order") {
            if let Ok(n) = v.trim().parse::<i32>() { cs.order = n; }
        }
        if let Some(v) = props.get("row-gap") {
            if let Some(l) = Length::parse(v) { cs.row_gap = l; }
        }
        if let Some(v) = props.get("column-gap") {
            if let Some(l) = Length::parse(v) { cs.column_gap = l; }
        }
        // Batch 15: border-*-width. CSS keywords (thin/medium/thick)
        // mapped: thin=1px, medium=3px, thick=5px (browser convention).
        let parse_border_width = |v: &str| -> Option<Length> {
            let t = v.trim().to_lowercase();
            match t.as_str() {
                "thin" => Some(Length::Px(1.0)),
                "medium" => Some(Length::Px(3.0)),
                "thick" => Some(Length::Px(5.0)),
                _ => Length::parse(&t),
            }
        };
        if let Some(v) = props.get("border-top-width") {
            if let Some(l) = parse_border_width(v) { cs.border_top_width = l; }
        }
        if let Some(v) = props.get("border-right-width") {
            if let Some(l) = parse_border_width(v) { cs.border_right_width = l; }
        }
        if let Some(v) = props.get("border-bottom-width") {
            if let Some(l) = parse_border_width(v) { cs.border_bottom_width = l; }
        }
        if let Some(v) = props.get("border-left-width") {
            if let Some(l) = parse_border_width(v) { cs.border_left_width = l; }
        }
        // Batch 16: border-*-color. currentColor keyword -> CurrentColor.
        let parse_border_color = |v: &str| -> Option<Color> {
            let t = v.trim();
            if t.eq_ignore_ascii_case("currentcolor") {
                return Some(Color::CurrentColor);
            }
            Color::parse(t)
        };
        if let Some(v) = props.get("border-top-color") {
            if let Some(c) = parse_border_color(v) { cs.border_top_color = c; }
        }
        if let Some(v) = props.get("border-right-color") {
            if let Some(c) = parse_border_color(v) { cs.border_right_color = c; }
        }
        if let Some(v) = props.get("border-bottom-color") {
            if let Some(c) = parse_border_color(v) { cs.border_bottom_color = c; }
        }
        if let Some(v) = props.get("border-left-color") {
            if let Some(c) = parse_border_color(v) { cs.border_left_color = c; }
        }
        // Batch 17: border-*-style. Expand_shorthand drive nahradila
        // `border-style: solid` -> 4 longhandy.
        if let Some(v) = props.get("border-top-style") {
            if let Some(s) = CsBorderStyle::parse(v) { cs.border_top_style = s; }
        }
        if let Some(v) = props.get("border-right-style") {
            if let Some(s) = CsBorderStyle::parse(v) { cs.border_right_style = s; }
        }
        if let Some(v) = props.get("border-bottom-style") {
            if let Some(s) = CsBorderStyle::parse(v) { cs.border_bottom_style = s; }
        }
        if let Some(v) = props.get("border-left-style") {
            if let Some(s) = CsBorderStyle::parse(v) { cs.border_left_style = s; }
        }
        // Batch 18: border-*-radius. Shorthand expand drive (line 121).
        if let Some(v) = props.get("border-top-left-radius") {
            if let Some(l) = Length::parse(v) { cs.border_top_left_radius = l; }
        }
        if let Some(v) = props.get("border-top-right-radius") {
            if let Some(l) = Length::parse(v) { cs.border_top_right_radius = l; }
        }
        if let Some(v) = props.get("border-bottom-right-radius") {
            if let Some(l) = Length::parse(v) { cs.border_bottom_right_radius = l; }
        }
        if let Some(v) = props.get("border-bottom-left-radius") {
            if let Some(l) = Length::parse(v) { cs.border_bottom_left_radius = l; }
        }
        // Batch 19: outline (width/style/color/offset). Outline shorthand uz
        // pres existing expand_shorthand "outline" parses parts.
        if let Some(v) = props.get("outline-width") {
            let t = v.trim().to_lowercase();
            let l = match t.as_str() {
                "thin" => Some(Length::Px(1.0)),
                "medium" => Some(Length::Px(3.0)),
                "thick" => Some(Length::Px(5.0)),
                _ => Length::parse(&t),
            };
            if let Some(l) = l { cs.outline_width = l; }
        }
        if let Some(v) = props.get("outline-style") {
            if let Some(s) = CsBorderStyle::parse(v) { cs.outline_style = s; }
        }
        if let Some(v) = props.get("outline-color") {
            if v.trim().eq_ignore_ascii_case("currentcolor") {
                cs.outline_color = Color::CurrentColor;
            } else if let Some(c) = Color::parse(v) {
                cs.outline_color = c;
            }
        }
        if let Some(v) = props.get("outline-offset") {
            if let Some(l) = Length::parse(v) { cs.outline_offset = l; }
        }
        // Batch 20: text-decoration (line/style/color/thickness).
        if let Some(v) = props.get("text-decoration-line") {
            if let Some(l) = CsTextDecorationLine::parse(v) { cs.text_decoration_line = l; }
        }
        if let Some(v) = props.get("text-decoration-style") {
            if let Some(s) = CsTextDecorationStyle::parse(v) { cs.text_decoration_style = s; }
        }
        if let Some(v) = props.get("text-decoration-color") {
            if v.trim().eq_ignore_ascii_case("currentcolor") {
                cs.text_decoration_color = Color::CurrentColor;
            } else if let Some(c) = Color::parse(v) {
                cs.text_decoration_color = c;
            }
        }
        if let Some(v) = props.get("text-decoration-thickness") {
            if v.trim().eq_ignore_ascii_case("auto") {
                cs.text_decoration_thickness = Length::Auto;
            } else if let Some(l) = Length::parse(v) {
                cs.text_decoration_thickness = l;
            }
        }
        // Batch 21: text-indent/-transform/-overflow + vertical-align.
        if let Some(v) = props.get("text-indent") {
            if let Some(l) = Length::parse(v) { cs.text_indent = l; }
        }
        if let Some(v) = props.get("text-transform") {
            if let Some(t) = CsTextTransform::parse(v) { cs.text_transform = t; }
        }
        if let Some(v) = props.get("text-overflow") {
            if let Some(t) = CsTextOverflow::parse(v) { cs.text_overflow = t; }
        }
        if let Some(v) = props.get("vertical-align") {
            if let Some(va) = CsVerticalAlign::parse(v) { cs.vertical_align = va; }
        }
        // Batch 22: list-style-type/-position/-image + tab-size.
        if let Some(v) = props.get("list-style-type") {
            cs.list_style_type = CsListStyleType::parse(v);
        }
        if let Some(v) = props.get("list-style-position") {
            if let Some(p) = CsListStylePosition::parse(v) { cs.list_style_position = p; }
        }
        if let Some(v) = props.get("list-style-image") {
            if let Some(i) = CsListStyleImage::parse(v) { cs.list_style_image = i; }
        }
        if let Some(v) = props.get("tab-size") {
            if let Ok(n) = v.trim().parse::<f32>() { cs.tab_size = n.max(0.0); }
        }
        // Batch 23: table props.
        if let Some(v) = props.get("border-collapse") {
            if let Some(b) = CsBorderCollapse::parse(v) { cs.border_collapse = b; }
        }
        if let Some(v) = props.get("border-spacing") {
            // 1 nebo 2 values: <h> <v> nebo <both>.
            let parts: Vec<&str> = v.split_whitespace().collect();
            match parts.len() {
                1 => {
                    if let Some(l) = Length::parse(parts[0]) {
                        cs.border_spacing_h = l.clone();
                        cs.border_spacing_v = l;
                    }
                }
                _ => {
                    if let Some(l) = Length::parse(parts[0]) { cs.border_spacing_h = l; }
                    if let Some(l) = Length::parse(parts[1]) { cs.border_spacing_v = l; }
                }
            }
        }
        if let Some(v) = props.get("table-layout") {
            if let Some(t) = CsTableLayout::parse(v) { cs.table_layout = t; }
        }
        if let Some(v) = props.get("caption-side") {
            if let Some(c) = CsCaptionSide::parse(v) { cs.caption_side = c; }
        }
        // Batch 24: object-fit/-position + aspect-ratio + resize.
        if let Some(v) = props.get("object-fit") {
            if let Some(o) = CsObjectFit::parse(v) { cs.object_fit = o; }
        }
        if let Some(v) = props.get("object-position") {
            let parts: Vec<&str> = v.split_whitespace().collect();
            let parse_pos = |s: &str| -> Option<Length> {
                match s.to_lowercase().as_str() {
                    "left" | "top" => Some(Length::Percent(0.0)),
                    "center" => Some(Length::Percent(50.0)),
                    "right" | "bottom" => Some(Length::Percent(100.0)),
                    _ => Length::parse(s),
                }
            };
            match parts.len() {
                1 => {
                    if let Some(l) = parse_pos(parts[0]) {
                        cs.object_position_x = l.clone();
                        cs.object_position_y = l;
                    }
                }
                _ => {
                    if let Some(l) = parse_pos(parts[0]) { cs.object_position_x = l; }
                    if let Some(l) = parse_pos(parts[1]) { cs.object_position_y = l; }
                }
            }
        }
        if let Some(v) = props.get("aspect-ratio") {
            let t = v.trim().to_lowercase();
            if t == "auto" {
                cs.aspect_ratio = None;
            } else if let Some((a, b)) = t.split_once('/') {
                let a: f32 = a.trim().parse().unwrap_or(0.0);
                let b: f32 = b.trim().parse().unwrap_or(0.0);
                if b > 0.0 { cs.aspect_ratio = Some(a / b); }
            } else if let Ok(n) = t.parse::<f32>() {
                if n > 0.0 { cs.aspect_ratio = Some(n); }
            }
        }
        if let Some(v) = props.get("resize") {
            if let Some(r) = CsResize::parse(v) { cs.resize = r; }
        }
        // Batch 25: transition-property/duration/timing-function/delay.
        if let Some(v) = props.get("transition-property") {
            cs.transition_property = v.clone();
        }
        if let Some(v) = props.get("transition-duration") {
            let lst = super::computed_style::parse_time_list(v);
            if !lst.is_empty() { cs.transition_duration = lst; }
        }
        if let Some(v) = props.get("transition-timing-function") {
            let lst = super::computed_style::parse_timing_function_list(v);
            if !lst.is_empty() { cs.transition_timing_function = lst; }
        }
        if let Some(v) = props.get("transition-delay") {
            let lst = super::computed_style::parse_time_list(v);
            if !lst.is_empty() { cs.transition_delay = lst; }
        }
        // Batch 26: animation timing (name/duration/timing/delay).
        if let Some(v) = props.get("animation-name") {
            let names: Vec<String> = v.split(',').map(|p| p.trim().to_string()).collect();
            if !names.is_empty() { cs.animation_name = names; }
        }
        if let Some(v) = props.get("animation-duration") {
            let lst = super::computed_style::parse_time_list(v);
            if !lst.is_empty() { cs.animation_duration = lst; }
        }
        if let Some(v) = props.get("animation-timing-function") {
            let lst = super::computed_style::parse_timing_function_list(v);
            if !lst.is_empty() { cs.animation_timing_function = lst; }
        }
        if let Some(v) = props.get("animation-delay") {
            let lst = super::computed_style::parse_time_list(v);
            if !lst.is_empty() { cs.animation_delay = lst; }
        }
        // Batch 27: animation control.
        if let Some(v) = props.get("animation-iteration-count") {
            let lst: Vec<f32> = v.split(',').map(|p| {
                let t = p.trim();
                if t.eq_ignore_ascii_case("infinite") { f32::INFINITY }
                else { t.parse().unwrap_or(1.0) }
            }).collect();
            if !lst.is_empty() { cs.animation_iteration_count = lst; }
        }
        if let Some(v) = props.get("animation-direction") {
            let lst: Vec<CsAnimationDirection> = v.split(',')
                .filter_map(|p| CsAnimationDirection::parse(p)).collect();
            if !lst.is_empty() { cs.animation_direction = lst; }
        }
        if let Some(v) = props.get("animation-fill-mode") {
            let lst: Vec<CsAnimationFillMode> = v.split(',')
                .filter_map(|p| CsAnimationFillMode::parse(p)).collect();
            if !lst.is_empty() { cs.animation_fill_mode = lst; }
        }
        if let Some(v) = props.get("animation-play-state") {
            let lst: Vec<CsAnimationPlayState> = v.split(',')
                .filter_map(|p| CsAnimationPlayState::parse(p)).collect();
            if !lst.is_empty() { cs.animation_play_state = lst; }
        }
        // Batch 28: transform + transform-origin + perspective.
        if let Some(v) = props.get("transform") {
            cs.transform = v.clone();
        }
        if let Some(v) = props.get("transform-origin") {
            let parts: Vec<&str> = v.split_whitespace().collect();
            let parse_pos = |s: &str| -> Option<Length> {
                match s.to_lowercase().as_str() {
                    "left" | "top" => Some(Length::Percent(0.0)),
                    "center" => Some(Length::Percent(50.0)),
                    "right" | "bottom" => Some(Length::Percent(100.0)),
                    _ => Length::parse(s),
                }
            };
            match parts.len() {
                1 => {
                    if let Some(l) = parse_pos(parts[0]) {
                        cs.transform_origin_x = l.clone();
                        cs.transform_origin_y = l;
                    }
                }
                _ => {
                    if let Some(l) = parse_pos(parts[0]) { cs.transform_origin_x = l; }
                    if let Some(l) = parse_pos(parts[1]) { cs.transform_origin_y = l; }
                }
            }
        }
        if let Some(v) = props.get("perspective") {
            if v.trim().eq_ignore_ascii_case("none") {
                cs.perspective = Length::None;
            } else if let Some(l) = Length::parse(v) {
                cs.perspective = l;
            }
        }
        // Batch 29: filter/backdrop-filter + mix-blend-mode + isolation.
        if let Some(v) = props.get("filter") {
            cs.filter = v.clone();
        }
        if let Some(v) = props.get("backdrop-filter") {
            cs.backdrop_filter = v.clone();
        }
        if let Some(v) = props.get("mix-blend-mode") {
            if let Some(b) = CsBlendMode::parse(v) { cs.mix_blend_mode = b; }
        }
        if let Some(v) = props.get("isolation") {
            if let Some(i) = CsIsolation::parse(v) { cs.isolation = i; }
        }
        // Batch 30: grid-template-cols/rows/areas + auto-flow.
        if let Some(v) = props.get("grid-template-columns") {
            cs.grid_template_columns = v.clone();
        }
        if let Some(v) = props.get("grid-template-rows") {
            cs.grid_template_rows = v.clone();
        }
        if let Some(v) = props.get("grid-template-areas") {
            cs.grid_template_areas = v.clone();
        }
        if let Some(v) = props.get("grid-auto-flow") {
            if let Some(f) = CsGridAutoFlow::parse(v) { cs.grid_auto_flow = f; }
        }
        // Batch 31: grid-column/row-start/end.
        if let Some(v) = props.get("grid-column-start") {
            cs.grid_column_start = CsGridLine::parse(v);
        }
        if let Some(v) = props.get("grid-column-end") {
            cs.grid_column_end = CsGridLine::parse(v);
        }
        if let Some(v) = props.get("grid-row-start") {
            cs.grid_row_start = CsGridLine::parse(v);
        }
        if let Some(v) = props.get("grid-row-end") {
            cs.grid_row_end = CsGridLine::parse(v);
        }
        // Batch 32: grid-auto-cols/rows + justify-items/-self.
        if let Some(v) = props.get("grid-auto-columns") {
            cs.grid_auto_columns = v.clone();
        }
        if let Some(v) = props.get("grid-auto-rows") {
            cs.grid_auto_rows = v.clone();
        }
        if let Some(v) = props.get("justify-items") {
            if let Some(j) = CsJustifyItems::parse(v) { cs.justify_items = j; }
        }
        if let Some(v) = props.get("justify-self") {
            if let Some(j) = CsJustifySelf::parse(v) { cs.justify_self = j; }
        }
        // Batch 33: shadows + clip-path + scroll-behavior.
        if let Some(v) = props.get("box-shadow") {
            cs.box_shadow = v.clone();
        }
        if let Some(v) = props.get("text-shadow") {
            cs.text_shadow = v.clone();
        }
        if let Some(v) = props.get("clip-path") {
            cs.clip_path = v.clone();
        }
        if let Some(v) = props.get("scroll-behavior") {
            if let Some(b) = CsScrollBehavior::parse(v) { cs.scroll_behavior = b; }
        }
        // Batch 34: background image/position/size/repeat.
        if let Some(v) = props.get("background-image") {
            cs.background_image = v.clone();
        }
        if let Some(v) = props.get("background-position") {
            cs.background_position = v.clone();
        }
        if let Some(v) = props.get("background-size") {
            cs.background_size = v.clone();
        }
        if let Some(v) = props.get("background-repeat") {
            cs.background_repeat = v.clone();
        }
        // Batch 35: bg attachment/clip/origin + caret-color.
        if let Some(v) = props.get("background-attachment") {
            if let Some(a) = CsBackgroundAttachment::parse(v) { cs.background_attachment = a; }
        }
        if let Some(v) = props.get("background-clip") {
            if let Some(c) = CsBackgroundClip::parse(v) { cs.background_clip = c; }
        }
        if let Some(v) = props.get("background-origin") {
            if let Some(c) = CsBackgroundClip::parse(v) { cs.background_origin = c; }
        }
        if let Some(v) = props.get("caret-color") {
            let t = v.trim().to_lowercase();
            if t == "auto" {
                cs.caret_color = Color::CurrentColor;
            } else if let Some(c) = Color::parse(v) {
                cs.caret_color = c;
            }
        }
        // Batch 36: appearance + content + counters.
        if let Some(v) = props.get("appearance").or_else(|| props.get("-webkit-appearance")) {
            cs.appearance = CsAppearance::parse(v);
        }
        if let Some(v) = props.get("content") {
            cs.content = v.clone();
        }
        if let Some(v) = props.get("counter-reset") {
            cs.counter_reset = v.clone();
        }
        if let Some(v) = props.get("counter-increment") {
            cs.counter_increment = v.clone();
        }
        // Batch 37: multi-column.
        if let Some(v) = props.get("column-count") {
            if let Some(c) = CsColumnCount::parse(v) { cs.column_count = c; }
        }
        if let Some(v) = props.get("column-width") {
            if v.trim().eq_ignore_ascii_case("auto") {
                cs.column_width = Length::Auto;
            } else if let Some(l) = Length::parse(v) {
                cs.column_width = l;
            }
        }
        if let Some(v) = props.get("column-fill") {
            if let Some(f) = CsColumnFill::parse(v) { cs.column_fill = f; }
        }
        if let Some(v) = props.get("column-span") {
            if let Some(s) = CsColumnSpan::parse(v) { cs.column_span = s; }
        }
        // Batch 38: scroll-margin top/right/bottom/left.
        if let Some(v) = props.get("scroll-margin-top") {
            if let Some(l) = Length::parse(v) { cs.scroll_margin_top = l; }
        }
        if let Some(v) = props.get("scroll-margin-right") {
            if let Some(l) = Length::parse(v) { cs.scroll_margin_right = l; }
        }
        if let Some(v) = props.get("scroll-margin-bottom") {
            if let Some(l) = Length::parse(v) { cs.scroll_margin_bottom = l; }
        }
        if let Some(v) = props.get("scroll-margin-left") {
            if let Some(l) = Length::parse(v) { cs.scroll_margin_left = l; }
        }
        // Batch 39: scroll-padding top/right/bottom/left.
        if let Some(v) = props.get("scroll-padding-top") {
            if let Some(l) = Length::parse(v) { cs.scroll_padding_top = l; }
        }
        if let Some(v) = props.get("scroll-padding-right") {
            if let Some(l) = Length::parse(v) { cs.scroll_padding_right = l; }
        }
        if let Some(v) = props.get("scroll-padding-bottom") {
            if let Some(l) = Length::parse(v) { cs.scroll_padding_bottom = l; }
        }
        if let Some(v) = props.get("scroll-padding-left") {
            if let Some(l) = Length::parse(v) { cs.scroll_padding_left = l; }
        }
        // Batch 40: scroll-snap-* + overscroll-behavior-*.
        if let Some(v) = props.get("scroll-snap-type") {
            cs.scroll_snap_type = v.clone();
        }
        if let Some(v) = props.get("scroll-snap-align") {
            if let Some(a) = CsScrollSnapAlign::parse(v) { cs.scroll_snap_align = a; }
        }
        if let Some(v) = props.get("overscroll-behavior-x") {
            if let Some(b) = CsOverscrollBehavior::parse(v) { cs.overscroll_behavior_x = b; }
        }
        if let Some(v) = props.get("overscroll-behavior-y") {
            if let Some(b) = CsOverscrollBehavior::parse(v) { cs.overscroll_behavior_y = b; }
        }
        // L5 step 4 batch 18: nove typed pole.
        if let Some(v) = props.get("letter-spacing") {
            cs.letter_spacing = crate::browser::computed_style::LetterSpacing::parse(v);
        }
        if let Some(v) = props.get("word-spacing") {
            cs.word_spacing = crate::browser::computed_style::LetterSpacing::parse(v);
        }
        if let Some(v) = props.get("accent-color") {
            cs.accent_color = crate::browser::computed_style::AccentColor::parse(v);
        }
        if let Some(v) = props.get("scrollbar-width") {
            cs.scrollbar_width = crate::browser::computed_style::ScrollbarWidth::parse(v);
        }
        if let Some(v) = props.get("scrollbar-color") {
            cs.scrollbar_color = crate::browser::computed_style::ScrollbarColor::parse(v);
        }
        if let Some(v) = props.get("image-rendering") {
            cs.image_rendering = crate::browser::computed_style::ImageRendering::parse(v);
        }
        if let Some(v) = props.get("text-underline-offset") {
            let t = v.trim();
            if t.eq_ignore_ascii_case("auto") {
                cs.text_underline_offset = Length::Auto;
            } else if let Some(l) = Length::parse(t) {
                cs.text_underline_offset = l;
            }
        }
        if let Some(v) = props.get("line-height-step") {
            if let Some(l) = Length::parse(v) { cs.line_height_step = l; }
        }
        if let Some(v) = props.get("speak") {
            cs.speak = crate::browser::computed_style::SpeakKind::parse(v);
        }
        if let Some(v) = props.get("mask-image") {
            cs.mask_image = v.clone();
        }
        if let Some(v) = props.get("shape-outside") {
            cs.shape_outside = crate::browser::computed_style::ShapeOutsideVal::parse(v);
        }
        if let Some(v) = props.get("shape-margin") {
            if let Some(l) = Length::parse(v) { cs.shape_margin = l; }
        }
        if let Some(v) = props.get("shape-image-threshold") {
            if let Ok(n) = v.trim().parse::<f32>() {
                cs.shape_image_threshold = n.clamp(0.0, 1.0);
            }
        }
        if let Some(v) = props.get("contain-intrinsic-block-size") {
            if let Some(l) = Length::parse(v) { cs.contain_intrinsic_block_size = l; }
        }
        if let Some(v) = props.get("contain-intrinsic-inline-size") {
            if let Some(l) = Length::parse(v) { cs.contain_intrinsic_inline_size = l; }
        }
        if let Some(v) = props.get("contain-intrinsic-size") {
            if let Some(l) = Length::parse(v) { cs.contain_intrinsic_size = l; }
        }
        if let Some(v) = props.get("orphans") {
            if let Ok(n) = v.trim().parse::<u32>() { cs.orphans = n; }
        }
        if let Some(v) = props.get("widows") {
            if let Ok(n) = v.trim().parse::<u32>() { cs.widows = n; }
        }
        if let Some(v) = props.get("border-image-source") {
            cs.border_image_source = v.clone();
        }
        if let Some(v) = props.get("border-image-slice") {
            cs.border_image_slice = v.clone();
        }
        if let Some(v) = props.get("border-image-width") {
            cs.border_image_width = v.clone();
        }
        if let Some(v) = props.get("text-emphasis") {
            cs.text_emphasis = v.clone();
        }
        if let Some(v) = props.get("text-emphasis-color") {
            if let Some(c) = Color::parse(v) { cs.text_emphasis_color = c; }
        }
        // EXPERIMENTAL CSS L4/L5 - raw string populate. Implementace v layout/paint
        // dodelat v dalsich session - prozatim getComputedStyle + devtools display.
        if let Some(v) = props.get("anchor-name") { cs.anchor_name = v.clone(); }
        if let Some(v) = props.get("position-anchor") { cs.position_anchor = v.clone(); }
        if let Some(v) = props.get("inset-area") { cs.inset_area = v.clone(); }
        if let Some(v) = props.get("view-transition-name") { cs.view_transition_name = v.clone(); }
        if let Some(v) = props.get("view-timeline-name") { cs.view_timeline_name = v.clone(); }
        if let Some(v) = props.get("view-timeline-axis") { cs.view_timeline_axis = v.clone(); }
        if let Some(v) = props.get("scroll-timeline-name") { cs.scroll_timeline_name = v.clone(); }
        if let Some(v) = props.get("scroll-timeline-axis") { cs.scroll_timeline_axis = v.clone(); }
        if let Some(v) = props.get("animation-timeline") { cs.animation_timeline_l5 = v.clone(); }
        if let Some(v) = props.get("content-visibility") {
            cs.content_visibility = crate::browser::computed_style::ContentVisibility::parse(v);
        }
        if let Some(v) = props.get("container") { cs.container = v.clone(); }
        if let Some(v) = props.get("container-type") {
            cs.container_type = crate::browser::computed_style::ContainerType::parse(v);
        }
        if let Some(v) = props.get("container-name") { cs.container_name = v.clone(); }
        if let Some(v) = props.get("text-wrap") {
            cs.text_wrap = crate::browser::computed_style::TextWrap::parse(v);
        }
        if let Some(v) = props.get("text-wrap-style") {
            cs.text_wrap_style = crate::browser::computed_style::TextWrapStyle::parse(v);
        }
        if let Some(v) = props.get("text-wrap-mode") {
            cs.text_wrap_mode = crate::browser::computed_style::TextWrapMode::parse(v);
        }
        if let Some(v) = props.get("text-box-trim") {
            cs.text_box_trim = crate::browser::computed_style::TextBoxTrim::parse(v);
        }
        if let Some(v) = props.get("text-box-edge") {
            cs.text_box_edge = crate::browser::computed_style::TextBoxEdge::parse(v);
        }
        if let Some(v) = props.get("field-sizing") {
            cs.field_sizing = crate::browser::computed_style::FieldSizing::parse(v);
        }
        if let Some(v) = props.get("print-color-adjust") {
            cs.print_color_adjust = crate::browser::computed_style::PrintColorAdjust::parse(v);
        }
        if let Some(v) = props.get("forced-color-adjust") {
            cs.forced_color_adjust = crate::browser::computed_style::ForcedColorAdjust::parse(v);
        }
        if let Some(v) = props.get("color-scheme") {
            cs.color_scheme = crate::browser::computed_style::ColorScheme::parse(v);
        }
        if let Some(v) = props.get("math-style") {
            cs.math_style = crate::browser::computed_style::MathStyle::parse(v);
        }
        if let Some(v) = props.get("math-depth") { cs.math_depth = v.clone(); }
        if let Some(v) = props.get("ruby-position") {
            cs.ruby_position = crate::browser::computed_style::RubyPosition::parse(v);
        }
        if let Some(v) = props.get("ruby-align") {
            cs.ruby_align = crate::browser::computed_style::RubyAlign::parse(v);
        }
        if let Some(v) = props.get("scroll-marker-group") { cs.scroll_marker_group = v.clone(); }
        computed.insert(*node_id, cs);
        // Konvertuj kazdou property na CascadeDecl s validity flag pro
        // batch 1 props (color/opacity/visibility/cursor) - parse Result
        // urcuje valid. Ostatni props zatim valid=true pokud PropertyId
        // known (Unknown = invalid).
        let mut decls: Vec<CascadeDecl> = Vec::with_capacity(props.len());
        for (idx, (raw_name, raw_val)) in props.iter().enumerate() {
            let property = PropertyId::parse(raw_name);
            let valid = match property {
                PropertyId::Unknown => false,
                PropertyId::Color => Color::parse(raw_val).is_some(),
                PropertyId::Opacity => raw_val.trim().parse::<f32>().is_ok(),
                PropertyId::Visibility => Visibility::parse(raw_val).is_some(),
                PropertyId::Display => CsDisplay::parse(raw_val).is_some(),
                PropertyId::Position => PositionKind::parse(raw_val).is_some(),
                PropertyId::ZIndex => ZIndex::parse(raw_val).is_some(),
                PropertyId::FontSize => Length::parse(raw_val).is_some(),
                PropertyId::FontWeight => is_valid_font_weight(raw_val),
                PropertyId::FontStyle => is_valid_font_style(raw_val),
                PropertyId::LineHeight => is_valid_line_height(raw_val),
                PropertyId::MarginTop | PropertyId::MarginRight
                | PropertyId::MarginBottom | PropertyId::MarginLeft
                | PropertyId::PaddingTop | PropertyId::PaddingRight
                | PropertyId::PaddingBottom | PropertyId::PaddingLeft
                | PropertyId::Width | PropertyId::Height
                | PropertyId::MinWidth | PropertyId::MinHeight
                | PropertyId::MaxWidth | PropertyId::MaxHeight
                | PropertyId::Top | PropertyId::Right
                | PropertyId::Bottom | PropertyId::Left
                    => Length::parse(raw_val).is_some(),
                PropertyId::BackgroundColor => Color::parse(raw_val).is_some(),
                PropertyId::FontFamily => !raw_val.trim().is_empty(),
                PropertyId::TextAlign => CsTextAlign::parse(raw_val).is_some(),
                PropertyId::WhiteSpace => WhiteSpace::parse(raw_val).is_some(),
                PropertyId::WordBreak => WordBreak::parse(raw_val).is_some(),
                PropertyId::OverflowWrap => OverflowWrap::parse(raw_val).is_some(),
                PropertyId::WritingMode => CsWritingMode::parse(raw_val).is_some(),
                PropertyId::Direction => CsDirection::parse(raw_val).is_some(),
                PropertyId::BoxSizing => CsBoxSizing::parse(raw_val).is_some(),
                PropertyId::PointerEvents => CsPointerEvents::parse(raw_val).is_some(),
                PropertyId::Overflow | PropertyId::OverflowX | PropertyId::OverflowY
                    => CsOverflow::parse(raw_val).is_some(),
                PropertyId::Float => CsFloat::parse(raw_val).is_some(),
                PropertyId::Clear => CsClear::parse(raw_val).is_some(),
                PropertyId::FlexDirection => CsFlexDirection::parse(raw_val).is_some(),
                PropertyId::FlexWrap => CsFlexWrap::parse(raw_val).is_some(),
                PropertyId::FlexGrow | PropertyId::FlexShrink
                    => raw_val.trim().parse::<f32>().is_ok(),
                PropertyId::JustifyContent => CsJustifyContent::parse(raw_val).is_some(),
                PropertyId::AlignItems => CsAlignItems::parse(raw_val).is_some(),
                PropertyId::AlignContent => CsAlignContent::parse(raw_val).is_some(),
                PropertyId::AlignSelf => CsAlignSelf::parse(raw_val).is_some(),
                PropertyId::FlexBasis => CsFlexBasis::parse(raw_val).is_some(),
                PropertyId::Order => raw_val.trim().parse::<i32>().is_ok(),
                PropertyId::RowGap | PropertyId::ColumnGap
                    => Length::parse(raw_val).is_some(),
                PropertyId::BorderTopWidth | PropertyId::BorderRightWidth
                | PropertyId::BorderBottomWidth | PropertyId::BorderLeftWidth
                | PropertyId::BorderWidth => {
                    let t = raw_val.trim().to_lowercase();
                    matches!(t.as_str(), "thin" | "medium" | "thick")
                        || Length::parse(raw_val).is_some()
                },
                PropertyId::BorderTopColor | PropertyId::BorderRightColor
                | PropertyId::BorderBottomColor | PropertyId::BorderLeftColor
                | PropertyId::BorderColor => {
                    raw_val.trim().eq_ignore_ascii_case("currentcolor")
                        || Color::parse(raw_val).is_some()
                },
                PropertyId::BorderTopStyle | PropertyId::BorderRightStyle
                | PropertyId::BorderBottomStyle | PropertyId::BorderLeftStyle
                | PropertyId::BorderStyle
                    => CsBorderStyle::parse(raw_val).is_some(),
                PropertyId::BorderTopLeftRadius | PropertyId::BorderTopRightRadius
                | PropertyId::BorderBottomRightRadius | PropertyId::BorderBottomLeftRadius
                | PropertyId::BorderRadius
                    => Length::parse(raw_val).is_some(),
                PropertyId::OutlineWidth => {
                    let t = raw_val.trim().to_lowercase();
                    matches!(t.as_str(), "thin" | "medium" | "thick")
                        || Length::parse(raw_val).is_some()
                },
                PropertyId::OutlineStyle => CsBorderStyle::parse(raw_val).is_some(),
                PropertyId::OutlineColor => {
                    raw_val.trim().eq_ignore_ascii_case("currentcolor")
                        || Color::parse(raw_val).is_some()
                },
                PropertyId::OutlineOffset => Length::parse(raw_val).is_some(),
                PropertyId::TextDecorationLine => CsTextDecorationLine::parse(raw_val).is_some(),
                PropertyId::TextDecorationStyle => CsTextDecorationStyle::parse(raw_val).is_some(),
                PropertyId::TextDecorationColor => {
                    raw_val.trim().eq_ignore_ascii_case("currentcolor")
                        || Color::parse(raw_val).is_some()
                },
                PropertyId::TextDecorationThickness => {
                    raw_val.trim().eq_ignore_ascii_case("auto")
                        || Length::parse(raw_val).is_some()
                },
                PropertyId::TextIndent => Length::parse(raw_val).is_some(),
                PropertyId::TextTransform => CsTextTransform::parse(raw_val).is_some(),
                PropertyId::TextOverflow => CsTextOverflow::parse(raw_val).is_some(),
                PropertyId::VerticalAlign => CsVerticalAlign::parse(raw_val).is_some(),
                // ListStyleType::parse vzdy uspeje (Custom fallback)
                PropertyId::ListStylePosition => CsListStylePosition::parse(raw_val).is_some(),
                PropertyId::ListStyleImage => CsListStyleImage::parse(raw_val).is_some(),
                PropertyId::TabSize => raw_val.trim().parse::<f32>().is_ok(),
                PropertyId::BorderCollapse => CsBorderCollapse::parse(raw_val).is_some(),
                PropertyId::BorderSpacing => {
                    let parts: Vec<&str> = raw_val.split_whitespace().collect();
                    !parts.is_empty() && parts.iter().all(|p| Length::parse(p).is_some())
                },
                PropertyId::TableLayout => CsTableLayout::parse(raw_val).is_some(),
                PropertyId::CaptionSide => CsCaptionSide::parse(raw_val).is_some(),
                PropertyId::ObjectFit => CsObjectFit::parse(raw_val).is_some(),
                // ObjectPosition: keyword nebo length per part - vzdy valid pri 1-2 tokens.
                PropertyId::ObjectPosition => {
                    let parts: Vec<&str> = raw_val.split_whitespace().collect();
                    !parts.is_empty() && parts.len() <= 2
                },
                PropertyId::AspectRatio => {
                    let t = raw_val.trim().to_lowercase();
                    t == "auto" || t.split_once('/').map(|(a, b)|
                        a.trim().parse::<f32>().is_ok() && b.trim().parse::<f32>().is_ok()
                    ).unwrap_or_else(|| t.parse::<f32>().is_ok())
                },
                PropertyId::Resize => CsResize::parse(raw_val).is_some(),
                // Cursor::parse vzdy uspeje (Custom fallback) - vsechny valid.
                _ => true,
            };
            decls.push(CascadeDecl {
                property,
                raw_value: raw_val.clone(),
                valid,
                important: false,
                specificity: CsSpec::ZERO,
                origin: CascadeOrigin::Author,
                source_order: idx as u32,
            });
        }
        declarations.insert(*node_id, decls);
    }
    CascadeOutput { style_map, computed, declarations }
}

// ─── L5 batch 3 helpers ─────────────────────────────────────────────

/// Parse `font-weight`: keyword (normal/bold/bolder/lighter) nebo cislo
/// 1..1000. Invalid -> default 400.
fn parse_font_weight(v: &str) -> u32 {
    let t = v.trim().to_lowercase();
    if let Ok(n) = t.parse::<u32>() { return n.clamp(1, 1000); }
    match t.as_str() {
        "bold" | "bolder" => 700,
        "lighter" => 300,
        "normal" => 400,
        _ => 400,
    }
}

fn is_valid_font_weight(v: &str) -> bool {
    let t = v.trim().to_lowercase();
    matches!(t.as_str(), "normal" | "bold" | "bolder" | "lighter")
        || t.parse::<u32>().is_ok()
}

fn is_valid_font_style(v: &str) -> bool {
    let t = v.trim().to_lowercase();
    t == "normal" || t == "italic" || t.starts_with("oblique")
}

/// Parse line-height: `normal` | <number> | <length>.
fn parse_line_height(v: &str) -> LineHeight {
    let t = v.trim();
    if t.eq_ignore_ascii_case("normal") { return LineHeight::Normal; }
    if let Ok(n) = t.parse::<f32>() { return LineHeight::Multiplier(n); }
    if let Some(l) = Length::parse(t) { return LineHeight::Length(l); }
    LineHeight::Normal
}

fn is_valid_line_height(v: &str) -> bool {
    let t = v.trim();
    if t.eq_ignore_ascii_case("normal") { return true; }
    if t.parse::<f32>().is_ok() { return true; }
    Length::parse(t).is_some()
}

/// Parse `font-family` comma-separated list. Quoted strings -> Named bez
/// uvozovek. Bareword -> Generic pokud match keyword, jinak Named.
fn parse_font_family(v: &str) -> Vec<FontFamily> {
    let mut out = Vec::new();
    for raw in v.split(',') {
        let trimmed = raw.trim();
        // Strip pairs of matching quotes (single nebo double).
        let stripped = if (trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        {
            if trimmed.len() >= 2 { &trimmed[1..trimmed.len()-1] } else { trimmed }
        } else {
            trimmed
        };
        if stripped.is_empty() { continue; }
        if let Some(g) = GenericFamily::parse(stripped) {
            out.push(FontFamily::Generic(g));
        } else {
            out.push(FontFamily::Named(stripped.to_string()));
        }
    }
    out
}

/// Per-element container query evaluation: container_sizes je mapa
/// node ptr (Rc::as_ptr) -> (width, height). Pri matchingu @container
/// rule najde nejblizsiho ancestora s container-type a pouzije jeho velikost.
/// Bez fallbacku na viewport - rules se aplikuji JEN pokud najdem container.
pub fn cascade_with_container_sizes(
    root: &Rc<Node>,
    stylesheets: &[Stylesheet],
    viewport_w: f32, viewport_h: f32,
    container_sizes: &HashMap<usize, (f32, f32)>,
) -> StyleMap {
    // Sjednotit jen media queries - container queries vyresime per-element.
    let mut effective: Vec<Stylesheet> = Vec::new();
    for sheet in stylesheets {
        let mut combined = sheet.clone();
        for mq in &sheet.media_queries {
            if super::css_parser::evaluate_media_query(&mq.query, viewport_w, viewport_h) {
                combined.rules.extend(mq.rules.clone());
            }
        }
        combined.media_queries.clear();
        combined.container_queries.clear(); // vyresime separe pres ancestor lookup
        effective.push(combined);
    }
    let mut style_map = cascade(root, &effective);
    // Druhy pruchod: per-element pro @container rules - bez double-apply.
    root.walk(&mut |node| {
        if !matches!(node.kind, NodeKind::Element { .. }) { return; }
        // Bez container ancestor (s container-type) -> rules se NEAPLIKUJI.
        // (Spec: kdyz neni container, query se nevyhodnoti)
        let container_size = find_container_size(node, container_sizes);
        if container_size.is_none() { return; }
        let (cw, ch) = container_size.unwrap();
        for sheet in stylesheets {
            for cq in &sheet.container_queries {
                if super::css_parser::evaluate_container_query(&cq.condition, cw, ch) {
                    for rule in &cq.rules {
                        for sel in &rule.selectors {
                            if matches_selector(node, sel) {
                                let entry = style_map.entry(node_id(node)).or_default();
                                let mut variables: HashMap<String, String> = HashMap::new();
                                for d in &rule.declarations {
                                    if d.property.starts_with("--") {
                                        variables.insert(d.property.clone(), d.value.clone());
                                    }
                                }
                                for d in &rule.declarations {
                                    let resolved = resolve_value(&d.value, &variables);
                                    let resolved = resolve_attr_in_value(&resolved, node);
                                    expand_shorthand(&d.property, &resolved, entry);
                                }
                            }
                        }
                    }
                }
            }
        }
    });
    style_map
}

/// CSS Transitions L2 - cascade jen @starting-style rules.
/// Vraci StyleMap s "from-state" hodnotami pro elementy. Pouziva se pro transition
/// starting state pri pridanim noveho elementu (nebo display:none -> visible).
/// Cascade pravidla z `sheet.starting_style_rules` jako kdyby byly nestandardni.
pub fn cascade_starting_style(root: &Rc<Node>, stylesheets: &[Stylesheet]) -> StyleMap {
    let mut style_map: StyleMap = HashMap::new();
    let variables: HashMap<String, String> = HashMap::new();
    root.walk(&mut |node| {
        if !matches!(node.kind, NodeKind::Element { .. }) { return; }
        for sheet in stylesheets {
            for rule in &sheet.starting_style_rules {
                for sel in &rule.selectors {
                    if matches_selector(node, sel) {
                        let entry = style_map.entry(node_id(node)).or_default();
                        for d in &rule.declarations {
                            let resolved = resolve_value(&d.value, &variables);
                            let resolved = resolve_attr_in_value(&resolved, node);
                            expand_shorthand(&d.property, &resolved, entry);
                        }
                    }
                }
            }
        }
    });
    style_map
}

/// Test zda je `node` v scope - tj. je descendant nektereho elementu matchujiciho
/// `root_sel`, a zaroven NENI descendant limit_sel (pokud limit dany).
/// Self je tez "descendant" (root sam je v scope).
pub fn node_in_scope(node: &Rc<Node>, root_sel: &str, limit_sel: Option<&str>) -> bool {
    let root_parsed = super::css_parser::parse_selectors(root_sel);
    let limit_parsed = limit_sel.map(super::css_parser::parse_selectors);
    // Najit ancestor (vc. self) co matchuje root.
    let mut cur = Some(Rc::clone(node));
    let mut found_root = false;
    while let Some(c) = cur {
        if root_parsed.iter().any(|s| matches_selector(&c, s)) {
            found_root = true;
            break;
        }
        cur = c.parent.borrow().upgrade();
    }
    if !found_root { return false; }
    // Pokud je dany limit, zjistit zda nejaky ancestor (vc. self) matchuje limit.
    // Pokud ano, node je MIMO scope (limit je exclusive).
    if let Some(lim) = limit_parsed {
        let mut cur = Some(Rc::clone(node));
        while let Some(c) = cur {
            if lim.iter().any(|s| matches_selector(&c, s)) {
                return false;
            }
            cur = c.parent.borrow().upgrade();
        }
    }
    true
}

/// Najde nejblizsi container ancestor a vrati jeho rozmery.
fn find_container_size(
    node: &Rc<Node>,
    container_sizes: &HashMap<usize, (f32, f32)>,
) -> Option<(f32, f32)> {
    let mut current = node.parent.borrow().upgrade();
    while let Some(parent) = current {
        if container_sizes.contains_key(&(Rc::as_ptr(&parent) as usize)) {
            return container_sizes.get(&(Rc::as_ptr(&parent) as usize)).copied();
        }
        current = parent.parent.borrow().upgrade();
    }
    None
}

/// Quick-reject klasifikator pro selectory: extracted z rightmost simple part.
/// Per-node check je O(num_classes + 1) namisto full matches_selector walk.
#[derive(Default)]
struct SelectorKey {
    /// Some pri tag != "*", lowercased. None = univerzalni / pseudo.
    tag: Option<String>,
    /// Some pri id na rightmost part.
    id: Option<String>,
    /// Classes na rightmost part. Vsechny musi byt na node.
    classes: Vec<String>,
}

impl SelectorKey {
    fn from_selector(sel: &super::css_parser::Selector) -> Self {
        let mut k = SelectorKey::default();
        if let Some(last) = sel.parts.last() {
            if let Some(t) = &last.tag {
                if t != "*" && t != "&" {
                    k.tag = Some(t.clone());
                }
            }
            k.id = last.id.clone();
            k.classes = last.classes.clone();
        }
        k
    }

    /// Quick-reject: nevyhneme se follow-up full matches_selector pri true.
    #[inline]
    fn might_match(&self, node_tag: &str, node_id: Option<&str>, node_classes: &str) -> bool {
        if let Some(t) = &self.tag {
            if t != node_tag { return false; }
        }
        if let Some(id) = &self.id {
            if node_id != Some(id.as_str()) { return false; }
        }
        for required_class in &self.classes {
            let mut found = false;
            for c in node_classes.split_whitespace() {
                if c == required_class.as_str() { found = true; break; }
            }
            if !found { return false; }
        }
        true
    }
}

/// Aplikuje stylesheet na DOM strom, vrati StyleMap.
pub fn cascade(root: &Rc<Node>, stylesheets: &[Stylesheet]) -> StyleMap {
    let mut style_map: StyleMap = HashMap::new();
    // Globalni :root variables - resolved jednou
    let mut variables: HashMap<String, String> = HashMap::new();
    // Globalni @function registry - vsechny funkce ze vsech stylesheets
    let mut functions: HashMap<String, super::css_parser::CssFunction> = HashMap::new();
    for sheet in stylesheets {
        for f in &sheet.functions {
            functions.insert(f.name.clone(), f.clone());
        }
    }
    // @property initial-value pro registrovane custom properties - aplikovan pred :root values
    for sheet in stylesheets {
        for prop in &sheet.registered_properties {
            if let Some(init) = &prop.initial_value {
                variables.entry(prop.name.clone()).or_insert_with(|| init.clone());
            }
        }
    }
    // :root globalni custom property collection.
    // Bere jen rules ktere CISTE selektoruji :root nebo html bez dalsich
    // class/id constraints. Drive `parts.is_empty()` falesne klasifikovalo
    // `.theme-dark` selector (parts mohly byt prazdne z parser jine cesty)
    // a cerne hodnoty z `.theme-dark` prepsaly --bg-app na #121212.
    for sheet in stylesheets {
        for rule in &sheet.rules {
            for sel in &rule.selectors {
                // Pure :root selector - jedna cast s tag "html" / ":root" /
                // pseudo "root", zadny class / id / atribut.
                let is_pure_root = sel.parts.len() == 1 && {
                    let p = &sel.parts[0];
                    p.classes.is_empty() && p.id.is_none() && p.attributes.is_empty()
                        && (p.tag.as_deref() == Some("html")
                            || p.tag.as_deref() == Some(":root")
                            || p.pseudo_classes.iter().any(|pc| pc == "root"))
                };
                if !is_pure_root { continue; }
                for decl in &rule.declarations {
                    if decl.property.starts_with("--") {
                        variables.insert(decl.property.clone(), decl.value.clone());
                    }
                }
            }
        }
    }

    if std::env::var("VAR_DEBUG").is_ok() {
        let mut keys: Vec<_> = variables.iter().collect();
        keys.sort_by(|a, b| a.0.cmp(b.0));
        for (k, v) in keys.iter().take(10) {
            eprintln!("[var] {} = {}", k, v);
        }
        eprintln!("[var] (total {} vars)", variables.len());
    }
    // PERF: precompute selector keys (rightmost simple part) per rule -
    // quick reject O(num_classes+1) misto plne matches_selector walk per node.
    // Vsechny selektory v rule sdilet keys (rule muze mit multiple selectors
    // comma-separated, kazdy ma vlastni key). Iterate paralel s sel uvnitr loop.
    // For each sheet:
    //   sheet_keys[i] = Vec<SelectorKey> jeden per sel v rule i (po flatten rules).
    let mut layered_keys: Vec<Vec<Vec<SelectorKey>>> = Vec::with_capacity(stylesheets.len()); // [sheet][rule_in_layer][sel]
    let mut unlayered_keys: Vec<Vec<Vec<SelectorKey>>> = Vec::with_capacity(stylesheets.len()); // [sheet][rule][sel]
    let mut scope_keys: Vec<Vec<Vec<Vec<SelectorKey>>>> = Vec::with_capacity(stylesheets.len()); // [sheet][scope][rule][sel]
    for sheet in stylesheets {
        // Layered (flat across all layers - lookup by index pak v cascade loop).
        let mut sheet_layered: Vec<Vec<SelectorKey>> = Vec::new();
        for (_, rules) in &sheet.layered_rules {
            for rule in rules {
                sheet_layered.push(rule.selectors.iter().map(SelectorKey::from_selector).collect());
            }
        }
        layered_keys.push(sheet_layered);
        // Unlayered.
        let sheet_unlayered: Vec<Vec<SelectorKey>> = sheet.rules.iter()
            .map(|r| r.selectors.iter().map(SelectorKey::from_selector).collect())
            .collect();
        unlayered_keys.push(sheet_unlayered);
        // Scopes.
        let sheet_scopes: Vec<Vec<Vec<SelectorKey>>> = sheet.scopes.iter()
            .map(|sc| sc.rules.iter()
                .map(|r| r.selectors.iter().map(SelectorKey::from_selector).collect())
                .collect())
            .collect();
        scope_keys.push(sheet_scopes);
    }
    // Prochazime DOM, pro kazdy element zkontrolujeme vsechny rules
    root.walk(&mut |node| {
        if !matches!(node.kind, NodeKind::Element { .. }) { return; }

        let mut matched_decls: Vec<((u32, u32, u32, usize), &super::css_parser::Declaration)> = Vec::new();
        let mut order = 0;

        // PERF: precompute node identity ONCE per node. tag_name_ref vraci &str
        // (drive `.tag_name()` clone'oval String per call).
        let node_tag = node.tag_name_ref().unwrap_or("");
        let node_id_str = node.attr("id");
        let node_id_opt = node_id_str.as_deref();
        let node_classes = node.attr("class").unwrap_or_default();

        // Debug breakpoint hook: BP_TAG/BP_ID/BP_CLASS env vars + IDE breakpoint
        // na `breakpoint_cascade` v src/debug_bp.rs.
        if crate::debug_bp::bp_enabled()
            && crate::debug_bp::bp_match(node_tag, node_id_opt.unwrap_or(""), &node_classes)
        {
            crate::debug_bp::breakpoint_cascade();
        }

        for (sheet_idx, sheet) in stylesheets.iter().enumerate() {
            // Layered rules nejprve (nizsi prio) - per CSS Cascade Layers L5.
            let mut layered_rule_idx = 0usize;
            for (layer_name, rules) in &sheet.layered_rules {
                let layer_priority = sheet.layer_order.iter().position(|n| n == layer_name)
                    .unwrap_or(0) as u32;
                for rule in rules {
                    let rule_keys = &layered_keys[sheet_idx][layered_rule_idx];
                    layered_rule_idx += 1;
                    for (sel_idx, sel) in rule.selectors.iter().enumerate() {
                        if sel.parts.last().map(|p| p.pseudo_element.is_some()).unwrap_or(false) {
                            continue;
                        }
                        // Quick reject before expensive matches_selector walk.
                        if !rule_keys[sel_idx].might_match(&node_tag, node_id_opt, &node_classes) {
                            continue;
                        }
                        if matches_selector(node, sel) {
                            let spec = specificity(sel);
                            for decl in &rule.declarations {
                                let key = (
                                    if decl.important { 1 } else { 0 },
                                    layer_priority,
                                    spec.0 * 1000 + spec.1 + spec.2,
                                    order,
                                );
                                matched_decls.push(((key.0, key.1, key.2, key.3), decl));
                                order += 1;
                            }
                        }
                    }
                }
            }
            // Unlayered (default) - nejvyssi prio (po !important).
            for (rule_idx, rule) in sheet.rules.iter().enumerate() {
                let rule_keys = &unlayered_keys[sheet_idx][rule_idx];
                for (sel_idx, sel) in rule.selectors.iter().enumerate() {
                    if sel.parts.last().map(|p| p.pseudo_element.is_some()).unwrap_or(false) {
                        continue;
                    }
                    if !rule_keys[sel_idx].might_match(&node_tag, node_id_opt, &node_classes) {
                        continue;
                    }
                    if matches_selector(node, sel) {
                        let spec = specificity(sel);
                        for decl in &rule.declarations {
                            let key = (
                                if decl.important { 1 } else { 0 },
                                u32::MAX,
                                spec.0 * 1000 + spec.1 + spec.2,
                                order,
                            );
                            matched_decls.push(((key.0, key.1, key.2, key.3), decl));
                            order += 1;
                        }
                    }
                }
            }
            // @scope rules.
            for (scope_idx, scope) in sheet.scopes.iter().enumerate() {
                if !node_in_scope(node, &scope.root_selector, scope.limit_selector.as_deref()) {
                    continue;
                }
                for (rule_idx, rule) in scope.rules.iter().enumerate() {
                    let rule_keys = &scope_keys[sheet_idx][scope_idx][rule_idx];
                    for (sel_idx, sel) in rule.selectors.iter().enumerate() {
                        if sel.parts.last().map(|p| p.pseudo_element.is_some()).unwrap_or(false) {
                            continue;
                        }
                        if !rule_keys[sel_idx].might_match(&node_tag, node_id_opt, &node_classes) {
                            continue;
                        }
                        if matches_selector(node, sel) {
                            let spec = specificity(sel);
                            for decl in &rule.declarations {
                                let key = (
                                    if decl.important { 1 } else { 0 },
                                    u32::MAX,
                                    spec.0 * 1000 + spec.1 + spec.2 + 1,
                                    order,
                                );
                                matched_decls.push(((key.0, key.1, key.2, key.3), decl));
                                order += 1;
                            }
                        }
                    }
                }
            }
        }

        // Sort podle (important, id_count, class+type, order) - vyssi kombinace vyhrava
        matched_decls.sort_by(|a, b| a.0.cmp(&b.0));

        let mut styles = HashMap::new();
        for (_, decl) in matched_decls {
            let resolved = resolve_value_with_funcs(&decl.value, &variables, &functions);
            let resolved = resolve_attr_in_value(&resolved, node);
            // CSS-wide keywords: inherit / initial / unset / revert / revert-layer.
            // `inherit` = remove + propagate_inherited dosadi parent (pro inherited
            // props). `initial` / `unset` / `revert` ucinne reset na default.
            // Bez handling `inherit` zustal jako literal string v mapa ->
            // bx.font_family = "inherit" -> lookup atlas "inherit" -> None ->
            // system default font, ne real parent.
            let kw = resolved.trim();
            if matches!(kw, "inherit" | "unset" | "initial" | "revert" | "revert-layer") {
                styles.remove(&decl.property);
                continue;
            }
            expand_shorthand(&decl.property, &resolved, &mut styles);
        }

        // Inline styly z attributu "style" maji nejvyssi prioritu (mimo !important rules)
        if let Some(inline) = node.attr("style") {
            for pair in inline.split(';') {
                if let Some(colon) = pair.find(':') {
                    let prop = pair[..colon].trim().to_string();
                    let val = pair[colon+1..].trim().to_string();
                    if !prop.is_empty() && !val.is_empty() {
                        let resolved = resolve_value(&val, &variables);
                        let resolved = resolve_attr_in_value(&resolved, node);
                        let kw = resolved.trim();
                        if matches!(kw, "inherit" | "unset" | "initial" | "revert" | "revert-layer") {
                            styles.remove(&prop);
                            continue;
                        }
                        expand_shorthand(&prop, &resolved, &mut styles);
                    }
                }
            }
        }

        style_map.insert(node_id(node), styles);
    });

    // UA tag defaults pro h1-h6 font-size + font-weight - musi byt v cascade
    // entry PRED propagate_inherited, jinak parent font-size inherit overrida
    // tag default (h2 v `body { font-size: 13px }` dostane 13 misto UA 24).
    apply_ua_tag_defaults(root, &mut style_map);
    // Inheritance pass: pro kazdy element, ktery NEMA explicit hodnotu pro
    // inherited CSS prop (font-*, color, text-*, line-height, ...), prevezme
    // hodnotu od parent. CSS spec: inherited props automaticky kaskaduji.
    propagate_inherited(root, &mut style_map, None);

    style_map
}

/// Aplikuje UA tag defaults (font-size pro h1-h6, font-weight bold) do
/// cascade entry. Volat PRED propagate_inherited - inherit pak respektuje
/// tag-specific UA values.
fn apply_ua_tag_defaults(node: &Rc<Node>, style_map: &mut StyleMap) {
    if let Some(tag) = node.tag_name_ref() {
        let (fs, bold) = match tag {
            "h1" => (Some("2em"), true),
            "h2" => (Some("1.5em"), true),
            "h3" => (Some("1.17em"), true),
            "h4" => (Some("1em"), true),
            "h5" => (Some("0.83em"), true),
            "h6" => (Some("0.67em"), true),
            "strong" | "b" => (None, true),
            _ => (None, false),
        };
        if fs.is_some() || bold {
            let entry = style_map.entry(node_id(node)).or_default();
            if let Some(v) = fs {
                entry.entry("font-size".into()).or_insert_with(|| v.to_string());
            }
            if bold {
                entry.entry("font-weight".into()).or_insert_with(|| "bold".to_string());
            }
        }
    }
    for ch in node.children.borrow().iter() {
        apply_ua_tag_defaults(ch, style_map);
    }
}

/// Recurse top-down a propaguj inherited props od parent na deti.
fn propagate_inherited(
    node: &Rc<Node>,
    style_map: &mut StyleMap,
    parent_styles: Option<&HashMap<String, String>>,
) {
    // Inherited CSS props per CSS spec. `font-size`, `font-weight`, `font-stretch`
    // jsou inherited (NE jen aproximace pres UA tag defaults v build_box). Bez
    // toho `body { font-size: 13px }` se NEPROPAGOVAL do deti - cascade vracela
    // chybejici font-size, layoutbox zustal default 16. (Pri h1-h6 UA tag
    // defaults v `apply_default_tag_styles` overrida bx.font_size jen pri
    // ne-CSS-specified value; entry mapy uz inherit-only, kdykoli rule s
    // explicit font-size winsne, jinak parent value.)
    const INHERITED: &[&str] = &[
        "font-family", "font-size", "font-weight", "font-style", "font-stretch",
        "font-variant", "font-feature-settings", "font-variation-settings",
        "color", "line-height", "letter-spacing", "word-spacing",
        "text-align",
        "text-indent", "text-transform", "white-space", "word-break", "overflow-wrap",
        "direction", "writing-mode", "visibility", "cursor", "list-style", "list-style-type",
        "list-style-position", "list-style-image", "quotes", "tab-size",
        // CSS variables (--foo) inherit. Bez tohoto :root vars nebyly available
        // pri deeper cascade lookup pres var() resolution v deti rules.
    ];
    if matches!(node.kind, NodeKind::Element { .. }) {
        let id = node_id(node);
        // Klonu parent_styles jako vector inherited slozka aplikacni:
        if let Some(parent) = parent_styles {
            let entry = style_map.entry(id).or_default();
            for &prop in INHERITED {
                if !entry.contains_key(prop) {
                    if let Some(v) = parent.get(prop) {
                        entry.insert(prop.into(), v.clone());
                    }
                }
            }
            // CSS custom properties (--foo) inherit per CSS Variables spec.
            // Bez tohoto deep child v stromu nemel pristup k :root --vars
            // pro var() resolution -> rules s var(--text-primary) vracely
            // empty/initial value.
            for (k, v) in parent.iter() {
                if k.starts_with("--") && !entry.contains_key(k) {
                    entry.insert(k.clone(), v.clone());
                }
            }
        }
    }
    // Get this node's styles AFTER inheritance to pass to children.
    let own_styles = style_map.get(&node_id(node)).cloned();
    let pass_styles = own_styles.as_ref().or(parent_styles);
    for ch in node.children.borrow().iter() {
        propagate_inherited(ch, style_map, pass_styles);
    }
}

/// Cascade jen pro pseudo-elements (::before / ::after / ...).
/// Vraci mapu (node_id, pseudo_name) -> computed styles, pro elementy co matchuji
/// selektor s pseudo_element.
pub fn cascade_pseudo(root: &Rc<Node>, stylesheets: &[Stylesheet]) -> PseudoStyleMap {
    let out: PseudoStyleMap = HashMap::new();

    // PERF fast-path: stylesheets without ::before / ::after / ::placeholder /
    // ::marker / ::first-letter etc -> celkove no pseudo selectors. Skip walk.
    // Flamegraph: cascade_pseudo 531 samples - dominantni. 90%+ stranek nepouziva.
    // Musi prochazet VSECHNY zdroje rules (unlayered + layered + scope +
    // starting-style + @media + @container), jinak pseudo v @layer nebo @media
    // by se ztratil v fast-path.
    let rule_has_pseudo = |r: &Rule| r.selectors.iter().any(|s|
        s.parts.last().map(|p| p.pseudo_element.is_some()).unwrap_or(false));
    let has_any_pseudo = stylesheets.iter().any(|sh| {
        sh.rules.iter().any(&rule_has_pseudo)
            || sh.layered_rules.iter().any(|(_, rs)| rs.iter().any(&rule_has_pseudo))
            || sh.scopes.iter().any(|sc| sc.rules.iter().any(&rule_has_pseudo))
            || sh.starting_style_rules.iter().any(&rule_has_pseudo)
            || sh.media_queries.iter().any(|mq| mq.rules.iter().any(&rule_has_pseudo))
            || sh.container_queries.iter().any(|cq| cq.rules.iter().any(&rule_has_pseudo))
    });
    if !has_any_pseudo {
        return out;
    }
    let mut out = out;

    // Recyclujeme variables z hlavniho cascade (jen :root)
    let mut variables: HashMap<String, String> = HashMap::new();
    for sheet in stylesheets {
        for rule in &sheet.rules {
            for sel in &rule.selectors {
                let is_root = sel.parts.iter().any(|p|
                    p.tag.as_deref() == Some("html") ||
                    p.pseudo_classes.iter().any(|pc| pc == "root")
                ) || sel.parts.is_empty();
                if !is_root { continue; }
                for decl in &rule.declarations {
                    if decl.property.starts_with("--") {
                        variables.insert(decl.property.clone(), decl.value.clone());
                    }
                }
            }
        }
    }

    root.walk(&mut |node| {
        if !matches!(node.kind, NodeKind::Element { .. }) { return; }

        // Pro kazdy pseudo-element name shromazdime matched declarations
        let mut by_pseudo: HashMap<String, Vec<((u32, u32, u32, usize), &super::css_parser::Declaration)>>
            = HashMap::new();
        let mut order = 0;

        for sheet in stylesheets {
            for rule in &sheet.rules {
                for sel in &rule.selectors {
                    // Najdi pseudo_element v poslední casti selectoru
                    let pe = sel.parts.last().and_then(|p| p.pseudo_element.clone());
                    let pe = match pe { Some(p) => p, None => continue };
                    if !matches_selector(node, sel) { continue; }
                    let spec = specificity(sel);
                    for decl in &rule.declarations {
                        let key = (
                            if decl.important { 1 } else { 0 },
                            spec.0,
                            spec.1 + spec.2,
                            order,
                        );
                        by_pseudo.entry(pe.clone()).or_default().push((key, decl));
                        order += 1;
                    }
                }
            }
        }

        for (pe_name, mut list) in by_pseudo {
            list.sort_by(|a, b| a.0.cmp(&b.0));
            let mut styles = HashMap::new();
            for (_, decl) in list {
                let resolved = resolve_value(&decl.value, &variables);
                let resolved = resolve_attr_in_value(&resolved, node);
                expand_shorthand(&decl.property, &resolved, &mut styles);
            }
            out.insert((node_id(node), pe_name), styles);
        }
    });

    out
}

/// Vrati pseudo-element styles pro dany node + name (pomocnik).
pub fn get_pseudo_styles<'a>(map: &'a PseudoStyleMap, node: &Rc<Node>, pseudo: &str)
    -> Option<&'a HashMap<String, String>>
{
    map.get(&(node_id(node), pseudo.to_string()))
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

    // Pro sibling combinatory drzime aktualni "scope node" - pri prvni iteraci
    // je to puvodni `node`, jeho parent je current_node uz nastavene.
    let mut scope_node = Rc::clone(node);

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
                    scope_node = Rc::clone(&p);
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
                        scope_node = Rc::clone(&p);
                        current_node = next;
                        found = true;
                        break;
                    }
                    let next = p.parent.borrow().upgrade();
                    current_node = next;
                }
                if !found { return false; }
            }
            Combinator::AdjacentSibling => {
                // Predchazejici sourozenec scope_node musi matchovat part
                let parent = scope_node.parent.borrow().upgrade();
                let parent = match parent { Some(p) => p, None => return false };
                let children = parent.children.borrow();
                let idx = children.iter().position(|c| Rc::ptr_eq(c, &scope_node));
                let idx = match idx { Some(i) => i, None => return false };
                // Najdi predchazejici element (skip text/comment)
                let mut prev: Option<Rc<Node>> = None;
                for j in (0..idx).rev() {
                    if matches!(children[j].kind, NodeKind::Element(_)) {
                        prev = Some(Rc::clone(&children[j]));
                        break;
                    }
                }
                let prev = match prev { Some(p) => p, None => return false };
                if !matches_simple(&prev, part) { return false; }
                if current_part == 0 { return true; }
                current_part -= 1;
                scope_node = Rc::clone(&prev);
                current_node = prev.parent.borrow().upgrade();
            }
            Combinator::GeneralSibling => {
                // Nektery predchazejici sourozenec musi matchovat part
                let parent = scope_node.parent.borrow().upgrade();
                let parent = match parent { Some(p) => p, None => return false };
                let children = parent.children.borrow();
                let idx = children.iter().position(|c| Rc::ptr_eq(c, &scope_node));
                let idx = match idx { Some(i) => i, None => return false };
                let mut found: Option<Rc<Node>> = None;
                for j in (0..idx).rev() {
                    if matches!(children[j].kind, NodeKind::Element(_))
                        && matches_simple(&children[j], part)
                    {
                        found = Some(Rc::clone(&children[j]));
                        break;
                    }
                }
                let prev = match found { Some(p) => p, None => return false };
                if current_part == 0 { return true; }
                current_part -= 1;
                scope_node = Rc::clone(&prev);
                current_node = prev.parent.borrow().upgrade();
            }
        }
    }
}

/// Kontroluje simple selector proti uzlu.
pub fn matches_simple(node: &Rc<Node>, sel: &SimpleSelector) -> bool {
    use super::css_parser::AttrOp;

    // PERF: tag_name_ref() vraci &str - bez String clone per call.
    let tag = match node.tag_name_ref() {
        Some(t) => t,
        None => return false,
    };

    if let Some(want_tag) = &sel.tag {
        // PERF: want_tag uz lowercased pri parse (selectors::parse). tag z
        // tag_name() take lowercased v DOM build. Bez to_lowercase() per match.
        if want_tag != "*" && want_tag != tag {
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
        // PERF: alloc-free contains check pres split_whitespace iter.
        // Drive `classes.collect::<Vec>` + `classes.contains` allocoval Vec per match.
        for required in &sel.classes {
            let mut found = false;
            for c in class_attr.split_whitespace() {
                if c == required.as_str() { found = true; break; }
            }
            if !found { return false; }
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
            (AttrOp::DashMatch, Some(want), Some(got)) => {
                // [lang|="en"] match "en" or "en-US" / "en-GB" prefixu.
                if got != want && !got.starts_with(&format!("{want}-")) { return false; }
            }
            _ => {}
        }
    }

    // Pseudo-classes (bez argumentu)
    for pc in &sel.pseudo_classes {
        match pc.as_str() {
            "root" => {
                if tag != "html" { return false; }
            }
            "first-child" => {
                let parent = node.parent.borrow().upgrade();
                if let Some(p) = parent {
                    let children = p.children.borrow();
                    let first_el = children.iter().find(|c| matches!(c.kind, NodeKind::Element(_)));
                    if first_el.map(|f| !Rc::ptr_eq(f, node)).unwrap_or(true) {
                        return false;
                    }
                }
            }
            "last-child" => {
                let parent = node.parent.borrow().upgrade();
                if let Some(p) = parent {
                    let children = p.children.borrow();
                    let last_el = children.iter().rev().find(|c| matches!(c.kind, NodeKind::Element(_)));
                    if last_el.map(|f| !Rc::ptr_eq(f, node)).unwrap_or(true) {
                        return false;
                    }
                }
            }
            "only-child" => {
                let parent = node.parent.borrow().upgrade();
                if let Some(p) = parent {
                    let children = p.children.borrow();
                    let count = children.iter().filter(|c| matches!(c.kind, NodeKind::Element(_))).count();
                    if count != 1 { return false; }
                }
            }
            "first-of-type" | "last-of-type" | "only-of-type" => {
                let parent = node.parent.borrow().upgrade();
                if let Some(p) = parent {
                    let children = p.children.borrow();
                    let same_tag: Vec<_> = children.iter()
                        .filter(|c| matches!(c.kind, NodeKind::Element(_)))
                        .filter(|c| c.tag_name().as_deref() == Some(tag))
                        .collect();
                    let pos = same_tag.iter().position(|c| Rc::ptr_eq(c, node));
                    let pos = match pos { Some(p) => p, None => return false };
                    match pc.as_str() {
                        "first-of-type" => if pos != 0 { return false; },
                        "last-of-type" => if pos != same_tag.len() - 1 { return false; },
                        "only-of-type" => if same_tag.len() != 1 { return false; },
                        _ => {}
                    }
                }
            }
            "empty" => {
                let children = node.children.borrow();
                let has_content = children.iter().any(|c| match &c.kind {
                    NodeKind::Element(_) => true,
                    NodeKind::Text(t) => !t.is_empty(),
                    _ => false,
                });
                if has_content { return false; }
            }
            "any-link" | "scope" => { /* OK */ }
            // Form attribute pseudo-classes - lze staticky overit z DOM attributes
            "required" => {
                if node.attr("required").is_none() { return false; }
            }
            "optional" => {
                // :optional - jen na form input/select/textarea co NEMA required
                let is_form = matches!(tag, "input" | "select" | "textarea");
                if !is_form || node.attr("required").is_some() { return false; }
            }
            "disabled" => {
                if node.attr("disabled").is_none() { return false; }
            }
            "enabled" => {
                let is_form = matches!(tag, "input" | "select" | "textarea" | "button");
                if !is_form || node.attr("disabled").is_some() { return false; }
            }
            "checked" => {
                // checkbox / radio s checked attributem
                if node.attr("checked").is_none() { return false; }
            }
            "read-only" => {
                let is_form = matches!(tag, "input" | "textarea");
                if !is_form { return false; }
                // readonly attribut nebo not text-like input
                if node.attr("readonly").is_none() {
                    return false;
                }
            }
            "read-write" => {
                let is_form = matches!(tag, "input" | "textarea");
                if !is_form || node.attr("readonly").is_some() || node.attr("disabled").is_some() {
                    return false;
                }
            }
            "placeholder-shown" => {
                // :placeholder-shown match pokud value je prazdne a element ma placeholder
                let has_placeholder = node.attr("placeholder").is_some();
                let value_empty = node.attr("value").map(|v| v.is_empty()).unwrap_or(true);
                if !has_placeholder || !value_empty { return false; }
            }
            "popover-open" => {
                // :popover-open match elementu s popover atributem co je open.
                // HTML L1: popover state = "open" / "closed". Aproximace: kdyz ma data-popover-open="true"
                // OR popover atribut + open atribut.
                if !node.has_attr("popover") { return false; }
                let is_open = node.attr("data-popover-open").as_deref() == Some("true")
                    || node.attr("open").is_some();
                if !is_open { return false; }
            }
            "open" => {
                // :open - <details>/<dialog>/<select>/<input> co jsou otevrene
                let tag = tag;
                if !matches!(tag, "details" | "dialog" | "select" | "input") { return false; }
                if node.attr("open").is_none() { return false; }
            }
            "closed" => {
                let tag = tag;
                if !matches!(tag, "details" | "dialog") { return false; }
                if node.attr("open").is_some() { return false; }
            }
            "modal" => {
                // :modal match modaln dialog / fullscreen
                if tag != "dialog" { return false; }
                if node.attr("open").is_none() { return false; }
                // Modal kdyz showModal() volano; aproximace: ma data-modal=true
                if node.attr("data-modal").as_deref() != Some("true") { return false; }
            }
            "fullscreen" => {
                if node.attr("data-fullscreen").as_deref() != Some("true") { return false; }
            }
            "indeterminate" => {
                // :indeterminate - checkbox/radio s indeterminate=true (nelze HTML, jen JS)
                if node.attr("data-indeterminate").as_deref() != Some("true") { return false; }
            }
            "blank" => {
                // :blank - empty input
                let val = node.attr("value").unwrap_or_default();
                if !val.is_empty() { return false; }
            }
            "user-valid" => {
                // Selectors L5: :user-valid - prvek byl uzivatelem zmenen + je validni
                // Aproximace: pokud ma data-user-valid="true" attribute, OR same logic jako :valid
                let is_form = matches!(tag, "input" | "select" | "textarea");
                if !is_form { return false; }
                if node.attr("data-user-valid").as_deref() != Some("true") {
                    if node.attr("required").is_some() {
                        let val = node.attr("value").unwrap_or_default();
                        if val.is_empty() { return false; }
                    }
                }
            }
            "user-invalid" => {
                let is_form = matches!(tag, "input" | "select" | "textarea");
                if !is_form { return false; }
                if node.attr("data-user-invalid").as_deref() != Some("true") {
                    let mut is_invalid = false;
                    if node.attr("required").is_some() {
                        let val = node.attr("value").unwrap_or_default();
                        if val.is_empty() { is_invalid = true; }
                    }
                    if !is_invalid { return false; }
                }
            }
            "valid" => {
                // :valid match pokud form input s required ma neprazdnou hodnotu
                let is_form = matches!(tag, "input" | "select" | "textarea" | "form");
                if !is_form { return false; }
                if node.attr("required").is_some() {
                    let val = node.attr("value").unwrap_or_default();
                    if val.is_empty() { return false; }
                }
                // type="email" - musi obsahovat @
                if let Some(ty) = node.attr("type") {
                    if ty == "email" {
                        let val = node.attr("value").unwrap_or_default();
                        if !val.is_empty() && !val.contains('@') { return false; }
                    }
                }
            }
            "invalid" => {
                let is_form = matches!(tag, "input" | "select" | "textarea" | "form");
                if !is_form { return false; }
                let mut is_invalid = false;
                if node.attr("required").is_some() {
                    let val = node.attr("value").unwrap_or_default();
                    if val.is_empty() { is_invalid = true; }
                }
                if let Some(ty) = node.attr("type") {
                    if ty == "email" {
                        let val = node.attr("value").unwrap_or_default();
                        if !val.is_empty() && !val.contains('@') { is_invalid = true; }
                    }
                }
                if !is_invalid { return false; }
            }
            "default" => {
                // :default match pro default-checked input + button[type=submit]
                let is_default = match tag {
                    "button" => node.attr("type").as_deref().unwrap_or("submit") == "submit",
                    "input" => node.attr("checked").is_some(),
                    _ => false,
                };
                if !is_default { return false; }
            }
            "in-range" | "out-of-range" => {
                // Vyzaduje runtime stav - skip
                return false;
            }
            // hover/active/focus runtime state - thread-local nodes nastaveny render loopem.
            "hover" => {
                if !is_node_or_ancestor_match(node, &HOVERED_NODE) { return false; }
            }
            "active" => {
                if !is_node_or_ancestor_match(node, &ACTIVE_NODE) { return false; }
            }
            "focus" | "focus-visible" => {
                if !is_node_match(node, &FOCUSED_NODE) { return false; }
            }
            "focus-within" => {
                if !is_node_or_ancestor_match(node, &FOCUSED_NODE) { return false; }
            }
            "visited" | "link" => return false,
            _ => {}
        }
    }

    // Funkcni pseudo-classes
    for pf in &sel.pseudo_funcs {
        match pf {
            super::css_parser::PseudoFunc::Is(args)
            | super::css_parser::PseudoFunc::Where(args) => {
                if !args.iter().any(|s| matches_selector(node, s)) { return false; }
            }
            super::css_parser::PseudoFunc::Not(args) => {
                if args.iter().any(|s| matches_selector(node, s)) { return false; }
            }
            super::css_parser::PseudoFunc::Has(args) => {
                // :has(selector) - existuje descendant matchujici selector
                if !has_matching_descendant(node, args) { return false; }
            }
            super::css_parser::PseudoFunc::NthChild { a, b, of_type, last } => {
                if !nth_child_matches(node, *a, *b, *of_type, *last, &tag) { return false; }
            }
            super::css_parser::PseudoFunc::Lang(lang_arg) => {
                // :lang(en) matches if element OR ancestor has lang="en" / "en-US" / etc.
                // BCP 47 prefix match: :lang(en) -> matches "en", "en-US", "en-GB", but not "fr".
                let arg_lower = lang_arg.to_lowercase();
                let mut current = Some(Rc::clone(node));
                let mut found = false;
                while let Some(n) = current {
                    if let Some(lang) = n.attr("lang") {
                        let lang_lower = lang.to_lowercase();
                        if lang_lower == arg_lower
                            || lang_lower.starts_with(&format!("{}-", arg_lower)) {
                            found = true;
                            break;
                        }
                    }
                    current = n.parent.borrow().upgrade();
                }
                if !found { return false; }
            }
            super::css_parser::PseudoFunc::Dir(dir_arg) => {
                // :dir(ltr|rtl) matches dle direction attr / inherited.
                let mut current = Some(Rc::clone(node));
                let mut dir_found: Option<String> = None;
                while let Some(n) = current {
                    if let Some(d) = n.attr("dir") {
                        dir_found = Some(d.to_lowercase());
                        break;
                    }
                    current = n.parent.borrow().upgrade();
                }
                let actual = dir_found.as_deref().unwrap_or("ltr");
                // "auto" je ltr (default text flow) - approximace.
                let resolved = if actual == "auto" { "ltr" } else { actual };
                if resolved != dir_arg { return false; }
            }
            super::css_parser::PseudoFunc::Unknown { .. } => {
                // Neznamy pseudo - nepouzit pravidlo (safe)
                return false;
            }
        }
    }

    true
}

/// :has(selector) - vrati true pokud nejaky descendant matchuje arg.
fn has_matching_descendant(node: &Rc<Node>, args: &[super::css_parser::Selector]) -> bool {
    let children = node.children.borrow();
    for child in children.iter() {
        if !matches!(child.kind, NodeKind::Element(_)) { continue; }
        if args.iter().any(|s| matches_selector(child, s)) { return true; }
        if has_matching_descendant(child, args) { return true; }
    }
    false
}

/// :nth-child / :nth-of-type / :nth-last-* matching.
/// an+b: vrati true pokud index splnuje (index = (n*a + b) pro n=0,1,2,...).
fn nth_child_matches(node: &Rc<Node>, a: i32, b: i32, of_type: bool, last: bool, tag: &str) -> bool {
    let parent = match node.parent.borrow().upgrade() { Some(p) => p, None => return false };
    let children = parent.children.borrow();
    let siblings: Vec<_> = children.iter()
        .filter(|c| matches!(c.kind, NodeKind::Element(_)))
        .filter(|c| !of_type || c.tag_name().as_deref() == Some(tag))
        .collect();
    let pos = siblings.iter().position(|c| Rc::ptr_eq(c, node));
    let pos = match pos { Some(p) => p, None => return false };
    let idx = if last { siblings.len() - 1 - pos + 1 } else { pos + 1 } as i32; // 1-based

    // Reseni an+b = idx -> (idx - b) % a == 0 a (idx - b) / a >= 0
    if a == 0 {
        return idx == b;
    }
    let diff = idx - b;
    if diff % a != 0 { return false; }
    diff / a >= 0
}

/// Vrati computed styles pro dany uzel (z StyleMap).
pub fn get_styles<'a>(map: &'a StyleMap, node: &Rc<Node>) -> Option<&'a HashMap<String, String>> {
    map.get(&node_id(node))
}

/// Parsovany shorthand `animation` property.
/// Spec je permisive co do poradi tokenu.
#[derive(Debug, Clone)]
pub struct AnimationSpec {
    pub name: String,
    pub duration_secs: f32,
    pub timing_function: String, // "linear" / "ease" / "ease-in" / "ease-out" / "ease-in-out" / "cubic-bezier(...)" / "steps(...)"
    pub iteration_count: f32,    // f32::INFINITY pro "infinite"
    pub direction: String,        // "normal" / "reverse" / "alternate" / "alternate-reverse"
    pub delay_secs: f32,
    pub fill_mode: String,        // "none" / "forwards" / "backwards" / "both"
    pub play_state: String,       // "running" / "paused"
}

impl AnimationSpec {
    pub fn from_styles(styles: &HashMap<String, String>) -> Option<AnimationSpec> {
        // PERF fast-path: vetsina elementu animation NEMA. Bail bez vsech parse
        // kroku. Flamegraph: AnimationSpec::from_styles 888 samples - dominantni.
        if !styles.contains_key("animation")
            && !styles.contains_key("animation-name") {
            return None;
        }
        // Bud `animation` shorthand, nebo `animation-name` + dalsi longhand.
        let mut name: Option<String> = None;
        let mut duration: f32 = 0.0;
        let mut timing: String = "linear".into();
        let mut iter: f32 = 1.0;
        let mut direction: String = "normal".into();
        let mut delay: f32 = 0.0;

        let mut fill_mode: String = "none".into();
        let mut play_state: String = "running".into();

        // Shorthand parsing - tokenizace respektuje zavorky (cubic-bezier(...), steps(...))
        // Multi-animation shorthand `a 3s, b 1s infinite` musi byt parsovany
        // separately - jinak jeden spec sluci tokeny vsech a vznikne
        // permanent infinite. Bereme jen PRVNI subspec (TODO multi-animation
        // tracking pres Vec<AnimationSpec> v active_animations).
        if let Some(short_full) = styles.get("animation") {
            let short_first = match split_top_level_commas(short_full).first() {
                Some(s) => s.to_string(),
                None => short_full.clone(),
            };
            for tok in tokenize_balanced(&short_first) {
                let tok = tok.as_str();
                if let Some(s) = parse_time(tok) {
                    if duration == 0.0 { duration = s; } else { delay = s; }
                } else if tok == "infinite" {
                    iter = f32::INFINITY;
                } else if let Ok(n) = tok.parse::<f32>() {
                    iter = n;
                } else if matches!(tok, "linear" | "ease" | "ease-in" | "ease-out" | "ease-in-out" | "step-start" | "step-end")
                    || tok.starts_with("cubic-bezier(") || tok.starts_with("steps(")
                {
                    timing = tok.to_string();
                } else if matches!(tok, "normal" | "reverse" | "alternate" | "alternate-reverse") {
                    direction = tok.to_string();
                } else if matches!(tok, "none" | "forwards" | "backwards" | "both") {
                    fill_mode = tok.to_string();
                } else if matches!(tok, "running" | "paused") {
                    play_state = tok.to_string();
                } else {
                    // Predpokladej name
                    if name.is_none() { name = Some(tok.to_string()); }
                }
            }
        }

        // Longhand override
        if let Some(v) = styles.get("animation-name") { name = Some(v.trim().to_string()); }
        if let Some(v) = styles.get("animation-duration").and_then(|s| parse_time(s.trim())) { duration = v; }
        if let Some(v) = styles.get("animation-timing-function") { timing = v.trim().to_string(); }
        if let Some(v) = styles.get("animation-iteration-count") {
            iter = if v.trim() == "infinite" { f32::INFINITY } else { v.trim().parse().unwrap_or(1.0) };
        }
        if let Some(v) = styles.get("animation-direction") { direction = v.trim().to_string(); }
        if let Some(v) = styles.get("animation-delay").and_then(|s| parse_time(s.trim())) { delay = v; }
        if let Some(v) = styles.get("animation-fill-mode") { fill_mode = v.trim().to_string(); }
        if let Some(v) = styles.get("animation-play-state") { play_state = v.trim().to_string(); }

        let name = name?;
        if name == "none" || duration <= 0.0 { return None; }
        Some(AnimationSpec {
            name, duration_secs: duration, timing_function: timing,
            iteration_count: iter, direction, delay_secs: delay,
            fill_mode, play_state,
        })
    }

    /// L5 step 4 Phase H: typed verze - cte z ComputedStyle. Tahne prvni entry
    /// z Vec<*> pro vsechny anim-* longhandy. Multi-animation support tu zatim
    /// neni - viz from_styles komentar.
    pub fn from_cs(cs: &crate::browser::computed_style::ComputedStyle) -> Option<AnimationSpec> {
        let name = cs.animation_name.first()?.clone();
        if name == "none" || name.is_empty() { return None; }
        let duration = cs.animation_duration.first().copied().unwrap_or(0.0);
        if duration <= 0.0 { return None; }
        let timing = cs.animation_timing_function.first()
            .map(|t| t.css_string()).unwrap_or_else(|| "linear".into());
        let iter = cs.animation_iteration_count.first().copied().unwrap_or(1.0);
        let direction = cs.animation_direction.first()
            .map(|d| d.css_string().to_string()).unwrap_or_else(|| "normal".into());
        let delay = cs.animation_delay.first().copied().unwrap_or(0.0);
        let fill_mode = cs.animation_fill_mode.first()
            .map(|f| f.css_string().to_string()).unwrap_or_else(|| "none".into());
        let play_state = cs.animation_play_state.first()
            .map(|p| p.css_string().to_string()).unwrap_or_else(|| "running".into());
        Some(AnimationSpec {
            name, duration_secs: duration, timing_function: timing,
            iteration_count: iter, direction, delay_secs: delay,
            fill_mode, play_state,
        })
    }
}

/// CSS Transitions L1 parsovany shorthand.
/// "transition: <prop> <duration> <timing-function> <delay> [, <next>]"
#[derive(Debug, Clone)]
pub struct TransitionSpec {
    pub property: String,           // "all" / "color" / "transform" / ...
    pub duration_secs: f32,
    pub timing_function: String,    // "linear" / "ease" / "cubic-bezier(...)" / ...
    pub delay_secs: f32,
}

impl TransitionSpec {
    /// Parsuje vsechny transitions z computed styles. Vraci seznam (mozne vice
    /// transitions oddelenych carkou, kazda pro jine property).
    pub fn from_styles(styles: &HashMap<String, String>) -> Vec<TransitionSpec> {
        // PERF fast-path: vetsina elementu transition NEMA. Bail bez parse / Vec alloc.
        // Pri 5000 elements × 60 fps = 300k volani per sec. Drive vsech 5000
        // procit a parsovat string per frame. Ted O(1) check.
        if !styles.contains_key("transition")
            && !styles.contains_key("transition-property") {
            return Vec::new();
        }
        let mut out = Vec::new();

        // Shorthand "transition" - muze obsahovat carku pro vice transitions
        if let Some(short) = styles.get("transition") {
            for entry in split_top_level_commas(short) {
                if let Some(spec) = Self::parse_one(entry.trim()) {
                    out.push(spec);
                }
            }
            if !out.is_empty() { return out; }
        }

        // Longhand: transition-property/-duration/-timing-function/-delay
        let props = styles.get("transition-property").map(|s| s.trim().to_string());
        let durations = styles.get("transition-duration").map(|s| s.trim().to_string());
        let timings = styles.get("transition-timing-function").map(|s| s.trim().to_string());
        let delays = styles.get("transition-delay").map(|s| s.trim().to_string());

        if let Some(p) = props {
            let p_list: Vec<&str> = p.split(',').map(|s| s.trim()).collect();
            let d_list: Vec<&str> = durations.as_deref().unwrap_or("0s").split(',').map(|s| s.trim()).collect();
            let t_list: Vec<&str> = timings.as_deref().unwrap_or("ease").split(',').map(|s| s.trim()).collect();
            let dl_list: Vec<&str> = delays.as_deref().unwrap_or("0s").split(',').map(|s| s.trim()).collect();

            for (i, prop) in p_list.iter().enumerate() {
                let dur = d_list.get(i % d_list.len()).copied().unwrap_or("0s");
                let timing = t_list.get(i % t_list.len()).copied().unwrap_or("ease");
                let delay = dl_list.get(i % dl_list.len()).copied().unwrap_or("0s");
                out.push(TransitionSpec {
                    property: prop.to_string(),
                    duration_secs: parse_time(dur).unwrap_or(0.0),
                    timing_function: timing.to_string(),
                    delay_secs: parse_time(delay).unwrap_or(0.0),
                });
            }
        }
        out
    }

    fn parse_one(entry: &str) -> Option<TransitionSpec> {
        let mut property: Option<String> = None;
        let mut duration: f32 = 0.0;
        let mut timing: String = "ease".into();
        let mut delay: f32 = 0.0;
        let mut times_seen = 0;

        for tok in tokenize_balanced(entry) {
            let tok = tok.as_str();
            if let Some(t) = parse_time(tok) {
                if times_seen == 0 { duration = t; } else { delay = t; }
                times_seen += 1;
            } else if matches!(tok, "linear" | "ease" | "ease-in" | "ease-out" | "ease-in-out" | "step-start" | "step-end")
                || tok.starts_with("cubic-bezier(") || tok.starts_with("steps(")
            {
                timing = tok.to_string();
            } else {
                if property.is_none() { property = Some(tok.to_string()); }
            }
        }
        let property = property.unwrap_or_else(|| "all".to_string());
        if duration <= 0.0 { return None; }
        Some(TransitionSpec { property, duration_secs: duration, timing_function: timing, delay_secs: delay })
    }
}

/// Tokenize string respektujici vyvazene zavorky (pro cubic-bezier/steps).
fn tokenize_balanced(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    for ch in s.chars() {
        match ch {
            '(' => { depth += 1; cur.push(ch); }
            ')' => { depth -= 1; cur.push(ch); }
            c if c.is_whitespace() && depth == 0 => {
                if !cur.is_empty() { tokens.push(std::mem::take(&mut cur)); }
            }
            _ => cur.push(ch),
        }
    }
    if !cur.is_empty() { tokens.push(cur); }
    tokens
}

/// Parsuje "2s" / "500ms" / "0.3s". Vrati sekundy.
fn parse_time(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(num) = s.strip_suffix("ms") {
        num.parse::<f32>().ok().map(|n| n / 1000.0)
    } else if let Some(num) = s.strip_suffix('s') {
        num.parse::<f32>().ok()
    } else {
        None
    }
}

/// Aplikuje easing na linearni progress (0..1).
fn apply_easing(t: f32, easing: &str) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let easing = easing.trim();
    match easing {
        "linear" => return t,
        "ease"        => return cubic_bezier(t, 0.25, 0.1, 0.25, 1.0),
        "ease-in"     => return cubic_bezier(t, 0.42, 0.0, 1.0, 1.0),
        "ease-out"    => return cubic_bezier(t, 0.0, 0.0, 0.58, 1.0),
        "ease-in-out" => return cubic_bezier(t, 0.42, 0.0, 0.58, 1.0),
        "step-start"  => return 1.0,
        "step-end"    => return if t >= 1.0 { 1.0 } else { 0.0 },
        _ => {}
    }
    // cubic-bezier(x1, y1, x2, y2)
    if let Some(args) = easing.strip_prefix("cubic-bezier(").and_then(|s| s.strip_suffix(')')) {
        let nums: Vec<f32> = args.split(',').filter_map(|p| p.trim().parse().ok()).collect();
        if nums.len() == 4 {
            return cubic_bezier(t, nums[0], nums[1], nums[2], nums[3]);
        }
    }
    // steps(n, jump-start|jump-end|jump-both|jump-none|start|end)
    if let Some(args) = easing.strip_prefix("steps(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = args.split(',').map(|s| s.trim()).collect();
        let n: i32 = parts.first().and_then(|p| p.parse().ok()).unwrap_or(1).max(1);
        let kind = parts.get(1).copied().unwrap_or("end");
        return apply_steps(t, n, kind);
    }
    t
}

/// CSS steps() - kvantizuje progress na n diskretnich kroku.
/// kind: "jump-start"/"start", "jump-end"/"end" (default), "jump-both", "jump-none"
fn apply_steps(t: f32, n: i32, kind: &str) -> f32 {
    let n = n as f32;
    match kind {
        "jump-start" | "start" => ((t * n).floor() + 1.0) / n,
        "jump-both"            => ((t * n).floor() + 1.0) / (n + 1.0),
        "jump-none" => {
            if n <= 1.0 { return 0.0; }
            (t * n).floor() / (n - 1.0)
        }
        _ /* jump-end / end */ => (t * n).floor() / n,
    }.clamp(0.0, 1.0)
}

/// Newton-iterace pro cubic-bezier easing kompletne na sjednoceni s CSS spec.
fn cubic_bezier(t: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    // Najdi parametr u takovy ze bezier_x(u) = t, vrat bezier_y(u).
    let mut u = t;
    for _ in 0..8 {
        let x = bezier(u, x1, x2);
        let dx = bezier_deriv(u, x1, x2);
        if dx.abs() < 1e-6 { break; }
        let diff = x - t;
        if diff.abs() < 1e-4 { break; }
        u -= diff / dx;
    }
    bezier(u.clamp(0.0, 1.0), y1, y2)
}

fn bezier(u: f32, p1: f32, p2: f32) -> f32 {
    let iu = 1.0 - u;
    3.0 * iu * iu * u * p1 + 3.0 * iu * u * u * p2 + u * u * u
}

fn bezier_deriv(u: f32, p1: f32, p2: f32) -> f32 {
    let iu = 1.0 - u;
    3.0 * iu * iu * p1 + 6.0 * iu * u * (p2 - p1) + 3.0 * u * u * (1.0 - p2)
}

/// Aktivni transition - po-spu sleduje stav per element + property.
#[derive(Debug, Clone)]
pub struct ActiveTransition {
    pub node_id: usize,
    pub property: String,
    pub from_value: String,
    pub to_value: String,
    pub spec: TransitionSpec,
    /// Cas v sekundach kdy transition zacala.
    pub start_time: f32,
}

/// Detekuje zmeny stylu mezi prev_map a current_map a vyrobi nove ActiveTransitions
/// pro elementy s `transition` property co maji match. Vsechny aktualne probihajici
/// transitions po dokonceni zmizi.
///
/// Vraci aktualizovany seznam transitions po teto frame iteraci.
pub fn detect_transitions(
    prev_map: &StyleMap,
    current_map: &StyleMap,
    active: Vec<ActiveTransition>,
    elapsed_secs: f32,
) -> Vec<ActiveTransition> {
    let mut result: Vec<ActiveTransition> = Vec::new();

    // Zachovaj aktivni transitions ktere jeste nedohrali
    for at in active {
        let total = at.spec.duration_secs + at.spec.delay_secs;
        if elapsed_secs - at.start_time < total {
            result.push(at);
        }
    }

    // Pro kazdy element v current detect zmeny vs prev
    for (node_id, cur) in current_map {
        let prev = match prev_map.get(node_id) { Some(p) => p, None => continue };
        let specs = TransitionSpec::from_styles(cur);
        if specs.is_empty() { continue; }

        for spec in &specs {
            // Match: bud "all" nebo konkretni property
            let props_to_check: Vec<&String> = if spec.property == "all" {
                cur.keys().collect()
            } else {
                if cur.contains_key(&spec.property) { vec![&spec.property] } else { vec![] }
            };

            for prop in props_to_check {
                let cur_val = cur.get(prop).map(|s| s.as_str()).unwrap_or("");
                let prev_val = prev.get(prop).map(|s| s.as_str()).unwrap_or("");
                if cur_val != prev_val && !prev_val.is_empty() {
                    // Skip pokud uz transition na tu prop existuje
                    if result.iter().any(|t| t.node_id == *node_id && t.property == *prop) { continue; }
                    result.push(ActiveTransition {
                        node_id: *node_id,
                        property: prop.clone(),
                        from_value: prev_val.to_string(),
                        to_value: cur_val.to_string(),
                        spec: spec.clone(),
                        start_time: elapsed_secs,
                    });
                }
            }
        }
    }
    result
}

/// Aplikuje aktivni transitions na current style map - interpoluje hodnoty.
pub fn apply_transitions(
    style_map: &mut StyleMap,
    active: &[ActiveTransition],
    elapsed_secs: f32,
) {
    for at in active {
        let t = elapsed_secs - at.start_time - at.spec.delay_secs;
        if t < 0.0 { continue; }
        let raw_progress = (t / at.spec.duration_secs).clamp(0.0, 1.0);
        let progress = apply_easing(raw_progress, &at.spec.timing_function);

        // Interpoluj hodnotu - pres parse_length jako f32
        let from = super::layout::parse_length(&at.from_value);
        let to = super::layout::parse_length(&at.to_value);
        let interpolated = if from != 0.0 || to != 0.0 {
            // Numericka prop: interpoluj
            let v = from + (to - from) * progress;
            // Zachovaj jednotku z to_value (heuristika)
            let unit = ["px", "em", "rem", "%", "vw", "vh", "deg", "rad"]
                .iter()
                .find(|u| at.to_value.ends_with(*u))
                .copied()
                .unwrap_or("px");
            format!("{v}{unit}")
        } else {
            // Non-numericka - krokove (snap)
            if progress < 0.5 { at.from_value.clone() } else { at.to_value.clone() }
        };

        if let Some(styles) = style_map.get_mut(&at.node_id) {
            styles.insert(at.property.clone(), interpolated);
        }
    }
}

/// Aplikuje scroll-driven animations - misto time elapsed pouzij scroll progress.
/// `scroll_progress` = scroll_y / max_scroll (0..1).
pub fn apply_scroll_animations(
    style_map: &mut StyleMap,
    stylesheets: &[Stylesheet],
    scroll_progress: f32,
) -> bool {
    use super::layout::interpolate_keyframes;
    let mut any_active = false;
    for styles in style_map.values_mut() {
        // Detect animation-timeline pres styles
        let timeline = styles.get("animation-timeline").cloned().unwrap_or_default();
        if !timeline.starts_with("scroll(") && timeline != "scroll" { continue; }
        let spec = match AnimationSpec::from_styles(styles) {
            Some(s) => s, None => continue,
        };
        let frames = stylesheets.iter()
            .flat_map(|s| s.keyframes.iter())
            .find(|k| k.name == spec.name);
        let frames = match frames { Some(k) => &k.frames, None => continue };
        let progress = scroll_progress.clamp(0.0, 1.0);
        let interp_vals = interpolate_keyframes(frames, progress);
        for (k, v) in interp_vals { styles.insert(k, v); }
        any_active = true;
    }
    any_active
}

/// Aplikuje runtime CSS animace na StyleMap pri zadanem elapsed time (sekundy).
/// Pro kazdy element s `animation` / `animation-name`:
///   1. Najdi @keyframes by name v stylesheets.
///   2. Vypocti progress dle duration / iter-count / direction / delay / easing.
///   3. Interpoluj keyframes a override do ComputedStyle.
///
/// Vrati true pokud nejaka animace probiha (= caller by mel re-redrawit).
/// L5 step 4 Phase 3: aplikuj interp_keyframes hodnotu na typed ComputedStyle.
/// Match na property name -> parse value -> set typed field. Pri unknown
/// property no-op. Pouziva apply_animations_typed pro per-frame anim values.
pub fn apply_animated_value_to_cs(cs: &mut crate::browser::computed_style::ComputedStyle, prop: &str, value: &str) {
    use crate::browser::computed_style::{Color, Length};
    let v = value.trim();
    match prop {
        "opacity" => {
            if let Ok(n) = v.parse::<f32>() { cs.opacity = n.clamp(0.0, 1.0); }
        }
        "color" => { if let Some(c) = Color::parse(v) { cs.color = c; } }
        "background-color" => { if let Some(c) = Color::parse(v) { cs.background_color = c; } }
        "border-top-color"   => { if let Some(c) = Color::parse(v) { cs.border_top_color = c; } }
        "border-right-color" => { if let Some(c) = Color::parse(v) { cs.border_right_color = c; } }
        "border-bottom-color"=> { if let Some(c) = Color::parse(v) { cs.border_bottom_color = c; } }
        "border-left-color"  => { if let Some(c) = Color::parse(v) { cs.border_left_color = c; } }
        "outline-color" => { if let Some(c) = Color::parse(v) { cs.outline_color = c; } }
        "caret-color" => { if let Some(c) = Color::parse(v) { cs.caret_color = c; } }
        "text-decoration-color" => { if let Some(c) = Color::parse(v) { cs.text_decoration_color = c; } }
        "text-emphasis-color" => { if let Some(c) = Color::parse(v) { cs.text_emphasis_color = c; } }
        // Length-based (resolve later v layout dispatch with real ctx).
        "width"  => { if let Some(l) = Length::parse(v) { cs.width = l; } }
        "height" => { if let Some(l) = Length::parse(v) { cs.height = l; } }
        "min-width"  => { if let Some(l) = Length::parse(v) { cs.min_width = l; } }
        "min-height" => { if let Some(l) = Length::parse(v) { cs.min_height = l; } }
        "max-width"  => { if let Some(l) = Length::parse(v) { cs.max_width = l; } }
        "max-height" => { if let Some(l) = Length::parse(v) { cs.max_height = l; } }
        "top" => { if let Some(l) = Length::parse(v) { cs.top = l; } }
        "right" => { if let Some(l) = Length::parse(v) { cs.right = l; } }
        "bottom" => { if let Some(l) = Length::parse(v) { cs.bottom = l; } }
        "left" => { if let Some(l) = Length::parse(v) { cs.left = l; } }
        "margin-top"    => { if let Some(l) = Length::parse(v) { cs.margin_top = l; } }
        "margin-right"  => { if let Some(l) = Length::parse(v) { cs.margin_right = l; } }
        "margin-bottom" => { if let Some(l) = Length::parse(v) { cs.margin_bottom = l; } }
        "margin-left"   => { if let Some(l) = Length::parse(v) { cs.margin_left = l; } }
        "padding-top"    => { if let Some(l) = Length::parse(v) { cs.padding_top = l; } }
        "padding-right"  => { if let Some(l) = Length::parse(v) { cs.padding_right = l; } }
        "padding-bottom" => { if let Some(l) = Length::parse(v) { cs.padding_bottom = l; } }
        "padding-left"   => { if let Some(l) = Length::parse(v) { cs.padding_left = l; } }
        "font-size" => { if let Some(l) = Length::parse(v) { cs.font_size = l; } }
        "border-top-width"    => { if let Some(l) = Length::parse(v) { cs.border_top_width = l; } }
        "border-right-width"  => { if let Some(l) = Length::parse(v) { cs.border_right_width = l; } }
        "border-bottom-width" => { if let Some(l) = Length::parse(v) { cs.border_bottom_width = l; } }
        "border-left-width"   => { if let Some(l) = Length::parse(v) { cs.border_left_width = l; } }
        "outline-width" => { if let Some(l) = Length::parse(v) { cs.outline_width = l; } }
        "outline-offset" => { if let Some(l) = Length::parse(v) { cs.outline_offset = l; } }
        "row-gap"    => { if let Some(l) = Length::parse(v) { cs.row_gap = l; } }
        "column-gap" => { if let Some(l) = Length::parse(v) { cs.column_gap = l; } }
        "border-top-left-radius"     => { if let Some(l) = Length::parse(v) { cs.border_top_left_radius = l; } }
        "border-top-right-radius"    => { if let Some(l) = Length::parse(v) { cs.border_top_right_radius = l; } }
        "border-bottom-right-radius" => { if let Some(l) = Length::parse(v) { cs.border_bottom_right_radius = l; } }
        "border-bottom-left-radius"  => { if let Some(l) = Length::parse(v) { cs.border_bottom_left_radius = l; } }
        // Raw String fields (cascade-typed).
        "transform" => { cs.transform = v.to_string(); }
        "filter" => { cs.filter = v.to_string(); }
        "backdrop-filter" => { cs.backdrop_filter = v.to_string(); }
        "clip-path" => { cs.clip_path = v.to_string(); }
        "box-shadow" => { cs.box_shadow = v.to_string(); }
        "text-shadow" => { cs.text_shadow = v.to_string(); }
        // f32 fields.
        "flex-grow" => { if let Ok(n) = v.parse::<f32>() { cs.flex_grow = n; } }
        "flex-shrink" => { if let Ok(n) = v.parse::<f32>() { cs.flex_shrink = n; } }
        _ => {} // Unknown anim prop - ignore.
    }
    // Mark as set (animated overlay = is_set).
    let pid = crate::browser::computed_style::PropertyId::parse(prop);
    if pid != crate::browser::computed_style::PropertyId::Unknown {
        cs.mark_set(pid);
    }
}

/// L5 step 4 Phase 3: typed apply_animations - mutates ComputedStyleMap per
/// frame s interpolated keyframe values. Pri animation tick:
/// 1. Spec parse z cs (animation_* typed Vec<>).
/// 2. interpolate_keyframes -> Vec<(prop_name, value_str)>.
/// 3. apply_animated_value_to_cs aplikuje na cs typed field.
pub fn apply_animations_typed(
    cmap: &mut crate::browser::computed_style::ComputedStyleMap,
    stylesheets: &[Stylesheet],
    elapsed_secs: f32,
) -> bool {
    use super::layout::interpolate_keyframes;
    let mut any_active = false;

    for cs in cmap.values_mut() {
        let spec = match AnimationSpec::from_cs(cs) {
            Some(s) => s, None => continue,
        };
        let frames = stylesheets.iter()
            .flat_map(|s| s.keyframes.iter())
            .find(|k| k.name == spec.name);
        let frames = match frames { Some(k) => &k.frames, None => continue };
        let t = elapsed_secs - spec.delay_secs;
        if t < 0.0 {
            if spec.fill_mode == "backwards" || spec.fill_mode == "both" {
                let initial = match spec.direction.as_str() {
                    "reverse" | "alternate-reverse" => 1.0,
                    _ => 0.0,
                };
                for (k, v) in interpolate_keyframes(frames, initial) {
                    apply_animated_value_to_cs(cs, &k, &v);
                }
                any_active = true;
            }
            continue;
        }
        if spec.play_state == "paused" {
            for (k, v) in interpolate_keyframes(frames, 0.0) {
                apply_animated_value_to_cs(cs, &k, &v);
            }
            continue;
        }
        let total_progress = t / spec.duration_secs;
        if total_progress >= spec.iteration_count {
            if spec.fill_mode == "forwards" || spec.fill_mode == "both" {
                let final_progress = match spec.direction.as_str() {
                    "reverse" => 0.0,
                    "alternate" if (spec.iteration_count as i32) % 2 == 0 => 0.0,
                    "alternate-reverse" if (spec.iteration_count as i32) % 2 == 0 => 1.0,
                    _ => 1.0,
                };
                for (k, v) in interpolate_keyframes(frames, final_progress) {
                    apply_animated_value_to_cs(cs, &k, &v);
                }
            }
            continue;
        }
        let iter_idx = total_progress.floor() as i32;
        let mut local = total_progress.fract();
        let reverse = match spec.direction.as_str() {
            "reverse" => true,
            "alternate" => iter_idx % 2 == 1,
            "alternate-reverse" => iter_idx % 2 == 0,
            _ => false,
        };
        if reverse { local = 1.0 - local; }
        let progress = apply_easing(local, &spec.timing_function);
        for (k, v) in interpolate_keyframes(frames, progress) {
            apply_animated_value_to_cs(cs, &k, &v);
        }
        any_active = true;
    }
    any_active
}

pub fn apply_animations(
    style_map: &mut StyleMap,
    stylesheets: &[Stylesheet],
    elapsed_secs: f32,
) -> bool {
    use super::layout::interpolate_keyframes;
    let mut any_active = false;

    for styles in style_map.values_mut() {
        let spec = match AnimationSpec::from_styles(styles) {
            Some(s) => s, None => continue,
        };

        // Najdi keyframes
        let frames = stylesheets.iter()
            .flat_map(|s| s.keyframes.iter())
            .find(|k| k.name == spec.name);
        let frames = match frames { Some(k) => &k.frames, None => continue };

        // Cas po zaciatku animace (bez delay)
        let t = elapsed_secs - spec.delay_secs;

        // Pred zacatkem (delay zatim probiha)
        if t < 0.0 {
            // animation-fill-mode: backwards / both -> aplikuj prvni snimek pred zacatkem
            if spec.fill_mode == "backwards" || spec.fill_mode == "both" {
                let initial = match spec.direction.as_str() {
                    "reverse" | "alternate-reverse" => 1.0,
                    _ => 0.0,
                };
                let interp_vals = interpolate_keyframes(frames, initial);
                for (k, v) in interp_vals { styles.insert(k, v); }
                any_active = true;
            }
            continue;
        }

        // Paused: pouzij fixed progress 0 (nebo posledni - zatim 0 pro jednoduchost)
        if spec.play_state == "paused" {
            // Pouzij prvni snimek
            let interp_vals = interpolate_keyframes(frames, 0.0);
            for (k, v) in interp_vals { styles.insert(k, v); }
            continue;
        }

        // Iter count check - dokonceni
        let total_progress = t / spec.duration_secs;
        if total_progress >= spec.iteration_count {
            // Animace dokoncena
            // animation-fill-mode: forwards / both -> drz posledni snimek
            // jinak (none / backwards) -> nepouzivat keyframes (vrati se na puvodni styl)
            if spec.fill_mode == "forwards" || spec.fill_mode == "both" {
                let final_progress = match spec.direction.as_str() {
                    "reverse" => 0.0,
                    "alternate" if (spec.iteration_count as i32) % 2 == 0 => 0.0,
                    "alternate-reverse" if (spec.iteration_count as i32) % 2 == 0 => 1.0,
                    _ => 1.0,
                };
                let interp_vals = interpolate_keyframes(frames, final_progress);
                for (k, v) in interp_vals { styles.insert(k, v); }
            }
            continue;
        }

        // Aktivni iteration
        let iter_idx = total_progress.floor() as i32;
        let mut local = total_progress.fract(); // 0..1 v ramci aktualni iterace

        // Direction handling
        let reverse = match spec.direction.as_str() {
            "reverse" => true,
            "alternate" => iter_idx % 2 == 1,
            "alternate-reverse" => iter_idx % 2 == 0,
            _ => false,
        };
        if reverse { local = 1.0 - local; }

        // Easing
        let progress = apply_easing(local, &spec.timing_function);

        let interp_vals = interpolate_keyframes(frames, progress);
        for (k, v) in interp_vals { styles.insert(k, v); }
        any_active = true;
    }

    any_active
}
