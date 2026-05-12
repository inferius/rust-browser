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
    /// CSS Display L3: contents - element zmizi, deti se chovaji jako primi descendants parenta
    Contents,
    /// list-item - block s ::marker
    ListItem,
    /// table / table-row / table-cell - tabulkove layouty
    Table,
    TableRow,
    TableCell,
    TableHeader,
    TableHeaderCell,
    TableCaption,
    InlineFlex,
    InlineGrid,
    /// Subgrid (Grid L2) - zatim layouted jako Grid
    Subgrid,
    /// Ruby (CJK)
    Ruby,
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
            "contents"     => Display::Contents,
            "list-item"    => Display::ListItem,
            "table"        => Display::Table,
            "table-row"    => Display::TableRow,
            "table-cell"   => Display::TableCell,
            "table-header-group" | "thead" => Display::TableHeader,
            "table-row-group" | "tbody" | "tfoot" => Display::TableHeader,
            "table-caption" => Display::TableCaption,
            "inline-flex"  => Display::InlineFlex,
            "inline-grid"  => Display::InlineGrid,
            "subgrid"      => Display::Subgrid,
            "ruby" | "ruby-base" | "ruby-text" => Display::Ruby,
            "none"         => Display::None,
            _ => Display::Block,
        }
    }
}

/// CSS pointer-events property - hit-test bypass.
/// auto = default (event captures), none = pass through (no hit).
/// Ostatni SVG-specific values (visiblePainted/fill/stroke/all/...) zatim
/// aproximovany na PointerEvents::Auto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PointerEvents {
    #[default]
    Auto,
    None,
}

impl PointerEvents {
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "none" => PointerEvents::None,
            _ => PointerEvents::Auto,
        }
    }
    pub fn is_none(self) -> bool { matches!(self, PointerEvents::None) }
}

/// CSS direction property - inline text flow direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

impl Direction {
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "rtl" => Direction::Rtl,
            _ => Direction::Ltr,
        }
    }
    pub fn is_rtl(self) -> bool { matches!(self, Direction::Rtl) }
}

/// CSS writing-mode property - smer toku textu.
/// horizontal-tb: text vodorovne, lines top-bottom (default).
/// vertical-rl: text vertikalne, columns right-to-left (japonstina, cinstina).
/// vertical-lr: text vertikalne, columns left-to-right.
/// sideways-rl / sideways-lr: text rotace 90deg + columns RL/LR (CSS L4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WritingMode {
    #[default]
    HorizontalTb,
    VerticalRl,
    VerticalLr,
    SidewaysRl,
    SidewaysLr,
}

impl WritingMode {
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "vertical-rl" => WritingMode::VerticalRl,
            "vertical-lr" => WritingMode::VerticalLr,
            "sideways-rl" => WritingMode::SidewaysRl,
            "sideways-lr" => WritingMode::SidewaysLr,
            _ => WritingMode::HorizontalTb,
        }
    }

    /// True pokud je text orientovan vertikalne (vertical-* nebo sideways-*).
    pub fn is_vertical(self) -> bool {
        !matches!(self, WritingMode::HorizontalTb)
    }
}

/// Aplikuje default styles per tag (browser user-agent stylesheet).
/// Inspirovano Chrome/Firefox UA stylesheet (margin/padding em-based).
fn apply_default_tag_styles(bx: &mut LayoutBox, tag: &str) {
    // Body default margin 8px (Chrome/Firefox).
    if tag == "body" {
        if bx.margin_top.is_none() { bx.margin_top = Some(8.0); }
        if bx.margin_right.is_none() { bx.margin_right = Some(8.0); }
        if bx.margin_bottom.is_none() { bx.margin_bottom = Some(8.0); }
        if bx.margin_left.is_none() { bx.margin_left = Some(8.0); }
        bx.line_height = 1.2;
        return;
    }
    match tag {
        // Headings: font-size + margin top/bottom em-based.
        "h1" => { bx.font_size = 32.0; bx.bold = true; bx.font_weight = 700;
                  bx.margin_top = bx.margin_top.or(Some(21.44));   // 0.67em * 32
                  bx.margin_bottom = bx.margin_bottom.or(Some(21.44)); }
        "h2" => { bx.font_size = 24.0; bx.bold = true; bx.font_weight = 700;
                  bx.margin_top = bx.margin_top.or(Some(19.92));   // 0.83em * 24
                  bx.margin_bottom = bx.margin_bottom.or(Some(19.92)); }
        "h3" => { bx.font_size = 18.72; bx.bold = true; bx.font_weight = 700;
                  bx.margin_top = bx.margin_top.or(Some(18.72));   // 1em
                  bx.margin_bottom = bx.margin_bottom.or(Some(18.72)); }
        "h4" => { bx.font_size = 16.0; bx.bold = true; bx.font_weight = 700;
                  bx.margin_top = bx.margin_top.or(Some(21.28));   // 1.33em
                  bx.margin_bottom = bx.margin_bottom.or(Some(21.28)); }
        "h5" => { bx.font_size = 13.28; bx.bold = true; bx.font_weight = 700;
                  bx.margin_top = bx.margin_top.or(Some(22.18));   // 1.67em
                  bx.margin_bottom = bx.margin_bottom.or(Some(22.18)); }
        "h6" => { bx.font_size = 10.72; bx.bold = true; bx.font_weight = 700;
                  bx.margin_top = bx.margin_top.or(Some(24.94));   // 2.33em
                  bx.margin_bottom = bx.margin_bottom.or(Some(24.94)); }
        "p" => {
            // 1em top/bottom margin (= 16px default).
            bx.margin_top = bx.margin_top.or(Some(16.0));
            bx.margin_bottom = bx.margin_bottom.or(Some(16.0));
        }
        "b" | "strong" => { bx.bold = true; bx.font_weight = 700; }
        "i" | "em" | "cite" | "var" | "address" | "dfn" => { bx.italic = true; }
        "u" | "ins" => { bx.text_underline = true; }
        "s" | "strike" | "del" => { /* line-through render TBD */ }
        "a" => {
            // Default: color blue + underline.
            if bx.text_color.is_none() {
                bx.text_color = Some([0, 0, 238, 255]);
            }
            bx.text_underline = true;
        }
        "ul" | "ol" => {
            // CSS UA stylesheet padding-inline-start: 40px - ale jen kdyz
            // ul/ol drzi default block/list-item layout. Site casto dela
            // `nav ul { display: flex; padding: 0 }` reset - pokud explicitni
            // display je flex/grid/inline-*, UA padding NE-aplikujeme (children
            // by se posunuly o 40 px doprava, viz mileneckaseznamka.cz menu).
            // Chrome dela stejne - flex container ma marker outside, neuvazuje
            // padding-inline-start pro layout marker space.
            if matches!(bx.display, Display::Block | Display::ListItem) {
                bx.padding_left = bx.padding_left.or(Some(40.0));
            }
            bx.margin_top = bx.margin_top.or(Some(16.0));
            bx.margin_bottom = bx.margin_bottom.or(Some(16.0));
        }
        "li" => { /* list-item display */ }
        "blockquote" => {
            bx.margin_top = bx.margin_top.or(Some(16.0));
            bx.margin_bottom = bx.margin_bottom.or(Some(16.0));
            bx.margin_left = bx.margin_left.or(Some(40.0));
            bx.margin_right = bx.margin_right.or(Some(40.0));
            // Visual default - light yellow tint (specific kontextu to oddeluje).
            // Mozno page CSS prepise pres background-color.
        }
        "pre" => {
            bx.margin_top = bx.margin_top.or(Some(16.0));
            bx.margin_bottom = bx.margin_bottom.or(Some(16.0));
            // Monospace by-implication, zatim default.
        }
        "code" | "kbd" | "samp" | "tt" => {
            // Light gray bg + monospace look. Pseudo-padding pres prirazeni padding
            // (true inline-paint by potreboval line-box-aware paint).
            if bx.bg_color.is_none() { bx.bg_color = Some([240, 240, 245, 255]); }
            if bx.text_color.is_none() { bx.text_color = Some([200, 50, 100, 255]); }
            if bx.border_radius == 0.0 { bx.border_radius = 3.0; }
            // Padding "vizualni" - vlozi 2px prostor mezi text a bg-rect.
            if bx.padding_left.is_none() { bx.padding_left = Some(4.0); }
            if bx.padding_right.is_none() { bx.padding_right = Some(4.0); }
        }
        "mark" => {
            // CSS UA: yellow bg + black text.
            if bx.bg_color.is_none() { bx.bg_color = Some([255, 240, 100, 255]); }
            if bx.text_color.is_none() { bx.text_color = Some([0, 0, 0, 255]); }
            if bx.border_radius == 0.0 { bx.border_radius = 2.0; }
            if bx.padding_left.is_none() { bx.padding_left = Some(2.0); }
            if bx.padding_right.is_none() { bx.padding_right = Some(2.0); }
        }
        "th" => {
            // Table header: bold + center text.
            bx.bold = true; bx.font_weight = 700;
            if bx.text_align == TextAlign::Left { bx.text_align = TextAlign::Center; }
            // Cells flex-grow=1 default - bez explicit width se rovnomerne
            // rozdistribuuji na sirku tabulky (CSS3 auto table layout aproximace).
            if bx.flex_grow == 0.0 { bx.flex_grow = 1.0; }
        }
        "td" => {
            if bx.flex_grow == 0.0 { bx.flex_grow = 1.0; }
        }
        "tr" => { /* table row, layout pres flex alias */ }
        "small" => {
            // CSS UA: smaller font (0.83em).
            bx.font_size *= 0.83;
        }
        "big" => {
            bx.font_size *= 1.17;
        }
        "sub" | "sup" => {
            // Smaller + offset (full impl by potreboval vertical-align).
            bx.font_size *= 0.75;
        }
        "hr" => {
            bx.border_top_width = Some(1.0);
            bx.border_color = Some([200, 200, 200, 255]);
            bx.margin_top = bx.margin_top.or(Some(8.0));
            bx.margin_bottom = bx.margin_bottom.or(Some(8.0));
        }
        "button" => {
            // Default browser button: padding, border, bg gray, rounded.
            if bx.padding == 0.0 && bx.padding_top.is_none() {
                bx.padding_top = Some(2.0);
                bx.padding_bottom = Some(3.0);
                bx.padding_left = Some(8.0);
                bx.padding_right = Some(8.0);
            }
            bx.border_width = bx.border_width.max(1.0);
            if bx.border_color.is_none() { bx.border_color = Some([118, 118, 118, 255]); }
            if bx.bg_color.is_none() { bx.bg_color = Some([239, 239, 239, 255]); }
        }
        "input" => {
            // Default border + light gray bg pro text inputs.
            let typ = bx.node.as_ref().and_then(|n| n.attr("type")).unwrap_or_else(|| "text".to_string()).to_lowercase();
            match typ.as_str() {
                "text" | "email" | "password" | "url" | "tel" | "search" | "number" => {
                    if bx.padding == 0.0 && bx.padding_top.is_none() {
                        bx.padding_top = Some(1.0);
                        bx.padding_bottom = Some(1.0);
                        bx.padding_left = Some(2.0);
                        bx.padding_right = Some(2.0);
                    }
                    if bx.border_width == 0.0 { bx.border_width = 1.0; }
                    if bx.border_color.is_none() { bx.border_color = Some([118, 118, 118, 255]); }
                    if bx.bg_color.is_none() { bx.bg_color = Some([255, 255, 255, 255]); }
                    if bx.rect.height == 0.0 { bx.rect.height = 21.0; }
                    if bx.rect.width == 0.0 { bx.rect.width = 154.0; }
                }
                _ => {}
            }
        }
        "table" => {
            bx.border_width = bx.border_width.max(0.0);
            bx.margin_top = bx.margin_top.or(Some(0.0));
        }
        "fieldset" => {
            bx.padding_top = bx.padding_top.or(Some(8.0));
            bx.padding_bottom = bx.padding_bottom.or(Some(8.0));
            bx.padding_left = bx.padding_left.or(Some(8.0));
            bx.padding_right = bx.padding_right.or(Some(8.0));
            bx.margin_left = bx.margin_left.or(Some(2.0));
            bx.margin_right = bx.margin_right.or(Some(2.0));
            bx.border_width = bx.border_width.max(2.0);
            if bx.border_color.is_none() { bx.border_color = Some([192, 192, 192, 255]); }
        }
        "canvas" => {
            // CSS default: 300x150 px (HTML spec)
            if bx.rect.width == 0.0 { bx.rect.width = 300.0; }
            if bx.rect.height == 0.0 { bx.rect.height = 150.0; }
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
        // Hidden tagy - obsah/atributy se nerendruju, jen processing.
        // <script> / <style> obsah byva extrahovan separatne (JS/CSS), bez
        // display:none se source kod renderoval jako text v body.
        "script" | "style" | "head" | "meta" | "title" | "link" | "base"
        | "noscript" | "template" | "param" | "source" | "track"
            => Display::None,
        "html" | "body" | "div" | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
        | "ul" | "ol" | "header" | "footer" | "main" | "section" | "article"
        | "nav" | "aside" | "form" | "blockquote"
        | "pre" | "hr" | "figure" | "figcaption"
            => Display::Block,
        "li" => Display::ListItem,
        // Table tagy: simulate pres flex (tr = flex-row container, td = flex-item).
        // Pravy table layout je TODO. Tahle aproximace ale dela cells vedle sebe.
        "table" => Display::Table,
        "tr" => Display::TableRow,
        "td" | "th" => Display::TableCell,
        "thead" | "tbody" | "tfoot" => Display::Block,
        "caption" => Display::TableCaption,
        "span" | "a" | "em" | "strong" | "b" | "i" | "u" | "code" | "small"
        | "br" | "img" | "input" | "label" | "button" | "select" | "textarea"
        | "canvas" | "svg" | "mark" | "kbd" | "samp" | "var" | "sub" | "sup"
        | "abbr" | "cite" | "q" | "s" | "del" | "ins" | "time" | "data"
        | "picture" | "video" | "audio" | "iframe" | "object" | "embed"
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
    /// Hash style entry tohoto uzlu (cascade) + DOM children pointer hashes.
    /// Pri rebuild layout pokud predchozi LayoutBox pro stejny node ma stejny
    /// fingerprint, lze celou subtree zkopirovat misto re-build (per-element
    /// cache). Hodnota 0 = ne-hashed nebo invalid.
    pub fingerprint: u64,
    pub rect: Rect,
    /// Pri position: sticky uchovava puvodni rect.y po layout pass.
    /// apply_sticky modifikuje rect.y per scroll - bez tohoto pole by druhy
    /// pruchod cetl modifikovany rect.y a interpretoval ho jako original.
    pub sticky_original_y: Option<f32>,
    /// Animacni baseline rect (pred apply_paint_animations). Soft path
    /// (cache hit + animuje width/height/left/top) mutuje `rect` per frame -
    /// bez baseline by druhy frame cetl uz mutovany rect a akumuloval offsets.
    /// Reset na None pri kazdem layout rebuild (cache miss).
    pub anim_baseline: Option<Rect>,
    pub display: Display,
    pub bg_color: Option<[u8; 4]>,   // RGBA
    pub text_color: Option<[u8; 4]>,
    pub text: Option<String>,
    pub tag: Option<String>,
    pub children: Vec<LayoutBox>,
    pub padding: f32,
    /// Asymmetric padding (None = pouzij `padding`).
    pub padding_top: Option<f32>,
    pub padding_right: Option<f32>,
    pub padding_bottom: Option<f32>,
    pub padding_left: Option<f32>,
    /// Asymmetric margin
    pub margin_top: Option<f32>,
    pub margin_right: Option<f32>,
    pub margin_bottom: Option<f32>,
    pub margin_left: Option<f32>,
    /// Margin pct (0..1) - pri Some, resolve proti containing block (NE pre-resolved px).
    pub margin_top_pct: Option<f32>,
    pub margin_right_pct: Option<f32>,
    pub margin_bottom_pct: Option<f32>,
    pub margin_left_pct: Option<f32>,
    /// Explicit width/height pct (0..1) - pri Some, ulozit puvodni percent.
    pub width_pct: Option<f32>,
    pub height_pct: Option<f32>,
    /// margin-*: auto flagy (absorbuji free space, CSS Margin auto).
    pub margin_top_auto: bool,
    pub margin_right_auto: bool,
    pub margin_bottom_auto: bool,
    pub margin_left_auto: bool,
    /// box-sizing: "content-box" (default) | "border-box".
    pub box_sizing: BoxSizing,
    pub margin: f32,
    /// CSS border-style: none (default) | solid | dashed | dotted | double | ...
    /// "none" = neukreslit border bez ohledu na width/color (CSS UA default).
    pub border_style: String,
    pub border_width: f32,
    /// Per-side border width (None = use border_width). CSS Backgrounds L3.
    pub border_top_width: Option<f32>,
    pub border_right_width: Option<f32>,
    pub border_bottom_width: Option<f32>,
    pub border_left_width: Option<f32>,
    pub border_color: Option<[u8; 4]>,
    pub font_size: f32,
    /// True kdyz font_size byl explicitly set z cascade (ne default).
    /// Pouzite v inline inheritance: parent fs propaguje na inline child JEN
    /// kdyz child sam nema explicit fs - drive resilo `(fs - 16.0).abs() < 0.001`
    /// sentinel test ktery selhal pri user CSS `font-size: 16px`.
    pub font_size_explicit: bool,
    pub text_align: TextAlign,
    pub bold: bool,
    /// CSS font-weight 1..1000. Default 400 (normal). Bold = 700 (>= 600 alias).
    /// Pri @font-face s vice weight variants atlas hleda nejblizsi match per
    /// CSS Fonts L4 spec - hot path: 500 prefer 500 > 400 > 300 > 600.
    pub font_weight: u32,
    /// font-style: italic / oblique. Ramp pres skew transform v rendereru
    /// (real italic font variant je TODO).
    pub italic: bool,
    pub border_radius: f32,
    pub line_height: f32,
    /// True kdyz line_height byl explicitly set z cascade (analogicky
    /// font_size_explicit). Drive `(lh - 1.2).abs() < 0.001` sentinel test
    /// selhal pri user CSS `line-height: 1.2`.
    pub line_height_explicit: bool,
    pub position: Position,
    /// Top/right/bottom/left offsety pro positioned elements (None = auto).
    pub offset_top: Option<f32>,
    pub offset_right: Option<f32>,
    pub offset_bottom: Option<f32>,
    pub offset_left: Option<f32>,
    /// CSS z-index integer (None = auto). Aplikuje se pri position != static.
    /// Pri stejnem parent paint sortuje children podle z (vyssi vise).
    pub z_index: Option<i32>,
    /// Opacity 0..1
    pub opacity: f32,
    /// Underline / strikethrough flagy
    pub text_underline: bool,
    pub text_strikethrough: bool,
    /// Gradient pozadi (linear/radial/conic) + barevne stops.
    pub bg_gradient: Option<BgGradient>,
    /// CSS Filter Effects - chain of color matrix operations + opacity + drop-shadow.
    pub filter: Vec<FilterOp>,
    /// CSS backdrop-filter - filter aplikovany na scenu za elementem.
    pub backdrop_filter: Vec<FilterOp>,
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
    /// text-decoration-color
    pub text_decoration_color: Option<[u8; 4]>,
    /// text-decoration-style: solid (default) | double | dotted | dashed | wavy
    pub text_decoration_style: String,
    /// text-decoration-thickness (px)
    pub text_decoration_thickness: f32,
    /// text-underline-offset (px)
    pub text_underline_offset: f32,
    /// text-indent (px)
    pub text_indent: f32,
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
    /// mask-image: url() / linear-gradient(...)
    pub mask_image: Option<String>,
    /// shape-outside: circle()/ellipse()/inset()/polygon()/url()
    pub shape_outside: Option<String>,
    /// direction: ltr (default) | rtl
    pub direction: Direction,
    /// writing-mode: horizontal-tb (default) | vertical-rl | vertical-lr
    pub writing_mode: WritingMode,
    /// content-visibility: visible (default) | auto | hidden
    pub content_visibility: String,
    /// contain-intrinsic-size: <length>
    pub contain_intrinsic_size: f32,
    /// counter-reset: name [n] [, name n]
    pub counter_reset: Vec<(String, i32)>,
    /// counter-increment: name [n] [, name n]
    pub counter_increment: Vec<(String, i32)>,
    /// backface-visibility: visible (default) | hidden
    pub backface_visibility: String,
    /// transform-style: flat (default) | preserve-3d
    pub transform_style: String,
    /// perspective: <length> | none
    pub perspective: Option<f32>,
    /// text-emphasis: <style> <color>
    pub text_emphasis: String,
    /// will-change: <prop list>
    pub will_change: String,
    /// isolation: auto | isolate
    pub isolation: String,
    /// mix-blend-mode: normal | multiply | screen | overlay | darken | lighten | ...
    pub mix_blend_mode: String,
    /// pointer-events: auto | none
    pub pointer_events: PointerEvents,
    /// user-select: auto | none | text | all
    pub user_select: String,
    /// caret-color
    pub caret_color: Option<[u8; 4]>,
    /// resize: none | both | horizontal | vertical
    pub resize: String,
    /// touch-action: auto | none | pan-x | pan-y | manipulation
    pub touch_action: String,
    /// hyphens: none | manual | auto
    pub hyphens: String,
    /// tab-size: <integer> | <length>
    pub tab_size: f32,
    /// word-break: normal | break-all | keep-all
    pub word_break: String,
    /// overflow-wrap: normal | break-word | anywhere
    pub overflow_wrap: String,
    /// text-wrap: wrap | nowrap | balance | pretty
    pub text_wrap: String,
    /// text-align-last: auto | left | right | center | justify
    pub text_align_last: String,
    /// list-style-type: disc | circle | square | decimal | none | ...
    pub list_style_type: String,
    /// list-style-position: outside | inside
    pub list_style_position: String,
    /// list-style-image: url(...) | none
    pub list_style_image: Option<String>,
    /// font-stretch
    pub font_stretch: String,
    /// font-variant
    pub font_variant: String,
    /// font-size-adjust
    pub font_size_adjust: String,
    /// font-feature-settings
    pub font_feature_settings: String,
    /// font-variation-settings
    pub font_variation_settings: String,
    /// font-display (z @font-face, ale lze ulozit i na element)
    pub font_display: String,
    /// text-orientation: mixed | upright | sideways
    pub text_orientation: String,
    /// text-combine-upright
    pub text_combine_upright: String,
    /// ruby-position / ruby-align
    pub ruby_position: String,
    pub ruby_align: String,
    /// quotes
    pub quotes: String,
    /// outline (= border outside box)
    pub outline_width: f32,
    pub outline_style: String,
    pub outline_color: Option<[u8; 4]>,
    pub outline_offset: f32,
    /// margin-trim
    pub margin_trim: String,
    /// CSS Anchor Positioning L1 - anchor-name (e.g. "--my-anchor")
    pub anchor_name: String,
    /// position-anchor: <name>
    pub position_anchor: String,
    /// inset-area (top / left / center / start / end / span-* keywords)
    pub inset_area: String,
    /// CSS Scroll-driven Animations - animation-timeline
    pub animation_timeline: String,
    /// scroll-timeline-name / scroll-timeline-axis
    pub scroll_timeline_name: String,
    pub scroll_timeline_axis: String,
    /// view-timeline-name / view-timeline-axis / view-timeline-inset
    pub view_timeline_name: String,
    pub view_timeline_axis: String,
    pub view_timeline_inset: String,
    /// CSS View Transitions L1 - view-transition-name
    pub view_transition_name: String,
    /// CSS Containment L3 - container-type (uz mam string), pridam container
    pub container_type: String,
    pub container_name: String,
    /// page-break-before / -after / -inside
    pub page_break_before: String,
    pub page_break_after: String,
    pub page_break_inside: String,
    /// break-before / -after / -inside (CSS Fragmentation L3)
    pub break_before: String,
    pub break_after: String,
    pub break_inside: String,
    pub orphans: i32,
    pub widows: i32,
    /// counter-set (CSS L3)
    pub counter_set: Vec<(String, i32)>,
    /// print-color-adjust / forced-color-adjust
    pub print_color_adjust: String,
    pub forced_color_adjust: String,
    /// font-synthesis variants
    pub font_synthesis: String,
    pub font_kerning: String,
    pub font_language_override: String,
    pub font_optical_sizing: String,
    pub font_smooth: String,
    /// CSS Text L4
    pub white_space_collapse: String,
    pub text_spacing_trim: String,
    pub text_size_adjust: String,
    pub line_height_step: f32,
    /// math-style / math-depth (CSS MathML / Math L1)
    pub math_style: String,
    pub math_depth: String,
    /// CSS speech (aural)
    pub speak: String,
    pub speak_as: String,
    /// CSS Generated content L3
    pub bookmark_label: String,
    pub bookmark_level: String,
    pub bookmark_state: String,
    pub string_set: String,
    /// CSS Logical Properties: float-block, clear-block (rare)
    pub float_value: String,
    pub clear_value: String,
    /// Image fitting (object-fit / object-position)
    pub object_fit: ObjectFit,
    pub object_position: String,
    /// Mix blend / background blend
    pub background_blend_mode: String,
    /// Image rendering hints
    pub image_rendering: ImageRendering,
    /// CSS Sizing L4 - aspect-ratio uz mam
    /// CSS sizing constraints - parsed CssLength (Auto / None / Px / Percent / ...).
    /// `resolve(ctx)` na callsite s explicit parent_size + font_size.
    /// Driv `*_v: String` + parse_length(s) at every use site - error-prone
    /// (default parent_size=16 silently converted "100%" -> 16 px).
    pub min_width: CssLength,
    pub max_width: CssLength,
    pub min_height: CssLength,
    pub max_height: CssLength,
    /// Explicitni CSS width (None = auto / neparsovano).
    pub explicit_width: Option<f32>,
    /// Explicitni CSS height (None = auto / neparsovano).
    pub explicit_height: Option<f32>,
    /// Flex properties (CSS Flexbox L1)
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    /// align-self per-item (override align-items): Auto = use parent's align-items.
    pub align_self: AlignSelf,
    /// justify-self per-item (grid): auto/start/end/center/stretch.
    /// (Drive: String. Migrace incremental - justify-self ma stejne variants jako
    /// align-self, ale s semantikou justify - reuse AlignSelf enum.)
    pub justify_self: AlignSelf,
    /// justify-items pro grid container (default pro children). String zatim.
    pub justify_items: String,
    /// grid-row-start: 1-based line, 0 = auto.
    pub grid_row_start: i32,
    pub grid_row_end: i32,
    pub grid_column_start: i32,
    pub grid_column_end: i32,
    /// grid-row-span / grid-column-span (alternativni k start/end).
    pub grid_row_span: i32,
    pub grid_column_span: i32,
    pub align_content: AlignContent,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: String,
    pub row_gap: f32,
    pub column_gap: f32,
    /// Percent gap (0..1) - pri Some, resolve proti container content size.
    pub row_gap_pct: Option<f32>,
    pub column_gap_pct: Option<f32>,
    /// CSS Logical Properties continued
    pub block_size_v: String,
    pub inline_size_v: String,
    /// table-layout, border-collapse, border-spacing, caption-side, empty-cells
    pub table_layout: TableLayout,
    pub border_collapse: BorderCollapse,
    pub border_spacing: String,
    pub caption_side: String,
    pub empty_cells: String,
    /// vertical-align
    pub vertical_align: String,
    /// CSS Backgrounds L4
    pub background_origin_v: String,
    pub background_clip_v: String,
    /// CSS Backgrounds L3 - border-image source URL.
    pub border_image_source: Option<String>,
    /// border-image-slice: top right bottom left (procenta nebo cisla).
    pub border_image_slice: [f32; 4],
    /// border-image-width: top right bottom left (px nebo numbers = multiplier).
    pub border_image_width: [f32; 4],
    /// border-image-repeat: stretch (default) | repeat | round | space (per axis).
    pub border_image_repeat: String,
    /// CSS Text Decor L4 - text-emphasis: <style> <color>.
    pub text_emphasis_style: String,
    pub text_emphasis_color: Option<[u8; 4]>,
    /// CSS Text Decor L3 - text-decoration-skip-ink: auto | none | all.
    pub text_decoration_skip_ink: String,
    /// CSS Forms L1 - field-sizing: fixed (default) | content.
    pub field_sizing: String,
    /// CSS Animations L2 - interpolate-size: numeric-only (default) | allow-keywords.
    pub interpolate_size: String,
    /// CSS Multi-column Layout L1 - column-count: 1+ (auto = 1 default).
    /// Pri > 1: layout_block rozdeli flow children rovnomerne do N sloupcu.
    pub column_count: u32,
    /// column-gap mezi sloupci (px).
    pub column_gap_multicol: f32,
    /// column-rule: width style color - separator mezi sloupci.
    pub column_rule_width: f32,
    pub column_rule_color: [u8; 4],
    pub column_rule_style: String,
    /// CSS Grid L1/L2 - grid-template-columns/rows raw string (parser-only zatim).
    /// Format: "<line-name>? <track-size> <line-name>? ..." napr. "[start] 1fr [mid] 2fr [end]".
    pub grid_template_columns: String,
    pub grid_template_rows: String,
    /// grid-template-areas: ASCII art layout per row.
    pub grid_template_areas: String,
    /// grid-area / grid-column / grid-row positioning string.
    pub grid_area: String,
    pub grid_column: String,
    pub grid_row: String,
    pub grid_auto_columns: String,
    pub grid_auto_rows: String,
    pub grid_auto_flow: String,
    pub shape_margin: f32,
    pub shape_image_threshold: f32,
    /// CSS Overflow L4 - scrollbar-gutter: auto | stable [both-edges].
    pub scrollbar_gutter: String,
    /// CSS SVG markers - marker-start/mid/end (SVG paths).
    pub marker_start: String,
    pub marker_mid: String,
    pub marker_end: String,
    /// CSS Backgrounds L4 - background-position-x / -y separately.
    pub background_position_x: String,
    pub background_position_y: String,
    /// CSS Images L3 - image-orientation: from-image | none | <angle>.
    pub image_orientation: String,
    /// CSS Text L4 - hyphenate-character / hyphenate-limit-chars.
    pub hyphenate_character: String,
    pub hyphenate_limit_chars: String,
    /// CSS Inline L3 - text-box-trim / text-box-edge.
    pub text_box_trim: String,
    pub text_box_edge: String,
    /// CSS Logical Props L1 - inset shorthand (top right bottom left).
    pub inset: [Option<f32>; 4],
    /// CSS Pseudo L4 - dialog / popover anchor positioning extensions
    pub anchor_default: String,
    /// CSS Position L3 - position-area: top, top-left, ...
    pub position_area: String,
    /// CSS Position L4 - position-try-fallbacks: a, b, c.
    pub position_try_fallbacks: String,
    /// CSS Text L4 - text-spacing.
    pub text_spacing: String,
    /// CSS Text L4 - text-autospace.
    pub text_autospace: String,
    /// CSS Inline L3 - vertical-align extends.
    pub initial_letter: String,
    pub ruby_overhang: String,
    pub ruby_merge: String,
    /// CSS Math L1 - MathML props.
    pub math_shift: String,
    /// CSS Transitions L2 - transition-behavior: normal | allow-discrete.
    pub transition_behavior: String,
    /// CSS Animations L2 - animation-composition: replace | add | accumulate.
    pub animation_composition: String,
    /// CSS Color L4 - color-interpolation, color-interpolation-filters.
    pub color_interpolation: String,
    /// CSS Lists L3 - lazy lookup pres animation-name pak counter manipulation
    pub lazy_counters: String,
    /// CSS Scroll-Driven Animations L1 (extras nad existujici impl)
    pub timeline_scope: String,
    pub animation_range_start: String,
    pub animation_range_end: String,
    /// CSS Carousels - scroll-marker / scroll-button
    pub scroll_marker_group: String,
    /// CSS Filter Effects L2 - filter alternative names.
    pub backdrop_filter_string: String,
    /// CSS Containment L2 - contain-intrinsic-block-size / contain-intrinsic-inline-size.
    pub contain_intrinsic_block_size: f32,
    pub contain_intrinsic_inline_size: f32,
    /// CSS Anchor L1 - anchor-scope: <name> | none | all.
    pub anchor_scope: String,
    /// CSS Position L4 - position-visibility: always | anchors-visible | no-overflow.
    pub position_visibility: String,
    /// CSS Display L4 - reading-flow: normal | flex-visual | flex-flow | grid-rows | grid-columns | grid-order.
    pub reading_flow: String,
    /// CSS Display L4 - reading-order.
    pub reading_order: String,
    /// CSS Filter Effects L1 - filter-tagged composite output.
    pub composite_op: String,
    /// CSS UI L4 - cursor advanced (uz hotov pro keywords; pridame zoom-in/out).
    pub cursor_extra: String,
    /// CSS Lists L3 - list-style-position: outside (default) | inside.
    pub list_style_position_v: String,
    /// CSS UI L4 - resize: none | both | horizontal | vertical | block | inline.
    pub resize_v: String,
    /// CSS Speech L1 - voice-family.
    pub voice_family: String,
    /// Box shadow: (offset_x, offset_y, blur, spread, color)
    /// (offset_x, offset_y, blur, spread, color, inset)
    pub box_shadow: Option<(f32, f32, f32, f32, [u8; 4], bool)>,
    /// Transform: simple translate/rotate/scale
    pub transform: Option<TransformOp>,
    /// Multi-op transform chain (transform: A() B() C()).
    pub transforms: Vec<TransformOp>,
    /// Overflow: hidden/scroll/visible/auto
    pub overflow_hidden: bool,
    /// overflow-x value (string - "visible"/"hidden"/"scroll"/"auto")
    pub overflow_x: Overflow,
    /// overflow-y value
    pub overflow_y: Overflow,
    /// scrollbar-width numericky (px).
    pub scrollbar_size: f32,
    /// White-space: nowrap zachazi text jako jeden radek
    pub white_space_nowrap: bool,
    /// Cursor (jen string - real impl pres OS cursor)
    pub cursor: Option<String>,
    /// Image src URL (z img tagu).
    pub image_src: Option<String>,
    /// Reference na puvodni DOM node (pro hit test -> events).
    pub node: Option<Rc<Node>>,
    /// ::placeholder - barva textu pro placeholder (input/textarea).
    pub placeholder_color: Option<[u8; 4]>,
    /// ::selection - barva pozadi vybrane oblasti textu.
    pub selection_bg: Option<[u8; 4]>,
    /// ::selection - barva textu vybrane oblasti.
    pub selection_color: Option<[u8; 4]>,
    /// Taffy compliance mode: skip default 20px height for empty leaf divs.
    /// Set true u boxu pochazejicich z taffy fixture parser.
    pub taffy_mode: bool,
    /// Pseudo-flex: byl Block, ale heuristika ho zmenila na Flex (align-items=baseline).
    /// Pri baseline calc pak pouzij synth (block) baseline misto first-child.
    pub pseudo_flex: bool,
    /// Intrinsic mode marker (pre-pass). Pri Flex: skip shrink (preserve max-content).
    pub taffy_intrinsic_mode: bool,
}

impl LayoutBox {
    pub fn new() -> Self {
        LayoutBox {
            fingerprint: 0,
            rect: Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
            sticky_original_y: None,
            anim_baseline: None,
            display: Display::Block,
            bg_color: None,
            text_color: None,
            text: None,
            tag: None,
            children: Vec::new(),
            padding: 0.0,
            padding_top: None,
            padding_right: None,
            padding_bottom: None,
            padding_left: None,
            margin_top: None,
            margin_right: None,
            margin_bottom: None,
            margin_left: None,
            margin_top_pct: None,
            margin_right_pct: None,
            margin_bottom_pct: None,
            margin_left_pct: None,
            width_pct: None,
            height_pct: None,
            margin_top_auto: false,
            margin_right_auto: false,
            margin_bottom_auto: false,
            margin_left_auto: false,
            box_sizing: BoxSizing::ContentBox,
            margin: 0.0,
            border_style: String::new(), // empty = ne-vykresleny (CSS spec none)
            border_width: 0.0,
            border_top_width: None,
            border_right_width: None,
            border_bottom_width: None,
            border_left_width: None,
            border_color: None,
            font_size: 16.0,
            font_size_explicit: false,
            text_align: TextAlign::Left,
            bold: false,
            font_weight: 400,
            italic: false,
            border_radius: 0.0,
            line_height: 1.2,
            line_height_explicit: false,
            position: Position::Static,
            offset_top: None,
            offset_right: None,
            offset_bottom: None,
            offset_left: None,
            z_index: None,
            opacity: 1.0,
            text_underline: false,
            text_strikethrough: false,
            overflow_hidden: false,
            overflow_x: Overflow::Visible,
            overflow_y: Overflow::Visible,
            scrollbar_size: 0.0,
            white_space_nowrap: false,
            cursor: None,
            bg_gradient: None,
            filter: Vec::new(),
            backdrop_filter: Vec::new(),
            backgrounds: Vec::new(),
            clip_path: None,
            text_shadow: None,
            text_transform: TextTransform::None,
            font_family: String::new(),
            text_decoration_color: None,
            text_decoration_style: String::new(),
            text_decoration_thickness: 1.0,
            text_underline_offset: 0.0,
            text_indent: 0.0,
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
            mask_image: None,
            shape_outside: None,
            direction: Direction::default(),
            writing_mode: WritingMode::default(),
            content_visibility: String::new(),
            contain_intrinsic_size: 0.0,
            counter_reset: Vec::new(),
            counter_increment: Vec::new(),
            backface_visibility: String::new(),
            transform_style: String::new(),
            perspective: None,
            text_emphasis: String::new(),
            will_change: String::new(),
            isolation: String::new(),
            mix_blend_mode: String::new(),
            pointer_events: PointerEvents::default(),
            user_select: String::new(),
            caret_color: None,
            resize: String::new(),
            touch_action: String::new(),
            hyphens: String::new(),
            tab_size: 8.0,
            word_break: String::new(),
            overflow_wrap: String::new(),
            text_wrap: String::new(),
            text_align_last: String::new(),
            list_style_type: String::new(),
            list_style_position: String::new(),
            list_style_image: None,
            font_stretch: String::new(),
            font_variant: String::new(),
            font_size_adjust: String::new(),
            font_feature_settings: String::new(),
            font_variation_settings: String::new(),
            font_display: String::new(),
            text_orientation: String::new(),
            text_combine_upright: String::new(),
            ruby_position: String::new(),
            ruby_align: String::new(),
            quotes: String::new(),
            outline_width: 0.0,
            outline_style: String::new(),
            outline_color: None,
            outline_offset: 0.0,
            margin_trim: String::new(),
            anchor_name: String::new(),
            position_anchor: String::new(),
            inset_area: String::new(),
            animation_timeline: String::new(),
            scroll_timeline_name: String::new(),
            scroll_timeline_axis: String::new(),
            view_timeline_name: String::new(),
            view_timeline_axis: String::new(),
            view_timeline_inset: String::new(),
            view_transition_name: String::new(),
            container_type: String::new(),
            container_name: String::new(),
            page_break_before: String::new(),
            page_break_after: String::new(),
            page_break_inside: String::new(),
            break_before: String::new(),
            break_after: String::new(),
            break_inside: String::new(),
            orphans: 2,
            widows: 2,
            counter_set: Vec::new(),
            print_color_adjust: String::new(),
            forced_color_adjust: String::new(),
            font_synthesis: String::new(),
            font_kerning: String::new(),
            font_language_override: String::new(),
            font_optical_sizing: String::new(),
            font_smooth: String::new(),
            white_space_collapse: String::new(),
            text_spacing_trim: String::new(),
            text_size_adjust: String::new(),
            line_height_step: 0.0,
            math_style: String::new(),
            math_depth: String::new(),
            speak: String::new(),
            speak_as: String::new(),
            bookmark_label: String::new(),
            bookmark_level: String::new(),
            bookmark_state: String::new(),
            string_set: String::new(),
            float_value: String::new(),
            clear_value: String::new(),
            object_fit: ObjectFit::Fill,
            object_position: String::new(),
            background_blend_mode: String::new(),
            image_rendering: ImageRendering::Auto,
            min_width: CssLength::Auto,
            max_width: CssLength::None,
            min_height: CssLength::Auto,
            max_height: CssLength::None,
            explicit_width: None,
            explicit_height: None,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::NoWrap,
            justify_content: JustifyContent::FlexStart,
            // CSS default pre flex/grid container je `stretch`. AlignItems::Stretch
            // = same default. Drive empty String -> match v parse fell-through.
            align_items: AlignItems::Stretch,
            align_self: AlignSelf::Auto,
            justify_self: AlignSelf::Auto,
            justify_items: String::new(),
            grid_row_start: 0,
            grid_row_end: 0,
            grid_column_start: 0,
            grid_column_end: 0,
            grid_row_span: 0,
            grid_column_span: 0,
            align_content: AlignContent::Normal,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: String::new(),
            row_gap: 0.0,
            column_gap: 0.0,
            row_gap_pct: None,
            column_gap_pct: None,
            block_size_v: String::new(),
            inline_size_v: String::new(),
            table_layout: TableLayout::Auto,
            border_collapse: BorderCollapse::Separate,
            border_spacing: String::new(),
            caption_side: String::new(),
            empty_cells: String::new(),
            vertical_align: String::new(),
            background_origin_v: String::new(),
            background_clip_v: String::new(),
            border_image_source: None,
            border_image_slice: [0.0; 4],
            border_image_width: [1.0; 4],
            border_image_repeat: String::new(),
            text_emphasis_style: String::new(),
            text_emphasis_color: None,
            text_decoration_skip_ink: String::new(),
            field_sizing: String::new(),
            interpolate_size: String::new(),
            column_count: 1,
            column_gap_multicol: 16.0,
            column_rule_width: 0.0,
            column_rule_color: [128, 128, 128, 255],
            column_rule_style: String::new(),
            grid_template_columns: String::new(),
            grid_template_rows: String::new(),
            grid_template_areas: String::new(),
            grid_area: String::new(),
            grid_column: String::new(),
            grid_row: String::new(),
            grid_auto_columns: String::new(),
            grid_auto_rows: String::new(),
            grid_auto_flow: String::new(),
            shape_margin: 0.0,
            shape_image_threshold: 0.0,
            scrollbar_gutter: String::new(),
            marker_start: String::new(),
            marker_mid: String::new(),
            marker_end: String::new(),
            background_position_x: String::new(),
            background_position_y: String::new(),
            image_orientation: String::new(),
            hyphenate_character: String::new(),
            hyphenate_limit_chars: String::new(),
            text_box_trim: String::new(),
            text_box_edge: String::new(),
            inset: [None; 4],
            anchor_default: String::new(),
            position_area: String::new(),
            position_try_fallbacks: String::new(),
            text_spacing: String::new(),
            text_autospace: String::new(),
            initial_letter: String::new(),
            ruby_overhang: String::new(),
            ruby_merge: String::new(),
            math_shift: String::new(),
            transition_behavior: String::new(),
            animation_composition: String::new(),
            color_interpolation: String::new(),
            lazy_counters: String::new(),
            timeline_scope: String::new(),
            animation_range_start: String::new(),
            animation_range_end: String::new(),
            scroll_marker_group: String::new(),
            backdrop_filter_string: String::new(),
            contain_intrinsic_block_size: 0.0,
            contain_intrinsic_inline_size: 0.0,
            anchor_scope: String::new(),
            position_visibility: String::new(),
            reading_flow: String::new(),
            reading_order: String::new(),
            composite_op: String::new(),
            cursor_extra: String::new(),
            list_style_position_v: String::new(),
            resize_v: String::new(),
            voice_family: String::new(),
            box_shadow: None,
            transform: None,
            transforms: Vec::new(),
            image_src: None,
            node: None,
            placeholder_color: None,
            selection_bg: None,
            selection_color: None,
            taffy_mode: false,
            pseudo_flex: false,
            taffy_intrinsic_mode: false,
        }
    }

    /// Hit test: vrati nejdetailnejsi (deepest) box obsahujici (x, y).
    pub fn hit_test(&self, x: f32, y: f32) -> Option<&LayoutBox> {
        // pointer-events: none -> element ignored pri hit test (vc deti pokud none nezruseno)
        if self.pointer_events.is_none() {
            return None;
        }
        // visibility: hidden -> taky skip hit test
        if self.opacity == 0.0 {
            return None;
        }
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
    layout_tree_with_pseudo_cached(root, style_map, pseudo_map, viewport_width, viewport_height, None)
}

/// Layout s per-element cache: pri opakovanem layoutu (pri animation/hover state
/// change) muzeme reuznat subtrees ze prev_root pokud jejich style + struct
/// fingerprint matches. Skip-uje style/struct rebuild na clean uzlech.
/// layout_dispatch (positioning) stale bezi cely - jen build je cached.
pub fn layout_tree_with_pseudo_cached(
    root: &Rc<Node>,
    style_map: &StyleMap,
    pseudo_map: &super::cascade::PseudoStyleMap,
    viewport_width: f32,
    viewport_height: f32,
    prev_root: Option<&LayoutBox>,
) -> LayoutBox {
    // Clear subtree hash memo - per-frame valid (style_map se mezi
    // frames mute pres apply_animations, takze cached hashes z prev frame
    // jsou stale). Within-frame still helps deduplikovat recursive volani.
    clear_subtree_hash_cache();
    let perf = std::env::var("PERF_DEBUG").is_ok();
    let perf_t = |label: &str, t: std::time::Instant| {
        if std::env::var("PERF_DEBUG").is_ok() {
            eprintln!("  [layout_tree::{}] {:.2} ms", label, t.elapsed().as_secs_f64() * 1000.0);
        }
    };
    // Build prev_node_map (node_ptr -> LayoutBox subtree) z prev_root.
    let _t = std::time::Instant::now();
    let mut prev_map: HashMap<usize, LayoutBox> = HashMap::new();
    if let Some(prev) = prev_root {
        collect_prev_boxes(prev, &mut prev_map);
    }
    perf_t("collect_prev_boxes", _t);
    let cache = if prev_map.is_empty() { None } else { Some(&prev_map) };
    // PERF: skip prvni "scrollbar detection" pass kdyz prev_root uz overflow
    // mel - rovnou pouz width - 15. Jinak by pri opetovnem rebuilu prelayoutoval
    // 2x. Detekce z prev: kdyz prev rect.width = viewport - 15 (presne match
    // s scrollbar adjustment), prev urcite mel overflow.
    const SCROLLBAR_W: f32 = 15.0;
    let prev_had_overflow = prev_root.as_ref()
        .map(|p| (p.rect.width - (viewport_width - SCROLLBAR_W)).abs() < 0.5)
        .unwrap_or(false);
    let initial_w = if prev_had_overflow && viewport_width > SCROLLBAR_W + 100.0 {
        viewport_width - SCROLLBAR_W
    } else {
        viewport_width
    };
    let _t = std::time::Instant::now();
    let mut layout_root = build_box_with_pseudo_cached(root, style_map, pseudo_map, cache);
    perf_t("build_box_with_pseudo_cached #1", _t);
    layout_root.rect.width = initial_w;
    layout_root.rect.height = viewport_height;
    let _t = std::time::Instant::now();
    layout_dispatch(&mut layout_root);
    perf_t("layout_dispatch #1", _t);
    // Detect overflow only kdyz jsme NEPOUZILI scrollbar adjustment uz na input.
    if !prev_had_overflow {
        let has_overflow = layout_root.children.iter()
            .filter_map(|c| if c.tag.as_deref() == Some("html") { Some(c) } else { None })
            .any(|html| html.rect.height > viewport_height
                || html.children.iter().any(|body| body.rect.height > viewport_height));
        if has_overflow && viewport_width > SCROLLBAR_W + 100.0 {
            if perf { eprintln!("  [layout_tree::scrollbar_repath] running second build+layout pass"); }
            let _t = std::time::Instant::now();
            layout_root = build_box_with_pseudo_cached(root, style_map, pseudo_map, cache);
            perf_t("build_box_with_pseudo_cached #2", _t);
            layout_root.rect.width = viewport_width - SCROLLBAR_W;
            layout_root.rect.height = viewport_height;
            let _t = std::time::Instant::now();
            layout_dispatch(&mut layout_root);
            perf_t("layout_dispatch #2", _t);
        }
    }
    let _t = std::time::Instant::now();
    let anchor_map = collect_anchors(&layout_root);
    perf_t("collect_anchors", _t);
    let _t = std::time::Instant::now();
    apply_anchor_positioning(&mut layout_root, &anchor_map);
    perf_t("apply_anchor_positioning", _t);
    let _t = std::time::Instant::now();
    apply_table_border_collapse(&mut layout_root, false);
    perf_t("apply_table_border_collapse", _t);
    layout_root
}

/// Walk prev LayoutBox tree, sber kazdy node_ptr -> LayoutBox subtree.
fn collect_prev_boxes(bx: &LayoutBox, map: &mut HashMap<usize, LayoutBox>) {
    if let Some(node) = &bx.node {
        let id = Rc::as_ptr(node) as usize;
        map.insert(id, bx.clone());
    }
    for ch in &bx.children {
        collect_prev_boxes(ch, map);
    }
}

// Vypocita subtree hash pres DOM walk + style entries + DOM children pointers.
// Pri cache check porovname s prev.fingerprint.
thread_local! {
    /// Memo cache pro compute_subtree_hash. Bez nej je hash O(N*depth)
    /// kvuli rekurzi do children pri kazdem volani. CLEAR pri zmene
    /// style_map / DOM mutace (cascade rebuild).
    static SUBTREE_HASH_CACHE: std::cell::RefCell<HashMap<usize, u64>>
        = std::cell::RefCell::new(HashMap::new());
    /// Image natural dims cache - src URL -> (natural_w, natural_h). Renderer
    /// populates pri load_image z ImageAtlas. Layout cte v flush_inline pro
    /// img/video replaced inline elementy aby spravne aplikoval max-width/height
    /// + aspect ratio. Bez tohoto img natural unknown -> default 100 fallback.
    pub(crate) static IMAGE_NATURAL_DIMS: std::cell::RefCell<HashMap<String, (f32, f32)>>
        = std::cell::RefCell::new(HashMap::new());
}

// Public API pro renderer: po load_image populate natural dims cache.
thread_local! {
    /// True pri kazdem set_image_natural_dims. Renderer cte + reset.
    /// Pri zmene cache: invalidate layout cache + trigger redraw (aby img
    /// s natural unknown -> default 100x100 v 1. pass re-layoutoval po load
    /// na real aspect 112x65 atd.).
    pub(crate) static IMAGE_NATURAL_DIMS_DIRTY: std::cell::Cell<bool>
        = std::cell::Cell::new(false);
}

pub fn set_image_natural_dims(src: &str, w: f32, h: f32) {
    IMAGE_NATURAL_DIMS.with(|c| {
        let mut m = c.borrow_mut();
        let changed = m.get(src).map(|&(ow, oh)| ow != w || oh != h).unwrap_or(true);
        m.insert(src.to_string(), (w, h));
        if changed {
            IMAGE_NATURAL_DIMS_DIRTY.with(|d| d.set(true));
        }
    });
}

/// Renderer cte tento flag per frame - pri true invalidate layout cache.
pub fn take_image_natural_dims_dirty() -> bool {
    IMAGE_NATURAL_DIMS_DIRTY.with(|d| {
        let v = d.get();
        d.set(false);
        v
    })
}

thread_local! {
    /// @font-face fonts registrovany renderem pres register_measure_font.
    /// measure_text_width_full lookup primary - aby measure pouzil STEJNY
    /// font co render. Bez tohoto Ubuntu Bold rasterizovan v atlas (sirsi
    /// glyphs), ale measure_text_width_full pouzival Times Bold (uzsi) ->
    /// cursor_x advance < real glyph width -> dalsi text prekrizen pres
    /// bold span.
    /// Key format: "<family>" / "<family>__bold__" / "<family>__italic__" / "<family>__bi__".
    pub(crate) static MEASURE_FONTS: std::cell::RefCell<HashMap<String, fontdue::Font>>
        = std::cell::RefCell::new(HashMap::new());
}

/// Renderer pri @font-face load registruje font pro measure. Pri parsing
/// font-weight: 600 (bold) variant ulozit pod "<family>__bold__" key etc.
/// Layout measure_text_width_full pak prefer tento font pred system fonts.
pub fn register_measure_font(key: &str, font: fontdue::Font) {
    MEASURE_FONTS.with(|m| {
        m.borrow_mut().insert(key.to_string(), font);
    });
}

/// Lookup pro measure_text_width_full. Vraci cloned font (fontdue::Font je
/// Clone via Arc-uvnitr, levne). bool bold = legacy wrapper for weight=700.
pub(crate) fn measure_font_for(family: &str, bold: bool, italic: bool) -> Option<fontdue::Font> {
    measure_font_for_weight(family, if bold { 700 } else { 400 }, italic)
}

/// Weight-aware lookup pres MEASURE_FONTS thread_local. CSS Fonts L4 nearest:
/// hleda exact `<family>__w<weight>__[i__]` key, jinak nearest weight.
pub(crate) fn measure_font_for_weight(family: &str, weight: u32, italic: bool) -> Option<fontdue::Font> {
    MEASURE_FONTS.with(|m| {
        let map = m.borrow();
        // Weight search order per CSS Fonts L4.
        let order: Vec<u32> = if weight < 400 {
            let mut v: Vec<u32> = (100..=weight).rev().collect();
            v.extend([400, 500, 600, 700, 800, 900].iter().copied());
            v
        } else if weight <= 500 {
            let mut v = vec![weight];
            if weight != 500 { v.push(500); }
            if weight != 400 { v.push(400); }
            v.extend([300, 200, 100, 600, 700, 800, 900].iter().copied());
            v
        } else {
            let mut v = vec![weight];
            for w in [600, 700, 800, 900] { if w != weight { v.push(w); } }
            v.extend([500, 400, 300, 200, 100].iter().copied());
            v
        };
        let suffix = if italic { "__i__" } else { "__" };
        let opp_suffix = if italic { "__" } else { "__i__" };
        for alt in family.split(',') {
            let trimmed = alt.trim().trim_matches('"').trim_matches('\'');
            if trimmed.is_empty() { continue; }
            // Try styled (italic match).
            for w in &order {
                let key = format!("{}__w{}{}", trimmed, w, suffix);
                if let Some(f) = map.get(&key) { return Some(f.clone()); }
            }
            // Try opposite italic.
            for w in &order {
                let key = format!("{}__w{}{}", trimmed, w, opp_suffix);
                if let Some(f) = map.get(&key) { return Some(f.clone()); }
            }
            // Legacy keys.
            if weight >= 600 && italic {
                if let Some(f) = map.get(&format!("{}__bi__", trimmed)) { return Some(f.clone()); }
            }
            if weight >= 600 {
                if let Some(f) = map.get(&format!("{}__bold__", trimmed)) { return Some(f.clone()); }
            }
            if italic {
                if let Some(f) = map.get(&format!("{}__italic__", trimmed)) { return Some(f.clone()); }
            }
            if let Some(f) = map.get(trimmed) { return Some(f.clone()); }
        }
        None
    })
}

/// Lookup natural dims (None pri zatim nenactenom obrazku).
pub fn get_image_natural_dims(src: &str) -> Option<(f32, f32)> {
    IMAGE_NATURAL_DIMS.with(|c| c.borrow().get(src).copied())
}

pub(crate) fn clear_subtree_hash_cache() {
    SUBTREE_HASH_CACHE.with(|c| c.borrow_mut().clear());
}

fn compute_subtree_hash(node: &Rc<Node>, style_map: &StyleMap) -> u64 {
    let id = Rc::as_ptr(node) as usize;
    if let Some(cached) = SUBTREE_HASH_CACHE.with(|c| c.borrow().get(&id).copied()) {
        return cached;
    }
    let h = compute_subtree_hash_uncached(node, style_map);
    SUBTREE_HASH_CACHE.with(|c| { c.borrow_mut().insert(id, h); });
    h
}

fn compute_subtree_hash_uncached(node: &Rc<Node>, style_map: &StyleMap) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = ahash::AHasher::default();
    let id = Rc::as_ptr(node) as usize;
    id.hash(&mut h);
    if let Some(tag) = node.tag_name() {
        tag.hash(&mut h);
    }
    // Text node check - hash text obsahu (text uzly nemaji tag).
    // PERF: pres text_content_ref pro direct &str borrow (drive text_content()
    // walked subtree + alocoval String, vola se z compute_subtree_hash pro
    // KAZDY text node).
    if node.tag_name_ref().is_none() {
        if let super::dom::NodeKind::Text(t) = &node.kind {
            t.hash(&mut h);
        }
    }
    // Styles - XOR commutative hash misto sort (drive `keys.collect()` +
    // `keys.sort()` alocoval Vec + O(N log N) per element. Pro 1000 elements
    // = 1000× Vec alloc + sort. XOR akumulator je commutative -> order-independent
    // bez sortovani).
    if let Some(styles) = super::cascade::get_styles(style_map, node) {
        let mut style_xor = 0u64;
        for (k, v) in styles.iter() {
            let mut hh = ahash::AHasher::default();
            k.hash(&mut hh);
            v.hash(&mut hh);
            style_xor ^= hh.finish();
        }
        style_xor.hash(&mut h);
    }
    // Children subtree hashes (recursive - kazdy compute_subtree_hash je memoized)
    for child in node.children.borrow().iter() {
        compute_subtree_hash(child, style_map).hash(&mut h);
    }
    h.finish()
}

/// Pri border-collapse:collapse na <table> child td/th get 1px border default.
fn apply_table_border_collapse(bx: &mut LayoutBox, in_collapse_table: bool) {
    let is_table = matches!(bx.tag.as_deref(), Some("table"));
    let collapse_here = is_table && bx.border_collapse.is_collapse();
    let inherit = in_collapse_table || collapse_here;
    let is_cell = matches!(bx.tag.as_deref(), Some("td") | Some("th"));
    if inherit && is_cell && bx.border_color.is_none() && bx.border_width == 0.0 {
        bx.border_width = 1.0;
        bx.border_color = Some([200, 200, 205, 255]);
    }
    // Tez table sam: defaultni border 1px na cely table.
    if collapse_here && bx.border_color.is_none() && bx.border_width == 0.0 {
        bx.border_width = 1.0;
        bx.border_color = Some([200, 200, 205, 255]);
    }
    for ch in &mut bx.children {
        apply_table_border_collapse(ch, inherit);
    }
}

/// Aplikuje position: sticky pri zadanem scroll offsetu.
/// Volat z render po layout, pred display_list build.
/// Sticky element: pri scroll dosahne urovne sticky -> drzi se na top.
pub fn apply_sticky(root: &mut LayoutBox, scroll_y: f32) {
    fn walk(bx: &mut LayoutBox, scroll_y: f32, parent_bottom: f32) {
        if matches!(bx.position, Position::Sticky) {
            // Init sticky_original_y na prvnim pruchodu - dale uz pouzivame ten,
            // protoze rect.y modifikujeme pri kazdem scroll. Bez tohoto by druhy
            // frame cetl shifted rect.y misto puvodni layout pozice.
            if bx.sticky_original_y.is_none() {
                bx.sticky_original_y = Some(bx.rect.y);
            }
            let original_y = bx.sticky_original_y.unwrap();
            let top = bx.offset_top.unwrap_or(0.0);
            let viewport_top = scroll_y + top;
            // Pokud element je nad viewport_top, posunout dolu (visible at viewport_top)
            if original_y < viewport_top {
                let new_y = viewport_top;
                // Don't push past parent bottom
                let max_y = parent_bottom - bx.rect.height;
                bx.rect.y = new_y.min(max_y).max(original_y);
            } else {
                // Mimo sticky range - reset na original (kdyz scroll_y zpet).
                bx.rect.y = original_y;
            }
        }
        let pb = bx.rect.y + bx.rect.height;
        for child in &mut bx.children {
            walk(child, scroll_y, pb);
        }
    }
    let pb = root.rect.y + root.rect.height;
    walk(root, scroll_y, pb);
}

/// Walk tree + collect anchor-name -> rect map.
fn collect_anchors(bx: &LayoutBox) -> std::collections::HashMap<String, Rect> {
    let mut out = std::collections::HashMap::new();
    fn walk(b: &LayoutBox, m: &mut std::collections::HashMap<String, Rect>) {
        if !b.anchor_name.is_empty() {
            // anchor-name muze byt "--name" - ulozim s prefixem
            m.insert(b.anchor_name.clone(), b.rect);
        }
        for c in &b.children { walk(c, m); }
    }
    walk(bx, &mut out);
    out
}

/// Aplikuje anchor positioning - pri elementu s position-anchor + inset-area
/// posun jeho pozice relativne k anchor.
fn apply_anchor_positioning(bx: &mut LayoutBox, anchors: &std::collections::HashMap<String, Rect>) {
    if !bx.position_anchor.is_empty() {
        if let Some(anchor_rect) = anchors.get(&bx.position_anchor) {
            // Aplikuj inset-area: top/bottom/left/right/center jako relative position
            let area = bx.inset_area.trim();
            let (tx, ty) = match area {
                "top"        => (anchor_rect.x, anchor_rect.y - bx.rect.height),
                "bottom"     => (anchor_rect.x, anchor_rect.y + anchor_rect.height),
                "left"       => (anchor_rect.x - bx.rect.width, anchor_rect.y),
                "right"      => (anchor_rect.x + anchor_rect.width, anchor_rect.y),
                "top left"   => (anchor_rect.x - bx.rect.width, anchor_rect.y - bx.rect.height),
                "top right"  => (anchor_rect.x + anchor_rect.width, anchor_rect.y - bx.rect.height),
                "bottom left"  => (anchor_rect.x - bx.rect.width, anchor_rect.y + anchor_rect.height),
                "bottom right" => (anchor_rect.x + anchor_rect.width, anchor_rect.y + anchor_rect.height),
                "center"     => (
                    anchor_rect.x + (anchor_rect.width - bx.rect.width) * 0.5,
                    anchor_rect.y + (anchor_rect.height - bx.rect.height) * 0.5,
                ),
                _ => (bx.rect.x, bx.rect.y),
            };
            bx.rect.x = tx;
            bx.rect.y = ty;
        }
    }
    for child in &mut bx.children {
        apply_anchor_positioning(child, anchors);
    }
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
                        // Detekuj jednotku ze source values: pokud original mel
                        // "px"/"em"/"rem"/"%", pouzij stejnou. Unitless props
                        // (opacity, z-index) zustanou bez jednotky.
                        let v0_str = d0.value.trim();
                        let unit = if v0_str.ends_with("px") { "px" }
                                   else if v0_str.ends_with("rem") { "rem" }
                                   else if v0_str.ends_with("em") { "em" }
                                   else if v0_str.ends_with('%') { "%" }
                                   else { "" };
                        out.insert(d0.property.clone(), format!("{v}{unit}"));
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
    // Aliases:
    // - inline-flex/grid -> flex/grid
    // - subgrid -> grid
    // - ruby -> inline
    // - list-item -> block
    // - table -> block (rows stack vertically)
    // - table-row -> flex-row (cells horizontalne)
    // - table-cell -> block (vertical stacking inside)
    let effective = match bx.display {
        Display::InlineFlex => Display::Flex,
        Display::InlineGrid | Display::Subgrid => Display::Grid,
        Display::ListItem => Display::Block,
        Display::Ruby => Display::Inline,
        Display::Table | Display::TableHeader => Display::Block,
        Display::TableRow => {
            // Aproximace: tr jako flex-row container, td/th uvnitr jsou flex items.
            // (FlexDirection::Row je default, bez explicit override netreba reseni.)
            Display::Flex
        }
        Display::TableCell | Display::TableHeaderCell | Display::TableCaption => Display::Block,
        Display::Contents => {
            // Element zmizi - layout-time prom contents skip a deti se chovaji jako parent's
            // Pro start: fallback na Inline aby children flowed.
            Display::Inline
        }
        d => d,
    };
    let saved = bx.display;
    bx.display = effective;
    layout_dispatch_inner(bx);
    bx.display = saved;
}

fn layout_dispatch_inner(bx: &mut LayoutBox) {
    // Auto-grow stack (deep DOM nesting -> deep flex/grid/block recursion).
    stacker::maybe_grow(32 * 1024, 8 * 1024 * 1024, || {
        match bx.display {
            Display::Flex => layout_flex(bx),
            Display::Grid => super::layout_engine::grid::layout_grid(bx),
            _ => layout_block(bx),
        }
    });
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
    let mut counters: HashMap<String, i32> = HashMap::new();
    build_box_inner(node, style_map, pseudo_map, &mut counters)
}

/// Cached varianta: pri match fingerprint reuznava prev subtree.
fn build_box_with_pseudo_cached(
    node: &Rc<Node>,
    style_map: &StyleMap,
    pseudo_map: &super::cascade::PseudoStyleMap,
    cache: Option<&HashMap<usize, LayoutBox>>,
) -> LayoutBox {
    let mut counters: HashMap<String, i32> = HashMap::new();
    build_box_inner_cached(node, style_map, pseudo_map, &mut counters, cache)
}

// Thread-local cache pro per-element layout reuse. Nastaveno
// layout_tree_with_pseudo_cached pred build, vycteno v build_box_inner pri
// recursive child build. Po dokonceni vraceno na None.
thread_local! {
    static LAYOUT_CACHE: std::cell::RefCell<Option<HashMap<usize, LayoutBox>>> =
        std::cell::RefCell::new(None);
}

fn build_box_inner_cached(
    node: &Rc<Node>,
    style_map: &StyleMap,
    pseudo_map: &super::cascade::PseudoStyleMap,
    counters: &mut HashMap<String, i32>,
    cache: Option<&HashMap<usize, LayoutBox>>,
) -> LayoutBox {
    // Set thread-local cache, run cached_inner_lookup, restore.
    let prev_set = LAYOUT_CACHE.with(|c| c.borrow().is_some());
    if !prev_set {
        if let Some(c) = cache {
            LAYOUT_CACHE.with(|tc| *tc.borrow_mut() = Some(c.clone()));
        }
    }
    // Try cache hit pri root.
    if let Some(c) = cache {
        let node_id = Rc::as_ptr(node) as usize;
        if let Some(prev) = c.get(&node_id) {
            let h = compute_subtree_hash(node, style_map);
            if prev.fingerprint == h && h != 0 {
                if !prev_set {
                    LAYOUT_CACHE.with(|tc| *tc.borrow_mut() = None);
                }
                // Cache je pouze pro structure/styles. rect.* (positions/sizes)
                // pochazi z prev layout_dispatch a stane se stale pri novem
                // layoutu (napr. po scrollbar reservation re-layout). Reset
                // rect cele subtree aby novy layout vychazel z 0.
                let mut cloned = prev.clone();
                reset_subtree_rect(&mut cloned);
                return cloned;
            }
        }
    }
    let mut bx = build_box_inner(node, style_map, pseudo_map, counters);
    bx.fingerprint = compute_subtree_hash(node, style_map);
    if !prev_set {
        LAYOUT_CACHE.with(|tc| *tc.borrow_mut() = None);
    }
    bx
}

/// Test pres thread-local cache: pri rekurzi v build_box_inner deti zkontroluj
/// cache. Volano z mista kde build_box_inner rekurzuje na child node.
fn cache_lookup_subtree(node: &Rc<Node>, style_map: &StyleMap) -> Option<LayoutBox> {
    LAYOUT_CACHE.with(|tc| {
        if let Some(cache) = tc.borrow().as_ref() {
            let node_id = Rc::as_ptr(node) as usize;
            if let Some(prev) = cache.get(&node_id) {
                let h = compute_subtree_hash(node, style_map);
                if prev.fingerprint == h && h != 0 {
                    let mut clone = prev.clone();
                    clone.fingerprint = h;
                    reset_subtree_rect(&mut clone);
                    return Some(clone);
                }
            }
        }
        None
    })
}

/// Reset rect (x/y/width/height) v cele subtree. Pouziti pri cache hit
/// nebo re-layout: struktura+styly se prevezmou, ale pozice/velikosti maji
/// byt prepoctene od nuly. Bez resetu by layout_block "grow only" logika
/// nedokazala shrinkovat (napr. pri scrollbar reservation second pass).
fn reset_subtree_rect(bx: &mut LayoutBox) {
    bx.rect = Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 };
    for ch in bx.children.iter_mut() {
        reset_subtree_rect(ch);
    }
}

fn build_box_inner(node: &Rc<Node>, style_map: &StyleMap, pseudo_map: &super::cascade::PseudoStyleMap, counters: &mut HashMap<String, i32>) -> LayoutBox {
    // Debug breakpoint hook: BP_TAG/BP_ID/BP_CLASS env vars + IDE breakpoint na
    // `breakpoint_build` v src/debug_bp.rs.
    if crate::debug_bp::bp_enabled() {
        let tag = node.tag_name().unwrap_or_default();
        let id = node.attr("id").unwrap_or_default();
        let class = node.attr("class").unwrap_or_default();
        if crate::debug_bp::bp_match(&tag, &id, &class) {
            crate::debug_bp::breakpoint_build();
        }
    }
    // Reset list-item counter pri novem ol/ul (CSS spec: kazdy list ma vlastni counter).
    if let Some(tag) = node.tag_name() {
        if tag == "ol" || tag == "ul" {
            counters.insert("list-item".into(), 0);
        }
    }
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

    // HTML `hidden` attribute = display:none (UA stylesheet rule).
    // Bez tohoto staticky vykreslime modaly/popupy ktere by JS zapnul jen
    // pri user akci. Realne stranky (google) tim skryvaji desitky elementu.
    if node.attr("hidden").is_some() {
        bx.display = Display::None;
    }
    // POZN: aria-hidden NENI display:none - je to accessibility signal pro
    // screen readery. Visual render ma byt normalni. Predchozi pokus to
    // zaridit byl chyba (skryval ikonky uvnitr buttonu na google).
    // <dialog> UA stylesheet: default display:none, [open] -> display:block.
    // <details> taky podobne ale toggle pres [open] na own children visibility.
    // Bez tohoto google search pouzivajici <dialog class="spch-dlg"> pro
    // share modal byl viditelny i bez user interakce.
    if let Some(tag) = node.tag_name() {
        match tag.as_str() {
            "dialog" => {
                if node.attr("open").is_none() {
                    bx.display = Display::None;
                }
            }
            _ => {}
        }
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
    // SVG tag: viewport z width/height attrs. explicit_* aby block/flex layout
    // respektoval SVG dimensions (jinak SVG roztaha na rodicovsky container).
    if bx.tag.as_deref() == Some("svg") {
        if let Some(w) = node.attr("width").and_then(|w| w.parse::<f32>().ok()) {
            bx.rect.width = w;
            bx.explicit_width = Some(w);
        }
        if let Some(h) = node.attr("height").and_then(|h| h.parse::<f32>().ok()) {
            bx.rect.height = h;
            bx.explicit_height = Some(h);
        }
        // SVG children: spocti rect z SVG atributu (x/y/cx/cy/r/rx/ry/x1/y1...)
        // a uloz na child LayoutBox pro proper devtools highlight + hit-test.
        // Pozice je relative to SVG box (paint pridava svg.rect.x/y origin).
        for child in bx.children.iter_mut() {
            let child_node = match &child.node { Some(n) => Rc::clone(n), None => continue };
            let attr_f = |name: &str, default: f32| -> f32 {
                child_node.attr(name).and_then(|v| v.parse().ok()).unwrap_or(default)
            };
            let tag = child.tag.clone().unwrap_or_default();
            let (cx_off, cy_off, cw, ch) = match tag.as_str() {
                "rect" => (
                    attr_f("x", 0.0), attr_f("y", 0.0),
                    attr_f("width", 0.0), attr_f("height", 0.0),
                ),
                "circle" => {
                    let cx = attr_f("cx", 0.0); let cy = attr_f("cy", 0.0);
                    let r = attr_f("r", 0.0);
                    (cx - r, cy - r, 2.0 * r, 2.0 * r)
                }
                "ellipse" => {
                    let cx = attr_f("cx", 0.0); let cy = attr_f("cy", 0.0);
                    let rx = attr_f("rx", 0.0); let ry = attr_f("ry", 0.0);
                    (cx - rx, cy - ry, 2.0 * rx, 2.0 * ry)
                }
                "line" => {
                    let x1 = attr_f("x1", 0.0); let y1 = attr_f("y1", 0.0);
                    let x2 = attr_f("x2", 0.0); let y2 = attr_f("y2", 0.0);
                    (x1.min(x2), y1.min(y2), (x2 - x1).abs(), (y2 - y1).abs())
                }
                "text" => {
                    let x = attr_f("x", 0.0); let y = attr_f("y", 0.0);
                    let fs = attr_f("font-size", 14.0);
                    (x, y - fs, fs * 8.0, fs)  // Approx text rect.
                }
                _ => continue,
            };
            child.rect.x = bx.rect.x + cx_off;
            child.rect.y = bx.rect.y + cy_off;
            child.rect.width = cw;
            child.rect.height = ch;
        }
    }
    // Video tag: replaced element (zatim bez decode - jen layout box + placeholder).
    // Default 300x150 per HTML spec, ale poster image / controls renderovany v paint.
    if bx.tag.as_deref() == Some("video") {
        if let Some(w) = node.attr("width").and_then(|w| w.parse::<f32>().ok()) {
            bx.rect.width = w;
        }
        if let Some(h) = node.attr("height").and_then(|h| h.parse::<f32>().ok()) {
            bx.rect.height = h;
        }
        if bx.rect.width == 0.0 { bx.rect.width = 300.0; }
        if bx.rect.height == 0.0 { bx.rect.height = 150.0; }
        // poster atribut - obrazek zobrazeny pred prehravanim. Pouzijeme bg image.
        if let Some(poster) = node.attr("poster") {
            bx.image_src = Some(poster);
        }
    }
    // Audio tag: replaced element. Default browser controls bar = 300x40.
    if bx.tag.as_deref() == Some("audio") {
        if bx.rect.width == 0.0 { bx.rect.width = 300.0; }
        if bx.rect.height == 0.0 { bx.rect.height = 40.0; }
    }
    // <select>: dropdown closed. Vyber selected <option> a renderuj jako text.
    // Default size: 120x24.
    if bx.tag.as_deref() == Some("select") {
        if bx.rect.width == 0.0 { bx.rect.width = 120.0; }
        if bx.rect.height == 0.0 { bx.rect.height = 24.0; }
        // Najdi selected option (nebo first).
        let options = node.children.borrow();
        let mut selected_text: Option<String> = None;
        let mut first_text: Option<String> = None;
        for ch in options.iter() {
            if ch.tag_name().as_deref() == Some("option") {
                let txt = ch.text_content();
                if first_text.is_none() { first_text = Some(txt.clone()); }
                if ch.attr("selected").is_some() {
                    selected_text = Some(txt);
                    break;
                }
            }
        }
        bx.text = selected_text.or(first_text);
    }
    // <textarea>: multi-line input. Default 200x50.
    if bx.tag.as_deref() == Some("textarea") {
        if bx.rect.width == 0.0 { bx.rect.width = 200.0; }
        if bx.rect.height == 0.0 { bx.rect.height = 60.0; }
    }
    // <progress>: progress bar.
    if bx.tag.as_deref() == Some("progress") {
        if bx.rect.width == 0.0 { bx.rect.width = 160.0; }
        if bx.rect.height == 0.0 { bx.rect.height = 14.0; }
    }
    // <meter>: meter bar.
    if bx.tag.as_deref() == Some("meter") {
        if bx.rect.width == 0.0 { bx.rect.width = 80.0; }
        if bx.rect.height == 0.0 { bx.rect.height = 14.0; }
    }

    if matches!(node.kind, NodeKind::Text(_)) {
        bx.display = Display::Inline;
        if let NodeKind::Text(t) = &node.kind {
            // CSS white-space: normal - sloucit BREAKABLE whitespace runs do
            // single space. NBSP (U+00A0) je NE-collapsable + ne-break - zachovan
            // jako samostatny char. Bez tohoto "119\u{a0}525" -> "119 525" (ASCII)
            // -> flush_inline split na 2 words "119" "525" -> wrap point mezi
            // nimi -> cislo zalamne.
            let mut collapsed = String::with_capacity(t.len());
            let mut prev_ws = false;
            for c in t.chars() {
                if c == '\u{00A0}' {
                    // NBSP zachovany doslovne (no collapse, no normalize).
                    collapsed.push(c);
                    prev_ws = false;
                } else if c.is_whitespace() {
                    if !prev_ws { collapsed.push(' '); }
                    prev_ws = true;
                } else {
                    collapsed.push(c);
                    prev_ws = false;
                }
            }
            // Pokud je collapsed jen samotny space -> empty (whitespace-only node).
            if !collapsed.trim().is_empty() {
                bx.text = Some(collapsed);
            }
        }
    }

    // Color parsing - if linear-gradient, parse jako gradient, jinak solid color.
    // Background shorthand s gradient HODNOTOU musi prepsat puvodni background-color
    // (nelze pouze fallback - cascade ne-cleartoval background-color pri shorthand
    // override). Preferujeme "background" pri gradient/image, jinak background-color.
    let bg_shorthand = s.get("background").map(|v| v.as_str()).unwrap_or("");
    let bg_is_gradient = bg_shorthand.contains("linear-gradient(")
        || bg_shorthand.contains("radial-gradient(")
        || bg_shorthand.contains("conic-gradient(");
    let bg_value = if bg_is_gradient {
        Some(bg_shorthand.to_string())
    } else {
        s.get("background-color").or(s.get("background")).cloned()
    };
    if let Some(c) = bg_value {
        if c.contains("linear-gradient(") || c.contains("radial-gradient(") || c.contains("conic-gradient(") {
            bx.bg_gradient = parse_any_gradient(&c);
        } else {
            bx.bg_color = parse_color(&c);
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
    // Filter chain + backdrop-filter
    if let Some(f) = s.get("filter") {
        bx.filter = parse_filter_chain(f);
    }
    if let Some(f) = s.get("backdrop-filter") {
        bx.backdrop_filter = parse_filter_chain(f);
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
    // text-decoration shorthand: `none` / `underline` / `line-through` / kombi.
    // Plus parsuje sub-props (color + style + thickness). Bez tohoto UA default
    // `<a>` underline neresilo override `text-decoration: none` v page CSS ->
    // tlacitka `<a class="btn">` mela underline.
    if let Some(td) = s.get("text-decoration").or_else(|| s.get("text-decoration-line")) {
        let v = td.trim().to_lowercase();
        // Multi-value: "underline solid red" - hledat keywordy.
        let has_none = v.split_whitespace().any(|t| t == "none");
        let has_underline = v.split_whitespace().any(|t| t == "underline");
        let has_strike = v.split_whitespace().any(|t| t == "line-through");
        if has_none {
            bx.text_underline = false;
            bx.text_strikethrough = false;
        } else {
            if has_underline { bx.text_underline = true; }
            if has_strike { bx.text_strikethrough = true; }
        }
    }
    // text-decoration L4 detail props
    if let Some(c) = s.get("text-decoration-color") {
        bx.text_decoration_color = parse_color(c);
    }
    if let Some(st) = s.get("text-decoration-style") {
        bx.text_decoration_style = st.trim().to_string();
    }
    if let Some(t) = s.get("text-decoration-thickness") {
        if t.trim() != "auto" { bx.text_decoration_thickness = parse_length(t); }
    }
    if let Some(o) = s.get("text-underline-offset") {
        if o.trim() != "auto" { bx.text_underline_offset = parse_length(o); }
    }
    if let Some(ti) = s.get("text-indent") {
        bx.text_indent = parse_length(ti);
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
    // border-image: url(...) <slice> / <width> [/ <outset>] <repeat>
    if let Some(src) = s.get("border-image-source") {
        let v = src.trim();
        if v != "none" {
            // strip url(...) -> URL
            let url = v.strip_prefix("url(").and_then(|x| x.strip_suffix(')'))
                .map(|u| u.trim_matches('"').trim_matches('\'').to_string())
                .unwrap_or_else(|| v.to_string());
            bx.border_image_source = Some(url);
        }
    }
    if let Some(sl) = s.get("border-image-slice") {
        let parts: Vec<&str> = sl.split_whitespace().filter(|p| *p != "fill").collect();
        let nums: Vec<f32> = parts.iter().map(|p| {
            p.trim_end_matches('%').parse::<f32>().unwrap_or(0.0)
        }).collect();
        bx.border_image_slice = match nums.len() {
            1 => [nums[0]; 4],
            2 => [nums[0], nums[1], nums[0], nums[1]],
            3 => [nums[0], nums[1], nums[2], nums[1]],
            _ if nums.len() >= 4 => [nums[0], nums[1], nums[2], nums[3]],
            _ => [0.0; 4],
        };
    }
    if let Some(bw) = s.get("border-image-width") {
        let nums: Vec<f32> = bw.split_whitespace()
            .map(|p| p.trim_end_matches("px").parse::<f32>().unwrap_or(1.0))
            .collect();
        bx.border_image_width = match nums.len() {
            1 => [nums[0]; 4],
            2 => [nums[0], nums[1], nums[0], nums[1]],
            3 => [nums[0], nums[1], nums[2], nums[1]],
            _ if nums.len() >= 4 => [nums[0], nums[1], nums[2], nums[3]],
            _ => [1.0; 4],
        };
    }
    if let Some(br) = s.get("border-image-repeat") {
        bx.border_image_repeat = br.trim().to_string();
    }
    // CSS Compositing L1
    if let Some(mbm) = s.get("mix-blend-mode") {
        bx.mix_blend_mode = mbm.trim().to_string();
    }
    if let Some(bbm) = s.get("background-blend-mode") {
        bx.background_blend_mode = bbm.trim().to_string();
    }
    // text-emphasis
    if let Some(tes) = s.get("text-emphasis-style") {
        bx.text_emphasis_style = tes.trim().to_string();
    }
    if let Some(tec) = s.get("text-emphasis-color") {
        bx.text_emphasis_color = parse_color(tec);
    }
    if let Some(te) = s.get("text-emphasis") {
        // Shorthand "<style> <color>"
        let parts: Vec<&str> = te.split_whitespace().collect();
        if !parts.is_empty() { bx.text_emphasis_style = parts[0].to_string(); }
        if parts.len() >= 2 { bx.text_emphasis_color = parse_color(parts[1]); }
    }
    // text-decoration-skip-ink
    if let Some(tdsi) = s.get("text-decoration-skip-ink") {
        bx.text_decoration_skip_ink = tdsi.trim().to_string();
    }
    // field-sizing (CSS Forms L1)
    if let Some(fs) = s.get("field-sizing") {
        bx.field_sizing = fs.trim().to_string();
    }
    // interpolate-size (CSS Animations L2)
    if let Some(is_) = s.get("interpolate-size") {
        bx.interpolate_size = is_.trim().to_string();
    }
    // CSS Grid L2 - named lines / areas (parser-only, taffy resolvuje track sizes)
    if let Some(gtc) = s.get("grid-template-columns") {
        bx.grid_template_columns = gtc.trim().to_string();
    }
    if let Some(gtr) = s.get("grid-template-rows") {
        bx.grid_template_rows = gtr.trim().to_string();
    }
    if let Some(gta) = s.get("grid-template-areas") {
        bx.grid_template_areas = gta.trim().to_string();
    }
    if let Some(ga) = s.get("grid-area") {
        bx.grid_area = ga.trim().to_string();
    }
    if let Some(gc) = s.get("grid-column") {
        bx.grid_column = gc.trim().to_string();
    }
    if let Some(gr) = s.get("grid-row") {
        bx.grid_row = gr.trim().to_string();
    }
    if let Some(gac) = s.get("grid-auto-columns") {
        bx.grid_auto_columns = gac.trim().to_string();
    }
    if let Some(gar) = s.get("grid-auto-rows") {
        bx.grid_auto_rows = gar.trim().to_string();
    }
    if let Some(gaf) = s.get("grid-auto-flow") {
        bx.grid_auto_flow = gaf.trim().to_string();
    }
    // CSS Shapes L1
    if let Some(so) = s.get("shape-outside") {
        let v = so.trim();
        if v != "none" { bx.shape_outside = Some(v.to_string()); }
    }
    if let Some(sm) = s.get("shape-margin") {
        bx.shape_margin = parse_length(sm);
    }
    if let Some(sit) = s.get("shape-image-threshold") {
        bx.shape_image_threshold = sit.trim().parse().unwrap_or(0.0);
    }
    // CSS Overflow L4
    if let Some(sg) = s.get("scrollbar-gutter") { bx.scrollbar_gutter = sg.trim().to_string(); }
    // SVG markers
    if let Some(m) = s.get("marker-start") { bx.marker_start = m.trim().to_string(); }
    if let Some(m) = s.get("marker-mid") { bx.marker_mid = m.trim().to_string(); }
    if let Some(m) = s.get("marker-end") { bx.marker_end = m.trim().to_string(); }
    // CSS Backgrounds L4 - position-x / -y
    if let Some(bpx) = s.get("background-position-x") { bx.background_position_x = bpx.trim().to_string(); }
    if let Some(bpy) = s.get("background-position-y") { bx.background_position_y = bpy.trim().to_string(); }
    // CSS Images L3
    if let Some(io) = s.get("image-orientation") { bx.image_orientation = io.trim().to_string(); }
    // hyphenate-* (Text L4)
    if let Some(hc) = s.get("hyphenate-character") { bx.hyphenate_character = hc.trim().trim_matches('"').trim_matches('\'').to_string(); }
    if let Some(hlc) = s.get("hyphenate-limit-chars") { bx.hyphenate_limit_chars = hlc.trim().to_string(); }
    // CSS Inline L3
    if let Some(t) = s.get("text-box-trim") { bx.text_box_trim = t.trim().to_string(); }
    if let Some(t) = s.get("text-box-edge") { bx.text_box_edge = t.trim().to_string(); }
    // CSS Position L3 - position-area / try-fallbacks
    if let Some(pa) = s.get("position-area") { bx.position_area = pa.trim().to_string(); }
    if let Some(ptf) = s.get("position-try-fallbacks") { bx.position_try_fallbacks = ptf.trim().to_string(); }
    if let Some(ad) = s.get("anchor-default") { bx.anchor_default = ad.trim().to_string(); }
    // CSS Text L4 - text-spacing
    if let Some(ts2) = s.get("text-spacing") { bx.text_spacing = ts2.trim().to_string(); }
    if let Some(ta) = s.get("text-autospace") { bx.text_autospace = ta.trim().to_string(); }
    if let Some(il) = s.get("initial-letter") { bx.initial_letter = il.trim().to_string(); }
    // CSS Ruby L1 (extras)
    if let Some(ro) = s.get("ruby-overhang") { bx.ruby_overhang = ro.trim().to_string(); }
    if let Some(rm) = s.get("ruby-merge") { bx.ruby_merge = rm.trim().to_string(); }
    // CSS Math L1 (extras)
    if let Some(msh) = s.get("math-shift") { bx.math_shift = msh.trim().to_string(); }
    // CSS Transitions L2 / Animations L2
    if let Some(tb) = s.get("transition-behavior") { bx.transition_behavior = tb.trim().to_string(); }
    if let Some(ac) = s.get("animation-composition") { bx.animation_composition = ac.trim().to_string(); }
    if let Some(ci) = s.get("color-interpolation") { bx.color_interpolation = ci.trim().to_string(); }
    // CSS Flexbox L1 properties
    if let Some(v) = s.get("box-sizing") { bx.box_sizing = BoxSizing::parse(v); }
    if let Some(v) = s.get("flex-direction") { bx.flex_direction = FlexDirection::parse(v); }
    if let Some(v) = s.get("flex-wrap") { bx.flex_wrap = FlexWrap::parse(v); }
    if let Some(v) = s.get("justify-content") { bx.justify_content = JustifyContent::parse(v); }
    if let Some(v) = s.get("align-items") { bx.align_items = AlignItems::parse(v); }
    if let Some(v) = s.get("align-content") { bx.align_content = AlignContent::parse(v); }
    if let Some(v) = s.get("align-self") { bx.align_self = AlignSelf::parse(v); }
    if let Some(v) = s.get("justify-self") { bx.justify_self = AlignSelf::parse(v); }
    if let Some(v) = s.get("flex-grow") { bx.flex_grow = v.trim().parse().unwrap_or(0.0); }
    if let Some(v) = s.get("flex-shrink") { bx.flex_shrink = v.trim().parse().unwrap_or(1.0); }
    if let Some(v) = s.get("flex-basis") { bx.flex_basis = v.trim().to_string(); }
    if let Some(v) = s.get("row-gap") { bx.row_gap = parse_length(v); }
    if let Some(v) = s.get("column-gap") {
        bx.column_gap = parse_length(v);
        bx.column_gap_multicol = parse_length(v);
    }
    // Multi-column Layout L1: column-count + columns shorthand + column-rule.
    if let Some(v) = s.get("column-count") {
        bx.column_count = v.trim().parse::<u32>().unwrap_or(1).max(1);
    }
    if let Some(v) = s.get("columns") {
        // shorthand: column-width column-count (any order, auto allowed).
        for tok in v.split_whitespace() {
            if let Ok(n) = tok.parse::<u32>() {
                bx.column_count = n.max(1);
            }
            // column-width ignorovan pro ted; jen count + auto.
        }
    }
    if let Some(v) = s.get("column-rule") {
        // shorthand: <width> <style> <color>
        for tok in v.split_whitespace() {
            if tok.ends_with("px") || tok.ends_with("em") {
                bx.column_rule_width = parse_length(tok);
            } else if matches!(tok, "solid" | "dashed" | "dotted" | "double" | "none") {
                bx.column_rule_style = tok.to_string();
            } else if let Some(c) = parse_color(tok) {
                bx.column_rule_color = c;
            }
        }
    }
    if let Some(v) = s.get("column-rule-width") { bx.column_rule_width = parse_length(v); }
    if let Some(v) = s.get("column-rule-style") { bx.column_rule_style = v.trim().to_string(); }
    if let Some(v) = s.get("column-rule-color") {
        if let Some(c) = parse_color(v.trim()) { bx.column_rule_color = c; }
    }
    if let Some(v) = s.get("gap") {
        let parts: Vec<&str> = v.split_whitespace().collect();
        if parts.len() == 1 {
            let g = parse_length(parts[0]);
            bx.row_gap = g; bx.column_gap = g;
        } else if parts.len() >= 2 {
            bx.row_gap = parse_length(parts[0]);
            bx.column_gap = parse_length(parts[1]);
        }
    }
    if let Some(v) = s.get("flex") {
        let v = v.trim();
        match v {
            "none" => { bx.flex_grow = 0.0; bx.flex_shrink = 0.0; bx.flex_basis = "auto".into(); }
            "auto" => { bx.flex_grow = 1.0; bx.flex_shrink = 1.0; bx.flex_basis = "auto".into(); }
            _ => {
                let parts: Vec<&str> = v.split_whitespace().collect();
                if let Some(p0) = parts.first() {
                    if let Ok(g) = p0.parse::<f32>() { bx.flex_grow = g; }
                }
                if let Some(p1) = parts.get(1) {
                    if let Ok(sh) = p1.parse::<f32>() { bx.flex_shrink = sh; }
                    else { bx.flex_basis = p1.to_string(); }
                }
                if let Some(p2) = parts.get(2) {
                    bx.flex_basis = p2.to_string();
                }
            }
        }
    }
    // CSS Scroll-Driven Animations L1 extras
    if let Some(v) = s.get("timeline-scope") { bx.timeline_scope = v.trim().to_string(); }
    if let Some(v) = s.get("animation-range-start") { bx.animation_range_start = v.trim().to_string(); }
    if let Some(v) = s.get("animation-range-end") { bx.animation_range_end = v.trim().to_string(); }
    // CSS Carousels (Scroll Driven Animations + scroll-marker)
    if let Some(v) = s.get("scroll-marker-group") { bx.scroll_marker_group = v.trim().to_string(); }
    // CSS Filters L2 backdrop-filter raw string (parsovan jinde do filter ops)
    if let Some(v) = s.get("backdrop-filter") { bx.backdrop_filter_string = v.trim().to_string(); }
    // CSS Containment intrinsic sizes
    if let Some(v) = s.get("contain-intrinsic-block-size") { bx.contain_intrinsic_block_size = parse_length(v); }
    if let Some(v) = s.get("contain-intrinsic-inline-size") { bx.contain_intrinsic_inline_size = parse_length(v); }
    // CSS Anchor L1 / Position L4
    if let Some(v) = s.get("anchor-scope") { bx.anchor_scope = v.trim().to_string(); }
    if let Some(v) = s.get("position-visibility") { bx.position_visibility = v.trim().to_string(); }
    // CSS Display L4 - reading flow / order
    if let Some(v) = s.get("reading-flow") { bx.reading_flow = v.trim().to_string(); }
    if let Some(v) = s.get("reading-order") { bx.reading_order = v.trim().to_string(); }
    // CSS Compositing L1 - composite-op
    if let Some(v) = s.get("composite") { bx.composite_op = v.trim().to_string(); }
    // CSS UI L4 - resize / list-style-position
    if let Some(v) = s.get("resize") { bx.resize_v = v.trim().to_string(); }
    if let Some(v) = s.get("list-style-position") { bx.list_style_position_v = v.trim().to_string(); }
    // CSS Speech L1 (voice-family extra)
    if let Some(v) = s.get("voice-family") { bx.voice_family = v.trim().to_string(); }
    // inset shorthand: top right bottom left
    if let Some(ins) = s.get("inset") {
        let parts: Vec<&str> = ins.split_whitespace().collect();
        let parse_one = |p: &str| -> Option<f32> {
            if p == "auto" { return None; }
            Some(parse_length(p))
        };
        match parts.len() {
            1 => { let v = parse_one(parts[0]); bx.inset = [v, v, v, v]; }
            2 => { let a = parse_one(parts[0]); let b = parse_one(parts[1]); bx.inset = [a, b, a, b]; }
            3 => {
                let a = parse_one(parts[0]); let b = parse_one(parts[1]); let c = parse_one(parts[2]);
                bx.inset = [a, b, c, b];
            }
            _ if parts.len() >= 4 => {
                bx.inset = [parse_one(parts[0]), parse_one(parts[1]), parse_one(parts[2]), parse_one(parts[3])];
            }
            _ => {}
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
    if let Some(m) = s.get("mask-image") {
        if m.trim() != "none" { bx.mask_image = Some(m.trim().to_string()); }
    }
    if let Some(so) = s.get("shape-outside") {
        if so.trim() != "none" { bx.shape_outside = Some(so.trim().to_string()); }
    }
    if let Some(d) = s.get("direction") {
        bx.direction = Direction::parse(d);
        // RTL: text-align default = right (pokud nezadany)
        if bx.direction.is_rtl() && s.get("text-align").is_none() {
            bx.text_align = TextAlign::Right;
        }
    }
    if let Some(wm) = s.get("writing-mode") {
        bx.writing_mode = WritingMode::parse(wm);
    }
    if let Some(cv) = s.get("content-visibility") {
        bx.content_visibility = cv.trim().to_string();
    }
    if let Some(cis) = s.get("contain-intrinsic-size") {
        bx.contain_intrinsic_size = parse_length(cis);
    }
    let parse_counter = |v: &str| -> Vec<(String, i32)> {
        let mut out = Vec::new();
        for entry in v.split(',') {
            let parts: Vec<&str> = entry.split_whitespace().collect();
            if let Some(name) = parts.first() {
                let n: i32 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
                out.push((name.to_string(), n));
            }
        }
        out
    };
    if let Some(v) = s.get("counter-reset") { bx.counter_reset = parse_counter(v); }
    if let Some(v) = s.get("counter-increment") { bx.counter_increment = parse_counter(v); }
    if let Some(v) = s.get("backface-visibility") { bx.backface_visibility = v.trim().to_string(); }
    if let Some(v) = s.get("transform-style") { bx.transform_style = v.trim().to_string(); }
    if let Some(v) = s.get("perspective") {
        if v.trim() != "none" { bx.perspective = Some(parse_length(v)); }
    }
    if let Some(v) = s.get("text-emphasis") { bx.text_emphasis = v.trim().to_string(); }
    if let Some(v) = s.get("will-change") { bx.will_change = v.trim().to_string(); }
    if let Some(v) = s.get("isolation") { bx.isolation = v.trim().to_string(); }
    if let Some(v) = s.get("mix-blend-mode") { bx.mix_blend_mode = v.trim().to_string(); }
    if let Some(v) = s.get("pointer-events") { bx.pointer_events = PointerEvents::parse(v); }
    if let Some(v) = s.get("user-select") { bx.user_select = v.trim().to_string(); }
    if let Some(v) = s.get("caret-color") {
        if v.trim() != "auto" { bx.caret_color = parse_color(v); }
    }
    if let Some(v) = s.get("resize") { bx.resize = v.trim().to_string(); }
    if let Some(v) = s.get("touch-action") { bx.touch_action = v.trim().to_string(); }
    if let Some(v) = s.get("hyphens") { bx.hyphens = v.trim().to_string(); }
    if let Some(v) = s.get("tab-size") {
        if let Ok(n) = v.trim().parse::<f32>() { bx.tab_size = n; }
        else { bx.tab_size = parse_length(v); }
    }
    if let Some(v) = s.get("word-break") { bx.word_break = v.trim().to_string(); }
    if let Some(v) = s.get("overflow-wrap").or(s.get("word-wrap")) { bx.overflow_wrap = v.trim().to_string(); }
    if let Some(v) = s.get("text-wrap") { bx.text_wrap = v.trim().to_string(); }
    if let Some(v) = s.get("text-align-last") { bx.text_align_last = v.trim().to_string(); }
    if let Some(v) = s.get("list-style-type") { bx.list_style_type = v.trim().to_string(); }
    if let Some(v) = s.get("list-style-position") { bx.list_style_position = v.trim().to_string(); }
    if let Some(v) = s.get("list-style-image") {
        if v.trim() != "none" { bx.list_style_image = Some(v.trim().to_string()); }
    }
    if let Some(v) = s.get("font-stretch") { bx.font_stretch = v.trim().to_string(); }
    if let Some(v) = s.get("font-variant") { bx.font_variant = v.trim().to_string(); }
    if let Some(v) = s.get("font-size-adjust") { bx.font_size_adjust = v.trim().to_string(); }
    if let Some(v) = s.get("font-feature-settings") { bx.font_feature_settings = v.trim().to_string(); }
    if let Some(v) = s.get("font-variation-settings") { bx.font_variation_settings = v.trim().to_string(); }
    if let Some(v) = s.get("font-display") { bx.font_display = v.trim().to_string(); }
    if let Some(v) = s.get("text-orientation") { bx.text_orientation = v.trim().to_string(); }
    if let Some(v) = s.get("text-combine-upright") { bx.text_combine_upright = v.trim().to_string(); }
    if let Some(v) = s.get("ruby-position") { bx.ruby_position = v.trim().to_string(); }
    if let Some(v) = s.get("ruby-align") { bx.ruby_align = v.trim().to_string(); }
    if let Some(v) = s.get("quotes") { bx.quotes = v.trim().to_string(); }
    if let Some(v) = s.get("outline-width") { bx.outline_width = parse_length(v); }
    if let Some(v) = s.get("outline-style") { bx.outline_style = v.trim().to_string(); }
    if let Some(v) = s.get("outline-color") {
        if v.trim() != "currentColor" { bx.outline_color = parse_color(v); }
    }
    if let Some(v) = s.get("outline-offset") { bx.outline_offset = parse_length(v); }
    if let Some(v) = s.get("margin-trim") { bx.margin_trim = v.trim().to_string(); }
    if let Some(v) = s.get("anchor-name") { bx.anchor_name = v.trim().to_string(); }
    if let Some(v) = s.get("position-anchor") { bx.position_anchor = v.trim().to_string(); }
    if let Some(v) = s.get("inset-area") { bx.inset_area = v.trim().to_string(); }
    if let Some(v) = s.get("animation-timeline") { bx.animation_timeline = v.trim().to_string(); }
    if let Some(v) = s.get("scroll-timeline-name") { bx.scroll_timeline_name = v.trim().to_string(); }
    if let Some(v) = s.get("scroll-timeline-axis") { bx.scroll_timeline_axis = v.trim().to_string(); }
    if let Some(v) = s.get("view-timeline-name") { bx.view_timeline_name = v.trim().to_string(); }
    if let Some(v) = s.get("view-timeline-axis") { bx.view_timeline_axis = v.trim().to_string(); }
    if let Some(v) = s.get("view-timeline-inset") { bx.view_timeline_inset = v.trim().to_string(); }
    if let Some(v) = s.get("view-transition-name") { bx.view_transition_name = v.trim().to_string(); }
    if let Some(v) = s.get("container-type") { bx.container_type = v.trim().to_string(); }
    if let Some(v) = s.get("container-name") { bx.container_name = v.trim().to_string(); }
    if let Some(v) = s.get("page-break-before") { bx.page_break_before = v.trim().to_string(); }
    if let Some(v) = s.get("page-break-after")  { bx.page_break_after  = v.trim().to_string(); }
    if let Some(v) = s.get("page-break-inside") { bx.page_break_inside = v.trim().to_string(); }
    if let Some(v) = s.get("break-before") { bx.break_before = v.trim().to_string(); }
    if let Some(v) = s.get("break-after")  { bx.break_after  = v.trim().to_string(); }
    if let Some(v) = s.get("break-inside") { bx.break_inside = v.trim().to_string(); }
    if let Some(v) = s.get("orphans") { bx.orphans = v.trim().parse().unwrap_or(2); }
    if let Some(v) = s.get("widows") { bx.widows = v.trim().parse().unwrap_or(2); }
    if let Some(v) = s.get("counter-set") { bx.counter_set = parse_counter(v); }
    if let Some(v) = s.get("print-color-adjust") { bx.print_color_adjust = v.trim().to_string(); }
    if let Some(v) = s.get("forced-color-adjust") { bx.forced_color_adjust = v.trim().to_string(); }
    if let Some(v) = s.get("font-synthesis") { bx.font_synthesis = v.trim().to_string(); }
    if let Some(v) = s.get("font-kerning") { bx.font_kerning = v.trim().to_string(); }
    if let Some(v) = s.get("font-language-override") { bx.font_language_override = v.trim().to_string(); }
    if let Some(v) = s.get("font-optical-sizing") { bx.font_optical_sizing = v.trim().to_string(); }
    if let Some(v) = s.get("font-smooth").or(s.get("-webkit-font-smoothing")) { bx.font_smooth = v.trim().to_string(); }
    if let Some(v) = s.get("white-space-collapse") { bx.white_space_collapse = v.trim().to_string(); }
    if let Some(v) = s.get("text-spacing-trim") { bx.text_spacing_trim = v.trim().to_string(); }
    if let Some(v) = s.get("text-size-adjust").or(s.get("-webkit-text-size-adjust")) { bx.text_size_adjust = v.trim().to_string(); }
    if let Some(v) = s.get("line-height-step") { bx.line_height_step = parse_length(v); }
    if let Some(v) = s.get("math-style") { bx.math_style = v.trim().to_string(); }
    if let Some(v) = s.get("math-depth") { bx.math_depth = v.trim().to_string(); }
    if let Some(v) = s.get("speak") { bx.speak = v.trim().to_string(); }
    if let Some(v) = s.get("speak-as") { bx.speak_as = v.trim().to_string(); }
    if let Some(v) = s.get("bookmark-label") { bx.bookmark_label = v.trim().to_string(); }
    if let Some(v) = s.get("bookmark-level") { bx.bookmark_level = v.trim().to_string(); }
    if let Some(v) = s.get("bookmark-state") { bx.bookmark_state = v.trim().to_string(); }
    if let Some(v) = s.get("string-set") { bx.string_set = v.trim().to_string(); }
    if let Some(v) = s.get("float") { bx.float_value = v.trim().to_string(); }
    if let Some(v) = s.get("clear") { bx.clear_value = v.trim().to_string(); }
    if let Some(v) = s.get("object-fit") { bx.object_fit = ObjectFit::parse(v); }
    if let Some(v) = s.get("object-position") { bx.object_position = v.trim().to_string(); }
    if let Some(v) = s.get("background-blend-mode") { bx.background_blend_mode = v.trim().to_string(); }
    if let Some(v) = s.get("image-rendering") { bx.image_rendering = ImageRendering::parse(v); }
    if let Some(v) = s.get("table-layout") { bx.table_layout = TableLayout::parse(v); }
    if let Some(v) = s.get("border-collapse") { bx.border_collapse = BorderCollapse::parse(v); }
    if let Some(v) = s.get("border-spacing") { bx.border_spacing = v.trim().to_string(); }
    if let Some(v) = s.get("caption-side") { bx.caption_side = v.trim().to_string(); }
    if let Some(v) = s.get("empty-cells") { bx.empty_cells = v.trim().to_string(); }
    if let Some(v) = s.get("vertical-align") { bx.vertical_align = v.trim().to_string(); }
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
    // Transform - single + chain
    if let Some(tr) = s.get("transform") {
        bx.transform = parse_transform(tr);
        bx.transforms = parse_transform_chain(tr);
    }
    if let Some(c) = s.get("color") {
        bx.text_color = parse_color(c);
    }

    // Padding / margin / border-width - prefer expanded shorthand
    let padding_v = s.get("padding-top").or(s.get("padding"));
    if let Some(p) = padding_v { bx.padding = parse_length(p); }
    // Asymmetricke padding longhands (musime preferovat pred shorthand `padding`).
    if let Some(p) = s.get("padding-top")    { bx.padding_top    = Some(parse_length(p)); }
    if let Some(p) = s.get("padding-right")  { bx.padding_right  = Some(parse_length(p)); }
    if let Some(p) = s.get("padding-bottom") { bx.padding_bottom = Some(parse_length(p)); }
    if let Some(p) = s.get("padding-left")   { bx.padding_left   = Some(parse_length(p)); }
    let margin_v = s.get("margin-top").or(s.get("margin"));
    if let Some(m) = margin_v { bx.margin = parse_length(m); }
    if let Some(m) = s.get("margin-top")    { bx.margin_top    = Some(parse_length(m)); }
    if let Some(m) = s.get("margin-right")  { bx.margin_right  = Some(parse_length(m)); }
    if let Some(m) = s.get("margin-bottom") { bx.margin_bottom = Some(parse_length(m)); }
    if let Some(m) = s.get("margin-left")   { bx.margin_left   = Some(parse_length(m)); }
    if let Some(b) = s.get("border-width") { bx.border_width = parse_length(b); }
    if let Some(bc) = s.get("border-color") { bx.border_color = parse_color(bc); }
    if let Some(bs) = s.get("border-style") { bx.border_style = bs.trim().to_string(); }
    if let Some(fs) = s.get("font-size") { bx.font_size = parse_length(fs); bx.font_size_explicit = true; }
    // Text align
    if let Some(ta) = s.get("text-align") {
        bx.text_align = match ta.trim() {
            "center"  => TextAlign::Center,
            "right"   => TextAlign::Right,
            "justify" => TextAlign::Justify,
            _ => TextAlign::Left,
        };
    }
    // Font weight - numeric 1..1000 + keywords (normal=400, bold=700, lighter, bolder).
    if let Some(fw) = s.get("font-weight") {
        let v = fw.trim();
        let weight: u32 = if let Ok(n) = v.parse::<u32>() {
            n
        } else {
            match v {
                "bold" => 700,
                "bolder" => 700,
                "lighter" => 300,
                "normal" => 400,
                _ => 400,
            }
        };
        bx.font_weight = weight;
        bx.bold = weight >= 600;
    }
    // Font style: italic / oblique.
    if let Some(fs) = s.get("font-style") {
        let v = fs.trim();
        bx.italic = v == "italic" || v == "oblique";
    }
    // Border radius
    if let Some(br) = s.get("border-radius") {
        bx.border_radius = parse_length(br);
    }
    // Explicit width / height z CSS (auto = None, min/max-content = special).
    // min-content: shrink-to-fit odhadem z textu. max-content: max-intrinsic.
    // fit-content: clamp na dostupnou sirku.
    if let Some(w) = s.get("width") {
        let v = w.trim();
        match v {
            "auto" => {}
            "min-content" | "max-content" | "fit-content" => {
                // Keyword ulozena pro layout_block - odhadneme per content
                let text_w = bx.text.as_deref().map(|t| measure_text_width(t, bx.font_size)).unwrap_or(0.0);
                bx.explicit_width = Some(text_w + 2.0 * bx.padding);
            }
            // Procenta ukladame jako pomer (0..1) pro pozdejsi resolve proti
            // parent inner_w v layout pass. parse_length na "100%" vracelo
            // 16 (default parent_size), coz davalo body height/width = 16
            // misto plne velikost viewport.
            _ if v.ends_with('%') => {
                let pct_str = &v[..v.len() - 1];
                if let Ok(p) = pct_str.parse::<f32>() {
                    bx.width_pct = Some(p / 100.0);
                }
            }
            _ => {
                let px = parse_length(v);
                if px > 0.0 { bx.explicit_width = Some(px); }
            }
        }
    }
    if let Some(h) = s.get("height") {
        let v = h.trim();
        if v != "auto" {
            if v.ends_with('%') {
                let pct_str = &v[..v.len() - 1];
                if let Ok(p) = pct_str.parse::<f32>() {
                    bx.height_pct = Some(p / 100.0);
                }
            } else {
                let px = parse_length(v);
                if px > 0.0 { bx.explicit_height = Some(px); }
            }
        }
    }
    // Length values - parsed into typed CssLength on cascade.
    if let Some(v) = s.get("min-width") { bx.min_width = CssLength::parse(v); }
    if let Some(v) = s.get("max-width") { bx.max_width = CssLength::parse(v); }
    if let Some(v) = s.get("min-height") { bx.min_height = CssLength::parse(v); }
    if let Some(v) = s.get("max-height") { bx.max_height = CssLength::parse(v); }
    // Line-height: cislo (multiplier) nebo length (px)
    if let Some(lh) = s.get("line-height") {
        let trimmed = lh.trim();
        if let Ok(num) = trimmed.parse::<f32>() {
            bx.line_height = num;
            bx.line_height_explicit = true;
        } else if trimmed.ends_with("px") || trimmed.ends_with("em") || trimmed.ends_with("rem") {
            // V px - prevest na multiplier
            let px = parse_length(trimmed);
            if bx.font_size > 0.0 {
                bx.line_height = px / bx.font_size;
                bx.line_height_explicit = true;
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
    // z-index: integer pro stacking order. "auto" / parse fail = None.
    if let Some(zv) = s.get("z-index") {
        let zt = zv.trim();
        if zt != "auto" {
            if let Ok(n) = zt.parse::<i32>() {
                bx.z_index = Some(n);
            }
        }
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
    // overflow shorthand: "hidden" | "visible" | "scroll" | "auto" | "clip"
    // alebo dve hodnoty "hidden auto" (x y).
    if let Some(ov) = s.get("overflow") {
        let t = ov.trim();
        bx.overflow_hidden = matches!(t, "hidden" | "clip");
        let parts: Vec<&str> = t.split_whitespace().collect();
        match parts.len() {
            1 => {
                bx.overflow_x = Overflow::parse(parts[0]);
                bx.overflow_y = Overflow::parse(parts[0]);
            }
            2 => {
                bx.overflow_x = Overflow::parse(parts[0]);
                bx.overflow_y = Overflow::parse(parts[1]);
            }
            _ => {}
        }
    }
    if let Some(ox) = s.get("overflow-x") {
        bx.overflow_x = Overflow::parse(ox);
        if bx.overflow_x.hides() { bx.overflow_hidden = true; }
    }
    if let Some(oy) = s.get("overflow-y") {
        bx.overflow_y = Overflow::parse(oy);
        if bx.overflow_y.hides() { bx.overflow_hidden = true; }
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

    // Counter API: aplikuj counter-reset + counter-increment pred pseudo-elements
    for (name, n) in &bx.counter_reset {
        counters.insert(name.clone(), *n);
    }
    for (name, n) in &bx.counter_increment {
        let cur = counters.get(name).copied().unwrap_or(0);
        counters.insert(name.clone(), cur + n);
    }

    // ::marker pro li - automaticky bullet/decimal podle list-style-type
    if bx.tag.as_deref() == Some("li") {
        let cur = counters.get("list-item").copied().unwrap_or(0) + 1;
        counters.insert("list-item".into(), cur);
        // list-style-image: pokud zadany url(), pouzit obrazek misto markeru
        if let Some(img_url) = &bx.list_style_image {
            if let Some(url_inner) = img_url.strip_prefix("url(").and_then(|s| s.strip_suffix(')')) {
                let cleaned = url_inner.trim().trim_matches('"').trim_matches('\'');
                let mut img_box = LayoutBox::new();
                img_box.display = Display::Inline;
                img_box.tag = Some("::marker".to_string());
                img_box.image_src = Some(cleaned.to_string());
                img_box.rect.width = bx.font_size;
                img_box.rect.height = bx.font_size;
                bx.children.push(img_box);
                return bx;
            }
        }
        // Default list-style-type: ol = decimal, ul = disc (CSS UA stylesheet).
        let parent_node = node.parent.borrow().upgrade();
        let parent_tag = parent_node.as_ref().and_then(|p| p.tag_name());
        let default_style = match parent_tag.as_deref() {
            Some("ol") => "decimal",
            _ => "disc",
        };
        // CSS list-style-type je inherited - pokud li nema explicit, podivej
        // se na rodicovsky <ol>/<ul> v style_map. Bez tohoto upper-roman na
        // ol class neaplikuje na li children.
        let parent_list_style = parent_node.as_ref().and_then(|p| {
            let pid = std::rc::Rc::as_ptr(p) as usize;
            style_map.get(&pid).and_then(|s| s.get("list-style-type")).cloned()
        });
        let style = if !bx.list_style_type.is_empty() {
            bx.list_style_type.as_str()
        } else if let Some(ref s) = parent_list_style {
            s.trim()
        } else {
            default_style
        };
        let marker_text = match style {
            "none" => String::new(),
            "decimal" => format!("{cur}. "),
            "decimal-leading-zero" => format!("{:02}. ", cur),
            "lower-roman" => format!("{}. ", to_roman(cur).to_lowercase()),
            "upper-roman" => format!("{}. ", to_roman(cur)),
            "lower-alpha" | "lower-latin" => {
                let c = ((b'a' + ((cur - 1) % 26) as u8) as char).to_string();
                format!("{c}. ")
            }
            "upper-alpha" | "upper-latin" => {
                let c = ((b'A' + ((cur - 1) % 26) as u8) as char).to_string();
                format!("{c}. ")
            }
            "circle" => "\u{25CB} ".to_string(),
            "square" => "\u{25A0} ".to_string(),
            _ /* disc default */ => "\u{2022} ".to_string(),
        };
        if !marker_text.is_empty() {
            let mut marker_box = LayoutBox::new();
            marker_box.display = Display::Inline;
            marker_box.tag = Some("::marker".to_string());
            marker_box.text = Some(marker_text);
            marker_box.text_color = bx.text_color;
            marker_box.font_size = bx.font_size;
            bx.children.push(marker_box);
        }
    }

    // Pseudo-element ::before - vlozit jako prvni virtualni child
    if let Some(pseudo_styles) = super::cascade::get_pseudo_styles(pseudo_map, node, "before") {
        if let Some(pb) = build_pseudo_box(node, pseudo_styles, counters) {
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
        // Per-element layout cache: pokud thread-local cache obsahuje uzel s
        // matching subtree fingerprint, reuznavame prev subtree (skip style/
        // struct rebuild). Pozice prepocteny v layout_dispatch.
        let cb = if let Some(cached) = cache_lookup_subtree(child, style_map) {
            cached
        } else {
            stacker::maybe_grow(32 * 1024, 8 * 1024 * 1024, || {
                let mut b = build_box_inner(child, style_map, pseudo_map, counters);
                b.fingerprint = compute_subtree_hash(child, style_map);
                b
            })
        };
        if cb.display != Display::None {
            // Text bez obsahu - zahodit
            if matches!(child.kind, NodeKind::Text(_)) && cb.text.is_none() {
                continue;
            }
            bx.children.push(cb);
        }
    }

    // Pseudo-element ::first-letter - rozdeli prvni inline text na (first_char, rest)
    if let Some(pseudo_styles) = super::cascade::get_pseudo_styles(pseudo_map, node, "first-letter") {
        // Najdi prvni inline child s text (skip leading whitespace text)
        for child in &mut bx.children {
            if matches!(child.display, Display::Inline) {
                if let Some(text) = child.text.clone() {
                    let trimmed = text.trim_start();
                    let leading_ws = text.len() - trimmed.len();
                    let mut chars = trimmed.chars();
                    if let Some(first_char) = chars.next() {
                        let rest: String = chars.collect();
                        // Vytvor pseudo box pro prvni char
                        let mut letter_text = String::new();
                        if leading_ws > 0 { letter_text.push_str(&text[..leading_ws]); }
                        letter_text.push(first_char);
                        let mut letter_box = LayoutBox::new();
                        letter_box.display = Display::Inline;
                        letter_box.tag = Some("::first-letter".to_string());
                        letter_box.text = Some(letter_text);
                        // Aplikuj pseudo styly
                        if let Some(c) = pseudo_styles.get("color") {
                            letter_box.text_color = parse_color(c);
                        }
                        if let Some(fs) = pseudo_styles.get("font-size") { letter_box.font_size = parse_length(fs); }
                        if let Some(fw) = pseudo_styles.get("font-weight") {
                            letter_box.bold = matches!(fw.trim(), "bold" | "700" | "800" | "900");
                        }
                        if let Some(bg) = pseudo_styles.get("background-color") {
                            letter_box.bg_color = parse_color(bg);
                        }
                        // Zkrátit puvodni text na rest
                        child.text = Some(rest);
                        // Insert pred child - vlozim na pozici child v collection
                        // (predchazejici cyklus bere prvni inline - vlozim hned).
                        let pos = bx.children.iter().position(|c|
                            matches!(c.display, Display::Inline)
                            && c.text.is_some()
                        ).unwrap_or(0);
                        bx.children.insert(pos, letter_box);
                        break;
                    }
                }
            }
        }
    }

    // Pseudo-element ::first-line - aproximace: prvni line obali styly do dalsiho text node
    // Plne implementace by potrebovala znat sirku line (po layout). Zde aproximace:
    // pridame styles pro ~prvni 50 chars textu.
    if let Some(pseudo_styles) = super::cascade::get_pseudo_styles(pseudo_map, node, "first-line") {
        for child in &mut bx.children {
            if matches!(child.display, Display::Inline) {
                if let Some(text) = child.text.clone() {
                    // Vyznacne v aktualni text box stylu (max prvnich ~50 chars)
                    let max = text.chars().take(50).count().min(text.len());
                    let split_idx = text.char_indices().take(max).last().map(|(i, c)| i + c.len_utf8()).unwrap_or(text.len());
                    let (first, rest) = text.split_at(split_idx);
                    let mut first_line_box = LayoutBox::new();
                    first_line_box.display = Display::Inline;
                    first_line_box.tag = Some("::first-line".to_string());
                    first_line_box.text = Some(first.to_string());
                    if let Some(c) = pseudo_styles.get("color") {
                        first_line_box.text_color = parse_color(c);
                    }
                    if let Some(fw) = pseudo_styles.get("font-weight") {
                        first_line_box.bold = matches!(fw.trim(), "bold" | "700" | "800" | "900");
                    }
                    child.text = Some(rest.to_string());
                    let pos = bx.children.iter().position(|c|
                        matches!(c.display, Display::Inline) && c.text.is_some()
                    ).unwrap_or(0);
                    bx.children.insert(pos, first_line_box);
                    break;
                }
            }
        }
    }

    // Pseudo-element ::after - posledni virtualni child
    if let Some(pseudo_styles) = super::cascade::get_pseudo_styles(pseudo_map, node, "after") {
        if let Some(pa) = build_pseudo_box(node, pseudo_styles, counters) {
            bx.children.push(pa);
        }
    }

    // ::placeholder - pro input/textarea: virtualni text child s placeholder textem
    let is_input_like = matches!(bx.tag.as_deref(), Some("input") | Some("textarea"));
    if is_input_like {
        if let Some(placeholder_text) = node.attr("placeholder") {
            let ph_styles = super::cascade::get_pseudo_styles(pseudo_map, node, "placeholder");
            let color = ph_styles
                .and_then(|s| s.get("color"))
                .and_then(|c| parse_color(c))
                .unwrap_or([169, 169, 169, 255]); // darkgray default
            bx.placeholder_color = Some(color);
            let mut ph_box = LayoutBox::new();
            ph_box.display = Display::Inline;
            ph_box.tag = Some("::placeholder".to_string());
            ph_box.text = Some(placeholder_text);
            ph_box.text_color = Some(color);
            ph_box.font_size = bx.font_size;
            if let Some(s) = ph_styles {
                if let Some(fs) = s.get("font-size") { ph_box.font_size = parse_length(fs); }
                if let Some(fw) = s.get("font-weight") {
                    ph_box.bold = matches!(fw.trim(), "bold" | "700" | "800" | "900");
                }
            }
            bx.children.push(ph_box);
        }
    }

    // ::selection - uloz barvy vyberu z pseudo map (aplikovano za behu pri renderovani)
    if let Some(sel_styles) = super::cascade::get_pseudo_styles(pseudo_map, node, "selection") {
        bx.selection_bg = sel_styles.get("background-color").and_then(|c| parse_color(c));
        bx.selection_color = sel_styles.get("color").and_then(|c| parse_color(c));
    }

    // ::backdrop - pro <dialog open>: vloz full-viewport backdrop box pred deti
    if bx.tag.as_deref() == Some("dialog") && node.attr("open").is_some() {
        let bd_styles = super::cascade::get_pseudo_styles(pseudo_map, node, "backdrop");
        let bg = bd_styles
            .and_then(|s| s.get("background-color"))
            .and_then(|c| parse_color(c))
            .unwrap_or([0, 0, 0, 128]); // pololpruhledna cerna default
        let mut backdrop_box = LayoutBox::new();
        backdrop_box.display = Display::Block;
        backdrop_box.tag = Some("::backdrop".to_string());
        backdrop_box.position = Position::Fixed;
        backdrop_box.offset_top = Some(0.0);
        backdrop_box.offset_left = Some(0.0);
        backdrop_box.bg_color = Some(bg);
        bx.children.insert(0, backdrop_box);
    }

    // Inherit font_size + line_height + bold/italic + colors do text node deti
    // (text nodes nemaji vlastni cascade entry). Bez teto inheritance flex/grid
    // pre-pass volaji intrinsic_content_width(text_node) s default font_size=16
    // misto realneho (napr. 0.7rem = 11.2). Vysledek: pre-pass merici sirka
    // mismatching s flush_inline merici sirkou -> spurious text wrap.
    let parent_fs = bx.font_size;
    let parent_lh = bx.line_height;
    let parent_bold = bx.bold;
    let parent_italic = bx.italic;
    let parent_color = bx.text_color;
    let parent_family = bx.font_family.clone();
    for ch in bx.children.iter_mut() {
        if ch.tag.is_none() {
            ch.font_size = parent_fs;
            if !ch.line_height_explicit {
                ch.line_height = parent_lh;
            }
            if !ch.bold { ch.bold = parent_bold; }
            if !ch.italic { ch.italic = parent_italic; }
            if ch.text_color.is_none() { ch.text_color = parent_color; }
            if ch.font_family.is_empty() { ch.font_family = parent_family.clone(); }
        }
    }

    bx
}

/// Vyrobi LayoutBox pro pseudo-element (::before / ::after) z computed styles.
/// Content property: "string", attr(name), counter(...) - implementovano: string a attr.
fn build_pseudo_box(parent_node: &Rc<Node>, styles: &HashMap<String, String>, counters: &HashMap<String, i32>) -> Option<LayoutBox> {
    let content_raw = styles.get("content")?;
    let text = parse_content_value(content_raw, parent_node, counters)?;
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
/// Unescape CSS string literal sequences (\\, \", \', \n, atd).
/// CSS spec section 4.3.7. Hex escapes \XXXXX (1-6 hex digits) jako Unicode CP.
fn unescape_css_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            // Hex escape: \XXXXXX optionally space-terminated
            if let Some(&next) = chars.peek() {
                if next.is_ascii_hexdigit() {
                    let mut hex = String::new();
                    for _ in 0..6 {
                        if let Some(&h) = chars.peek() {
                            if h.is_ascii_hexdigit() {
                                hex.push(h);
                                chars.next();
                            } else { break; }
                        } else { break; }
                    }
                    // Optional whitespace terminator
                    if let Some(&w) = chars.peek() {
                        if w == ' ' || w == '\t' || w == '\n' { chars.next(); }
                    }
                    if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                        if let Some(ch) = char::from_u32(cp) {
                            out.push(ch);
                            continue;
                        }
                    }
                    out.push_str(&hex);
                    continue;
                }
                // Single char escape: \", \', \\, \n, atd.
                chars.next();
                match next {
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    other => out.push(other),
                }
                continue;
            }
            // Trailing \: ignore
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_content_value(raw: &str, parent: &Rc<Node>, counters: &HashMap<String, i32>) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() || s == "none" || s == "normal" { return None; }

    // String literal - unescape \X sequences (\\, \", \', \n, etc).
    if let Some(stripped) = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return Some(unescape_css_string(stripped));
    }
    if let Some(stripped) = s.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        return Some(unescape_css_string(stripped));
    }

    // attr(name)
    if let Some(inner) = s.strip_prefix("attr(").and_then(|s| s.strip_suffix(')')) {
        let name = inner.trim();
        return parent.attr(name).or(Some(String::new()));
    }

    // counter(name) - vrati hodnotu z counter state
    if let Some(inner) = s.strip_prefix("counter(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
        let name = parts[0];
        let val = counters.get(name).copied().unwrap_or(0);
        return Some(val.to_string());
    }
    // counters(name, separator) - hierarchicky concat (zjednoduseni: jen aktualni)
    if let Some(inner) = s.strip_prefix("counters(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
        let name = parts[0];
        let val = counters.get(name).copied().unwrap_or(0);
        return Some(val.to_string());
    }

    // url(...) - placeholder
    if s.starts_with("url(") {
        return Some(String::new());
    }

    Some(raw.to_string())
}

/// Flex layout - vlastni implementace (taffy-inspired, MIT).
/// Dispatchuje do `super::layout_engine::flex::layout_flex`.
fn layout_flex(bx: &mut LayoutBox) {
    super::layout_engine::flex::layout_flex(bx);
}

/// Block layout: kazdy block dite je vlastni radek, sirka = parent.
/// Inline deti se sbiraji do "line boxu" a wrappuji.
/// Vertical writing-mode layout: deti se stackuji po ose X (lr = leva->prava, rl = prava->leva).
/// Inline (text) deti se take layoutuji svisle.
/// Recursivne posune cely LayoutBox subtree o (dx, dy). Pouziva se pri
/// Position::Relative offsetech a pri animation tick (left/top keyframes).
pub fn shift_subtree(bx: &mut LayoutBox, dx: f32, dy: f32) {
    bx.rect.x += dx;
    bx.rect.y += dy;
    for ch in bx.children.iter_mut() {
        shift_subtree(ch, dx, dy);
    }
}

fn layout_block_vertical(bx: &mut LayoutBox) {
    let inner_x = bx.rect.x + bx.padding + bx.border_width;
    let inner_y = bx.rect.y + bx.padding + bx.border_width;
    let inner_h = bx.rect.height - 2.0 * (bx.padding + bx.border_width);
    // Pro vertical-rl startujeme od prava; pro lr od leva.
    let right_to_left = matches!(bx.writing_mode, WritingMode::VerticalRl);

    let mut cursor_x = if right_to_left {
        inner_x + (bx.rect.width - 2.0 * (bx.padding + bx.border_width))
    } else {
        inner_x
    };

    for child in bx.children.iter_mut() {
        if matches!(child.display, Display::None) { continue; }
        let child_w = child.explicit_width.unwrap_or(20.0);
        let child_h = child.explicit_height.unwrap_or(inner_h);
        child.rect.height = child_h;
        child.rect.width  = child_w;
        if right_to_left {
            cursor_x -= child_w;
            child.rect.x = cursor_x;
        } else {
            child.rect.x = cursor_x;
            cursor_x += child_w;
        }
        child.rect.y = inner_y;
        layout_dispatch(child);
    }

    // Auto-vypocet sirky podle children
    let used_w = if right_to_left {
        (inner_x + bx.rect.width - 2.0 * (bx.padding + bx.border_width)) - cursor_x
    } else {
        cursor_x - inner_x
    };
    if bx.rect.width < used_w + 2.0 * (bx.padding + bx.border_width) {
        bx.rect.width = used_w + 2.0 * (bx.padding + bx.border_width);
    }
}

/// CSS Multi-column Layout L1 - rozdelit flow children do N rovnomernych sloupcu.
/// Heuristic balance: rozdeli children rovnomerne dle pocet (nejjednodussi -
/// real spec dela balance dle vyska pres iteracni algoritmus).
fn layout_block_multicol(bx: &mut LayoutBox) {
    let pad_l = bx.padding_left.unwrap_or(bx.padding);
    let pad_r = bx.padding_right.unwrap_or(bx.padding);
    let pad_t = bx.padding_top.unwrap_or(bx.padding);
    let pad_b = bx.padding_bottom.unwrap_or(bx.padding);
    let inner_x = bx.rect.x + pad_l + bx.border_width;
    let inner_y = bx.rect.y + pad_t + bx.border_width;
    let inner_w = bx.rect.width - pad_l - pad_r - 2.0 * bx.border_width;
    let n_cols = bx.column_count.max(1) as f32;
    let gap = bx.column_gap_multicol;
    // Sirka jedneho sloupce: (inner_w - gap*(n-1)) / n.
    let col_w = ((inner_w - gap * (n_cols - 1.0)) / n_cols).max(1.0);
    // Round-robin distribuce children do sloupcu.
    // Real CSS balance dle vyska: sum heights -> target = total/n; distribuje
    // children sekvencne, pri overfull preskoci na dalsi col. Aproximujeme.
    let n_children = bx.children.len();
    if n_children == 0 {
        // Empty container - inner_h zustane podle padding.
        if !bx.taffy_mode || bx.rect.height == 0.0 {
            bx.rect.height = pad_t + pad_b + 2.0 * bx.border_width;
        }
        return;
    }
    // Pre-pass: layout kazdy child as standalone (rect.x/y bude prepocitano)
    // pro znalost child.rect.height (potrebuju pri balance).
    for child in bx.children.iter_mut() {
        child.rect.x = inner_x;
        child.rect.y = inner_y;
        child.rect.width = col_w;
        if let Some(eh) = child.explicit_height {
            child.rect.height = eh;
        } else if child.rect.height == 0.0 && child.text.is_some() {
            child.rect.height = child.font_size * child.line_height;
        }
        layout_dispatch(child);
    }
    // Total content height pro target_h.
    let total_h: f32 = bx.children.iter().map(|c| {
        let m_t = c.margin_top.unwrap_or(c.margin);
        let m_b = c.margin_bottom.unwrap_or(c.margin);
        c.rect.height + m_t + m_b
    }).sum();
    let target_h = (total_h / n_cols).max(1.0);
    // Distribuce: greedy fill kazdeho sloupce do target_h.
    let mut col_idx = 0u32;
    let mut col_y = inner_y;
    let mut col_max_h = 0.0_f32;
    for child in bx.children.iter_mut() {
        let m_t = child.margin_top.unwrap_or(child.margin);
        let m_b = child.margin_bottom.unwrap_or(child.margin);
        let h = child.rect.height + m_t + m_b;
        // Pokud aktualni col by overfull a neni posledni, prejdi do dalsiho.
        if col_idx + 1 < bx.column_count && (col_y - inner_y) + h > target_h && (col_y - inner_y) > 0.0 {
            col_max_h = col_max_h.max(col_y - inner_y);
            col_idx += 1;
            col_y = inner_y;
        }
        let col_x = inner_x + (col_idx as f32) * (col_w + gap);
        child.rect.x = col_x;
        child.rect.y = col_y + m_t;
        child.rect.width = col_w;
        // Re-layout child s novou rect (column-internal flow).
        layout_dispatch(child);
        col_y += h;
        col_max_h = col_max_h.max(col_y - inner_y);
    }
    // Container vyska = nejvetsi sloupec + padding.
    if !bx.taffy_mode || bx.rect.height == 0.0 {
        bx.rect.height = pad_t + col_max_h + pad_b + 2.0 * bx.border_width;
    }
}

pub fn layout_block(bx: &mut LayoutBox) {
    // writing-mode: vertical-rl / vertical-lr - block axis zmena na X
    let vertical = bx.writing_mode.is_vertical();
    if vertical {
        layout_block_vertical(bx);
        return;
    }
    // Multi-column Layout L1: pri column-count > 1 rozdeli flow do N sloupcu.
    if bx.column_count > 1 {
        layout_block_multicol(bx);
        return;
    }
    // V taffy_mode: pamatuj puvodni height (uz nastaveny rodicem). Pak po vypoctu
    // content_h NEpresahnout puvodni hodnotu (parent height = constraint).
    let preset_height_taffy = if bx.taffy_mode { bx.rect.height } else { 0.0 };

    // Asymmetric padding wins, jinak shorthand `padding`. Margin je VNEJSI
    // padding (mezi boxy), neovlivnuje inner content area. inner_w = rect.width
    // - padding - border_width. Margin uz aplikoval parent layout pri pozicovani
    // tohoto boxu.
    let pad_l = bx.padding_left.unwrap_or(bx.padding);
    let pad_r = bx.padding_right.unwrap_or(bx.padding);
    let pad_t = bx.padding_top.unwrap_or(bx.padding);
    let inner_x = bx.rect.x + pad_l + bx.border_width;
    let inner_y = bx.rect.y + pad_t + bx.border_width;
    let inner_w = bx.rect.width - pad_l - pad_r - 2.0 * bx.border_width;

    let mut cursor_y = inner_y;
    // Inline run - sbiraji se inline boxy do line buffer, flush pri block child nebo konci
    let mut inline_buffer: Vec<usize> = Vec::new();
    // CSS margin collapse: adjacent vertical margins se nesakcaji, ale beru max.
    let mut prev_margin_bottom: f32 = 0.0;
    let mut had_prev_block = false;

    // CSS Floats L1 - aktivni floats v aktualnim block formating context.
    // (x, y, width, height, side: 'l'/'r'). Block siblings shrink x/width o
    // floats prekryvajici cursor_y. Cleared pri clear: left/right/both.
    let mut floats: Vec<(f32, f32, f32, f32, char)> = Vec::new();
    let active_float_bounds = |y: f32, floats: &[(f32, f32, f32, f32, char)],
                               inner_x: f32, inner_w: f32| -> (f32, f32) {
        // Vraci (left_offset, available_width) pro radek na pozici y.
        let mut left_off = 0.0_f32;
        let mut right_off = 0.0_f32;
        for (fx, fy, fw, fh, side) in floats {
            if y + 1.0 >= *fy && y < *fy + *fh {
                if *side == 'l' {
                    left_off = left_off.max(fx + fw - inner_x);
                } else {
                    right_off = right_off.max((inner_x + inner_w) - fx);
                }
            }
        }
        let avail = (inner_w - left_off - right_off).max(0.0);
        (left_off, avail)
    };

    let mut i = 0;
    while i < bx.children.len() {
        let display = bx.children[i].display;
        let float_v = bx.children[i].float_value.clone();
        let clear_v = bx.children[i].clear_value.clone();

        // CSS Position: absolute / fixed - out-of-flow. Treat as block, skip flow
        // advance. Bez tohoto byl display:inline + position:absolute prvek
        // (napr. .login-section #lost_pwd_button) zacleneny do inline flow a
        // jeho intrinsic h=100 nafouklo parent content height.
        if matches!(bx.children[i].position, Position::Absolute | Position::Fixed) {
            if !inline_buffer.is_empty() {
                cursor_y = flush_inline(bx, &inline_buffer, inner_x, cursor_y, inner_w);
                inline_buffer.clear();
            }
            // Fixed: CB = viewport, ne parent.
            let is_fixed = matches!(bx.children[i].position, Position::Fixed);
            let (vw, vh) = super::cascade::MATH_VIEWPORT.with(|c| *c.borrow());
            let (cb_x, cb_y, cb_w, cb_h) = if is_fixed && vw > 0.0 && vh > 0.0 {
                (0.0, 0.0, vw, vh)
            } else {
                (inner_x, inner_y, inner_w, bx.rect.height
                    - bx.padding_top.unwrap_or(bx.padding)
                    - bx.padding_bottom.unwrap_or(bx.padding)
                    - 2.0 * bx.border_width)
            };
            let child = &mut bx.children[i];
            // Width: explicit / auto -> shrink-to-fit (use cb_w upper bound).
            child.rect.width = child.explicit_width.unwrap_or_else(|| {
                if let Some(p) = child.width_pct { cb_w * p } else { cb_w }
            });
            if let Some(eh) = child.explicit_height {
                child.rect.height = eh;
            } else if let Some(p) = child.height_pct {
                // height: N% na abs/fixed -> resolvuj proti CB height. Bez toho
                // photo-box (position:absolute; height:100%) zustal h=0, img
                // uvnitr cetl parent_h=0 a spadl na advance_h=24.
                child.rect.height = cb_h * p;
            }
            // Apply offset (top/left/right/bottom) relative to CB.
            child.rect.x = cb_x + child.offset_left.unwrap_or(0.0);
            child.rect.y = cb_y + child.offset_top.unwrap_or(0.0);
            if let Some(r) = child.offset_right {
                child.rect.x = cb_x + cb_w - r - child.rect.width;
            }
            // Treat display:inline as block for layout (inline OOF rare edge case).
            let saved_display = child.display;
            if matches!(child.display, Display::Inline) {
                child.display = Display::Block;
            }
            layout_dispatch(child);
            child.display = saved_display;
            if let Some(b) = bx.children[i].offset_bottom {
                let h = bx.children[i].rect.height;
                bx.children[i].rect.y = cb_y + cb_h - b - h;
            }
            i += 1;
            continue;
        }

        // CSS clear: posun cursor_y pod aktivni floats odpovidajici strany.
        if clear_v == "left" || clear_v == "right" || clear_v == "both" {
            for (_fx, fy, _fw, fh, side) in &floats {
                let cleared = match clear_v.as_str() {
                    "left" => *side == 'l',
                    "right" => *side == 'r',
                    "both" => true,
                    _ => false,
                };
                if cleared {
                    cursor_y = cursor_y.max(fy + fh);
                }
            }
        }

        // Float left/right: pozicovani na inner edge, advance cursor_y NE.
        if (float_v == "left" || float_v == "right") && display != Display::None {
            if !inline_buffer.is_empty() {
                cursor_y = flush_inline(bx, &inline_buffer, inner_x, cursor_y, inner_w);
                inline_buffer.clear();
            }
            let child = &mut bx.children[i];
            let m_l = child.margin_left.unwrap_or(child.margin);
            let m_r = child.margin_right.unwrap_or(child.margin);
            let m_t = child.margin_top.unwrap_or(child.margin);
            let cw = child.explicit_width.unwrap_or(100.0).max(1.0);
            let ch = child.explicit_height.unwrap_or(child.rect.height.max(50.0));
            child.rect.width = cw;
            child.rect.height = ch;
            child.rect.y = cursor_y + m_t;
            if float_v == "left" {
                let (left_off, _) = active_float_bounds(cursor_y, &floats, inner_x, inner_w);
                child.rect.x = inner_x + left_off + m_l;
            } else {
                let mut right_off = 0.0_f32;
                for (fx, fy, _fw, fh, side) in &floats {
                    if cursor_y + 1.0 >= *fy && cursor_y < *fy + *fh && *side == 'r' {
                        right_off = right_off.max((inner_x + inner_w) - fx);
                    }
                }
                child.rect.x = inner_x + inner_w - right_off - cw - m_r;
            }
            layout_dispatch(child);
            let side = if float_v == "left" { 'l' } else { 'r' };
            floats.push((child.rect.x, child.rect.y, cw + m_l + m_r,
                ch + m_t + child.margin_bottom.unwrap_or(child.margin), side));
            i += 1;
            continue;
        }

        match display {
            Display::Block | Display::Flex | Display::Grid
            | Display::ListItem | Display::Table | Display::TableHeader
            | Display::TableRow
            | Display::TableCell | Display::TableHeaderCell | Display::TableCaption
            | Display::Subgrid => {
                if !inline_buffer.is_empty() {
                    cursor_y = flush_inline(bx, &inline_buffer, inner_x, cursor_y, inner_w);
                    inline_buffer.clear();
                    had_prev_block = false;
                    prev_margin_bottom = 0.0;
                }
                let child = &mut bx.children[i];
                // Effective margin: asymmetric (margin_top/right/bottom/left) wins, jinak shorthand `margin`.
                let m_t = child.margin_top.unwrap_or(child.margin);
                let m_b = child.margin_bottom.unwrap_or(child.margin);
                let m_l = child.margin_left.unwrap_or(child.margin);
                let m_r = child.margin_right.unwrap_or(child.margin);
                // Margin collapse: m_t aplikuje se jen do max(prev_m_b, m_t).
                // Pokud prev_m_b uz byla pricteta, sktorta o min(prev_m_b, m_t).
                let collapsed_m_t = if had_prev_block { (m_t - prev_margin_bottom).max(0.0) } else { m_t };
                // Aktivni float bounds na cursor_y (CSS Float L1).
                let (float_left_off, float_avail_w) = active_float_bounds(
                    cursor_y + collapsed_m_t, &floats, inner_x, inner_w);
                child.rect.x = inner_x + float_left_off + m_l;
                child.rect.y = cursor_y + collapsed_m_t;
                // Width: explicit_width (px), pak width_pct (% z parent inner_w),
                // pak fall-back na full available width.
                child.rect.width = if let Some(px) = child.explicit_width {
                    px
                } else if let Some(pct) = child.width_pct {
                    (float_avail_w - m_l - m_r) * pct
                } else {
                    float_avail_w - m_l - m_r
                };
                // Clamp dle min-width / max-width. Procenta resolve proti
                // parent inner_w (jinak parse_length da default 16 -> clamp
                // na 16 a layout cely zkolaboval).
                // (Drive lokalni `pct_or_len` helper - zastaraly, CssLength dela
                // totez s spravnym unit handlingem.)
                let parent_w_for_pct = float_avail_w - m_l - m_r;
                let cw_ctx_b = ResolveCtx { parent_size: parent_w_for_pct, font_size: child.font_size, ..Default::default() };
                if child.min_width.is_specified() {
                    let mn = child.min_width.resolve(&cw_ctx_b);
                    if mn > 0.0 { child.rect.width = child.rect.width.max(mn); }
                }
                if child.max_width.is_specified() {
                    let mx = child.max_width.resolve(&cw_ctx_b);
                    if mx > 0.0 { child.rect.width = child.rect.width.min(mx); }
                }
                // explicit_height z CSS height prop; jinak height_pct (% z parent height);
                // jinak auto (content-based).
                if let Some(eh) = child.explicit_height {
                    child.rect.height = eh;
                } else if let Some(pct) = child.height_pct {
                    // Resolvuje proti parent rect.height. Pri parent height=auto
                    // (0 / unknown), zustane 0 a fallback do content-based.
                    let parent_h = bx.rect.height
                        - bx.padding_top.unwrap_or(bx.padding)
                        - bx.padding_bottom.unwrap_or(bx.padding)
                        - 2.0 * bx.border_width;
                    if parent_h > 0.0 {
                        child.rect.height = parent_h * pct;
                    }
                } else if child.rect.height == 0.0 {
                    child.rect.height = if child.text.is_some() {
                        child.font_size * child.line_height + child.padding * 2.0
                    } else if child.taffy_mode && child.children.is_empty() {
                        // Taffy mode: prazdny leaf div ma 0 vysku (CSS spec).
                        0.0
                    } else {
                        20.0
                    };
                }
                // Clamp dle min-height / max-height. Percent resolve proti
                // parent inner_h (cely block layout fix per width/height %).
                let parent_h_for_pct = (bx.rect.height
                    - bx.padding_top.unwrap_or(bx.padding)
                    - bx.padding_bottom.unwrap_or(bx.padding)
                    - 2.0 * bx.border_width).max(0.0);
                let ch_ctx_b = ResolveCtx { parent_size: parent_h_for_pct, font_size: child.font_size, ..Default::default() };
                if child.min_height.is_specified() {
                    let mn = child.min_height.resolve(&ch_ctx_b);
                    if mn > 0.0 { child.rect.height = child.rect.height.max(mn); }
                }
                if child.max_height.is_specified() {
                    let mx = child.max_height.resolve(&ch_ctx_b);
                    if mx > 0.0 { child.rect.height = child.rect.height.min(mx); }
                }
                layout_dispatch(child);
                // Re-clamp PO layout_dispatch: layout_block / layout_flex set
                // rect.height na intrinsic content height a tim prepisly pre-
                // clamp min-height. CSS spec: min-height = lower bound vzdy.
                // Bez tohoto .top-container { min-height: 93px } po layout
                // s short content (cca 60px) zustal 60 misto 93.
                if child.min_height.is_specified() {
                    let mn = child.min_height.resolve(&ch_ctx_b);
                    if mn > 0.0 { child.rect.height = child.rect.height.max(mn); }
                }
                if child.max_height.is_specified() {
                    let mx = child.max_height.resolve(&ch_ctx_b);
                    if mx > 0.0 { child.rect.height = child.rect.height.min(mx); }
                }

                // TableRow: cells maji vyssi rect.height nez tr default (20px)
                // diky padding. Pri rendering vyssi cell prelize do dalsi rady
                // a borders prochazi pres text. Bubble max child height do tr.
                if matches!(child.display, Display::TableRow) && child.explicit_height.is_none() {
                    let max_cell_h = child.children.iter()
                        .filter(|c| matches!(c.display, Display::TableCell | Display::TableHeaderCell))
                        .map(|c| c.rect.height)
                        .fold(0.0f32, f32::max);
                    if max_cell_h > child.rect.height {
                        child.rect.height = max_cell_h;
                        // Cells musia mit stejnou height (align stretch).
                        for c in child.children.iter_mut() {
                            if matches!(c.display, Display::TableCell | Display::TableHeaderCell) {
                                c.rect.height = max_cell_h;
                            }
                        }
                    }
                }

                // Apply position offsety
                // Sticky position se chova jako Relative pri default scroll - JE v normal
                // flow (zabira misto). Drive: Sticky vyjmut -> nasledujici sibling
                // overlap (header h=48 + page se kreslila pri y=0 stejne).
                let is_in_flow = matches!(child.position,
                    Position::Static | Position::Relative | Position::Sticky);
                match child.position {
                    Position::Relative => {
                        let dy = child.offset_top.unwrap_or(0.0);
                        let dx = child.offset_left.unwrap_or(0.0);
                        if dx != 0.0 || dy != 0.0 {
                            // Shift child + ALL descendants. layout_dispatch
                            // uz pozicovalo descendants relativne k child.rect,
                            // takze pri Position::Relative musime cely subtree
                            // posunout (jinak by text uvnitr animovaneho elem
                            // zustal staticky).
                            shift_subtree(child, dx, dy);
                        }
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
                    cursor_y += child.rect.height + collapsed_m_t + m_b;
                    prev_margin_bottom = m_b;
                    had_prev_block = true;
                }
                // Absolute/fixed neposunuji cursor_y - jsou out of flow
            }
            Display::Inline | Display::InlineBlock | Display::Contents
            | Display::Ruby
            | Display::InlineFlex | Display::InlineGrid => {
                inline_buffer.push(i);
            }
            Display::None => {}
        }
        i += 1;
    }
    if !inline_buffer.is_empty() {
        cursor_y = flush_inline(bx, &inline_buffer, inner_x, cursor_y, inner_w);
    }

    // Auto-vypocet vysky podle children. Asymmetric padding pres _top/_bottom
    // (jinak shorthand `padding`). border_width pridava na obe strany.
    let content_h = cursor_y - inner_y;
    let block_pad_t = bx.padding_top.unwrap_or(bx.padding);
    let block_pad_b = bx.padding_bottom.unwrap_or(bx.padding);
    let bound = content_h + block_pad_t + block_pad_b + 2.0 * bx.border_width;
    // Bound > 0 + bez explicit_height + bez taffy_mode preset + ma node (skip
    // umely layout_root): override misto grow-only. Bez tohoto by stale h (z
    // pre-pass kdy parent mel mensi width) prevazila spravne novy bound.
    // - bound == 0 (empty block bez obsahu): zachovat (placeholder 20 z parent
    //   iter pro empty divs - HTML5 spec compatible).
    // - layout_root (bez node): zachovat viewport height set extension.
    let preserve_grow = bx.explicit_height.is_some()
        || (bx.taffy_mode && preset_height_taffy > 0.0)
        || (bound == 0.0 && bx.children.is_empty() && bx.text.is_none())
        || bx.tag.is_none()
        || matches!(bx.tag.as_deref(), Some("html") | Some("body"));
    if preserve_grow {
        if bx.rect.height < bound {
            bx.rect.height = bound;
        }
    } else {
        bx.rect.height = bound;
    }
    // V taffy_mode: pokud rodic ji uz nastavil (preset > 0), nepresahnout (parent
    // constraint - flex/grid item v constrained kontextu nesmi rust nad parent).
    if bx.taffy_mode && preset_height_taffy > 0.0 {
        bx.rect.height = preset_height_taffy;
    }
}

/// Flush inline buffer: rozmista inline boxy s wrapem.
/// Vraci new cursor_y po vsech radkach.
fn flush_inline(bx: &mut LayoutBox, indices: &[usize], inner_x: f32, start_y: f32, inner_w: f32) -> f32 {
    let mut cursor_x = inner_x;
    let mut cursor_y = start_y;
    let parent_font_size = bx.font_size;
    let parent_bold = bx.bold;
    let parent_weight = bx.font_weight;
    let parent_italic = bx.italic;
    let parent_color = bx.text_color;
    let parent_underline = bx.text_underline;
    let parent_strikethrough = bx.text_strikethrough;
    let parent_decor_style = bx.text_decoration_style.clone();
    let parent_decor_color = bx.text_decoration_color;
    let parent_font_family = bx.font_family.clone();
    let parent_line_height = bx.line_height;
    let line_height_default = parent_font_size * 1.2;
    let mut line_height = line_height_default;
    // Tracking whitespace boundary mezi sousednimi inline siblings.
    let mut prev_had_trailing_space = false;

    for (sib_idx, &idx) in indices.iter().enumerate() {
        // Inherit inheritable CSS props od parentu pri text/inline children co
        // je nemaji explicit. Inheritable per CSS spec: color, font-*, text-*.
        // Text node deti (tag=None) nemaji vlastni cascade entry - vzdy dedi
        // vse od parentu. Inline elementy maji svoji cascade entry - dedi jen
        // pokud sami nemaji explicit value.
        let is_text_node = bx.children[idx].tag.is_none();
        // Text node: vzdy override font_size z parentu (nema vlastni cascade).
        // Inline element: jen kdyz nema explicit font-size z cascade
        // (font_size_explicit flag - drive sentinel test selhal pri user
        // CSS font-size: 16px).
        if is_text_node || !bx.children[idx].font_size_explicit {
            bx.children[idx].font_size = parent_font_size;
        }
        if !bx.children[idx].bold && parent_bold {
            bx.children[idx].bold = parent_bold;
        }
        // font_weight inherit (pri 400 default = ne-explicit).
        if bx.children[idx].font_weight == 400 && parent_weight != 400 {
            bx.children[idx].font_weight = parent_weight;
        }
        if !bx.children[idx].italic && parent_italic {
            bx.children[idx].italic = true;
        }
        if bx.children[idx].text_color.is_none() && parent_color.is_some() {
            bx.children[idx].text_color = parent_color;
        }
        if !bx.children[idx].text_underline && parent_underline {
            bx.children[idx].text_underline = true;
        }
        if !bx.children[idx].text_strikethrough && parent_strikethrough {
            bx.children[idx].text_strikethrough = true;
        }
        if bx.children[idx].text_decoration_style.is_empty() && !parent_decor_style.is_empty() {
            bx.children[idx].text_decoration_style = parent_decor_style.clone();
        }
        if bx.children[idx].text_decoration_color.is_none() && parent_decor_color.is_some() {
            bx.children[idx].text_decoration_color = parent_decor_color;
        }
        if bx.children[idx].font_family.is_empty() && !parent_font_family.is_empty() {
            bx.children[idx].font_family = parent_font_family.clone();
        }
        // line-height inherited (CSS spec). Text node ma default 1.4 nebo 1.2,
        // pri parentu s explicit line-height (60px na .anim-box / .filter-box)
        // text uvnitr potrebuje stejnou line-height pro vertical center
        // (paint v_offset z line_height_px).
        let is_text_node_lh = bx.children[idx].tag.is_none();
        if is_text_node_lh || !bx.children[idx].line_height_explicit {
            bx.children[idx].line_height = parent_line_height;
        }
        let bx_clone = bx.children[idx].clone();
        let font_size = bx_clone.font_size;
        // CSS normal line-height = 1.2 (browsers convention). 1.4 byl prilis
        // velky padding -> tlacitka mela visual extra space below text.
        // Pri smaller-font inline elementu (.btn 14 v body 16) advance_h pouzij
        // bx_clone vlastni line_height, ne parent default.
        let own_lh_px = bx_clone.line_height * font_size;
        let advance_h = own_lh_px.max(font_size * 1.2);
        // Inline replaced element s explicit_height (svg, img s height attr,
        // canvas, iframe) - line_height musi pokryt celou jejich vysku, jinak
        // section content_h ignoruje a section nedosahne pod SVG/img.
        let line_h_for_this = if let Some(eh) = bx_clone.explicit_height {
            advance_h.max(eh)
        } else {
            advance_h
        };
        line_height = line_height.max(line_h_for_this);
        // space_w odpovida realne glyph width mezery v aktualnim fontu - sjednoceni
        // s measure_text_width(t), ktery v intrinsic_content_width pre-pass meri
        // celou string vc. space glyfu. Bez tohoto byly intrinsic measure (s glyph
        // space) a flush_inline measure (s synthetic 0.27*fs) rozdilne -> spurious
        // text wrap kdyz pre-pass dal min sirku nez flush_inline potrebuje.
        let space_w = {
            let g = measure_text_width_weight(" ", font_size, bx_clone.font_weight, bx_clone.italic, &bx_clone.font_family);
            if g > 0.0 { g } else { font_size * 0.27 }
        };

        // <br> = force linebreak. CSS spec: prazdny inline element ktery
        // vyemituje newline na konci current line + cursor reset na start
        // dalsi line. Bez tohoto br se choval jako prazdny inline = ignored.
        if bx_clone.tag.as_deref() == Some("br") {
            // Zarid aby br box mel non-zero rect pro hit-test / devtools.
            bx.children[idx].rect.x = cursor_x;
            bx.children[idx].rect.y = cursor_y;
            bx.children[idx].rect.width = 0.0;
            bx.children[idx].rect.height = advance_h;
            cursor_y += line_height;
            cursor_x = inner_x;
            line_height = line_height_default;
            prev_had_trailing_space = false;
            continue;
        }

        if let Some(text) = &bx_clone.text {
            // Detect leading/trailing whitespace pro spravne mezery na hranicich.
            // NBSP (U+00A0) NEPOCITAM jako whitespace boundary - "x\u{A0}" konci
            // NBSP ktery musi zustat cast slova, ne sibling space.
            let is_brk = |c: char| c.is_whitespace() && c != '\u{00A0}';
            let leading_ws = text.starts_with(is_brk);
            let trailing_ws = text.ends_with(is_brk);
            // Pre-pridame mezeru kdyz prev mel trailing OR ja mam leading + nejsem prvni.
            let need_pre_space = sib_idx > 0 && (prev_had_trailing_space || leading_ws);
            if need_pre_space && cursor_x > inner_x {
                cursor_x += space_w;
            }

            // Split na BREAKABLE whitespace - NBSP (U+00A0) zachova jako cast
            // slova (CSS spec: no-break-space drzi cluster, wrap point ne).
            // Bez tohoto "119&nbsp;520" zalamne na 2. radek + cursor reset zvlast.
            let is_break_ws = |c: char| c.is_whitespace() && c != '\u{00A0}';
            let words: Vec<&str> = text.split(is_break_ws)
                .filter(|s| !s.is_empty())
                .collect();
            // Strip leading/trailing whitespace z bx.text JEN pro text nody
            // (tag=None). Predtim rendering bx.text=" a " emitoval (mezera + a
            // + mezera) pres rect.x, ale cursor pricital jen "a" - overlap.
            // Pseudo-elementy (::before/::after) si content nesahej.
            // Buduj text se vlozenymi '\n' na wrap pointech. Render-side handle.
            // Pri single-word > inner_w break na char level (overflow-wrap).
            let mut wrapped_text = String::new();
            let break_word = bx_clone.overflow_wrap.as_str() == "break-word"
                || bx_clone.overflow_wrap.as_str() == "anywhere"
                || bx_clone.word_break.as_str() == "break-all";
            // Tracker max line-end x napric vsemi radky - pro spravnou rect.width
            // pri wrapovanem textu (cursor_x pri exitu = jen last line end).
            let mut max_line_end_x: f32 = cursor_x;
            for (wi, word) in words.iter().enumerate() {
                let w = measure_text_width_weight(word, font_size, bx_clone.font_weight, bx_clone.italic, &bx_clone.font_family);
                let inter_word_space = if wi > 0 { space_w } else { 0.0 };
                // Pri inner_w <= 0 (pre-pass parent.rect.width=0) NE wrap -
                // vsech slov v jedne line. Real layout pak prepocita s
                // spravnym inner_w. Bez teto guard kazde slovo wrap -> 8x lines.
                // Slop tolerance 0.5 px - text se shoduje mezi pre-pass intrinsic
                // a flush_inline word-by-word jen v ramci FP ulpu (~1e-6). Pri
                // exactni rovnosti by ANY epsilon > 0 v measure trigger wrap.
                // Slop tolerance 0.5 px - text se shoduje mezi pre-pass intrinsic
                // (measure_text_width(t)) a flush_inline word-by-word jen v ramci
                // FP ulpu (~1e-6). Pri exactni rovnosti by ANY epsilon > 0 trigger
                // wrap; tim padem napriklad letter-spacing/text-transform spans
                // dostavaly h=2x kvuli spurious wrapu na presne hranici.
                let needs_wrap = inner_w > 0.0
                    && cursor_x + inter_word_space + w > inner_x + inner_w + 0.5
                    && cursor_x > inner_x;
                if needs_wrap {
                    // Pre-wrap zaznam soucasne line end (cursor_x na konci predchozi line).
                    max_line_end_x = max_line_end_x.max(cursor_x);
                    cursor_y += line_height;
                    cursor_x = inner_x;
                    if !wrapped_text.is_empty() && !wrapped_text.ends_with('\n') {
                        wrapped_text.push('\n');
                    }
                } else if wi > 0 {
                    wrapped_text.push(' ');
                }
                // Single-word overflow: break_word/anywhere -> rozseka slovo na chars.
                // Pri inner_w <= 0 (pre-pass) NE break - dochazi single line.
                if break_word && inner_w > 0.0 && w > (inner_x + inner_w - cursor_x) && w > 0.0 {
                    let mut acc_w = 0.0;
                    let chars: Vec<char> = word.chars().collect();
                    for ch in chars {
                        let ch_w = measure_text_width_weight(&ch.to_string(), font_size, bx_clone.font_weight, bx_clone.italic, &bx_clone.font_family);
                        if cursor_x + acc_w + ch_w > inner_x + inner_w && cursor_x > inner_x {
                            cursor_y += line_height;
                            cursor_x = inner_x;
                            acc_w = 0.0;
                            wrapped_text.push('\n');
                        }
                        wrapped_text.push(ch);
                        acc_w += ch_w;
                    }
                    cursor_x += acc_w;
                    if wi == 0 {
                        bx.children[idx].rect.x = inner_x;
                        bx.children[idx].rect.y = cursor_y;
                        bx.children[idx].rect.width = w;
                        bx.children[idx].rect.height = advance_h;
                    }
                } else {
                    let x = cursor_x + inter_word_space;
                    wrapped_text.push_str(word);
                    if wi == 0 {
                        bx.children[idx].rect.x = x;
                        bx.children[idx].rect.y = cursor_y;
                        bx.children[idx].rect.width = w;
                        bx.children[idx].rect.height = advance_h;
                    }
                    cursor_x = x + w;
                }
            }
            if bx_clone.tag.is_none() {
                bx.children[idx].text = Some(wrapped_text);
            }
            // Update text rect: span vsech radku (max line end x), ne jen
            // posledniho. Pro wrapped text rect.width = max(line_end - inner_x)
            // -> hit-test + cursor I-beam pokryje cely visual text bounding.
            // Pri multi-line + first line start = inner_x set rect.x = inner_x.
            let final_y = bx.children[idx].rect.y;
            let lines = ((cursor_y - final_y) / advance_h.max(1.0)).round() + 1.0;
            // Final line end - cursor_x je end of last word.
            max_line_end_x = max_line_end_x.max(cursor_x);
            let multi_line = lines >= 2.0;
            if multi_line {
                // Snap rect.x na inner_x (lines 2+ zacinaji odsud); width
                // = max line end - inner_x (= sirka nejsiriho radku).
                bx.children[idx].rect.x = inner_x;
                bx.children[idx].rect.width = (max_line_end_x - inner_x).max(0.0);
            } else {
                let final_x = bx.children[idx].rect.x;
                bx.children[idx].rect.width = (cursor_x - final_x).max(bx.children[idx].rect.width);
            }
            bx.children[idx].rect.height = (advance_h * lines).max(advance_h);
            prev_had_trailing_space = trailing_ws;
        } else if !bx_clone.children.is_empty() {
            // Inline element s childen (napr. <span><em>text</em></span>) - flatten.
            // Pre-pridame mezeru pokud prev sibling mel trailing space.
            if sib_idx > 0 && prev_had_trailing_space && cursor_x > inner_x {
                cursor_x += space_w;
            }
            // Margin pro inline-block elementy (button, image apod.) - prida
            // mezeru pred element. CSS spec: margin-left bere effekt na inline.
            let mar_l = bx_clone.margin_left.unwrap_or(bx_clone.margin);
            let mar_r = bx_clone.margin_right.unwrap_or(bx_clone.margin);
            cursor_x += mar_l;
            // Inline padding (CSS: <code>, <span class=num> maji padding-left/right).
            // Asymmetric padding wins, jinak shorthand `padding`.
            let pad_l = bx_clone.padding_left.unwrap_or(bx_clone.padding);
            let pad_r = bx_clone.padding_right.unwrap_or(bx_clone.padding);
            let pad_t = bx_clone.padding_top.unwrap_or(bx_clone.padding);
            let pad_b = bx_clone.padding_bottom.unwrap_or(bx_clone.padding);
            // Replaced inline elementy s explicit width/height (svg, img, canvas
            // s width attr nebo CSS width) prefer explicit dimension. Inak by
            // text_w (sum text children) byl fallback na font_size pri SVG bez
            // text children -> SVG smrstil se na 16 px misto attr 400.
            let explicit_w = bx_clone.explicit_width
                .or_else(|| if bx_clone.rect.width > 0.0 { Some(bx_clone.rect.width) } else { None });
            let explicit_h = bx_clone.explicit_height
                .or_else(|| if bx_clone.rect.height > 0.0 { Some(bx_clone.rect.height) } else { None });
            let estimated_w = explicit_w.unwrap_or_else(|| {
                let inherited_bold = bx_clone.bold;
                // Replaced inline children (img/svg/canvas/picture > img) -
                // jejich natural/explicit width dotahnout do estimated_w.
                // Bez tohoto <picture> s <img width=200> mel sum_text_w=0
                // -> estimated_w padl na font_size (16). Picture sirka 0.
                let replaced_w: f32 = bx_clone.children.iter()
                    .map(|c| {
                        let cw = c.explicit_width.unwrap_or(0.0).max(c.rect.width);
                        // Picture / img natural dims z thread_local cache.
                        let natural = c.image_src.as_ref()
                            .and_then(|s| get_image_natural_dims(s))
                            .map(|(w, _)| w).unwrap_or(0.0);
                        cw.max(natural)
                    })
                    .sum::<f32>();
                // Family-aware measure - bez tohoto Bold "119\u{a0}514" pres
                // Ubuntu Bold mereny pres system Times Bold (uzsi metrics) ->
                // sirka 58 misto real 65 -> fc-blue box uzsi nez render.
                let inherited_italic = bx_clone.italic;
                let inherited_family = bx_clone.font_family.clone();
                let text_w = bx_clone.children.iter()
                    .filter_map(|c| c.text.as_ref())
                    .map(|t| measure_text_width_full(t, font_size, inherited_bold, inherited_italic, &inherited_family))
                    .sum::<f32>();
                // Pri replaced inner (img/svg/picture) prefer real width. Pri text
                // jen children pouzij text_w. Pri kombinaci max + sum text content.
                let combined = if replaced_w > 0.0 {
                    replaced_w.max(text_w)
                } else {
                    text_w.max(font_size)
                };
                combined + pad_l + pad_r
            });
            let element_h = explicit_h.unwrap_or(advance_h + pad_t + pad_b);
            // Inline-block s padding (button) je vyssi nez advance_h - line_height
            // musi pokryt cele element_h aby cursor_y advance pod nej a section
            // content_h zahrnula pad bottom.
            line_height = line_height.max(element_h);
            if cursor_x + estimated_w > inner_x + inner_w && cursor_x > inner_x {
                cursor_y += line_height;
                cursor_x = inner_x;
            }
            // Baseline alignment: pri smaller inline font (<small>, <sub>, <sup>)
            // posun rect.y dolu o rozdil parent_font_size - element_font_size.
            // Tim padne baseline elementu na spolecny baseline radky.
            // (Bez shiftu se text vykresloval s top-aligned line, vypadal raised.)
            let baseline_shift = (parent_font_size - bx_clone.font_size).max(0.0);
            bx.children[idx].rect.x = cursor_x;
            bx.children[idx].rect.y = cursor_y + baseline_shift;
            bx.children[idx].rect.width = estimated_w;
            bx.children[idx].rect.height = element_h;
            // Layout vnoreny obsah - layout_block pouzije rect + padding.
            layout_block(&mut bx.children[idx]);
            // Po layout_block re-read rect.width - layout muze vnitrek vetsi
            // (text wrap, padding, deeper nesting) nez pre-pass estimated_w.
            // Bez re-read by cursor_x advanced jen o estimated_w -> overlap
            // nasledujiciho inline sibling pres real sirku tohoto elementu.
            // Mileneckaseznamka.cz "Celkem 29498" + "Inzeraty 119 504" - bold
            // span s deeper children dostal real width vetsi nez sum chars.
            let real_w = bx.children[idx].rect.width.max(estimated_w);
            cursor_x += real_w + mar_r;
            // Inline element bez text trailing -> default no trailing space.
            prev_had_trailing_space = false;
        } else {
            // Replaced inline element bez children + bez text (canvas, img,
            // video, iframe, input). Pouziva explicit_width/explicit_height
            // z attributu nebo CSS. Bez teto branch zustane rect 0,0 a element
            // se renderoval v levem hornim rohu.
            // Width: explicit_width (px) wins, pak width_pct (% z parent inner_w),
            // pak natural rect.width, pak font_size fallback.
            // Mileneckaseznamka.cz: .photo-box img { width: 100% } -> width_pct=1.0,
            // bez tohoto se img drzelo na natural 100x100 misto plne photo-box.
            let mar_l_r = bx_clone.margin_left.unwrap_or(bx_clone.margin);
            let mar_r_r = bx_clone.margin_right.unwrap_or(bx_clone.margin);
            let parent_w_for_img = (inner_w - mar_l_r - mar_r_r).max(0.0);
            let parent_h_for_img = if bx.rect.height > 0.0 { bx.rect.height } else { advance_h };
            // Img/video replaced inline element sizing per CSS spec:
            // - Natural dims z nacteneho obrazku (IMAGE_NATURAL_DIMS thread_local
            //   populated z renderer load_image).
            // - explicit_width/height (CSS width/height) wins.
            // - max-width/height clamp ZACHOVAVA aspect ratio (per spec):
            //   pri natural 800x600 + max-w 175 + max-h 175 -> jedna z os
            //   prizpusobi do max, druha proporcionalne mensi.
            let is_img_replaced = matches!(bx_clone.tag.as_deref(), Some("img") | Some("video"));
            // Debug breakpoint hook pro img sizing (BP_TAG=img / BP_CLASS=... + IDE BP).
            if is_img_replaced && crate::debug_bp::bp_enabled() {
                let class = bx_clone.node.as_ref()
                    .and_then(|n| n.attr("class")).unwrap_or_default();
                let id = bx_clone.node.as_ref()
                    .and_then(|n| n.attr("id")).unwrap_or_default();
                let tag = bx_clone.tag.as_deref().unwrap_or("");
                if crate::debug_bp::bp_match(tag, &id, &class) {
                    crate::debug_bp::breakpoint_layout();
                }
            }
            // Resolve max values (Infinity pri none/auto - CssLength::resolve_max).
            let img_ctx_w = ResolveCtx { parent_size: parent_w_for_img, font_size: bx_clone.font_size, ..Default::default() };
            let img_ctx_h = ResolveCtx { parent_size: parent_h_for_img, font_size: bx_clone.font_size, ..Default::default() };
            let max_w_resolved = bx_clone.max_width.resolve_max(&img_ctx_w);
            let max_h_resolved = bx_clone.max_height.resolve_max(&img_ctx_h);
            // Lookup natural dims pro img/video (None pokud zatim nenacten).
            let natural_dims: Option<(f32, f32)> = if is_img_replaced {
                bx_clone.image_src.as_ref()
                    .and_then(|s| get_image_natural_dims(s))
            } else { None };
            // Compute w/h zachovavajici aspect ratio kdyz natural znaty + bez
            // explicit dims. Fallback heuristic kdy natural unknown: max-w/h
            // (pri obou pct = square box-fill, nepokazi aspect kdyz parent ctverec).
            let (w_pre, h_pre) = if is_img_replaced
                && bx_clone.explicit_width.is_none() && bx_clone.explicit_height.is_none()
                && bx_clone.width_pct.is_none() && bx_clone.height_pct.is_none()
            {
                // Detect "fill parent" intent: BOTH max-width AND max-height jsou
                // procenta. To je nejcastejsi pattern .photo-box img / hero
                // images / responsive thumbnails kde site spravne expanduje
                // img na container size. Pro maly natural (icon-sized) i Chrome
                // odpovida fill behavior. CSS spec technicky stale "no upscale",
                // ale real-world site-design ocekava expand. Heuristic match.
                let fill_intent = is_img_replaced
                    && bx_clone.max_width.is_percent()
                    && bx_clone.max_height.is_percent();
                if let Some((nw, nh)) = natural_dims {
                    // Natural znaty. Scale fit-within max-w/max-h zachovavaje aspect.
                    let scale_w = if max_w_resolved.is_finite() { max_w_resolved / nw } else { 1.0 };
                    let scale_h = if max_h_resolved.is_finite() { max_h_resolved / nh } else { 1.0 };
                    // Pri fill_intent allow upscale (scale > 1.0). Bez intent
                    // standard CSS clamp = never upscale.
                    let scale = if fill_intent {
                        scale_w.min(scale_h)
                    } else {
                        scale_w.min(scale_h).min(1.0)
                    };
                    (nw * scale, nh * scale)
                } else if max_w_resolved.is_finite() && max_h_resolved.is_finite() {
                    // Natural unknown + obe max nastavene: pouzij min jako square fit
                    // (zachovava 1:1 aspect, neroztahuje img v nesymetrickem boxu).
                    let s = max_w_resolved.min(max_h_resolved);
                    (s, s)
                } else if max_w_resolved.is_finite() {
                    // Jen max-w: img bude max_w x default placeholder (cca natural).
                    let nw_def = bx_clone.rect.width.max(100.0);
                    let nh_def = bx_clone.rect.height.max(100.0);
                    let ratio = nh_def / nw_def;
                    (max_w_resolved, max_w_resolved * ratio)
                } else if max_h_resolved.is_finite() {
                    let nw_def = bx_clone.rect.width.max(100.0);
                    let nh_def = bx_clone.rect.height.max(100.0);
                    let ratio = nw_def / nh_def;
                    (max_h_resolved * ratio, max_h_resolved)
                } else {
                    // Natural unknown + no max: stay at default rect.width/height.
                    (bx_clone.rect.width.max(font_size), bx_clone.rect.height.max(advance_h))
                }
            } else {
                // Non-replaced / has explicit dim -> pouziva existing path.
                let w_calc = if let Some(px) = bx_clone.explicit_width {
                    px
                } else if let Some(pct) = bx_clone.width_pct {
                    parent_w_for_img * pct
                } else if bx_clone.rect.width > 0.0 {
                    bx_clone.rect.width
                } else {
                    font_size
                };
                let h_calc = if let Some(px) = bx_clone.explicit_height {
                    px
                } else if let Some(pct) = bx_clone.height_pct {
                    if bx.rect.height > 0.0 { bx.rect.height * pct } else { advance_h }
                } else if bx_clone.rect.height > 0.0 {
                    bx_clone.rect.height
                } else {
                    advance_h
                };
                (w_calc, h_calc)
            };
            // Apply min-width clamp (max uz aplikovan v aspect-aware vypoctu).
            let w = if bx_clone.min_width.is_specified() {
                let mn = bx_clone.min_width.resolve(&img_ctx_w);
                if mn > 0.0 { w_pre.max(mn) } else { w_pre }
            } else { w_pre };
            // Pro non-img cesty aplikuj max separately (img cesta uz scale dela).
            let w = if !is_img_replaced && max_w_resolved.is_finite() {
                w.min(max_w_resolved)
            } else { w };
            let h = if bx_clone.min_height.is_specified() {
                let mn = bx_clone.min_height.resolve(&img_ctx_h);
                if mn > 0.0 { h_pre.max(mn) } else { h_pre }
            } else { h_pre };
            let h = if !is_img_replaced && max_h_resolved.is_finite() {
                h.min(max_h_resolved)
            } else { h };
            if sib_idx > 0 && prev_had_trailing_space && cursor_x > inner_x {
                cursor_x += space_w;
            }
            if cursor_x + w > inner_x + inner_w && cursor_x > inner_x {
                cursor_y += line_height;
                cursor_x = inner_x;
            }
            bx.children[idx].rect.x = cursor_x;
            bx.children[idx].rect.y = cursor_y;
            bx.children[idx].rect.width = w;
            bx.children[idx].rect.height = h;
            line_height = line_height.max(h);
            cursor_x += w;
            prev_had_trailing_space = false;
        }
    }
    cursor_y + line_height
}

/// Real vypocet sirky textu pres globalni shared font.
/// Fallback na heuristiku kdyz font neni dostupny.
pub fn measure_text_width(text: &str, font_size: f32) -> f32 {
    measure_text_width_styled(text, font_size, false)
}

pub fn measure_text_width_styled(text: &str, font_size: f32, bold: bool) -> f32 {
    measure_text_width_full(text, font_size, bold, false, "")
}

/// Family-aware measure: vyhleda monospace/serif/sans-serif font dle CSS
/// font-family list. Pri zadnem matchu fallback na default (Times). Bez tohoto
/// by Courier-New text mereny default Times davalo zcela odlisne sirky -
/// kazdy span text na stranky s monospace body by nasel jine wrap pointy.
pub fn measure_text_width_full(text: &str, font_size: f32, bold: bool, italic: bool, family: &str) -> f32 {
    measure_text_width_impl(text, font_size, if bold { 700 } else { 400 }, italic, family)
}

pub fn measure_text_width_weight(text: &str, font_size: f32, weight: u32, italic: bool, family: &str) -> f32 {
    measure_text_width_impl(text, font_size, weight, italic, family)
}

fn measure_text_width_impl(text: &str, font_size: f32, weight: u32, italic: bool, family: &str) -> f32 {
    let bold = weight >= 600;
    use std::sync::OnceLock;
    static FONT: OnceLock<Option<fontdue::Font>> = OnceLock::new();
    static FONT_BOLD: OnceLock<Option<fontdue::Font>> = OnceLock::new();
    static FONT_MONO: OnceLock<Option<fontdue::Font>> = OnceLock::new();
    static FONT_MONO_BOLD: OnceLock<Option<fontdue::Font>> = OnceLock::new();
    static FONT_SANS: OnceLock<Option<fontdue::Font>> = OnceLock::new();
    static FONT_SANS_BOLD: OnceLock<Option<fontdue::Font>> = OnceLock::new();

    let _ = italic; // italic font by-mela rovnou stejnou advance jako regular zhruba

    let font_opt = FONT.get_or_init(|| {
        super::render::try_load_default_font()
            .and_then(|data| fontdue::Font::from_bytes(data, fontdue::FontSettings::default()).ok())
    });
    let font_bold_opt = FONT_BOLD.get_or_init(|| {
        load_font_first(&[
            "C:\\Windows\\Fonts\\timesbd.ttf",
            "C:\\Windows\\Fonts\\segoeuib.ttf",
            "C:\\Windows\\Fonts\\arialbd.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSerif-Bold.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSerif-Bold.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        ])
    });
    let mono_opt = FONT_MONO.get_or_init(|| {
        load_font_first(&[
            "C:\\Windows\\Fonts\\cour.ttf",   // Courier New
            "C:\\Windows\\Fonts\\consola.ttf", // Consolas
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
        ])
    });
    let mono_bold_opt = FONT_MONO_BOLD.get_or_init(|| {
        load_font_first(&[
            "C:\\Windows\\Fonts\\courbd.ttf",
            "C:\\Windows\\Fonts\\consolab.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationMono-Bold.ttf",
        ])
    });
    let sans_opt = FONT_SANS.get_or_init(|| {
        load_font_first(&[
            "C:\\Windows\\Fonts\\segoeui.ttf",
            "C:\\Windows\\Fonts\\arial.ttf",
            "C:\\Windows\\Fonts\\verdana.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ])
    });
    let sans_bold_opt = FONT_SANS_BOLD.get_or_init(|| {
        load_font_first(&[
            "C:\\Windows\\Fonts\\segoeuib.ttf",
            "C:\\Windows\\Fonts\\arialbd.ttf",
            "C:\\Windows\\Fonts\\verdanab.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        ])
    });

    // PRIORITY: pri @font-face registrovany font (Ubuntu, Roboto, atd. pres
    // register_measure_font) prefer ho pred system fallback. Bez tohoto bold
    // mereni pres Times Bold (system), render pres Ubuntu Bold (@font-face) =
    // sirka neshodi, dalsi span overlaps.
    let registered = measure_font_for_weight(family, weight, italic);

    // PERF: family classification cached pres thread_local HashMap. Bez teto
    // cache `family.to_lowercase()` + 13 `.contains()` checks per measure call
    // (= per inline word measure, hundreds per layout). Common families
    // resolve O(1) po prvnim hitu.
    let (is_mono, is_sans) = classify_family_cached(family);

    let system_font: Option<&fontdue::Font> = if is_mono {
        if bold { mono_bold_opt.as_ref().or(mono_opt.as_ref()) } else { mono_opt.as_ref() }
    } else if is_sans {
        if bold { sans_bold_opt.as_ref().or(sans_opt.as_ref()) } else { sans_opt.as_ref() }
    } else {
        if bold { font_bold_opt.as_ref().or(font_opt.as_ref()) } else { font_opt.as_ref() }
    };
    // Fallback chain: pokud zvolena varianta nenalezena, defaultni font.
    let system_font = system_font.or(font_opt.as_ref());
    // Active = @font-face registered preferred, system fallback.
    let active_font: Option<&fontdue::Font> = registered.as_ref().or(system_font);
    let fake_bold_pad = if bold && active_font.map(|f| !std::ptr::eq(f, font_opt.as_ref().unwrap_or(f))).unwrap_or(false) { 0.0 } else if bold { 1.0 } else { 0.0 };
    let _ = fake_bold_pad; // reset - simple semantics: bez fake-bold pad, mereno z bold font kdyz dostupne

    match active_font {
        Some(font) => {
            // PERF: per-char advance memo cache. Drive `font.metrics(ch, fs)` per
            // call (fontdue HashMap lookup ~100 ns each). Pri 898 samples
            // measure_text_width_full v flame = ~30% paint time. Cache key
            // = (font ptr, char, font_size bits) -> advance_width f32.
            thread_local! {
                static ADVANCE_CACHE: std::cell::RefCell<
                    std::collections::HashMap<(usize, char, u32), f32, ahash::RandomState>
                > = std::cell::RefCell::new(
                    std::collections::HashMap::with_hasher(ahash::RandomState::new())
                );
            }
            let font_ptr = font as *const _ as usize;
            let fs_bits = font_size.to_bits();
            // Fallback chain pro chars co primary font neumi (diakritika v Times
            // Roman ASCII subset, CJK v Latin fontu). Iteruje pres dostupne fonts
            // dokud najde non-zero advance.
            let fallback_fonts: [Option<&fontdue::Font>; 4] = [
                sans_opt.as_ref(),
                font_opt.as_ref(),
                font_bold_opt.as_ref(),
                mono_opt.as_ref(),
            ];
            let mut total = 0.0_f32;
            for ch in text.chars() {
                let key = (font_ptr, ch, fs_bits);
                let cached = ADVANCE_CACHE.with(|c| c.borrow().get(&key).copied());
                let aw = if let Some(v) = cached {
                    v
                } else {
                    let mut v = font.metrics(ch, font_size).advance_width;
                    // Pri 0 advance (glyph index = 0 = missing) -> fallback chain.
                    // ASCII space/control skip (legitimne 0 advance for control).
                    if v <= 0.0 && (ch as u32) >= 0x20 && font.lookup_glyph_index(ch) == 0 {
                        for fb in &fallback_fonts {
                            if let Some(ff) = fb {
                                if std::ptr::eq(*ff, font) { continue; }
                                if ff.lookup_glyph_index(ch) != 0 {
                                    v = ff.metrics(ch, font_size).advance_width;
                                    if v > 0.0 { break; }
                                }
                            }
                        }
                        // Last resort: nominal char width (0.5em).
                        if v <= 0.0 { v = font_size * 0.5; }
                    }
                    ADVANCE_CACHE.with(|c| c.borrow_mut().insert(key, v));
                    v
                };
                total += aw;
            }
            total
        }
        None => {
            let avg_char_w = font_size * 0.55;
            text.chars().count() as f32 * avg_char_w
        }
    }
}

/// Family classification: (is_monospace, is_sans_serif). Cached per family
/// string. False+False = serif/unknown fallback.
fn classify_family_cached(family: &str) -> (bool, bool) {
    thread_local! {
        static CACHE: std::cell::RefCell<std::collections::HashMap<String, (bool, bool), ahash::RandomState>>
            = std::cell::RefCell::new(std::collections::HashMap::with_hasher(ahash::RandomState::new()));
    }
    if let Some(v) = CACHE.with(|c| c.borrow().get(family).copied()) {
        return v;
    }
    let f_lower = family.to_lowercase();
    let is_mono = f_lower.contains("monospace")
        || f_lower.contains("courier")
        || f_lower.contains("consolas")
        || f_lower.contains("monaco")
        || f_lower.contains("menlo");
    let is_sans = !is_mono && (
        f_lower.contains("sans-serif")
        || f_lower.contains("arial")
        || f_lower.contains("helvetica")
        || f_lower.contains("segoe")
        || f_lower.contains("verdana")
        || f_lower.contains("inter")
        || f_lower.contains("roboto")
        || f_lower.contains("system-ui"));
    let v = (is_mono, is_sans);
    CACHE.with(|c| c.borrow_mut().insert(family.to_string(), v));
    v
}

fn load_font_first(paths: &[&str]) -> Option<fontdue::Font> {
    for path in paths {
        if let Ok(d) = std::fs::read(path) {
            if let Ok(f) = fontdue::Font::from_bytes(d, fontdue::FontSettings::default()) {
                return Some(f);
            }
        }
    }
    None
}

/// Parse barvu z CSS string.
/// Podpora: #RGB, #RRGGBB, #RRGGBBAA, rgb()/rgba() (legacy + modern), hsl()/hsla(),
///          hwb(), lab(), lch(), oklab(), oklch(), color-mix(), nazvy.



/// Top-level comma split - vraci Vec<String> trimmed.
pub(super) fn split_top_level_commas_string(s: &str) -> Vec<String> {
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

pub(super) fn split_top_level_whitespace_str(s: &str) -> Vec<&str> {
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



/// Parse transform: "translate(10px, 20px)" / "rotate(45deg)" / "scale(1.5)".
/// Parsuje cely transform chain ("translate(10px) rotate(45deg) scale(1.5)").
/// Vyvazene zavorky pri tokenize.

pub(super) fn split_top_level_commas(s: &str) -> Vec<&str> {
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


mod length;
#[allow(unused_imports)]
pub use length::{parse_length, parse_length_ctx, parse_length_or_pct};

pub mod css_length;
pub use css_length::{CssLength, ResolveCtx};

pub mod overflow;
pub use overflow::Overflow;

pub mod flex_enums;
pub use flex_enums::{FlexDirection, FlexWrap, JustifyContent, AlignItems, AlignSelf, AlignContent, BoxSizing,
                       ObjectFit, BorderCollapse, TableLayout, ImageRendering};

mod shadows;
pub use shadows::{parse_text_shadow, parse_box_shadow};

mod shape_fn;
#[allow(unused_imports)]
pub use shape_fn::{ShapeFunction, parse_shape_function};

mod transform;
#[allow(unused_imports)]
pub use transform::{compute_transform_matrix, needs_3d_pipeline};

mod filter;
#[allow(unused_imports)]
pub use filter::{FilterOp, parse_filter_chain, apply_filter_chain, compute_color_matrix, is_identity_matrix};

mod backgrounds;
#[allow(unused_imports)]
pub use backgrounds::{BgGradient, BgGradientKind, ClipPath, parse_clip_path, to_roman, BgLayer, BgPosition, BgSize, BgRepeat, BgBox, BgAttachment, parse_bg_position, parse_bg_size, parse_bg_repeat, parse_bg_box, parse_bg_attachment};

mod gradients;
#[allow(unused_imports)]
pub use gradients::{parse_any_gradient, parse_radial_gradient, parse_conic_gradient, parse_linear_gradient};

mod transform_parse;
#[allow(unused_imports)]
pub use transform_parse::{parse_transform_chain, parse_transform};

mod color;
#[allow(unused_imports)]
pub use color::parse_color;
