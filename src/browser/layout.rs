/// Layout engine - tvori box tree z DOM + computed styles.
///
/// Zatim zakladni block layout. Inline/flex/grid pozdeji.
/// Box tree je separator: kazdy DOM uzel ma 0..N boxu.

use std::collections::HashMap;
use std::rc::Rc;
use super::dom::{Node, NodeKind};
use super::cascade::StyleMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Display {
    Block,
    Inline,
    InlineBlock,
    None,
}

impl Display {
    pub fn from_str(s: &str) -> Self {
        match s.trim() {
            "block"        => Display::Block,
            "inline"       => Display::Inline,
            "inline-block" => Display::InlineBlock,
            "none"         => Display::None,
            _ => Display::Block, // default block pro div/p, inline pro span - simplifikace
        }
    }
}

/// Default display per tag (HTML semantika).
pub fn default_display(tag: &str) -> Display {
    match tag {
        "html" | "body" | "div" | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
        | "ul" | "ol" | "li" | "header" | "footer" | "main" | "section" | "article"
        | "nav" | "aside" | "form" | "table" | "tr" | "td" | "th" | "blockquote"
        | "pre" | "hr" | "figure" | "figcaption"
            => Display::Block,
        "span" | "a" | "em" | "strong" | "b" | "i" | "u" | "code" | "small"
        | "br" | "img" | "input" | "label" | "button" | "select" | "textarea"
            => Display::Inline,
        _ => Display::Block,
    }
}

/// Bounding box - position + dimenze.
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone)]
pub struct LayoutBox {
    pub rect: Rect,
    pub display: Display,
    pub bg_color: Option<[u8; 4]>,   // RGBA
    pub text_color: Option<[u8; 4]>,
    pub text: Option<String>,
    pub tag: Option<String>,
    pub children: Vec<LayoutBox>,
    pub padding: f32,
    pub margin: f32,
    pub border_width: f32,
    pub border_color: Option<[u8; 4]>,
    pub font_size: f32,
}

impl LayoutBox {
    pub fn new() -> Self {
        LayoutBox {
            rect: Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
            display: Display::Block,
            bg_color: None,
            text_color: None,
            text: None,
            tag: None,
            children: Vec::new(),
            padding: 0.0,
            margin: 0.0,
            border_width: 0.0,
            border_color: None,
            font_size: 16.0,
        }
    }
}

/// Hlavni layout funkce - z DOM + styles vytvori box tree.
/// Viewport rozmery dany jako parametr.
pub fn layout_tree(
    root: &Rc<Node>,
    style_map: &StyleMap,
    viewport_width: f32,
    viewport_height: f32,
) -> LayoutBox {
    let mut layout_root = build_box(root, style_map);
    layout_root.rect.width = viewport_width;
    layout_root.rect.height = viewport_height;
    layout_block(&mut layout_root);
    layout_root
}

/// Rekurzivne stavi LayoutBox z Node.
fn build_box(node: &Rc<Node>, style_map: &StyleMap) -> LayoutBox {
    let mut bx = LayoutBox::new();

    let styles = super::cascade::get_styles(style_map, node);
    let empty: HashMap<String, String> = HashMap::new();
    let s = styles.unwrap_or(&empty);

    // Display
    if let Some(disp) = s.get("display") {
        bx.display = Display::from_str(disp);
    } else if let Some(tag) = node.tag_name() {
        bx.display = default_display(&tag);
    }

    bx.tag = node.tag_name();

    if matches!(node.kind, NodeKind::Text(_)) {
        bx.display = Display::Inline;
        if let NodeKind::Text(t) = &node.kind {
            let trimmed = t.trim();
            if !trimmed.is_empty() {
                bx.text = Some(trimmed.to_string());
            }
        }
    }

    // Color parsing
    if let Some(c) = s.get("background-color").or(s.get("background")) {
        bx.bg_color = parse_color(c);
    }
    if let Some(c) = s.get("color") {
        bx.text_color = parse_color(c);
    }

    // Padding / margin / border-width
    if let Some(p) = s.get("padding") { bx.padding = parse_length(p); }
    if let Some(m) = s.get("margin")  { bx.margin = parse_length(m); }
    if let Some(b) = s.get("border-width") { bx.border_width = parse_length(b); }
    if let Some(bc) = s.get("border-color") { bx.border_color = parse_color(bc); }
    if let Some(fs) = s.get("font-size") { bx.font_size = parse_length(fs); }

    // Children - skip None display, skip whitespace-only text uzly
    for child in node.children.borrow().iter() {
        // Pre-filter: prazdne text uzly nepokracujeme rekursi
        if let NodeKind::Text(t) = &child.kind {
            if t.trim().is_empty() { continue; }
        }
        // Comment / DocType / CDATA preskocime
        if !matches!(child.kind,
            NodeKind::Element { .. } | NodeKind::Text(_) | NodeKind::Document)
        {
            continue;
        }
        let cb = build_box(child, style_map);
        if cb.display != Display::None {
            // Text bez obsahu - zahodit
            if matches!(child.kind, NodeKind::Text(_)) && cb.text.is_none() {
                continue;
            }
            bx.children.push(cb);
        }
    }
    bx
}

/// Block layout: kazdy block dite je vlastni radek, sirka = parent.
/// Inline deti se sbiraji do "line boxu" a wrappuji.
fn layout_block(bx: &mut LayoutBox) {
    let inner_x = bx.rect.x + bx.padding + bx.margin + bx.border_width;
    let inner_y = bx.rect.y + bx.padding + bx.margin + bx.border_width;
    let inner_w = bx.rect.width - 2.0 * (bx.padding + bx.margin + bx.border_width);

    let mut cursor_y = inner_y;
    // Inline run - sbiraji se inline boxy do line buffer, flush pri block child nebo konci
    let mut inline_buffer: Vec<usize> = Vec::new();

    let mut i = 0;
    while i < bx.children.len() {
        let display = bx.children[i].display;
        match display {
            Display::Block => {
                if !inline_buffer.is_empty() {
                    cursor_y = flush_inline(bx, &inline_buffer, inner_x, cursor_y, inner_w);
                    inline_buffer.clear();
                }
                let child = &mut bx.children[i];
                child.rect.x = inner_x + child.margin;
                child.rect.y = cursor_y + child.margin;
                child.rect.width = inner_w - 2.0 * child.margin;
                if child.rect.height == 0.0 {
                    child.rect.height = if child.text.is_some() {
                        child.font_size * 1.4 + child.padding * 2.0
                    } else {
                        20.0
                    };
                }
                layout_block(child);
                cursor_y += child.rect.height + 2.0 * child.margin;
            }
            Display::Inline | Display::InlineBlock => {
                inline_buffer.push(i);
            }
            Display::None => {}
        }
        i += 1;
    }
    if !inline_buffer.is_empty() {
        cursor_y = flush_inline(bx, &inline_buffer, inner_x, cursor_y, inner_w);
    }

    // Auto-vypocet vysky podle children
    let content_h = cursor_y - inner_y;
    if bx.rect.height < content_h + 2.0 * (bx.padding + bx.border_width) {
        bx.rect.height = content_h + 2.0 * (bx.padding + bx.border_width);
    }
}

/// Flush inline buffer: rozmista inline boxy s wrapem.
/// Vraci new cursor_y po vsech radkach.
fn flush_inline(bx: &mut LayoutBox, indices: &[usize], inner_x: f32, start_y: f32, inner_w: f32) -> f32 {
    let mut cursor_x = inner_x;
    let mut cursor_y = start_y;
    let line_height_default = 19.2; // 16 * 1.2
    let mut line_height = line_height_default;

    for &idx in indices {
        // Zachyceni boxu cele
        let bx_clone = bx.children[idx].clone();
        let font_size = bx_clone.font_size;
        let advance_h = (font_size * 1.4).max(line_height_default);
        line_height = line_height.max(advance_h);

        if let Some(text) = &bx_clone.text {
            // Rozdel na slova, kazde slovo merit a wrappovat
            let words: Vec<&str> = text.split_whitespace().collect();
            for (wi, word) in words.iter().enumerate() {
                let w = measure_text_width(word, font_size);
                let space_w = if wi > 0 { font_size * 0.3 } else { 0.0 };
                if cursor_x + space_w + w > inner_x + inner_w && cursor_x > inner_x {
                    cursor_y += line_height;
                    cursor_x = inner_x;
                }
                let x = cursor_x + space_w;
                // Pridame fragment-style box (jen pro paint - prepiseme child position)
                // V paint pristupu: bx.children[idx] ma jen jednu pozici - ale my mame N slov
                // Reseni: prirad bxu prvni pozici slova; pro presnost by potreboval zvlastni lineBox
                if wi == 0 {
                    bx.children[idx].rect.x = x;
                    bx.children[idx].rect.y = cursor_y;
                    bx.children[idx].rect.width = w;
                    bx.children[idx].rect.height = advance_h;
                } else {
                    // Slovo na novem radku v ramci stejneho elementu - vytvorime virtual fragment
                    // Pro zjednoduseni zatim slijeme do jedne `text` s preformatted layout
                    // (correct approach by mela rozdelit na fragmenty)
                    // Jako workaround: spojeny text na puvodni pozici, wrappuje renderer
                    // (necelo idealni - ale prijatelne)
                }
                cursor_x = x + w;
            }
        } else if !bx_clone.children.is_empty() {
            // Inline element s childen (napr. <span><em>text</em></span>) - flatten
            // Aktualne: jen umisti samotny element jako jeden inline blok
            let estimated_w = (bx_clone.children.iter()
                .filter_map(|c| c.text.as_ref())
                .map(|t| measure_text_width(t, font_size))
                .sum::<f32>())
                .max(font_size);
            if cursor_x + estimated_w > inner_x + inner_w && cursor_x > inner_x {
                cursor_y += line_height;
                cursor_x = inner_x;
            }
            bx.children[idx].rect.x = cursor_x;
            bx.children[idx].rect.y = cursor_y;
            bx.children[idx].rect.width = estimated_w;
            bx.children[idx].rect.height = advance_h;
            // Layout vnoreny obsah
            layout_block(&mut bx.children[idx]);
            cursor_x += estimated_w;
        }
    }
    cursor_y + line_height
}

/// Priblizny vypocet sirky textu (font measuring fallback).
/// Real implementace by mela pouzit fontdue::Metrics, ale to vyzaduje pristup k Font.
/// Pro layout fazi pouzivame heuristiku: priblizna sirka znaku = font_size * 0.55
pub fn measure_text_width(text: &str, font_size: f32) -> f32 {
    let avg_char_w = font_size * 0.55;
    text.chars().count() as f32 * avg_char_w
}

/// Parse barvu z CSS string. Podpora: #RGB, #RRGGBB, rgb(R,G,B), rgba(R,G,B,A), nazvy.
pub fn parse_color(s: &str) -> Option<[u8; 4]> {
    let s = s.trim().to_lowercase();
    // Hex #RGB nebo #RRGGBB
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 3 {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            return Some([r, g, b, 255]);
        }
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some([r, g, b, 255]);
        }
        if hex.len() == 8 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            return Some([r, g, b, a]);
        }
    }
    // rgb(r, g, b) / rgba(r, g, b, a)
    if let Some(inner) = s.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        if parts.len() == 3 {
            let r = parts[0].parse::<u8>().ok()?;
            let g = parts[1].parse::<u8>().ok()?;
            let b = parts[2].parse::<u8>().ok()?;
            return Some([r, g, b, 255]);
        }
    }
    if let Some(inner) = s.strip_prefix("rgba(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        if parts.len() == 4 {
            let r = parts[0].parse::<u8>().ok()?;
            let g = parts[1].parse::<u8>().ok()?;
            let b = parts[2].parse::<u8>().ok()?;
            let a = (parts[3].parse::<f32>().ok()? * 255.0) as u8;
            return Some([r, g, b, a]);
        }
    }
    // Named colors (subset)
    match s.as_str() {
        "black"   => Some([0, 0, 0, 255]),
        "white"   => Some([255, 255, 255, 255]),
        "red"     => Some([255, 0, 0, 255]),
        "green"   => Some([0, 128, 0, 255]),
        "blue"    => Some([0, 0, 255, 255]),
        "yellow"  => Some([255, 255, 0, 255]),
        "cyan"    => Some([0, 255, 255, 255]),
        "magenta" => Some([255, 0, 255, 255]),
        "gray" | "grey" => Some([128, 128, 128, 255]),
        "lightgray" | "lightgrey" => Some([211, 211, 211, 255]),
        "darkgray"  | "darkgrey"  => Some([169, 169, 169, 255]),
        "transparent" => Some([0, 0, 0, 0]),
        _ => None,
    }
}

/// Parse delku v px nebo em. Vraci pixely.
/// Delsi suffixy musi byt kontrolovany drive (rem pred em).
pub fn parse_length(s: &str) -> f32 {
    let s = s.trim();
    if let Some(num) = s.strip_suffix("rem") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 16.0;
    }
    if let Some(num) = s.strip_suffix("px") {
        return num.trim().parse().unwrap_or(0.0);
    }
    if let Some(num) = s.strip_suffix("em") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 16.0;
    }
    if let Some(num) = s.strip_suffix('%') {
        let _: f32 = num.trim().parse().unwrap_or(0.0);
        return 0.0;
    }
    s.parse().unwrap_or(0.0)
}
