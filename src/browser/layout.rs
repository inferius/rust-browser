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
pub enum TextTransform {
    None,
    Uppercase,
    Lowercase,
    Capitalize,
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
    /// 3D - z osa nepouzite pri 2D rendering (zkracene na xy).
    Translate3D { x: f32, y: f32, z: f32 },
    Rotate3D { x: f32, y: f32, z: f32, angle_rad: f32 },
    Scale3D { x: f32, y: f32, z: f32 },
    /// 4x4 matice serializovana row-major (16 floats).
    Matrix3D([f32; 16]),
    /// Perspective(<length>) - hloubka pohledu.
    Perspective(f32),
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
        "canvas" => {
            // CSS default: 300x150 px (HTML spec)
            if bx.rect.width == 0.0 { bx.rect.width = 300.0; }
            if bx.rect.height == 0.0 { bx.rect.height = 150.0; }
            // Default bg cerny aby canvas byl viditelny
            if bx.bg_color.is_none() { bx.bg_color = Some([0, 0, 0, 255]); }
        }
        "svg" => {
            // SVG default 300x150 (jako canvas) pokud nedano viewBox/width/height
            if bx.rect.width == 0.0 { bx.rect.width = 300.0; }
            if bx.rect.height == 0.0 { bx.rect.height = 150.0; }
        }
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
        | "canvas" | "svg"
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
    /// Gradient pozadi (linear/radial/conic) + barevne stops.
    pub bg_gradient: Option<BgGradient>,
    /// CSS Filter Effects - chain of color matrix operations + opacity + drop-shadow.
    pub filter: Vec<FilterOp>,
    /// Background layers (Backgrounds L3) - jen prvni layer pouzity zatim,
    /// vice layeru emitted bottom-to-top kdyz pridana plne podpora.
    pub backgrounds: Vec<BgLayer>,
    /// CSS clip-path: inset(...) / circle(...) / ellipse(...) / polygon(...).
    pub clip_path: Option<ClipPath>,
    /// text-shadow: (offset_x, offset_y, blur, color).
    pub text_shadow: Option<(f32, f32, f32, [u8; 4])>,
    /// text-transform: none / uppercase / lowercase / capitalize
    pub text_transform: TextTransform,
    /// font-family - prvni nazev z comma-separated list (nejvyssi prio).
    pub font_family: String,
    /// letter-spacing pridava extra mezeru mezi znaky (px).
    pub letter_spacing: f32,
    /// word-spacing pridava extra mezeru mezi slovy (px).
    pub word_spacing: f32,
    /// aspect-ratio: width / height. None = auto.
    pub aspect_ratio: Option<f32>,
    /// color-scheme: "light" | "dark" | "light dark" | "normal" - preference.
    pub color_scheme: String,
    /// accent-color: vlastni barva accent (form controls).
    pub accent_color: Option<[u8; 4]>,
    /// CSS Containment - bitfield: layout / paint / size / style.
    /// 1 = layout, 2 = paint, 4 = size, 8 = style.
    pub contain: u8,
    /// scroll-behavior: "auto" (default) | "smooth"
    pub scroll_behavior: String,
    /// scrollbar-width: "auto" | "thin" | "none"
    pub scrollbar_width: String,
    /// scrollbar-color: (thumb_color, track_color)
    pub scrollbar_color: Option<([u8; 4], [u8; 4])>,
    /// overscroll-behavior: "auto" | "contain" | "none"
    pub overscroll_behavior: String,
    /// scroll-snap-type: "none" | "x mandatory" | "y proximity" / ...
    pub scroll_snap_type: String,
    /// scroll-snap-align: "none" | "start" | "end" | "center"
    pub scroll_snap_align: String,
    /// scroll-padding (top right bottom left v px)
    pub scroll_padding: [f32; 4],
    /// scroll-margin
    pub scroll_margin: [f32; 4],
    /// Box shadow: (offset_x, offset_y, blur, spread, color)
    /// (offset_x, offset_y, blur, spread, color, inset)
    pub box_shadow: Option<(f32, f32, f32, f32, [u8; 4], bool)>,
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
            filter: Vec::new(),
            backgrounds: Vec::new(),
            clip_path: None,
            text_shadow: None,
            text_transform: TextTransform::None,
            font_family: String::new(),
            letter_spacing: 0.0,
            word_spacing: 0.0,
            aspect_ratio: None,
            color_scheme: String::new(),
            accent_color: None,
            contain: 0,
            scroll_behavior: String::new(),
            scrollbar_width: String::new(),
            scrollbar_color: None,
            overscroll_behavior: String::new(),
            scroll_snap_type: String::new(),
            scroll_snap_align: String::new(),
            scroll_padding: [0.0; 4],
            scroll_margin: [0.0; 4],
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
    let empty_pseudo = super::cascade::PseudoStyleMap::new();
    layout_tree_with_pseudo(root, style_map, &empty_pseudo, viewport_width, viewport_height)
}

/// Layout s pseudo-element style map - vyrobi virtualni LayoutBox pro
/// ::before / ::after children.
pub fn layout_tree_with_pseudo(
    root: &Rc<Node>,
    style_map: &StyleMap,
    pseudo_map: &super::cascade::PseudoStyleMap,
    viewport_width: f32,
    viewport_height: f32,
) -> LayoutBox {
    let mut layout_root = build_box_with_pseudo(root, style_map, pseudo_map);
    layout_root.rect.width = viewport_width;
    layout_root.rect.height = viewport_height;
    layout_dispatch(&mut layout_root);
    layout_root
}

/// Aplikuje @keyframes animation pri zadanem time.
/// Pro element s `animation-name: foo` najde keyframes a interpoluje hodnoty.
/// Funkce ne implementovana plne - zatim helper pro budoucnost.
pub fn interpolate_keyframes(
    frames: &[(f32, Vec<super::css_parser::Declaration>)],
    progress: f32,
) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    if frames.is_empty() { return out; }
    if progress <= frames[0].0 {
        for d in &frames[0].1 { out.insert(d.property.clone(), d.value.clone()); }
        return out;
    }
    if progress >= frames.last().unwrap().0 {
        for d in &frames.last().unwrap().1 { out.insert(d.property.clone(), d.value.clone()); }
        return out;
    }
    // Najdi mezi-rame
    for win in frames.windows(2) {
        let (p0, decls0) = (&win[0].0, &win[0].1);
        let (p1, decls1) = (&win[1].0, &win[1].1);
        if progress >= *p0 && progress <= *p1 {
            let t = (progress - p0) / (p1 - p0);
            // Interpoluj kazdou prop pokud je v obou + cislo
            for d0 in decls0 {
                let d1 = decls1.iter().find(|d| d.property == d0.property);
                if let Some(d1) = d1 {
                    let v0 = parse_length(&d0.value);
                    let v1 = parse_length(&d1.value);
                    if v0 != 0.0 || v1 != 0.0 {
                        let v = v0 + (v1 - v0) * t;
                        out.insert(d0.property.clone(), format!("{v}px"));
                    } else {
                        // Non-numeric: vrat starsi (no real interpolace)
                        out.insert(d0.property.clone(),
                            if t < 0.5 { d0.value.clone() } else { d1.value.clone() });
                    }
                } else {
                    out.insert(d0.property.clone(), d0.value.clone());
                }
            }
            break;
        }
    }
    out
}

/// Vybira layout algoritmus podle display.
fn layout_dispatch(bx: &mut LayoutBox) {
    match bx.display {
        Display::Flex | Display::Grid => layout_flex(bx),
        _ => layout_block(bx),
    }
}

/// Wrap pres build_box_with_pseudo s prazdnou pseudo mapou.
fn build_box(node: &Rc<Node>, style_map: &StyleMap) -> LayoutBox {
    let empty = super::cascade::PseudoStyleMap::new();
    build_box_with_pseudo(node, style_map, &empty)
}

/// Rekurzivne stavi LayoutBox z Node + virtualni boxy pro ::before / ::after.
fn build_box_with_pseudo(
    node: &Rc<Node>,
    style_map: &StyleMap,
    pseudo_map: &super::cascade::PseudoStyleMap,
) -> LayoutBox {
    build_box_inner(node, style_map, pseudo_map)
}

fn build_box_inner(node: &Rc<Node>, style_map: &StyleMap, pseudo_map: &super::cascade::PseudoStyleMap) -> LayoutBox {
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

    // Canvas tag: precti width/height attributes
    if bx.tag.as_deref() == Some("canvas") {
        if let Some(w) = node.attr("width").and_then(|w| w.parse::<f32>().ok()) {
            bx.rect.width = w;
        }
        if let Some(h) = node.attr("height").and_then(|h| h.parse::<f32>().ok()) {
            bx.rect.height = h;
        }
    }
    // SVG tag: viewport z width/height
    if bx.tag.as_deref() == Some("svg") {
        if let Some(w) = node.attr("width").and_then(|w| w.parse::<f32>().ok()) {
            bx.rect.width = w;
        }
        if let Some(h) = node.attr("height").and_then(|h| h.parse::<f32>().ok()) {
            bx.rect.height = h;
        }
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
        if c.contains("linear-gradient(") || c.contains("radial-gradient(") || c.contains("conic-gradient(") {
            bx.bg_gradient = parse_any_gradient(c);
        } else {
            bx.bg_color = parse_color(c);
        }
    }
    // Backgrounds L3 - multiple layers (oddelene carkou), pole ulozene shora-dolu
    {
        // background-image: image1, image2, image3
        // background-position: pos1, pos2, pos3
        // background-repeat:   r1, r2, r3
        // ... atd.
        // Vyrobi N layeru kde N = max poctu z prop.
        let images: Vec<String> = s.get("background-image")
            .map(|v| split_top_level_commas_string(v))
            .unwrap_or_default();
        let positions: Vec<String> = s.get("background-position")
            .map(|v| split_top_level_commas_string(v))
            .unwrap_or_default();
        let sizes: Vec<String> = s.get("background-size")
            .map(|v| split_top_level_commas_string(v))
            .unwrap_or_default();
        let repeats: Vec<String> = s.get("background-repeat")
            .map(|v| split_top_level_commas_string(v))
            .unwrap_or_default();
        let clips: Vec<String> = s.get("background-clip")
            .map(|v| split_top_level_commas_string(v))
            .unwrap_or_default();
        let origins: Vec<String> = s.get("background-origin")
            .map(|v| split_top_level_commas_string(v))
            .unwrap_or_default();
        let attachments: Vec<String> = s.get("background-attachment")
            .map(|v| split_top_level_commas_string(v))
            .unwrap_or_default();

        let count = [images.len(), positions.len(), sizes.len(), repeats.len()]
            .iter().max().copied().unwrap_or(0).max(1);

        for i in 0..count {
            let mut layer = BgLayer::default();
            // Color jen na posledni layer (CSS spec)
            if i == count - 1 {
                if let Some(c) = s.get("background-color") { layer.color = parse_color(c); }
            }
            if let Some(img) = images.get(i) {
                let img = img.trim();
                if img.contains("linear-gradient(") || img.contains("radial-gradient(") || img.contains("conic-gradient(") {
                    layer.gradient = parse_any_gradient(img);
                } else if let Some(url_stripped) = img.strip_prefix("url(").and_then(|s| s.strip_suffix(")")) {
                    let cleaned = url_stripped.trim().trim_matches('"').trim_matches('\'');
                    layer.image_src = Some(cleaned.to_string());
                } else if img != "none" {
                    layer.image_src = Some(img.to_string());
                }
            }
            if let Some(p) = positions.get(i) { layer.position = parse_bg_position(p); }
            if let Some(sz) = sizes.get(i) { layer.size = parse_bg_size(sz); }
            if let Some(r) = repeats.get(i) { layer.repeat = parse_bg_repeat(r); }
            if let Some(c) = clips.get(i) { layer.clip = parse_bg_box(c); }
            if let Some(o) = origins.get(i) { layer.origin = parse_bg_box(o); }
            if let Some(a) = attachments.get(i) { layer.attachment = parse_bg_attachment(a); }

            if layer.color.is_some() || layer.image_src.is_some() || layer.gradient.is_some() {
                bx.backgrounds.push(layer);
            }
        }
    }
    // Box shadow
    if let Some(sh) = s.get("box-shadow") {
        bx.box_shadow = parse_box_shadow(sh);
    }
    // Filter chain
    if let Some(f) = s.get("filter") {
        bx.filter = parse_filter_chain(f);
    }
    // clip-path
    if let Some(cp) = s.get("clip-path") {
        bx.clip_path = parse_clip_path(cp);
    }
    // text-shadow: parsuje "offset_x offset_y blur color"
    if let Some(ts) = s.get("text-shadow") {
        bx.text_shadow = parse_text_shadow(ts);
    }
    // font-family - vez prvni z comma-separated list (CSS spec: try in order)
    if let Some(ff) = s.get("font-family") {
        let first = ff.split(',').next().unwrap_or("").trim()
            .trim_matches('"').trim_matches('\'');
        bx.font_family = first.to_string();
    }
    // text-transform
    if let Some(tt) = s.get("text-transform") {
        bx.text_transform = match tt.trim() {
            "uppercase"  => TextTransform::Uppercase,
            "lowercase"  => TextTransform::Lowercase,
            "capitalize" => TextTransform::Capitalize,
            _            => TextTransform::None,
        };
    }
    // letter-spacing / word-spacing
    if let Some(ls) = s.get("letter-spacing") {
        if ls.trim() != "normal" { bx.letter_spacing = parse_length(ls); }
    }
    if let Some(ws) = s.get("word-spacing") {
        if ws.trim() != "normal" { bx.word_spacing = parse_length(ws); }
    }
    // color-scheme
    if let Some(cs) = s.get("color-scheme") {
        bx.color_scheme = cs.trim().to_string();
    }
    // accent-color
    if let Some(ac) = s.get("accent-color") {
        if ac.trim() != "auto" {
            bx.accent_color = parse_color(ac);
        }
    }
    // scroll-behavior
    if let Some(sb) = s.get("scroll-behavior") {
        bx.scroll_behavior = sb.trim().to_string();
    }
    // scrollbar-width
    if let Some(sw) = s.get("scrollbar-width") {
        bx.scrollbar_width = sw.trim().to_string();
    }
    // scrollbar-color: thumb track
    if let Some(sc) = s.get("scrollbar-color") {
        let parts: Vec<&str> = sc.split_whitespace().collect();
        if parts.len() >= 2 {
            if let (Some(thumb), Some(track)) = (parse_color(parts[0]), parse_color(parts[1])) {
                bx.scrollbar_color = Some((thumb, track));
            }
        }
    }
    // overscroll-behavior
    if let Some(ob) = s.get("overscroll-behavior") {
        bx.overscroll_behavior = ob.trim().to_string();
    }
    // scroll-snap
    if let Some(sst) = s.get("scroll-snap-type") {
        bx.scroll_snap_type = sst.trim().to_string();
    }
    if let Some(ssa) = s.get("scroll-snap-align") {
        bx.scroll_snap_align = ssa.trim().to_string();
    }
    let parse_4 = |v: &str| -> [f32; 4] {
        let parts: Vec<&str> = v.split_whitespace().collect();
        match parts.len() {
            1 => { let a = parse_length(parts[0]); [a, a, a, a] }
            2 => { let a = parse_length(parts[0]); let b = parse_length(parts[1]); [a, b, a, b] }
            3 => [parse_length(parts[0]), parse_length(parts[1]), parse_length(parts[2]), parse_length(parts[1])],
            4 => [parse_length(parts[0]), parse_length(parts[1]), parse_length(parts[2]), parse_length(parts[3])],
            _ => [0.0; 4],
        }
    };
    if let Some(sp) = s.get("scroll-padding") { bx.scroll_padding = parse_4(sp); }
    if let Some(sm) = s.get("scroll-margin")  { bx.scroll_margin  = parse_4(sm); }
    // contain - CSS Containment L3
    if let Some(c) = s.get("contain") {
        let mut bits = 0u8;
        for tok in c.split_whitespace() {
            match tok {
                "layout" => bits |= 1,
                "paint"  => bits |= 2,
                "size"   => bits |= 4,
                "style"  => bits |= 8,
                "content" => bits |= 1 | 2 | 8, // layout + paint + style
                "strict"  => bits |= 1 | 2 | 4 | 8, // vsechno
                _ => {}
            }
        }
        bx.contain = bits;
    }
    // aspect-ratio: "16 / 9" / "1.5" / "auto"
    if let Some(ar) = s.get("aspect-ratio") {
        let s = ar.trim();
        if s != "auto" {
            if let Some((w, h)) = s.split_once('/') {
                let w: f32 = w.trim().parse().unwrap_or(1.0);
                let h: f32 = h.trim().parse().unwrap_or(1.0);
                if h > 0.0 { bx.aspect_ratio = Some(w / h); }
            } else if let Ok(r) = s.parse::<f32>() {
                bx.aspect_ratio = Some(r);
            }
        }
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

    // Pseudo-element ::before - vlozit jako prvni virtualni child
    if let Some(pseudo_styles) = super::cascade::get_pseudo_styles(pseudo_map, node, "before") {
        if let Some(pb) = build_pseudo_box(node, pseudo_styles) {
            bx.children.push(pb);
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
        let cb = build_box_inner(child, style_map, pseudo_map);
        if cb.display != Display::None {
            // Text bez obsahu - zahodit
            if matches!(child.kind, NodeKind::Text(_)) && cb.text.is_none() {
                continue;
            }
            bx.children.push(cb);
        }
    }

    // Pseudo-element ::after - posledni virtualni child
    if let Some(pseudo_styles) = super::cascade::get_pseudo_styles(pseudo_map, node, "after") {
        if let Some(pa) = build_pseudo_box(node, pseudo_styles) {
            bx.children.push(pa);
        }
    }

    bx
}

/// Vyrobi LayoutBox pro pseudo-element (::before / ::after) z computed styles.
/// Content property: "string", attr(name), counter(...) - implementovano: string a attr.
fn build_pseudo_box(parent_node: &Rc<Node>, styles: &HashMap<String, String>) -> Option<LayoutBox> {
    let content_raw = styles.get("content")?;
    let text = parse_content_value(content_raw, parent_node)?;
    if text.is_empty() { return None; }

    let mut bx = LayoutBox::new();
    bx.display = Display::Inline;
    bx.tag = Some("::pseudo".to_string());
    bx.text = Some(text);

    // Apply styly z pseudo styles
    if let Some(c) = styles.get("color") {
        bx.text_color = parse_color(c);
    }
    let bg_value = styles.get("background-color").or(styles.get("background"));
    if let Some(c) = bg_value {
        if c.contains("linear-gradient(") {
            bx.bg_gradient = parse_any_gradient(c);
        } else {
            bx.bg_color = parse_color(c);
        }
    }
    if let Some(fs) = styles.get("font-size") { bx.font_size = parse_length(fs); }
    // font-weight: zatim nepouzivame na pseudo-box level (LayoutBox tu prop nedrzi).
    let _ = styles.get("font-weight");
    if let Some(p) = styles.get("padding") { bx.padding = parse_length(p); }
    if let Some(m) = styles.get("margin") { bx.margin = parse_length(m); }

    Some(bx)
}

/// Parsuje `content` value:
/// - "string" -> String
/// - 'string' -> String
/// - attr(name) -> hodnota atributu na parent node
/// - normal / none -> None
/// - counter(name) -> placeholder "1" (counters out of scope zatim)
fn parse_content_value(raw: &str, parent: &Rc<Node>) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() || s == "none" || s == "normal" { return None; }

    // String literal
    if let Some(stripped) = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return Some(stripped.to_string());
    }
    if let Some(stripped) = s.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        return Some(stripped.to_string());
    }

    // attr(name)
    if let Some(inner) = s.strip_prefix("attr(").and_then(|s| s.strip_suffix(')')) {
        let name = inner.trim();
        return parent.attr(name).or(Some(String::new()));
    }

    // counter(name) - placeholder
    if s.starts_with("counter(") {
        return Some("0".to_string());
    }

    // url(...) - placeholder
    if s.starts_with("url(") {
        return Some(String::new());
    }

    Some(s.to_string())
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

/// Parse barvu z CSS string.
/// Podpora: #RGB, #RRGGBB, #RRGGBBAA, rgb()/rgba() (legacy + modern), hsl()/hsla(),
///          hwb(), lab(), lch(), oklab(), oklch(), color-mix(), nazvy.
pub fn parse_color(s: &str) -> Option<[u8; 4]> {
    let s = s.trim().to_lowercase();

    // Hex #RGB / #RRGGBB / #RRGGBBAA / #RGBA (4 char)
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 3 {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            return Some([r, g, b, 255]);
        }
        if hex.len() == 4 {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            let a = u8::from_str_radix(&hex[3..4], 16).ok()? * 17;
            return Some([r, g, b, a]);
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

    // rgb()/rgba() - legacy (carky) i modern (mezery + lomitko alpha)
    if let Some(inner) = s.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')'))
        .or_else(|| s.strip_prefix("rgba(").and_then(|s| s.strip_suffix(')'))) {
        let (parts, alpha) = split_color_args(inner);
        if parts.len() >= 3 {
            let r = parse_color_byte(parts[0])?;
            let g = parse_color_byte(parts[1])?;
            let b = parse_color_byte(parts[2])?;
            let a = alpha.or_else(|| parts.get(3).and_then(|p| parse_alpha(p))).unwrap_or(255);
            return Some([r, g, b, a]);
        }
    }

    // hsl()/hsla()
    if let Some(inner) = s.strip_prefix("hsl(").and_then(|s| s.strip_suffix(')'))
        .or_else(|| s.strip_prefix("hsla(").and_then(|s| s.strip_suffix(')'))) {
        let (parts, alpha) = split_color_args(inner);
        if parts.len() >= 3 {
            let h = parse_angle_deg(parts[0])?;
            let sat = parse_percent_or_num(parts[1])?;
            let lit = parse_percent_or_num(parts[2])?;
            let a = alpha.or_else(|| parts.get(3).and_then(|p| parse_alpha(p))).unwrap_or(255);
            let (r, g, b) = hsl_to_rgb(h, sat, lit);
            return Some([r, g, b, a]);
        }
    }

    // hwb(h w% b%)
    if let Some(inner) = s.strip_prefix("hwb(").and_then(|s| s.strip_suffix(')')) {
        let (parts, alpha) = split_color_args(inner);
        if parts.len() >= 3 {
            let h = parse_angle_deg(parts[0])?;
            let w = parse_percent_or_num(parts[1])?;
            let bl = parse_percent_or_num(parts[2])?;
            let a = alpha.or_else(|| parts.get(3).and_then(|p| parse_alpha(p))).unwrap_or(255);
            let (r, g, b) = hwb_to_rgb(h, w, bl);
            return Some([r, g, b, a]);
        }
    }

    // lab(l a b)
    if let Some(inner) = s.strip_prefix("lab(").and_then(|s| s.strip_suffix(')')) {
        let (parts, alpha) = split_color_args(inner);
        if parts.len() >= 3 {
            let l = parse_percent_or_num_scaled(parts[0], 100.0)?;
            let a_lab = parts[1].parse::<f32>().ok()?;
            let b_lab = parts[2].parse::<f32>().ok()?;
            let a = alpha.or_else(|| parts.get(3).and_then(|p| parse_alpha(p))).unwrap_or(255);
            let (r, g, b) = lab_to_rgb(l, a_lab, b_lab);
            return Some([r, g, b, a]);
        }
    }

    // lch(l c h)
    if let Some(inner) = s.strip_prefix("lch(").and_then(|s| s.strip_suffix(')')) {
        let (parts, alpha) = split_color_args(inner);
        if parts.len() >= 3 {
            let l = parse_percent_or_num_scaled(parts[0], 100.0)?;
            let c = parts[1].parse::<f32>().ok()?;
            let h = parse_angle_deg(parts[2])?;
            let a = alpha.or_else(|| parts.get(3).and_then(|p| parse_alpha(p))).unwrap_or(255);
            let (la, lb) = (c * h.to_radians().cos(), c * h.to_radians().sin());
            let (r, g, b) = lab_to_rgb(l, la, lb);
            return Some([r, g, b, a]);
        }
    }

    // oklab(l a b) - L je 0..1 (nebo 0%..100%)
    if let Some(inner) = s.strip_prefix("oklab(").and_then(|s| s.strip_suffix(')')) {
        let (parts, alpha) = split_color_args(inner);
        if parts.len() >= 3 {
            let l = parse_percent_or_num_scaled(parts[0], 1.0)?;
            let a_ok = parts[1].parse::<f32>().ok()?;
            let b_ok = parts[2].parse::<f32>().ok()?;
            let a = alpha.or_else(|| parts.get(3).and_then(|p| parse_alpha(p))).unwrap_or(255);
            let (r, g, b) = oklab_to_rgb(l, a_ok, b_ok);
            return Some([r, g, b, a]);
        }
    }

    // oklch(l c h)
    if let Some(inner) = s.strip_prefix("oklch(").and_then(|s| s.strip_suffix(')')) {
        let (parts, alpha) = split_color_args(inner);
        if parts.len() >= 3 {
            let l = parse_percent_or_num_scaled(parts[0], 1.0)?;
            let c = parts[1].parse::<f32>().ok()?;
            let h = parse_angle_deg(parts[2])?;
            let a = alpha.or_else(|| parts.get(3).and_then(|p| parse_alpha(p))).unwrap_or(255);
            let (la, lb) = (c * h.to_radians().cos(), c * h.to_radians().sin());
            let (r, g, b) = oklab_to_rgb(l, la, lb);
            return Some([r, g, b, a]);
        }
    }

    // color-mix(in <space>, c1 [<%>], c2 [<%>])
    if let Some(inner) = s.strip_prefix("color-mix(").and_then(|s| s.strip_suffix(')')) {
        return parse_color_mix(inner);
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
        "orange"  => Some([255, 165, 0, 255]),
        "purple"  => Some([128, 0, 128, 255]),
        "pink"    => Some([255, 192, 203, 255]),
        "brown"   => Some([165, 42, 42, 255]),
        "transparent" => Some([0, 0, 0, 0]),
        _ => None,
    }
}

/// Split argumentu barvy: respektuje modern syntax `r g b / a` (lomitko = alpha).
/// Vrati (positional_args, optional_alpha_byte).
fn split_color_args(inner: &str) -> (Vec<&str>, Option<u8>) {
    // Pokud obsahuje '/', alpha je za nim
    let (main, alpha_str) = match inner.split_once('/') {
        Some((m, a)) => (m, Some(a.trim())),
        None => (inner, None),
    };
    // Modern: mezery; legacy: carky. Tolerujem oboje.
    let parts: Vec<&str> = if main.contains(',') {
        main.split(',').map(str::trim).collect()
    } else {
        main.split_whitespace().collect()
    };
    let alpha = alpha_str.and_then(parse_alpha);
    (parts, alpha)
}

fn parse_color_byte(s: &str) -> Option<u8> {
    let s = s.trim();
    if let Some(p) = s.strip_suffix('%') {
        let v: f32 = p.trim().parse().ok()?;
        return Some((v.clamp(0.0, 100.0) / 100.0 * 255.0).round() as u8);
    }
    let v: f32 = s.parse().ok()?;
    Some(v.clamp(0.0, 255.0).round() as u8)
}

fn parse_alpha(s: &str) -> Option<u8> {
    let s = s.trim();
    if let Some(p) = s.strip_suffix('%') {
        let v: f32 = p.trim().parse().ok()?;
        return Some((v.clamp(0.0, 100.0) / 100.0 * 255.0).round() as u8);
    }
    let v: f32 = s.parse().ok()?;
    Some((v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn parse_angle_deg(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(v) = s.strip_suffix("deg") { v.trim().parse().ok() }
    else if let Some(v) = s.strip_suffix("rad") {
        v.trim().parse::<f32>().ok().map(|r| r.to_degrees())
    }
    else if let Some(v) = s.strip_suffix("turn") {
        v.trim().parse::<f32>().ok().map(|t| t * 360.0)
    }
    else { s.parse().ok() }
}

fn parse_percent_or_num(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(p) = s.strip_suffix('%') { p.trim().parse::<f32>().ok().map(|v| v / 100.0) }
    else { s.parse().ok() }
}

/// Pro Lab/Oklab L: percent -> scale (lab je 0..100, oklab 0..1).
fn parse_percent_or_num_scaled(s: &str, scale: f32) -> Option<f32> {
    let s = s.trim();
    if let Some(p) = s.strip_suffix('%') { p.trim().parse::<f32>().ok().map(|v| v / 100.0 * scale) }
    else { s.parse().ok() }
}

/// HSL -> RGB. h v stupnich, s/l v 0..1.
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let h = ((h % 360.0) + 360.0) % 360.0 / 360.0;
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    if s == 0.0 {
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let r = hue_to_rgb(p, q, h + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h);
    let b = hue_to_rgb(p, q, h - 1.0 / 3.0);
    ((r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8)
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 { t += 1.0; }
    if t > 1.0 { t -= 1.0; }
    if t < 1.0 / 6.0 { return p + (q - p) * 6.0 * t; }
    if t < 1.0 / 2.0 { return q; }
    if t < 2.0 / 3.0 { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
    p
}

/// HWB -> RGB. h v stupnich, w/b v 0..1.
fn hwb_to_rgb(h: f32, w: f32, b: f32) -> (u8, u8, u8) {
    let mut w = w.clamp(0.0, 1.0);
    let mut b = b.clamp(0.0, 1.0);
    if w + b > 1.0 {
        let sum = w + b;
        w /= sum;
        b /= sum;
    }
    let (r, g, bl) = hsl_to_rgb(h, 1.0, 0.5);
    let f = |v: u8| {
        let x = v as f32 / 255.0;
        let out = x * (1.0 - w - b) + w;
        (out * 255.0).clamp(0.0, 255.0).round() as u8
    };
    (f(r), f(g), f(bl))
}

/// OkLab -> sRGB (Bjorn Ottosson algoritmus).
fn oklab_to_rgb(l: f32, a: f32, b: f32) -> (u8, u8, u8) {
    let l_ = l + 0.3963377774 * a + 0.2158037573 * b;
    let m_ = l - 0.1055613458 * a - 0.0638541728 * b;
    let s_ = l - 0.0894841775 * a - 1.2914855480 * b;
    let l3 = l_ * l_ * l_;
    let m3 = m_ * m_ * m_;
    let s3 = s_ * s_ * s_;
    let r =  4.0767416621 * l3 - 3.3077115913 * m3 + 0.2309699292 * s3;
    let g = -1.2684380046 * l3 + 2.6097574011 * m3 - 0.3413193965 * s3;
    let b_ = -0.0041960863 * l3 - 0.7034186147 * m3 + 1.7076147010 * s3;
    (linear_to_srgb_u8(r), linear_to_srgb_u8(g), linear_to_srgb_u8(b_))
}

/// CIE Lab -> sRGB. Vstup: L 0..100, a/b ~ -128..127.
fn lab_to_rgb(l: f32, a: f32, b: f32) -> (u8, u8, u8) {
    let fy = (l + 16.0) / 116.0;
    let fx = a / 500.0 + fy;
    let fz = fy - b / 200.0;
    let f_inv = |t: f32| {
        let d = 6.0 / 29.0;
        if t > d { t * t * t } else { 3.0 * d * d * (t - 4.0 / 29.0) }
    };
    // D65 illuminant
    let xn = 0.95047; let yn = 1.0; let zn = 1.08883;
    let x = xn * f_inv(fx);
    let y = yn * f_inv(fy);
    let z = zn * f_inv(fz);
    // XYZ -> linear sRGB (D65)
    let r =  3.2404542 * x - 1.5371385 * y - 0.4985314 * z;
    let g = -0.9692660 * x + 1.8760108 * y + 0.0415560 * z;
    let b_ = 0.0556434 * x - 0.2040259 * y + 1.0572252 * z;
    (linear_to_srgb_u8(r), linear_to_srgb_u8(g), linear_to_srgb_u8(b_))
}

/// Linear -> sRGB gamma encoding + clamp + round to u8.
fn linear_to_srgb_u8(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let s = if v <= 0.0031308 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0).clamp(0.0, 255.0).round() as u8
}

fn srgb_u8_to_linear(v: u8) -> f32 {
    let s = v as f32 / 255.0;
    if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
}

/// sRGB -> OkLab.
fn rgb_to_oklab(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = srgb_u8_to_linear(r);
    let g = srgb_u8_to_linear(g);
    let b = srgb_u8_to_linear(b);
    let l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b;
    let m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b;
    let s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b;
    let l_ = l.cbrt();
    let m_ = m.cbrt();
    let s_ = s.cbrt();
    (
        0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
        1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
        0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
    )
}

/// color-mix(in <space>, color1 [<%>], color2 [<%>]).
/// Podpora space: srgb, oklab, oklch, lab, lch, hsl, hwb.
fn parse_color_mix(inner: &str) -> Option<[u8; 4]> {
    // Format: "in <space>, c1 [<%>], c2 [<%>]"
    let parts = split_top_level_commas(inner);
    if parts.len() < 3 { return None; }
    let in_clause = parts[0].trim();
    let space = in_clause.strip_prefix("in ")?.trim().to_lowercase();
    // Cisla mohou byt napr. "red 30%" / "red"
    let parse_color_pct = |s: &str| -> Option<([u8; 4], f32)> {
        let s = s.trim();
        // Najdi posledni "<num>%" - znaci vahu
        if let Some(pct_idx) = s.rfind('%') {
            let space_idx = s[..pct_idx].rfind(char::is_whitespace);
            if let Some(si) = space_idx {
                let pct: f32 = s[si..pct_idx].trim().parse().ok()?;
                let col = parse_color(s[..si].trim())?;
                return Some((col, pct / 100.0));
            }
        }
        let col = parse_color(s)?;
        Some((col, -1.0)) // signal: nezadana vaha
    };
    let (c1, p1) = parse_color_pct(&parts[1])?;
    let (c2, p2) = parse_color_pct(&parts[2])?;

    // Normalize percent vah
    let (w1, w2) = match (p1, p2) {
        (-1.0, -1.0) => (0.5, 0.5),
        (a, -1.0)    => (a, 1.0 - a),
        (-1.0, b)    => (1.0 - b, b),
        (a, b) => {
            let sum = a + b;
            if sum > 0.0 { (a / sum, b / sum) } else { (0.5, 0.5) }
        }
    };

    // Mix dle prostoru
    let (r, g, b) = match space.as_str() {
        "srgb" => (
            (c1[0] as f32 * w1 + c2[0] as f32 * w2).round() as u8,
            (c1[1] as f32 * w1 + c2[1] as f32 * w2).round() as u8,
            (c1[2] as f32 * w1 + c2[2] as f32 * w2).round() as u8,
        ),
        "oklab" | "oklch" => {
            let (l1, a1, b1) = rgb_to_oklab(c1[0], c1[1], c1[2]);
            let (l2, a2, b2) = rgb_to_oklab(c2[0], c2[1], c2[2]);
            let l = l1 * w1 + l2 * w2;
            let a = a1 * w1 + a2 * w2;
            let b = b1 * w1 + b2 * w2;
            oklab_to_rgb(l, a, b)
        }
        _ => {
            // Fallback: srgb
            (
                (c1[0] as f32 * w1 + c2[0] as f32 * w2).round() as u8,
                (c1[1] as f32 * w1 + c2[1] as f32 * w2).round() as u8,
                (c1[2] as f32 * w1 + c2[2] as f32 * w2).round() as u8,
            )
        }
    };
    let alpha = (c1[3] as f32 * w1 + c2[3] as f32 * w2).round() as u8;
    Some([r, g, b, alpha])
}

/// Vsechny varianty gradientu pro background-image.
#[derive(Debug, Clone)]
pub struct BgGradient {
    pub kind: BgGradientKind,
    pub stops: Vec<(f32, [u8; 4])>,
}

#[derive(Debug, Clone)]
pub enum BgGradientKind {
    Linear { angle_deg: f32 },
    /// cx/cy/radius v procentech (0..1). radius_pct = polomer relativni k farthest-corner.
    Radial { cx_pct: f32, cy_pct: f32, radius_pct: f32 },
    /// start_angle_deg: poradi od 12 hod. cx/cy v procentech.
    Conic { cx_pct: f32, cy_pct: f32, start_angle_deg: f32 },
}

/// CSS clip-path L1 - tvar pro clipping elementu.
#[derive(Debug, Clone, PartialEq)]
pub enum ClipPath {
    /// inset(top right bottom left [round <radius>])
    Inset { top: f32, right: f32, bottom: f32, left: f32, radius: f32 },
    /// circle(<r> [at <pos>])
    Circle { cx_pct: f32, cy_pct: f32, radius_pct: f32 },
    /// ellipse(<rx> <ry> [at <pos>])
    Ellipse { cx_pct: f32, cy_pct: f32, rx_pct: f32, ry_pct: f32 },
    /// polygon(p1, p2, ...) - kazdy bod (x_pct, y_pct).
    Polygon(Vec<(f32, f32)>),
}

pub fn parse_clip_path(s: &str) -> Option<ClipPath> {
    let s = s.trim();
    if s == "none" || s.is_empty() { return None; }
    if let Some(args) = s.strip_prefix("inset(").and_then(|s| s.strip_suffix(')')) {
        return parse_inset_clip(args);
    }
    if let Some(args) = s.strip_prefix("circle(").and_then(|s| s.strip_suffix(')')) {
        return parse_circle_clip(args);
    }
    if let Some(args) = s.strip_prefix("ellipse(").and_then(|s| s.strip_suffix(')')) {
        return parse_ellipse_clip(args);
    }
    if let Some(args) = s.strip_prefix("polygon(").and_then(|s| s.strip_suffix(')')) {
        return parse_polygon_clip(args);
    }
    None
}

fn parse_inset_clip(args: &str) -> Option<ClipPath> {
    let mut radius = 0.0;
    let main = if let Some((before, r)) = args.split_once("round ") {
        radius = parse_length(r.trim());
        before.trim()
    } else {
        args.trim()
    };
    let parts: Vec<&str> = main.split_whitespace().collect();
    let (t, r, b, l) = match parts.len() {
        1 => (parts[0], parts[0], parts[0], parts[0]),
        2 => (parts[0], parts[1], parts[0], parts[1]),
        3 => (parts[0], parts[1], parts[2], parts[1]),
        4 => (parts[0], parts[1], parts[2], parts[3]),
        _ => return None,
    };
    Some(ClipPath::Inset {
        top: parse_length(t),
        right: parse_length(r),
        bottom: parse_length(b),
        left: parse_length(l),
        radius,
    })
}

fn parse_circle_clip(args: &str) -> Option<ClipPath> {
    let mut cx_pct: f32 = 0.5;
    let mut cy_pct: f32 = 0.5;
    let mut radius_pct: f32 = 0.5;
    let (before_at, after_at) = args.split_once(" at ")
        .map(|(a, b)| (a.trim(), Some(b.trim())))
        .unwrap_or((args.trim(), None));
    if !before_at.is_empty() {
        if let Some(p) = before_at.strip_suffix('%') {
            radius_pct = p.parse::<f32>().ok()? / 100.0;
        }
    }
    if let Some(pos) = after_at {
        let pos_parts: Vec<&str> = pos.split_whitespace().collect();
        let kw = |t: &str| -> Option<f32> {
            match t {
                "left" | "top" => Some(0.0),
                "center" => Some(0.5),
                "right" | "bottom" => Some(1.0),
                s if s.ends_with('%') => s.trim_end_matches('%').parse::<f32>().ok().map(|v| v / 100.0),
                _ => None,
            }
        };
        if let Some(v) = pos_parts.first().and_then(|t| kw(t)) { cx_pct = v; }
        if let Some(v) = pos_parts.get(1).and_then(|t| kw(t)) { cy_pct = v; }
    }
    Some(ClipPath::Circle { cx_pct, cy_pct, radius_pct })
}

fn parse_ellipse_clip(args: &str) -> Option<ClipPath> {
    let mut cx_pct: f32 = 0.5;
    let mut cy_pct: f32 = 0.5;
    let mut rx_pct: f32 = 0.5;
    let mut ry_pct: f32 = 0.5;
    let (before_at, after_at) = args.split_once(" at ")
        .map(|(a, b)| (a.trim(), Some(b.trim())))
        .unwrap_or((args.trim(), None));
    let parts: Vec<&str> = before_at.split_whitespace().collect();
    if let Some(p) = parts.first().and_then(|t| t.strip_suffix('%'))
        .and_then(|p| p.parse::<f32>().ok()) { rx_pct = p / 100.0; }
    if let Some(p) = parts.get(1).and_then(|t| t.strip_suffix('%'))
        .and_then(|p| p.parse::<f32>().ok()) { ry_pct = p / 100.0; }
    if let Some(pos) = after_at {
        let pp: Vec<&str> = pos.split_whitespace().collect();
        let kw = |t: &str| -> Option<f32> {
            match t {
                "left" | "top" => Some(0.0),
                "center" => Some(0.5),
                "right" | "bottom" => Some(1.0),
                s if s.ends_with('%') => s.trim_end_matches('%').parse::<f32>().ok().map(|v| v / 100.0),
                _ => None,
            }
        };
        if let Some(v) = pp.first().and_then(|t| kw(t)) { cx_pct = v; }
        if let Some(v) = pp.get(1).and_then(|t| kw(t)) { cy_pct = v; }
    }
    Some(ClipPath::Ellipse { cx_pct, cy_pct, rx_pct, ry_pct })
}

fn parse_polygon_clip(args: &str) -> Option<ClipPath> {
    // "0 0, 100% 0, 50% 100%"
    let mut points = Vec::new();
    for pair in args.split(',') {
        let parts: Vec<&str> = pair.split_whitespace().collect();
        if parts.len() < 2 { continue; }
        let x = parse_pct_or_px(parts[0]);
        let y = parse_pct_or_px(parts[1]);
        points.push((x, y));
    }
    if points.is_empty() { None } else { Some(ClipPath::Polygon(points)) }
}

fn parse_pct_or_px(s: &str) -> f32 {
    if let Some(p) = s.strip_suffix('%') { p.parse::<f32>().unwrap_or(0.0) / 100.0 }
    else { parse_length(s) / 100.0 } // approximace - px za 100 = 1.0
}

/// CSS Backgrounds L3 - jedna vrstva.
#[derive(Debug, Clone, Default)]
pub struct BgLayer {
    pub color: Option<[u8; 4]>,
    pub image_src: Option<String>,
    pub gradient: Option<BgGradient>,
    pub position: BgPosition,
    pub size: BgSize,
    pub repeat: BgRepeat,
    pub clip: BgBox,
    pub origin: BgBox,
    pub attachment: BgAttachment,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BgPosition {
    /// Px / % offset od top-left
    Px(f32, f32),
    /// Procenta (0..1)
    Pct(f32, f32),
    /// Mix - x px, y %
    Mixed { x_px: Option<f32>, x_pct: Option<f32>, y_px: Option<f32>, y_pct: Option<f32> },
}
impl Default for BgPosition {
    fn default() -> Self { BgPosition::Pct(0.0, 0.0) }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BgSize {
    /// Auto - puvodni image rozmer
    Auto,
    Cover,
    Contain,
    /// Lengths: (w, h) - None = auto
    Length { w: Option<f32>, h: Option<f32> },
    /// Procenta vuci kontejneru
    Pct { w: Option<f32>, h: Option<f32> },
}
impl Default for BgSize {
    fn default() -> Self { BgSize::Auto }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BgRepeat {
    Repeat,
    RepeatX,
    RepeatY,
    NoRepeat,
    Space,
    Round,
}
impl Default for BgRepeat {
    fn default() -> Self { BgRepeat::Repeat }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BgBox {
    BorderBox,
    PaddingBox,
    ContentBox,
}
impl Default for BgBox {
    fn default() -> Self { BgBox::BorderBox }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BgAttachment {
    Scroll,
    Fixed,
    Local,
}
impl Default for BgAttachment {
    fn default() -> Self { BgAttachment::Scroll }
}

/// Parsuje background-position: "left", "center", "50%", "10px 20px", "right top".
pub fn parse_bg_position(s: &str) -> BgPosition {
    let parts: Vec<&str> = s.split_whitespace().collect();
    let kw_pct = |kw: &str| -> Option<f32> {
        match kw {
            "left" | "top" => Some(0.0),
            "center" => Some(0.5),
            "right" | "bottom" => Some(1.0),
            _ => None,
        }
    };
    let parse_axis = |t: &str| -> (Option<f32>, Option<f32>) {
        // (px, pct)
        if let Some(p) = kw_pct(t) { return (None, Some(p)); }
        if let Some(p) = t.strip_suffix('%') {
            return (None, p.parse::<f32>().ok().map(|v| v / 100.0));
        }
        let v = parse_length(t);
        (Some(v), None)
    };
    match parts.len() {
        0 => BgPosition::default(),
        1 => {
            let (px, pct) = parse_axis(parts[0]);
            BgPosition::Mixed { x_px: px, x_pct: pct, y_px: None, y_pct: Some(0.5) }
        }
        _ => {
            let (xpx, xpct) = parse_axis(parts[0]);
            let (ypx, ypct) = parse_axis(parts[1]);
            BgPosition::Mixed { x_px: xpx, x_pct: xpct, y_px: ypx, y_pct: ypct }
        }
    }
}

/// Parsuje background-size: "auto", "cover", "contain", "100px 200px", "50% auto".
pub fn parse_bg_size(s: &str) -> BgSize {
    let s = s.trim();
    match s {
        "cover" => return BgSize::Cover,
        "contain" => return BgSize::Contain,
        "auto" | "" => return BgSize::Auto,
        _ => {}
    }
    let parts: Vec<&str> = s.split_whitespace().collect();
    let parse_one = |t: &str| -> Option<f32> {
        if t == "auto" { return None; }
        if let Some(p) = t.strip_suffix('%') {
            return p.parse::<f32>().ok().map(|v| v / 100.0);
        }
        Some(parse_length(t))
    };
    if parts.len() == 1 {
        // pct path
        if parts[0].ends_with('%') {
            return BgSize::Pct { w: parse_one(parts[0]), h: None };
        }
        return BgSize::Length { w: parse_one(parts[0]), h: None };
    }
    let w = parse_one(parts[0]);
    let h = parse_one(parts[1]);
    if parts[0].ends_with('%') || parts[1].ends_with('%') {
        BgSize::Pct { w, h }
    } else {
        BgSize::Length { w, h }
    }
}

pub fn parse_bg_repeat(s: &str) -> BgRepeat {
    match s.trim() {
        "no-repeat" => BgRepeat::NoRepeat,
        "repeat-x"  => BgRepeat::RepeatX,
        "repeat-y"  => BgRepeat::RepeatY,
        "space"     => BgRepeat::Space,
        "round"     => BgRepeat::Round,
        _ => BgRepeat::Repeat,
    }
}

pub fn parse_bg_box(s: &str) -> BgBox {
    match s.trim() {
        "padding-box" => BgBox::PaddingBox,
        "content-box" => BgBox::ContentBox,
        _ => BgBox::BorderBox,
    }
}

pub fn parse_bg_attachment(s: &str) -> BgAttachment {
    match s.trim() {
        "fixed" => BgAttachment::Fixed,
        "local" => BgAttachment::Local,
        _ => BgAttachment::Scroll,
    }
}

/// CSS Filter Effects L1 - jednotliva operace.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterOp {
    Blur(f32),                   // pixelu
    Brightness(f32),             // 0..1+
    Contrast(f32),               // 0..1+
    Grayscale(f32),              // 0..1
    HueRotate(f32),              // ve stupnich
    Invert(f32),                 // 0..1
    Saturate(f32),               // 0..1+
    Sepia(f32),                  // 0..1
    Opacity(f32),                // 0..1
    /// drop-shadow(offset_x offset_y blur color)
    DropShadow { ox: f32, oy: f32, blur: f32, color: [u8; 4] },
}

/// Parsuje filter / backdrop-filter property na chain operaci.
/// "filter: blur(2px) brightness(1.2) hue-rotate(45deg)" -> 3 ops.
pub fn parse_filter_chain(s: &str) -> Vec<FilterOp> {
    let s = s.trim();
    if s.is_empty() || s == "none" { return Vec::new(); }
    let mut out = Vec::new();
    let mut chars = s.char_indices().peekable();
    while let Some(&(start, _)) = chars.peek() {
        // Skip whitespace
        while let Some(&(_, c)) = chars.peek() {
            if c.is_whitespace() { chars.next(); } else { break; }
        }
        let start_idx = match chars.peek() { Some(&(i, _)) => i, None => break };
        // Read function name do `(`
        let mut name_end = start_idx;
        while let Some(&(i, c)) = chars.peek() {
            if c == '(' { name_end = i; chars.next(); break; }
            if c.is_whitespace() {
                // Mozna keyword bez argumentu - skip
                break;
            }
            chars.next();
            name_end = i + c.len_utf8();
        }
        let name = &s[start_idx..name_end];
        if name.is_empty() { break; }
        // Read args do `)` - respektovat nesteni
        let arg_start = match chars.peek() { Some(&(i, _)) => i, None => break };
        let _ = start;
        let mut depth = 1;
        let mut arg_end = arg_start;
        while let Some(&(i, c)) = chars.peek() {
            arg_end = i;
            if c == '(' { depth += 1; }
            if c == ')' { depth -= 1; if depth == 0 { chars.next(); break; } }
            chars.next();
        }
        let args = &s[arg_start..arg_end];
        if let Some(op) = parse_filter_one(name, args) {
            out.push(op);
        }
    }
    out
}

fn parse_filter_one(name: &str, args: &str) -> Option<FilterOp> {
    let args = args.trim();
    let parse_pct_or_num = |s: &str| -> Option<f32> {
        let s = s.trim();
        if let Some(p) = s.strip_suffix('%') { p.trim().parse::<f32>().ok().map(|v| v / 100.0) }
        else { s.parse().ok() }
    };
    Some(match name {
        "blur" => FilterOp::Blur(parse_length(args)),
        "brightness" => FilterOp::Brightness(parse_pct_or_num(args)?),
        "contrast"   => FilterOp::Contrast(parse_pct_or_num(args)?),
        "grayscale"  => FilterOp::Grayscale(parse_pct_or_num(args)?),
        "hue-rotate" => {
            // deg / rad / turn
            if let Some(v) = args.strip_suffix("deg") { FilterOp::HueRotate(v.trim().parse().ok()?) }
            else if let Some(v) = args.strip_suffix("rad") {
                FilterOp::HueRotate(v.trim().parse::<f32>().ok()?.to_degrees())
            }
            else if let Some(v) = args.strip_suffix("turn") {
                FilterOp::HueRotate(v.trim().parse::<f32>().ok()? * 360.0)
            }
            else { FilterOp::HueRotate(args.parse().ok()?) }
        }
        "invert"   => FilterOp::Invert(parse_pct_or_num(args)?),
        "saturate" => FilterOp::Saturate(parse_pct_or_num(args)?),
        "sepia"    => FilterOp::Sepia(parse_pct_or_num(args)?),
        "opacity"  => FilterOp::Opacity(parse_pct_or_num(args)?),
        "drop-shadow" => {
            // "<ox> <oy> [<blur>] <color>"
            let parts: Vec<&str> = split_top_level_whitespace_str(args);
            if parts.len() < 3 { return None; }
            let ox = parse_length(parts[0]);
            let oy = parse_length(parts[1]);
            let mut blur = 0.0f32;
            let mut color_idx = 2;
            if parts[2].chars().next().map(|c| c.is_ascii_digit() || c == '.').unwrap_or(false)
                || parts[2].ends_with("px") || parts[2].ends_with("em")
            {
                blur = parse_length(parts[2]);
                color_idx = 3;
            }
            if color_idx >= parts.len() { return None; }
            let rest: String = parts[color_idx..].join(" ");
            let color = parse_color(&rest)?;
            FilterOp::DropShadow { ox, oy, blur, color }
        }
        _ => return None,
    })
}

/// Aplikuje filter chain na barvu (CPU). Kazdy filter modifikuje RGBA postupne.
/// Implementace: brightness/contrast/grayscale/sepia/invert/saturate/hue-rotate/opacity.
/// Blur a drop-shadow vyzaduji multi-pass - tady ignorovany (ne-color manipulation).
pub fn apply_filter_chain(rgba: [u8; 4], chain: &[FilterOp]) -> [u8; 4] {
    if chain.is_empty() { return rgba; }
    let mut r = rgba[0] as f32 / 255.0;
    let mut g = rgba[1] as f32 / 255.0;
    let mut b = rgba[2] as f32 / 255.0;
    let mut a = rgba[3] as f32 / 255.0;

    for op in chain {
        match *op {
            FilterOp::Brightness(v) => {
                r *= v; g *= v; b *= v;
            }
            FilterOp::Contrast(c) => {
                r = (r - 0.5) * c + 0.5;
                g = (g - 0.5) * c + 0.5;
                b = (b - 0.5) * c + 0.5;
            }
            FilterOp::Grayscale(amount) => {
                let amt = amount.clamp(0.0, 1.0);
                let lum = 0.299 * r + 0.587 * g + 0.114 * b;
                r = r * (1.0 - amt) + lum * amt;
                g = g * (1.0 - amt) + lum * amt;
                b = b * (1.0 - amt) + lum * amt;
            }
            FilterOp::Sepia(amount) => {
                let amt = amount.clamp(0.0, 1.0);
                let nr = 0.393 * r + 0.769 * g + 0.189 * b;
                let ng = 0.349 * r + 0.686 * g + 0.168 * b;
                let nb = 0.272 * r + 0.534 * g + 0.131 * b;
                r = r * (1.0 - amt) + nr * amt;
                g = g * (1.0 - amt) + ng * amt;
                b = b * (1.0 - amt) + nb * amt;
            }
            FilterOp::Invert(amount) => {
                let amt = amount.clamp(0.0, 1.0);
                r = r * (1.0 - amt) + (1.0 - r) * amt;
                g = g * (1.0 - amt) + (1.0 - g) * amt;
                b = b * (1.0 - amt) + (1.0 - b) * amt;
            }
            FilterOp::Saturate(s) => {
                let lum = 0.299 * r + 0.587 * g + 0.114 * b;
                r = lum + (r - lum) * s;
                g = lum + (g - lum) * s;
                b = lum + (b - lum) * s;
            }
            FilterOp::HueRotate(deg) => {
                let rad = deg.to_radians();
                let cos = rad.cos();
                let sin = rad.sin();
                // Standardni hue rotation matrix (NTSC luminance basis)
                let lr = 0.213; let lg = 0.715; let lb = 0.072;
                let m = [
                    lr + cos*(1.0-lr) + sin*(-lr),     lg + cos*(-lg) + sin*(-lg),       lb + cos*(-lb) + sin*(1.0-lb),
                    lr + cos*(-lr) + sin*(0.143),      lg + cos*(1.0-lg) + sin*(0.140),  lb + cos*(-lb) + sin*(-0.283),
                    lr + cos*(-lr) + sin*(-(1.0-lr)),  lg + cos*(-lg) + sin*(lg),        lb + cos*(1.0-lb) + sin*(lb),
                ];
                let nr = m[0]*r + m[1]*g + m[2]*b;
                let ng = m[3]*r + m[4]*g + m[5]*b;
                let nb = m[6]*r + m[7]*g + m[8]*b;
                r = nr; g = ng; b = nb;
            }
            FilterOp::Opacity(o) => {
                a *= o.clamp(0.0, 1.0);
            }
            FilterOp::Blur(_) | FilterOp::DropShadow { .. } => {
                // Multi-pass / shape effects - aktualne ignorujem (TODO render-to-texture)
            }
        }
    }
    [
        (r.clamp(0.0, 1.0) * 255.0) as u8,
        (g.clamp(0.0, 1.0) * 255.0) as u8,
        (b.clamp(0.0, 1.0) * 255.0) as u8,
        (a.clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

/// Top-level comma split - vraci Vec<String> trimmed.
fn split_top_level_commas_string(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for c in s.chars() {
        if let Some(q) = quote {
            cur.push(c);
            if c == q { quote = None; }
            continue;
        }
        match c {
            '"' | '\'' => { quote = Some(c); cur.push(c); }
            '(' => { depth += 1; cur.push(c); }
            ')' => { depth -= 1; cur.push(c); }
            ',' if depth == 0 => {
                if !cur.trim().is_empty() { tokens.push(cur.trim().to_string()); }
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() { tokens.push(cur.trim().to_string()); }
    tokens
}

fn split_top_level_whitespace_str(s: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut start = 0;
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            c if c.is_whitespace() && depth == 0 => {
                if i > start { tokens.push(&s[start..i]); }
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    if start < s.len() { tokens.push(&s[start..]); }
    tokens.into_iter().filter(|t| !t.is_empty()).collect()
}

/// Parsuje linear-gradient / radial-gradient / conic-gradient.
pub fn parse_any_gradient(s: &str) -> Option<BgGradient> {
    let s = s.trim();
    if s.starts_with("linear-gradient(") {
        let (angle, stops) = parse_linear_gradient(s)?;
        return Some(BgGradient {
            kind: BgGradientKind::Linear { angle_deg: angle },
            stops,
        });
    }
    if s.starts_with("radial-gradient(") {
        return parse_radial_gradient(s);
    }
    if s.starts_with("conic-gradient(") {
        return parse_conic_gradient(s);
    }
    None
}

/// Parse radial-gradient([circle|ellipse] [at <position>], color1, color2, ...).
/// Zjednoduseno: bere "circle" jako default, position default center, radius
/// = farthest-corner. Tezi az `at <pos>` syntax. Vsechny dalsi tokeny pred
/// prvnim color jsou ignored.
pub fn parse_radial_gradient(s: &str) -> Option<BgGradient> {
    let inner = s.trim().strip_prefix("radial-gradient(")?.strip_suffix(')')?;
    let parts = split_top_level_commas(inner);
    if parts.len() < 2 { return None; }

    // Pocatecni "config" cast (pres pozici, tvar, velikost). Heuristika:
    // pokud prvni cast neobsahuje barvu, je to config.
    let mut start_idx = 0;
    let mut cx_pct: f32 = 0.5;
    let mut cy_pct: f32 = 0.5;
    let radius_pct: f32 = 1.0; // farthest-corner default

    let first = parts[0].trim();
    if parse_color(first).is_none() && !first.contains('%') || first.contains("at ") {
        // Detekuj "at <pos>"
        if let Some(at_idx) = first.find("at ") {
            let pos = first[at_idx + 3..].trim();
            let pos_parts: Vec<&str> = pos.split_whitespace().collect();
            // Pozice: keyword (left/center/right/top/bottom) nebo procenta
            let pct_or_kw = |kw: &str| -> Option<f32> {
                match kw {
                    "left" | "top" => Some(0.0),
                    "center" => Some(0.5),
                    "right" | "bottom" => Some(1.0),
                    s if s.ends_with('%') => s.trim_end_matches('%').parse::<f32>().ok().map(|v| v / 100.0),
                    _ => None,
                }
            };
            if pos_parts.len() >= 1 {
                if let Some(v) = pct_or_kw(pos_parts[0]) { cx_pct = v; }
            }
            if pos_parts.len() >= 2 {
                if let Some(v) = pct_or_kw(pos_parts[1]) { cy_pct = v; }
            }
        }
        start_idx = 1;
    }

    let stops = parse_gradient_stops(&parts[start_idx..]);
    if stops.is_empty() { return None; }
    Some(BgGradient {
        kind: BgGradientKind::Radial { cx_pct, cy_pct, radius_pct },
        stops,
    })
}

/// Parse conic-gradient([from <angle>] [at <pos>], color1, color2, ...).
pub fn parse_conic_gradient(s: &str) -> Option<BgGradient> {
    let inner = s.trim().strip_prefix("conic-gradient(")?.strip_suffix(')')?;
    let parts = split_top_level_commas(inner);
    if parts.len() < 2 { return None; }

    let mut start_idx = 0;
    let mut cx_pct: f32 = 0.5;
    let mut cy_pct: f32 = 0.5;
    let mut start_angle_deg: f32 = 0.0;

    let first = parts[0].trim();
    if parse_color(first).is_none() {
        // "from 30deg at center" / "from 0deg" / "at 50% 50%"
        if let Some(from_idx) = first.find("from ") {
            let after = &first[from_idx + 5..];
            let token: String = after.chars().take_while(|c| !c.is_whitespace()).collect();
            if let Some(deg) = token.strip_suffix("deg") {
                start_angle_deg = deg.parse().unwrap_or(0.0);
            }
        }
        if let Some(at_idx) = first.find("at ") {
            let pos = first[at_idx + 3..].trim();
            let pos_parts: Vec<&str> = pos.split_whitespace().collect();
            let pct_or_kw = |kw: &str| -> Option<f32> {
                match kw {
                    "left" | "top" => Some(0.0),
                    "center" => Some(0.5),
                    "right" | "bottom" => Some(1.0),
                    s if s.ends_with('%') => s.trim_end_matches('%').parse::<f32>().ok().map(|v| v / 100.0),
                    _ => None,
                }
            };
            if pos_parts.len() >= 1 {
                if let Some(v) = pct_or_kw(pos_parts[0]) { cx_pct = v; }
            }
            if pos_parts.len() >= 2 {
                if let Some(v) = pct_or_kw(pos_parts[1]) { cy_pct = v; }
            }
        }
        start_idx = 1;
    }

    let stops = parse_gradient_stops(&parts[start_idx..]);
    if stops.is_empty() { return None; }
    Some(BgGradient {
        kind: BgGradientKind::Conic { cx_pct, cy_pct, start_angle_deg },
        stops,
    })
}

/// Parsuje gradient stops "red 50%" / "red" do (offset 0..1, color).
fn parse_gradient_stops(parts: &[&str]) -> Vec<(f32, [u8; 4])> {
    let mut stops: Vec<(f32, [u8; 4])> = Vec::new();
    let n = parts.len();
    for (i, p) in parts.iter().enumerate() {
        let trimmed = p.trim();
        let (color_str, offset) = if let Some(percent_idx) = trimmed.rfind('%') {
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
    stops
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

/// Parse text-shadow: "offset_x offset_y blur color" / "offset_x offset_y color".
pub fn parse_text_shadow(s: &str) -> Option<(f32, f32, f32, [u8; 4])> {
    let s = s.trim();
    if s == "none" || s.is_empty() { return None; }
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 3 { return None; }
    let ox = parse_length(parts[0]);
    let oy = parse_length(parts[1]);
    let mut blur = 0.0f32;
    let mut color_idx = 2;
    if parts[2].chars().next().map(|c| c.is_ascii_digit() || c == '.').unwrap_or(false)
        || parts[2].ends_with("px") || parts[2].ends_with("em")
    {
        blur = parse_length(parts[2]);
        color_idx = 3;
    }
    if color_idx >= parts.len() { return None; }
    let rest: String = parts[color_idx..].join(" ");
    let color = parse_color(&rest)?;
    Some((ox, oy, blur, color))
}

/// Parse box-shadow: "[inset] offset_x offset_y blur spread color".
pub fn parse_box_shadow(s: &str) -> Option<(f32, f32, f32, f32, [u8; 4], bool)> {
    let s = s.trim();
    if s == "none" { return None; }
    // Detect "inset" kdekoliv v retezci - odeber + zaznamenej.
    let mut inset = false;
    let cleaned: String = s.split_whitespace()
        .filter(|w| {
            if *w == "inset" { inset = true; false } else { true }
        })
        .collect::<Vec<_>>()
        .join(" ");

    let parts: Vec<&str> = cleaned.split_whitespace().collect();
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
    Some((ox, oy, blur, spread, color, inset))
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
    // 3D varianty
    if let Some(inner) = s.strip_prefix("translate3d(").and_then(|x| x.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        let x = parts.first().map(|p| parse_length(p)).unwrap_or(0.0);
        let y = parts.get(1).map(|p| parse_length(p)).unwrap_or(0.0);
        let z = parts.get(2).map(|p| parse_length(p)).unwrap_or(0.0);
        return Some(TransformOp::Translate3D { x, y, z });
    }
    if let Some(inner) = s.strip_prefix("translateZ(").and_then(|x| x.strip_suffix(')')) {
        return Some(TransformOp::Translate3D { x: 0.0, y: 0.0, z: parse_length(inner) });
    }
    if let Some(inner) = s.strip_prefix("rotate3d(").and_then(|x| x.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        let x = parts.first().and_then(|p| p.parse::<f32>().ok()).unwrap_or(0.0);
        let y = parts.get(1).and_then(|p| p.parse::<f32>().ok()).unwrap_or(0.0);
        let z = parts.get(2).and_then(|p| p.parse::<f32>().ok()).unwrap_or(0.0);
        let ang = parts.get(3).and_then(|p| {
            if let Some(d) = p.strip_suffix("deg") { d.parse::<f32>().ok().map(|v| v.to_radians()) }
            else if let Some(r) = p.strip_suffix("rad") { r.parse().ok() }
            else { None }
        }).unwrap_or(0.0);
        return Some(TransformOp::Rotate3D { x, y, z, angle_rad: ang });
    }
    if let Some(inner) = s.strip_prefix("rotateX(").and_then(|x| x.strip_suffix(')')) {
        let ang = if let Some(d) = inner.trim().strip_suffix("deg") { d.parse::<f32>().ok().map(|v| v.to_radians()).unwrap_or(0.0) }
            else { 0.0 };
        return Some(TransformOp::Rotate3D { x: 1.0, y: 0.0, z: 0.0, angle_rad: ang });
    }
    if let Some(inner) = s.strip_prefix("rotateY(").and_then(|x| x.strip_suffix(')')) {
        let ang = if let Some(d) = inner.trim().strip_suffix("deg") { d.parse::<f32>().ok().map(|v| v.to_radians()).unwrap_or(0.0) }
            else { 0.0 };
        return Some(TransformOp::Rotate3D { x: 0.0, y: 1.0, z: 0.0, angle_rad: ang });
    }
    if let Some(inner) = s.strip_prefix("rotateZ(").and_then(|x| x.strip_suffix(')')) {
        let ang = if let Some(d) = inner.trim().strip_suffix("deg") { d.parse::<f32>().ok().map(|v| v.to_radians()).unwrap_or(0.0) }
            else { 0.0 };
        return Some(TransformOp::Rotate(ang));
    }
    if let Some(inner) = s.strip_prefix("scale3d(").and_then(|x| x.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        let x = parts.first().and_then(|p| p.parse().ok()).unwrap_or(1.0);
        let y = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(1.0);
        let z = parts.get(2).and_then(|p| p.parse().ok()).unwrap_or(1.0);
        return Some(TransformOp::Scale3D { x, y, z });
    }
    if let Some(inner) = s.strip_prefix("matrix3d(").and_then(|x| x.strip_suffix(')')) {
        let parts: Vec<f32> = inner.split(',')
            .filter_map(|p| p.trim().parse().ok())
            .collect();
        if parts.len() == 16 {
            let mut m = [0.0; 16];
            m.copy_from_slice(&parts);
            return Some(TransformOp::Matrix3D(m));
        }
    }
    if let Some(inner) = s.strip_prefix("perspective(").and_then(|x| x.strip_suffix(')')) {
        return Some(TransformOp::Perspective(parse_length(inner)));
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

/// Parse delku v px/em/rem/vw/vh/% (bez kontextu - vraci 0 pro %).
/// Delsi suffixy musi byt kontrolovany drive.
pub fn parse_length(s: &str) -> f32 {
    parse_length_ctx(s, 1024.0, 768.0, 16.0)
}

/// Parse delky s viewport kontextem (pro vw/vh/% support).
pub fn parse_length_ctx(s: &str, vw: f32, vh: f32, parent_size: f32) -> f32 {
    let s = s.trim();
    if let Some(num) = s.strip_suffix("rem") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 16.0;
    }
    if let Some(num) = s.strip_suffix("vmin") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vw.min(vh) / 100.0;
    }
    if let Some(num) = s.strip_suffix("vmax") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vw.max(vh) / 100.0;
    }
    if let Some(num) = s.strip_suffix("px") {
        return num.trim().parse().unwrap_or(0.0);
    }
    if let Some(num) = s.strip_suffix("em") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 16.0;
    }
    if let Some(num) = s.strip_suffix("vw") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vw / 100.0;
    }
    if let Some(num) = s.strip_suffix("vh") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vh / 100.0;
    }
    // CSS Container Queries L1 jednotky - aktualne aproximace = viewport
    // (presna implementace by potrebovala lookup nejblizsiho ancestor s container-type).
    if let Some(num) = s.strip_suffix("cqw") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vw / 100.0;
    }
    if let Some(num) = s.strip_suffix("cqh") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vh / 100.0;
    }
    if let Some(num) = s.strip_suffix("cqi") {
        // inline = horizontal v default writing-mode
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vw / 100.0;
    }
    if let Some(num) = s.strip_suffix("cqb") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vh / 100.0;
    }
    if let Some(num) = s.strip_suffix("cqmin") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vw.min(vh) / 100.0;
    }
    if let Some(num) = s.strip_suffix("cqmax") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vw.max(vh) / 100.0;
    }
    if let Some(num) = s.strip_suffix("pt") {
        // 1pt = 1.333px
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 1.333;
    }
    if let Some(num) = s.strip_suffix('%') {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * parent_size / 100.0;
    }
    s.parse().unwrap_or(0.0)
}
