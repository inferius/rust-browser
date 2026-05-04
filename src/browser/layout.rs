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
    Flex,
    Grid,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Position {
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransformOp {
    Translate(f32, f32),
    Rotate(f32),  // radians
    Scale(f32, f32),
    None,
}

impl Display {
    pub fn from_str(s: &str) -> Self {
        match s.trim() {
            "block"        => Display::Block,
            "inline"       => Display::Inline,
            "inline-block" => Display::InlineBlock,
            "flex"         => Display::Flex,
            "grid"         => Display::Grid,
            "none"         => Display::None,
            _ => Display::Block,
        }
    }
}

/// Aplikuje default styles per tag (browser user-agent stylesheet).
fn apply_default_tag_styles(bx: &mut LayoutBox, tag: &str) {
    match tag {
        "h1" => { bx.font_size = 32.0; bx.bold = true; bx.margin = 8.0; }
        "h2" => { bx.font_size = 24.0; bx.bold = true; bx.margin = 8.0; }
        "h3" => { bx.font_size = 20.0; bx.bold = true; bx.margin = 6.0; }
        "h4" => { bx.font_size = 16.0; bx.bold = true; bx.margin = 6.0; }
        "h5" => { bx.font_size = 14.0; bx.bold = true; bx.margin = 4.0; }
        "h6" => { bx.font_size = 12.0; bx.bold = true; bx.margin = 4.0; }
        "p" => { bx.margin = 8.0; }
        "b" | "strong" => { bx.bold = true; }
        "ul" | "ol" => { bx.padding = 16.0; bx.margin = 8.0; }
        "li" => { bx.margin = 2.0; }
        "blockquote" => { bx.margin = 16.0; bx.padding = 8.0; }
        "pre" | "code" => { /* monospace by-implication, zatim default */ }
        "hr" => { bx.border_width = 1.0; bx.border_color = Some([200, 200, 200, 255]); }
        "a" => { /* color modra typicky pres CSS */ }
        _ => {}
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
    pub text_align: TextAlign,
    pub bold: bool,
    pub border_radius: f32,
    pub line_height: f32,
    pub position: Position,
    /// Top/right/bottom/left offsety pro positioned elements (None = auto).
    pub offset_top: Option<f32>,
    pub offset_right: Option<f32>,
    pub offset_bottom: Option<f32>,
    pub offset_left: Option<f32>,
    /// Opacity 0..1
    pub opacity: f32,
    /// Underline / strikethrough flagy
    pub text_underline: bool,
    pub text_strikethrough: bool,
    /// Linear gradient pozadi: (angle_deg, Vec<(offset, color)>)
    pub bg_gradient: Option<(f32, Vec<(f32, [u8; 4])>)>,
    /// Box shadow: (offset_x, offset_y, blur, spread, color)
    pub box_shadow: Option<(f32, f32, f32, f32, [u8; 4])>,
    /// Transform: simple translate/rotate/scale
    pub transform: Option<TransformOp>,
    /// Overflow: hidden/scroll/visible/auto
    pub overflow_hidden: bool,
    /// White-space: nowrap zachazi text jako jeden radek
    pub white_space_nowrap: bool,
    /// Cursor (jen string - real impl pres OS cursor)
    pub cursor: Option<String>,
    /// Image src URL (z img tagu).
    pub image_src: Option<String>,
    /// Reference na puvodni DOM node (pro hit test -> events).
    pub node: Option<Rc<Node>>,
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
            text_align: TextAlign::Left,
            bold: false,
            border_radius: 0.0,
            line_height: 1.4,
            position: Position::Static,
            offset_top: None,
            offset_right: None,
            offset_bottom: None,
            offset_left: None,
            opacity: 1.0,
            text_underline: false,
            text_strikethrough: false,
            overflow_hidden: false,
            white_space_nowrap: false,
            cursor: None,
            bg_gradient: None,
            box_shadow: None,
            transform: None,
            image_src: None,
            node: None,
        }
    }

    /// Hit test: vrati nejdetailnejsi (deepest) box obsahujici (x, y).
    pub fn hit_test(&self, x: f32, y: f32) -> Option<&LayoutBox> {
        if x < self.rect.x || x > self.rect.x + self.rect.width
            || y < self.rect.y || y > self.rect.y + self.rect.height
        {
            return None;
        }
        // Zkus deti nejdriv (deepest first)
        for child in &self.children {
            if let Some(hit) = child.hit_test(x, y) {
                return Some(hit);
            }
        }
        Some(self)
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
    layout_dispatch(&mut layout_root);
    layout_root
}

/// Vybira layout algoritmus podle display.
fn layout_dispatch(bx: &mut LayoutBox) {
    match bx.display {
        Display::Flex | Display::Grid => layout_flex(bx),
        _ => layout_block(bx),
    }
}

/// Rekurzivne stavi LayoutBox z Node.
fn build_box(node: &Rc<Node>, style_map: &StyleMap) -> LayoutBox {
    let mut bx = LayoutBox::new();
    bx.node = Some(Rc::clone(node));

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

    // Apply browser default styles per tag (user-agent stylesheet)
    if let Some(tag) = bx.tag.clone() {
        apply_default_tag_styles(&mut bx, &tag);
    }

    // Img tag: precti src + width/height
    if bx.tag.as_deref() == Some("img") {
        bx.image_src = node.attr("src");
        if let Some(w) = node.attr("width").and_then(|w| w.parse::<f32>().ok()) {
            bx.rect.width = w;
        }
        if let Some(h) = node.attr("height").and_then(|h| h.parse::<f32>().ok()) {
            bx.rect.height = h;
        }
        if bx.rect.height == 0.0 { bx.rect.height = 100.0; }
        if bx.rect.width == 0.0 { bx.rect.width = 100.0; }
    }

    if matches!(node.kind, NodeKind::Text(_)) {
        bx.display = Display::Inline;
        if let NodeKind::Text(t) = &node.kind {
            let trimmed = t.trim();
            if !trimmed.is_empty() {
                bx.text = Some(trimmed.to_string());
            }
        }
    }

    // Color parsing - if linear-gradient, parse jako gradient, jinak solid color
    let bg_value = s.get("background-color").or(s.get("background"));
    if let Some(c) = bg_value {
        if c.contains("linear-gradient(") {
            bx.bg_gradient = parse_linear_gradient(c);
        } else {
            bx.bg_color = parse_color(c);
        }
    }
    // Box shadow
    if let Some(sh) = s.get("box-shadow") {
        bx.box_shadow = parse_box_shadow(sh);
    }
    // Transform
    if let Some(tr) = s.get("transform") {
        bx.transform = parse_transform(tr);
    }
    if let Some(c) = s.get("color") {
        bx.text_color = parse_color(c);
    }

    // Padding / margin / border-width - prefer expanded shorthand
    let padding_v = s.get("padding-top").or(s.get("padding"));
    if let Some(p) = padding_v { bx.padding = parse_length(p); }
    let margin_v = s.get("margin-top").or(s.get("margin"));
    if let Some(m) = margin_v { bx.margin = parse_length(m); }
    if let Some(b) = s.get("border-width") { bx.border_width = parse_length(b); }
    if let Some(bc) = s.get("border-color") { bx.border_color = parse_color(bc); }
    if let Some(fs) = s.get("font-size") { bx.font_size = parse_length(fs); }
    // Text align
    if let Some(ta) = s.get("text-align") {
        bx.text_align = match ta.trim() {
            "center"  => TextAlign::Center,
            "right"   => TextAlign::Right,
            "justify" => TextAlign::Justify,
            _ => TextAlign::Left,
        };
    }
    // Font weight - bold per HTML semantika
    if let Some(fw) = s.get("font-weight") {
        bx.bold = fw.contains("bold") || fw.parse::<u32>().map(|n| n >= 600).unwrap_or(false);
    }
    // Border radius
    if let Some(br) = s.get("border-radius") {
        bx.border_radius = parse_length(br);
    }
    // Line-height: cislo (multiplier) nebo length (px)
    if let Some(lh) = s.get("line-height") {
        let trimmed = lh.trim();
        if let Ok(num) = trimmed.parse::<f32>() {
            bx.line_height = num;
        } else if trimmed.ends_with("px") || trimmed.ends_with("em") || trimmed.ends_with("rem") {
            // V px - prevest na multiplier
            let px = parse_length(trimmed);
            if bx.font_size > 0.0 {
                bx.line_height = px / bx.font_size;
            }
        }
    }
    // Position
    if let Some(pos) = s.get("position") {
        bx.position = match pos.trim() {
            "relative" => Position::Relative,
            "absolute" => Position::Absolute,
            "fixed"    => Position::Fixed,
            "sticky"   => Position::Sticky,
            _ => Position::Static,
        };
    }
    // Top/right/bottom/left offsety
    if let Some(v) = s.get("top")    { bx.offset_top    = Some(parse_length(v)); }
    if let Some(v) = s.get("right")  { bx.offset_right  = Some(parse_length(v)); }
    if let Some(v) = s.get("bottom") { bx.offset_bottom = Some(parse_length(v)); }
    if let Some(v) = s.get("left")   { bx.offset_left   = Some(parse_length(v)); }
    // Opacity
    if let Some(o) = s.get("opacity") {
        bx.opacity = o.trim().parse::<f32>().unwrap_or(1.0).clamp(0.0, 1.0);
    }
    // Text-decoration
    if let Some(td) = s.get("text-decoration") {
        let t = td.to_lowercase();
        if t.contains("underline")    { bx.text_underline = true; }
        if t.contains("line-through") { bx.text_strikethrough = true; }
    }
    // Overflow
    if let Some(ov) = s.get("overflow") {
        bx.overflow_hidden = matches!(ov.trim(), "hidden" | "clip");
    }
    // White-space
    if let Some(ws) = s.get("white-space") {
        bx.white_space_nowrap = ws.trim() == "nowrap";
    }
    // Cursor
    if let Some(cur) = s.get("cursor") {
        bx.cursor = Some(cur.trim().to_string());
    }
    // Default underline pro <a> tag (pokud nebyla explicitne odebrana)
    if let Some(tag) = bx.tag.clone() {
        if tag == "a" && s.get("text-decoration").is_none() {
            bx.text_underline = true;
        }
    }

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

/// Flex layout pres taffy crate.
fn layout_flex(bx: &mut LayoutBox) {
    use taffy::prelude::*;

    let inner_x = bx.rect.x + bx.padding + bx.margin + bx.border_width;
    let inner_y = bx.rect.y + bx.padding + bx.margin + bx.border_width;
    let inner_w = bx.rect.width - 2.0 * (bx.padding + bx.margin + bx.border_width);

    let mut taffy: TaffyTree<()> = TaffyTree::new();
    let mut child_nodes: Vec<NodeId> = Vec::new();

    // Vytvor child nodes
    for ch in bx.children.iter() {
        let est_w = if let Some(t) = &ch.text {
            measure_text_width(t, ch.font_size)
        } else { 100.0 };
        let est_h = if ch.text.is_some() { ch.font_size * 1.4 } else { 50.0 };
        let style = Style {
            size: Size { width: length(est_w), height: length(est_h) },
            margin: taffy::geometry::Rect::length(ch.margin),
            padding: taffy::geometry::Rect::length(ch.padding),
            ..Default::default()
        };
        if let Ok(node) = taffy.new_leaf(style) {
            child_nodes.push(node);
        }
    }

    let parent_style = Style {
        display: taffy::Display::Flex,
        size: Size { width: length(inner_w), height: auto() },
        flex_wrap: FlexWrap::Wrap,
        gap: Size { width: length(8.0), height: length(8.0) },
        ..Default::default()
    };

    let root = match taffy.new_with_children(parent_style, &child_nodes) {
        Ok(r) => r,
        Err(_) => return,
    };

    let _ = taffy.compute_layout(root, Size {
        width: AvailableSpace::Definite(inner_w),
        height: AvailableSpace::MinContent,
    });

    // Aplikuj layout zpet do bx.children
    for (i, node) in child_nodes.iter().enumerate() {
        if let Ok(layout) = taffy.layout(*node) {
            let child = &mut bx.children[i];
            child.rect.x = inner_x + layout.location.x;
            child.rect.y = inner_y + layout.location.y;
            child.rect.width = layout.size.width;
            child.rect.height = layout.size.height;
            // Recursive layout uvnitr child boxu
            layout_block(child);
        }
    }

    // Update parent height na zaklade celkove vysky deti
    if let Ok(layout) = taffy.layout(root) {
        let needed_h = layout.size.height + 2.0 * (bx.padding + bx.border_width);
        if bx.rect.height < needed_h {
            bx.rect.height = needed_h;
        }
    }
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
            Display::Block | Display::Flex | Display::Grid => {
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
                        child.font_size * child.line_height + child.padding * 2.0
                    } else {
                        20.0
                    };
                }
                layout_dispatch(child);

                // Apply position offsety
                let is_in_flow = matches!(child.position, Position::Static | Position::Relative);
                match child.position {
                    Position::Relative => {
                        if let Some(t) = child.offset_top  { child.rect.y += t; }
                        if let Some(l) = child.offset_left { child.rect.x += l; }
                    }
                    Position::Absolute | Position::Fixed => {
                        // Pro Absolute/Fixed: prepocitej polohu z parent
                        if let Some(t) = child.offset_top {
                            child.rect.y = inner_y + t;
                        }
                        if let Some(l) = child.offset_left {
                            child.rect.x = inner_x + l;
                        }
                        if let Some(r) = child.offset_right {
                            child.rect.x = inner_x + inner_w - r - child.rect.width;
                        }
                    }
                    _ => {}
                }
                if is_in_flow {
                    cursor_y += child.rect.height + 2.0 * child.margin;
                }
                // Absolute/fixed neposunuji cursor_y - jsou out of flow
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

/// Real vypocet sirky textu pres globalni shared font.
/// Fallback na heuristiku kdyz font neni dostupny.
pub fn measure_text_width(text: &str, font_size: f32) -> f32 {
    use std::sync::OnceLock;
    static FONT: OnceLock<Option<fontdue::Font>> = OnceLock::new();

    let font_opt = FONT.get_or_init(|| {
        super::render::try_load_default_font()
            .and_then(|data| fontdue::Font::from_bytes(data, fontdue::FontSettings::default()).ok())
    });

    match font_opt {
        Some(font) => {
            text.chars().map(|ch| {
                font.metrics(ch, font_size).advance_width
            }).sum()
        }
        None => {
            // Fallback heuristika
            let avg_char_w = font_size * 0.55;
            text.chars().count() as f32 * avg_char_w
        }
    }
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

/// Parse linear-gradient(angle, color, color, ...) -> (angle_deg, stops).
pub fn parse_linear_gradient(s: &str) -> Option<(f32, Vec<(f32, [u8; 4])>)> {
    let s = s.trim();
    let inner = s.strip_prefix("linear-gradient(")?.strip_suffix(')')?;
    // Split na comma respektujici parentheses
    let parts = split_top_level_commas(inner);
    if parts.len() < 2 { return None; }

    let mut angle = 180.0; // default top->bottom
    let mut start_idx = 0;
    let first = parts[0].trim();
    if let Some(deg_str) = first.strip_suffix("deg") {
        if let Ok(a) = deg_str.trim().parse::<f32>() {
            angle = a;
            start_idx = 1;
        }
    } else if first.starts_with("to ") {
        angle = match first.trim_start_matches("to ").trim() {
            "top"    => 0.0,
            "right"  => 90.0,
            "bottom" => 180.0,
            "left"   => 270.0,
            "top right" | "right top" => 45.0,
            "bottom right" | "right bottom" => 135.0,
            "bottom left" | "left bottom" => 225.0,
            "top left" | "left top" => 315.0,
            _ => 180.0,
        };
        start_idx = 1;
    }

    let mut stops: Vec<(f32, [u8; 4])> = Vec::new();
    let n = parts.len() - start_idx;
    for (i, p) in parts[start_idx..].iter().enumerate() {
        // "red 50%" nebo jen "red"
        let trimmed = p.trim();
        let (color_str, offset) = if let Some(percent_idx) = trimmed.rfind('%') {
            // Najdi mezeru pred %
            let space_idx = trimmed[..percent_idx].rfind(' ').unwrap_or(0);
            let pct: f32 = trimmed[space_idx..percent_idx].trim().parse().unwrap_or(0.0);
            (trimmed[..space_idx].trim().to_string(), pct / 100.0)
        } else {
            let default_offset = if n <= 1 { 0.0 } else { i as f32 / (n - 1) as f32 };
            (trimmed.to_string(), default_offset)
        };
        if let Some(c) = parse_color(&color_str) {
            stops.push((offset, c));
        }
    }
    if stops.is_empty() { return None; }
    Some((angle, stops))
}

/// Parse box-shadow: "offset_x offset_y blur spread color".
pub fn parse_box_shadow(s: &str) -> Option<(f32, f32, f32, f32, [u8; 4])> {
    let s = s.trim();
    if s == "none" { return None; }
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 3 { return None; }
    let ox = parse_length(parts[0]);
    let oy = parse_length(parts[1]);
    let mut blur = 0.0f32;
    let mut spread = 0.0f32;
    let mut color = [0u8, 0, 0, 128];
    let mut i = 2;
    if i < parts.len() && (parts[i].ends_with("px") || parts[i].ends_with("em") || parts[i].chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)) {
        blur = parse_length(parts[i]);
        i += 1;
    }
    if i < parts.len() && (parts[i].ends_with("px") || parts[i].ends_with("em") || parts[i].chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)) {
        spread = parse_length(parts[i]);
        i += 1;
    }
    if i < parts.len() {
        // Zbyle parts spojeny - barva (mohla obsahovat "rgb(...)")
        let rest: String = parts[i..].join(" ");
        if let Some(c) = parse_color(&rest) {
            color = c;
        }
    }
    Some((ox, oy, blur, spread, color))
}

/// Parse transform: "translate(10px, 20px)" / "rotate(45deg)" / "scale(1.5)".
pub fn parse_transform(s: &str) -> Option<TransformOp> {
    let s = s.trim();
    if s == "none" { return Some(TransformOp::None); }
    if let Some(inner) = s.strip_prefix("translate(").and_then(|x| x.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        let tx = parts.first().map(|p| parse_length(p)).unwrap_or(0.0);
        let ty = parts.get(1).map(|p| parse_length(p)).unwrap_or(0.0);
        return Some(TransformOp::Translate(tx, ty));
    }
    if let Some(inner) = s.strip_prefix("translateX(").and_then(|x| x.strip_suffix(')')) {
        return Some(TransformOp::Translate(parse_length(inner), 0.0));
    }
    if let Some(inner) = s.strip_prefix("translateY(").and_then(|x| x.strip_suffix(')')) {
        return Some(TransformOp::Translate(0.0, parse_length(inner)));
    }
    if let Some(inner) = s.strip_prefix("rotate(").and_then(|x| x.strip_suffix(')')) {
        let trimmed = inner.trim();
        let deg: f32 = if let Some(d) = trimmed.strip_suffix("deg") {
            d.trim().parse().unwrap_or(0.0)
        } else if let Some(r) = trimmed.strip_suffix("rad") {
            return Some(TransformOp::Rotate(r.trim().parse().unwrap_or(0.0)));
        } else { 0.0 };
        return Some(TransformOp::Rotate(deg.to_radians()));
    }
    if let Some(inner) = s.strip_prefix("scale(").and_then(|x| x.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        let sx = parts.first().and_then(|p| p.parse::<f32>().ok()).unwrap_or(1.0);
        let sy = parts.get(1).and_then(|p| p.parse::<f32>().ok()).unwrap_or(sx);
        return Some(TransformOp::Scale(sx, sy));
    }
    None
}

fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < s.len() {
        parts.push(&s[start..]);
    }
    parts
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
