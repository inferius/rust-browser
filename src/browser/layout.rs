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
        "li" => { bx.margin = 2.0; bx.padding = 4.0; }
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
    pub margin: f32,
    pub border_width: f32,
    /// Per-side border width (None = use border_width). CSS Backgrounds L3.
    pub border_top_width: Option<f32>,
    pub border_right_width: Option<f32>,
    pub border_bottom_width: Option<f32>,
    pub border_left_width: Option<f32>,
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
    pub direction: String,
    /// writing-mode: horizontal-tb (default) | vertical-rl | vertical-lr
    pub writing_mode: String,
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
    pub pointer_events: String,
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
    pub object_fit: String,
    pub object_position: String,
    /// Mix blend / background blend
    pub background_blend_mode: String,
    /// Image rendering hints
    pub image_rendering: String,
    /// CSS Sizing L4 - aspect-ratio uz mam
    pub min_width_v: String,
    pub max_width_v: String,
    pub min_height_v: String,
    pub max_height_v: String,
    /// Explicitni CSS width (None = auto / neparsovano).
    pub explicit_width: Option<f32>,
    /// Explicitni CSS height (None = auto / neparsovano).
    pub explicit_height: Option<f32>,
    /// Flex properties (CSS Flexbox L1)
    pub flex_direction: String,
    pub flex_wrap: String,
    pub justify_content: String,
    pub align_items: String,
    /// align-self per-item (override align-items): auto/start/end/center/stretch/baseline
    pub align_self: String,
    /// justify-self per-item (grid): auto/start/end/center/stretch
    pub justify_self: String,
    /// grid-row-start: 1-based line, 0 = auto.
    pub grid_row_start: i32,
    pub grid_row_end: i32,
    pub grid_column_start: i32,
    pub grid_column_end: i32,
    /// grid-row-span / grid-column-span (alternativni k start/end).
    pub grid_row_span: i32,
    pub grid_column_span: i32,
    pub align_content: String,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: String,
    pub row_gap: f32,
    pub column_gap: f32,
    /// CSS Logical Properties continued
    pub block_size_v: String,
    pub inline_size_v: String,
    /// table-layout, border-collapse, border-spacing, caption-side, empty-cells
    pub table_layout: String,
    pub border_collapse: String,
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
            padding_top: None,
            padding_right: None,
            padding_bottom: None,
            padding_left: None,
            margin_top: None,
            margin_right: None,
            margin_bottom: None,
            margin_left: None,
            margin: 0.0,
            border_width: 0.0,
            border_top_width: None,
            border_right_width: None,
            border_bottom_width: None,
            border_left_width: None,
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
            direction: String::new(),
            writing_mode: String::new(),
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
            pointer_events: String::new(),
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
            object_fit: String::new(),
            object_position: String::new(),
            background_blend_mode: String::new(),
            image_rendering: String::new(),
            min_width_v: String::new(),
            max_width_v: String::new(),
            min_height_v: String::new(),
            max_height_v: String::new(),
            explicit_width: None,
            explicit_height: None,
            flex_direction: String::new(),
            flex_wrap: String::new(),
            justify_content: String::new(),
            align_items: String::new(),
            align_self: String::new(),
            justify_self: String::new(),
            grid_row_start: 0,
            grid_row_end: 0,
            grid_column_start: 0,
            grid_column_end: 0,
            grid_row_span: 0,
            grid_column_span: 0,
            align_content: String::new(),
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: String::new(),
            row_gap: 0.0,
            column_gap: 0.0,
            block_size_v: String::new(),
            inline_size_v: String::new(),
            table_layout: String::new(),
            border_collapse: String::new(),
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
        }
    }

    /// Hit test: vrati nejdetailnejsi (deepest) box obsahujici (x, y).
    pub fn hit_test(&self, x: f32, y: f32) -> Option<&LayoutBox> {
        // pointer-events: none -> element ignored pri hit test (vc deti pokud none nezruseno)
        if self.pointer_events == "none" {
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
    let mut layout_root = build_box_with_pseudo(root, style_map, pseudo_map);
    layout_root.rect.width = viewport_width;
    layout_root.rect.height = viewport_height;
    layout_dispatch(&mut layout_root);
    // Post-pass: anchor positioning resolve
    let anchor_map = collect_anchors(&layout_root);
    apply_anchor_positioning(&mut layout_root, &anchor_map);
    layout_root
}

/// Aplikuje position: sticky pri zadanem scroll offsetu.
/// Volat z render po layout, pred display_list build.
/// Sticky element: pri scroll dosahne urovne sticky -> drzi se na top.
pub fn apply_sticky(root: &mut LayoutBox, scroll_y: f32) {
    fn walk(bx: &mut LayoutBox, scroll_y: f32, parent_bottom: f32) {
        if matches!(bx.position, Position::Sticky) {
            let top = bx.offset_top.unwrap_or(0.0);
            let original_y = bx.rect.y;
            let viewport_top = scroll_y + top;
            // Pokud element je nad viewport_top, posunout dolu (visible at viewport_top)
            if original_y < viewport_top {
                let new_y = viewport_top;
                // Don't push past parent bottom
                let max_y = parent_bottom - bx.rect.height;
                bx.rect.y = new_y.min(max_y).max(original_y);
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
    // Aliases - inline-flex/grid -> flex/grid; subgrid -> grid; ruby -> inline; list-item -> block
    let effective = match bx.display {
        Display::InlineFlex => Display::Flex,
        Display::InlineGrid | Display::Subgrid => Display::Grid,
        Display::ListItem => Display::Block,
        Display::Ruby => Display::Inline,
        Display::Table | Display::TableHeader => Display::Block,
        Display::TableRow => Display::Inline,
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
    match bx.display {
        Display::Flex => layout_flex(bx),
        Display::Grid => super::layout_engine::grid::layout_grid(bx),
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
    let mut counters: HashMap<String, i32> = HashMap::new();
    build_box_inner(node, style_map, pseudo_map, &mut counters)
}

fn build_box_inner(node: &Rc<Node>, style_map: &StyleMap, pseudo_map: &super::cascade::PseudoStyleMap, counters: &mut HashMap<String, i32>) -> LayoutBox {
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
    if let Some(v) = s.get("flex-direction") { bx.flex_direction = v.trim().to_string(); }
    if let Some(v) = s.get("flex-wrap") { bx.flex_wrap = v.trim().to_string(); }
    if let Some(v) = s.get("justify-content") { bx.justify_content = v.trim().to_string(); }
    if let Some(v) = s.get("align-items") { bx.align_items = v.trim().to_string(); }
    if let Some(v) = s.get("align-content") { bx.align_content = v.trim().to_string(); }
    if let Some(v) = s.get("flex-grow") { bx.flex_grow = v.trim().parse().unwrap_or(0.0); }
    if let Some(v) = s.get("flex-shrink") { bx.flex_shrink = v.trim().parse().unwrap_or(1.0); }
    if let Some(v) = s.get("flex-basis") { bx.flex_basis = v.trim().to_string(); }
    if let Some(v) = s.get("row-gap") { bx.row_gap = parse_length(v); }
    if let Some(v) = s.get("column-gap") { bx.column_gap = parse_length(v); }
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
        bx.direction = d.trim().to_string();
        // RTL: text-align default = right (pokud nezadany)
        if bx.direction == "rtl" && s.get("text-align").is_none() {
            bx.text_align = TextAlign::Right;
        }
    }
    if let Some(wm) = s.get("writing-mode") {
        bx.writing_mode = wm.trim().to_string();
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
    if let Some(v) = s.get("pointer-events") { bx.pointer_events = v.trim().to_string(); }
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
    if let Some(v) = s.get("object-fit") { bx.object_fit = v.trim().to_string(); }
    if let Some(v) = s.get("object-position") { bx.object_position = v.trim().to_string(); }
    if let Some(v) = s.get("background-blend-mode") { bx.background_blend_mode = v.trim().to_string(); }
    if let Some(v) = s.get("image-rendering") { bx.image_rendering = v.trim().to_string(); }
    if let Some(v) = s.get("table-layout") { bx.table_layout = v.trim().to_string(); }
    if let Some(v) = s.get("border-collapse") { bx.border_collapse = v.trim().to_string(); }
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
            _ => {
                let px = parse_length(v);
                if px > 0.0 { bx.explicit_width = Some(px); }
            }
        }
    }
    if let Some(h) = s.get("height") {
        let v = h.trim();
        if v != "auto" {
            let px = parse_length(v);
            if px > 0.0 { bx.explicit_height = Some(px); }
        }
    }
    if let Some(v) = s.get("min-width") {
        let pv = v.trim();
        bx.min_width_v = pv.to_string();
    }
    if let Some(v) = s.get("max-width") {
        let pv = v.trim();
        bx.max_width_v = pv.to_string();
    }
    if let Some(v) = s.get("min-height") {
        let pv = v.trim();
        bx.min_height_v = pv.to_string();
    }
    if let Some(v) = s.get("max-height") {
        let pv = v.trim();
        bx.max_height_v = pv.to_string();
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
        let style = if bx.list_style_type.is_empty() { "disc" } else { bx.list_style_type.as_str() };
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
        let cb = build_box_inner(child, style_map, pseudo_map, counters);
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
fn parse_content_value(raw: &str, parent: &Rc<Node>, counters: &HashMap<String, i32>) -> Option<String> {
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
fn layout_block_vertical(bx: &mut LayoutBox) {
    let inner_x = bx.rect.x + bx.padding + bx.border_width;
    let inner_y = bx.rect.y + bx.padding + bx.border_width;
    let inner_h = bx.rect.height - 2.0 * (bx.padding + bx.border_width);
    // Pro vertical-rl startujeme od prava; pro lr od leva.
    let right_to_left = bx.writing_mode == "vertical-rl";

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

pub fn layout_block(bx: &mut LayoutBox) {
    // writing-mode: vertical-rl / vertical-lr - block axis zmena na X
    let vertical = matches!(bx.writing_mode.as_str(), "vertical-rl" | "vertical-lr");
    if vertical {
        layout_block_vertical(bx);
        return;
    }

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
            Display::Block | Display::Flex | Display::Grid
            | Display::ListItem | Display::Table | Display::TableHeader
            | Display::TableCell | Display::TableHeaderCell | Display::TableCaption
            | Display::Subgrid => {
                if !inline_buffer.is_empty() {
                    cursor_y = flush_inline(bx, &inline_buffer, inner_x, cursor_y, inner_w);
                    inline_buffer.clear();
                }
                let child = &mut bx.children[i];
                child.rect.x = inner_x + child.margin;
                child.rect.y = cursor_y + child.margin;
                // explicit_width z CSS width prop; jinak fill parent
                child.rect.width = child.explicit_width.unwrap_or(inner_w - 2.0 * child.margin);
                // Clamp dle min-width / max-width
                if !child.min_width_v.is_empty() && child.min_width_v != "none" {
                    let mn = parse_length(&child.min_width_v.clone());
                    if mn > 0.0 { child.rect.width = child.rect.width.max(mn); }
                }
                if !child.max_width_v.is_empty() && child.max_width_v != "none" {
                    let mx = parse_length(&child.max_width_v.clone());
                    if mx > 0.0 { child.rect.width = child.rect.width.min(mx); }
                }
                // explicit_height z CSS height prop; jinak auto (content-based)
                if let Some(eh) = child.explicit_height {
                    child.rect.height = eh;
                } else if child.rect.height == 0.0 {
                    child.rect.height = if child.text.is_some() {
                        child.font_size * child.line_height + child.padding * 2.0
                    } else {
                        20.0
                    };
                }
                // Clamp dle min-height / max-height
                if !child.min_height_v.is_empty() && child.min_height_v != "none" {
                    let mn = parse_length(&child.min_height_v.clone());
                    if mn > 0.0 { child.rect.height = child.rect.height.max(mn); }
                }
                if !child.max_height_v.is_empty() && child.max_height_v != "none" {
                    let mx = parse_length(&child.max_height_v.clone());
                    if mx > 0.0 { child.rect.height = child.rect.height.min(mx); }
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
            Display::Inline | Display::InlineBlock | Display::Contents
            | Display::TableRow | Display::Ruby
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

    // CSS Color L4 - color() function s namespace
    // color(srgb r g b [/ a]), color(display-p3 r g b), color(rec2020 r g b), atd.
    // Pro nase ucely vsechny ColorSpace mapujeme primo na sRGB (pro rec2020/p3 by se mela
    // udelat gamut mapping, ale to je out-of-scope - vetsina renderer fallbackuje stejne).
    if let Some(inner) = s.strip_prefix("color(").and_then(|s| s.strip_suffix(')')) {
        return parse_color_function(inner);
    }

    // CSS Color L5 - relative color: rgb(from <color> r g b [/ alpha])
    // Vyhodnoti zdrojovou barvu, jeji slozky pristupne jako r/g/b/alpha keywordy.
    if let Some(inner) = s.strip_prefix("rgb(from ").and_then(|s| s.strip_suffix(')'))
        .or_else(|| s.strip_prefix("rgba(from ").and_then(|s| s.strip_suffix(')'))) {
        return parse_relative_rgb(inner);
    }
    if let Some(inner) = s.strip_prefix("hsl(from ").and_then(|s| s.strip_suffix(')'))
        .or_else(|| s.strip_prefix("hsla(from ").and_then(|s| s.strip_suffix(')'))) {
        return parse_relative_hsl(inner);
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

    // CSS Color L5 - contrast(<color> vs <list>) - vrati nejvic kontrast color
    if let Some(inner) = s.strip_prefix("contrast(").and_then(|s| s.strip_suffix(')')) {
        return parse_contrast(inner);
    }

    // CSS Color L5 - contrast-color(<color>) - vrati black/white podle pozadi
    if let Some(inner) = s.strip_prefix("contrast-color(").and_then(|s| s.strip_suffix(')')) {
        // Vyhodnoti zda bg je svetly nebo tmavy a vrati opacny
        let bg = parse_color(inner.trim())?;
        let luma = (0.2126 * bg[0] as f32 + 0.7152 * bg[1] as f32 + 0.0722 * bg[2] as f32) / 255.0;
        return Some(if luma > 0.5 {
            [0, 0, 0, 255]   // tmavy text na svetlem bg
        } else {
            [255, 255, 255, 255]  // svetly text na tmavem bg
        });
    }

    // CSS Color L5 - light-dark(<light>, <dark>) - vrati prvni v light mode, druhy v dark
    // (Bez color scheme detekci - vraci prvni)
    if let Some(inner) = s.strip_prefix("light-dark(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = inner.splitn(2, ',').collect();
        if let Some(first) = parts.first() {
            return parse_color(first.trim());
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
        "cyan" | "aqua" => Some([0, 255, 255, 255]),
        "magenta" | "fuchsia" => Some([255, 0, 255, 255]),
        "gray" | "grey" => Some([128, 128, 128, 255]),
        "lightgray" | "lightgrey" => Some([211, 211, 211, 255]),
        "darkgray"  | "darkgrey"  => Some([169, 169, 169, 255]),
        "silver" => Some([192, 192, 192, 255]),
        "maroon" => Some([128, 0, 0, 255]),
        "navy"   => Some([0, 0, 128, 255]),
        "teal"   => Some([0, 128, 128, 255]),
        "olive"  => Some([128, 128, 0, 255]),
        "lime"   => Some([0, 255, 0, 255]),
        "orange"  => Some([255, 165, 0, 255]),
        "orangered" => Some([255, 69, 0, 255]),
        "coral"  => Some([255, 127, 80, 255]),
        "tomato" => Some([255, 99, 71, 255]),
        "salmon" => Some([250, 128, 114, 255]),
        "gold"   => Some([255, 215, 0, 255]),
        "khaki"  => Some([240, 230, 140, 255]),
        "purple"  => Some([128, 0, 128, 255]),
        "violet" => Some([238, 130, 238, 255]),
        "indigo" => Some([75, 0, 130, 255]),
        "pink"    => Some([255, 192, 203, 255]),
        "hotpink" | "deeppink" => Some([255, 20, 147, 255]),
        "brown"   => Some([165, 42, 42, 255]),
        "chocolate" => Some([210, 105, 30, 255]),
        "sienna"  => Some([160, 82, 45, 255]),
        "tan"     => Some([210, 180, 140, 255]),
        "beige"   => Some([245, 245, 220, 255]),
        "ivory"   => Some([255, 255, 240, 255]),
        "snow"    => Some([255, 250, 250, 255]),
        "lavender" => Some([230, 230, 250, 255]),
        "skyblue" | "lightskyblue" => Some([135, 206, 235, 255]),
        "steelblue" => Some([70, 130, 180, 255]),
        "royalblue" => Some([65, 105, 225, 255]),
        "dodgerblue" => Some([30, 144, 255, 255]),
        "deepskyblue" => Some([0, 191, 255, 255]),
        "cornflowerblue" => Some([100, 149, 237, 255]),
        "cadetblue" => Some([95, 158, 160, 255]),
        "turquoise" | "mediumturquoise" => Some([64, 224, 208, 255]),
        "lightblue" => Some([173, 216, 230, 255]),
        "powderblue" => Some([176, 224, 230, 255]),
        "mintcream" => Some([245, 255, 250, 255]),
        "honeydew" => Some([240, 255, 240, 255]),
        "palegreen" | "lightgreen" => Some([144, 238, 144, 255]),
        "mediumseagreen" => Some([60, 179, 113, 255]),
        "seagreen"  => Some([46, 139, 87, 255]),
        "forestgreen" => Some([34, 139, 34, 255]),
        "darkgreen"  => Some([0, 100, 0, 255]),
        "yellowgreen" => Some([154, 205, 50, 255]),
        "lawngreen"   => Some([124, 252, 0, 255]),
        "chartreuse"  => Some([127, 255, 0, 255]),
        "springgreen" => Some([0, 255, 127, 255]),
        "mediumspringgreen" => Some([0, 250, 154, 255]),
        "limegreen"   => Some([50, 205, 50, 255]),
        "crimson"     => Some([220, 20, 60, 255]),
        "darkred"     => Some([139, 0, 0, 255]),
        "firebrick"   => Some([178, 34, 34, 255]),
        "indianred"   => Some([205, 92, 92, 255]),
        "rosybrown"   => Some([188, 143, 143, 255]),
        "lightcoral"  => Some([240, 128, 128, 255]),
        "mistyrose"   => Some([255, 228, 225, 255]),
        "antiquewhite" => Some([250, 235, 215, 255]),
        "linen"        => Some([250, 240, 230, 255]),
        "bisque"       => Some([255, 228, 196, 255]),
        "peachpuff"    => Some([255, 218, 185, 255]),
        "moccasin"     => Some([255, 228, 181, 255]),
        "papayawhip"   => Some([255, 239, 213, 255]),
        "blanchedalmond" => Some([255, 235, 205, 255]),
        "wheat"        => Some([245, 222, 179, 255]),
        "burlywood"    => Some([222, 184, 135, 255]),
        "sandybrown"   => Some([244, 164, 96, 255]),
        "goldenrod"    => Some([218, 165, 32, 255]),
        "darkgoldenrod" => Some([184, 134, 11, 255]),
        "peru"         => Some([205, 133, 63, 255]),
        "saddlebrown"  => Some([139, 69, 19, 255]),
        "darkslategray" | "darkslategrey" => Some([47, 79, 79, 255]),
        "slategray" | "slategrey" => Some([112, 128, 144, 255]),
        "lightslategray" | "lightslategrey" => Some([119, 136, 153, 255]),
        "dimgray" | "dimgrey" => Some([105, 105, 105, 255]),
        "gainsboro" => Some([220, 220, 220, 255]),
        "whitesmoke" => Some([245, 245, 245, 255]),
        "aliceblue"  => Some([240, 248, 255, 255]),
        "ghostwhite" => Some([248, 248, 255, 255]),
        "seashell"   => Some([255, 245, 238, 255]),
        "floralwhite" => Some([255, 250, 240, 255]),
        "oldlace"    => Some([253, 245, 230, 255]),
        "cornsilk"   => Some([255, 248, 220, 255]),
        "lemonchiffon" => Some([255, 250, 205, 255]),
        "lightyellow" => Some([255, 255, 224, 255]),
        "lightgoldenrodyellow" => Some([250, 250, 210, 255]),
        "palegoldenrod" => Some([238, 232, 170, 255]),
        "darkkhaki"  => Some([189, 183, 107, 255]),
        "rebeccapurple" => Some([102, 51, 153, 255]),
        "mediumpurple"  => Some([147, 112, 219, 255]),
        "blueviolet"    => Some([138, 43, 226, 255]),
        "darkviolet"    => Some([148, 0, 211, 255]),
        "darkorchid"    => Some([153, 50, 204, 255]),
        "mediumorchid"  => Some([186, 85, 211, 255]),
        "orchid"        => Some([218, 112, 214, 255]),
        "plum"          => Some([221, 160, 221, 255]),
        "thistle"       => Some([216, 191, 216, 255]),
        "darkmagenta"   => Some([139, 0, 139, 255]),
        "midnightblue"  => Some([25, 25, 112, 255]),
        "darkblue"      => Some([0, 0, 139, 255]),
        "mediumblue"    => Some([0, 0, 205, 255]),
        "darkslateblue" => Some([72, 61, 139, 255]),
        "slateblue"     => Some([106, 90, 205, 255]),
        "mediumslateblue" => Some([123, 104, 238, 255]),
        "lightsteelblue" => Some([176, 196, 222, 255]),
        "azure"         => Some([240, 255, 255, 255]),
        "aquamarine"    => Some([127, 255, 212, 255]),
        "mediumaquamarine" => Some([102, 205, 170, 255]),
        "darkcyan"      => Some([0, 139, 139, 255]),
        "lightcyan"     => Some([224, 255, 255, 255]),
        "paleturquoise" => Some([175, 238, 238, 255]),
        "darkturquoise" => Some([0, 206, 209, 255]),
        "lightseagreen" => Some([32, 178, 170, 255]),
        "mediumvioletred" => Some([199, 21, 133, 255]),
        "palevioletred"   => Some([219, 112, 147, 255]),
        "darksalmon"    => Some([233, 150, 122, 255]),
        "lightsalmon"   => Some([255, 160, 122, 255]),
        "transparent" => Some([0, 0, 0, 0]),
        // CSS Color L4 system-color keywords (light mode defaults, CSS spec sRGB hodnoty)
        "canvas"           => Some([255, 255, 255, 255]),
        "canvastext"       => Some([0, 0, 0, 255]),
        "linktext"         => Some([0, 0, 238, 255]),
        "visitedtext"      => Some([85, 26, 139, 255]),
        "activetext"       => Some([255, 0, 0, 255]),
        "buttonface"       => Some([240, 240, 240, 255]),
        "buttontext"       => Some([0, 0, 0, 255]),
        "buttonborder"     => Some([173, 173, 173, 255]),
        "field"            => Some([255, 255, 255, 255]),
        "fieldtext"        => Some([0, 0, 0, 255]),
        "highlight"        => Some([0, 120, 215, 255]),
        "highlighttext"    => Some([255, 255, 255, 255]),
        "selecteditem"     => Some([0, 120, 215, 255]),
        "selecteditemtext" => Some([255, 255, 255, 255]),
        "mark"             => Some([255, 255, 0, 255]),
        "marktext"         => Some([0, 0, 0, 255]),
        "graytext"         => Some([109, 109, 109, 255]),
        "accentcolor"      => Some([0, 120, 215, 255]),
        "accentcolortext"  => Some([255, 255, 255, 255]),
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

/// CSS Shapes L1 - parsovany shape pro shape-outside.
#[derive(Debug, Clone, PartialEq)]
pub enum ShapeFunction {
    /// inset(top right bottom left [round <radius>])
    Inset { top: f32, right: f32, bottom: f32, left: f32, radius: f32 },
    /// circle(radius at cx cy) - radius v procentech (0..1)
    Circle { radius_pct: f32, cx_pct: f32, cy_pct: f32 },
    /// ellipse(rx ry at cx cy)
    Ellipse { rx_pct: f32, ry_pct: f32, cx_pct: f32, cy_pct: f32 },
    /// polygon(point1, point2, ...)
    Polygon(Vec<(f32, f32)>),
}

/// Parsuje shape-outside hodnotu (circle / ellipse / polygon / inset).
pub fn parse_shape_function(value: &str) -> Option<ShapeFunction> {
    let v = value.trim();
    if let Some(inner) = v.strip_prefix("circle(").and_then(|s| s.strip_suffix(')')) {
        let inner = inner.trim();
        let (rad, cxcy) = match inner.find(" at ") {
            Some(idx) => (&inner[..idx], &inner[idx+4..]),
            None => (inner, "50% 50%"),
        };
        let rad_pct = parse_percent_or_length_pct(rad.trim()).unwrap_or(0.5);
        let cs: Vec<&str> = cxcy.split_whitespace().collect();
        let cx = cs.get(0).and_then(|s| parse_percent_or_length_pct(s)).unwrap_or(0.5);
        let cy = cs.get(1).and_then(|s| parse_percent_or_length_pct(s)).unwrap_or(0.5);
        return Some(ShapeFunction::Circle { radius_pct: rad_pct, cx_pct: cx, cy_pct: cy });
    }
    if let Some(inner) = v.strip_prefix("ellipse(").and_then(|s| s.strip_suffix(')')) {
        let inner = inner.trim();
        let (rxry, cxcy) = match inner.find(" at ") {
            Some(idx) => (&inner[..idx], &inner[idx+4..]),
            None => (inner, "50% 50%"),
        };
        let rs: Vec<&str> = rxry.split_whitespace().collect();
        let rx = rs.get(0).and_then(|s| parse_percent_or_length_pct(s)).unwrap_or(0.5);
        let ry = rs.get(1).and_then(|s| parse_percent_or_length_pct(s)).unwrap_or(0.5);
        let cs: Vec<&str> = cxcy.split_whitespace().collect();
        let cx = cs.get(0).and_then(|s| parse_percent_or_length_pct(s)).unwrap_or(0.5);
        let cy = cs.get(1).and_then(|s| parse_percent_or_length_pct(s)).unwrap_or(0.5);
        return Some(ShapeFunction::Ellipse { rx_pct: rx, ry_pct: ry, cx_pct: cx, cy_pct: cy });
    }
    if let Some(inner) = v.strip_prefix("inset(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split_whitespace().collect();
        let nums: Vec<f32> = parts.iter().take_while(|p| !p.starts_with("round"))
            .filter_map(|p| parse_percent_or_length_pct(p)).collect();
        let (top, right, bottom, left) = match nums.len() {
            1 => (nums[0], nums[0], nums[0], nums[0]),
            2 => (nums[0], nums[1], nums[0], nums[1]),
            3 => (nums[0], nums[1], nums[2], nums[1]),
            n if n >= 4 => (nums[0], nums[1], nums[2], nums[3]),
            _ => (0.0, 0.0, 0.0, 0.0),
        };
        let radius = parts.iter().skip_while(|p| **p != "round").nth(1)
            .and_then(|p| parse_percent_or_length_pct(p)).unwrap_or(0.0);
        return Some(ShapeFunction::Inset { top, right, bottom, left, radius });
    }
    if let Some(inner) = v.strip_prefix("polygon(").and_then(|s| s.strip_suffix(')')) {
        let mut pts = Vec::new();
        for pair in inner.split(',') {
            let coords: Vec<&str> = pair.split_whitespace().collect();
            if coords.len() >= 2 {
                let x = parse_percent_or_length_pct(coords[0]).unwrap_or(0.0);
                let y = parse_percent_or_length_pct(coords[1]).unwrap_or(0.0);
                pts.push((x, y));
            }
        }
        return Some(ShapeFunction::Polygon(pts));
    }
    None
}

fn parse_percent_or_length_pct(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(p) = s.strip_suffix('%') {
        return p.trim().parse::<f32>().ok().map(|v| v / 100.0);
    }
    if s.ends_with("px") || s.ends_with("em") || s.ends_with("rem") {
        // Pri absence box rozmeru zatim approximace: 16px = 1rem -> 1.0
        return Some(parse_length(s) / 100.0);
    }
    s.parse::<f32>().ok()
}

/// CSS Color L4 - color() function s namespace.
/// Format: `color(<colorspace> <c1> <c2> <c3> [/ <alpha>])`.
/// Podporovane spaces: srgb, srgb-linear, display-p3, rec2020, a98-rgb, prophoto-rgb,
/// xyz, xyz-d50, xyz-d65. Pro rasterizaci vsechny mapovane primo do sRGB byte (clamp).
fn parse_color_function(inner: &str) -> Option<[u8; 4]> {
    let trimmed = inner.trim();
    let (space, rest) = split_first_color_token(trimmed)?;
    let (parts, alpha) = split_balanced_args(rest);
    if parts.len() < 3 { return None; }
    // V default 0..1 range pro vsechny color spaces. Procenta = 0..100%.
    let parse_chan = |s: &str| -> Option<f32> {
        let s = s.trim();
        if s == "none" { return Some(0.0); }
        if let Some(p) = s.strip_suffix('%') { return p.trim().parse::<f32>().ok().map(|v| v / 100.0); }
        s.parse::<f32>().ok()
    };
    let c1 = parse_chan(parts[0])?;
    let c2 = parse_chan(parts[1])?;
    let c3 = parse_chan(parts[2])?;
    // Pro display-p3 / rec2020 / a98-rgb proste pouzijeme hodnoty jako sRGB
    // (gamut mapping nedelame - vetsina cili je v sRGB ranged).
    let (r, g, b) = match space {
        "srgb-linear" | "srgb" | "display-p3" | "a98-rgb" | "prophoto-rgb" | "rec2020" => {
            ((c1 * 255.0).round() as u8, (c2 * 255.0).round() as u8, (c3 * 255.0).round() as u8)
        }
        "xyz" | "xyz-d50" | "xyz-d65" => {
            // XYZ -> sRGB matrix (D65)
            let r = 3.2404542 * c1 - 1.5371385 * c2 - 0.4985314 * c3;
            let g = -0.9692660 * c1 + 1.8760108 * c2 + 0.0415560 * c3;
            let b = 0.0556434 * c1 - 0.2040259 * c2 + 1.0572252 * c3;
            ((r.clamp(0.0, 1.0) * 255.0).round() as u8,
             (g.clamp(0.0, 1.0) * 255.0).round() as u8,
             (b.clamp(0.0, 1.0) * 255.0).round() as u8)
        }
        _ => return None,
    };
    let a = if let Some(byte) = alpha {
        byte
    } else if let Some(s) = parts.get(3) {
        parse_alpha(s.trim()).unwrap_or(255)
    } else { 255 };
    Some([r, g, b, a])
}

/// CSS Color L5 relative color: rgb(from <color> r g b [/ a]).
/// Slozky source barvy dostupne jako r/g/b/alpha keywordy (0..255 / 0..1).
/// Podporuje cisla, procenta, none, calc(r * 0.5).
fn parse_relative_rgb(inner: &str) -> Option<[u8; 4]> {
    let trimmed = inner.trim();
    let (src_str, rest) = split_first_color_token(trimmed)?;
    let src = parse_color(src_str)?;
    let rs = src[0] as f32; let gs = src[1] as f32; let bs = src[2] as f32;
    let alpha_s = src[3] as f32 / 255.0;
    let (parts, explicit_alpha) = split_balanced_args(rest);
    if parts.len() < 3 { return None; }
    let r = eval_color_component(parts[0], &[("r", rs), ("g", gs), ("b", bs), ("alpha", alpha_s * 255.0)], 255.0)?;
    let g = eval_color_component(parts[1], &[("r", rs), ("g", gs), ("b", bs), ("alpha", alpha_s * 255.0)], 255.0)?;
    let b = eval_color_component(parts[2], &[("r", rs), ("g", gs), ("b", bs), ("alpha", alpha_s * 255.0)], 255.0)?;
    let a: u8 = if let Some(byte) = explicit_alpha {
        byte
    } else if let Some(alpha_str) = parts.get(3) {
        let val = eval_color_component(alpha_str, &[("r", rs), ("g", gs), ("b", bs), ("alpha", alpha_s)], 1.0)?;
        (val.clamp(0.0, 1.0) * 255.0).round() as u8
    } else { src[3] };
    Some([r as u8, g as u8, b as u8, a])
}

/// CSS Color L5 relative HSL: hsl(from <color> h s l [/ a]).
fn parse_relative_hsl(inner: &str) -> Option<[u8; 4]> {
    let trimmed = inner.trim();
    let (src_str, rest) = split_first_color_token(trimmed)?;
    let src = parse_color(src_str)?;
    let (h, sat, lit) = rgb_to_hsl(src[0], src[1], src[2]);
    let alpha_s = src[3] as f32;
    let (parts, explicit_alpha) = split_balanced_args(rest);
    if parts.len() < 3 { return None; }
    let new_h = eval_color_component(parts[0], &[("h", h), ("s", sat * 100.0), ("l", lit * 100.0), ("alpha", alpha_s)], 360.0)?;
    let new_s = eval_color_component(parts[1], &[("h", h), ("s", sat * 100.0), ("l", lit * 100.0), ("alpha", alpha_s)], 100.0)?;
    let new_l = eval_color_component(parts[2], &[("h", h), ("s", sat * 100.0), ("l", lit * 100.0), ("alpha", alpha_s)], 100.0)?;
    let (r, g, b) = hsl_to_rgb(new_h, new_s / 100.0, new_l / 100.0);
    let a = if let Some(byte) = explicit_alpha { byte } else { src[3] };
    Some([r, g, b, a])
}

/// Vyhodnoti slozku barvy v relative color: cislo, "none", procento,
/// keyword (r/g/b/h/s/l/alpha), calc(<keyword> * 0.5).
fn eval_color_component(s: &str, vars: &[(&str, f32)], scale: f32) -> Option<f32> {
    let s = s.trim();
    if s == "none" { return Some(0.0); }
    // Keyword substituce
    for (name, val) in vars {
        if s == *name { return Some(*val); }
    }
    // calc(...) - jednoduchy: keyword * num nebo num * keyword
    if let Some(inner) = s.strip_prefix("calc(").and_then(|x| x.strip_suffix(')')) {
        return eval_simple_calc(inner.trim(), vars);
    }
    if let Some(p) = s.strip_suffix('%') {
        let v: f32 = p.trim().parse().ok()?;
        return Some(v / 100.0 * scale);
    }
    s.parse::<f32>().ok()
}

/// Mini-calc pro relative color: <a> <op> <b> kde a/b muze byt keyword nebo cislo.
fn eval_simple_calc(s: &str, vars: &[(&str, f32)]) -> Option<f32> {
    for op in ['+', '-', '*', '/'] {
        if let Some(idx) = s.rfind(op) {
            let (l, r) = s.split_at(idx);
            let r = &r[1..];
            let lv = eval_color_component(l.trim(), vars, 255.0)?;
            let rv = eval_color_component(r.trim(), vars, 255.0)?;
            return Some(match op {
                '+' => lv + rv, '-' => lv - rv,
                '*' => lv * rv, '/' => lv / rv,
                _ => return None,
            });
        }
    }
    eval_color_component(s, vars, 255.0)
}

/// Split argumentu pro relative color - respektuje zavorky (calc(...)).
/// Vrati (positional, optional_alpha_byte). Splituje na top-level mezerach.
fn split_balanced_args(inner: &str) -> (Vec<&str>, Option<u8>) {
    let bytes = inner.as_bytes();
    let mut depth = 0i32;
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let mut alpha_split: Option<usize> = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'/' if depth == 0 => { alpha_split = Some(i); break; }
            b' ' | b'\t' if depth == 0 => {
                if start < i { parts.push(inner[start..i].trim()); }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    let main_end = alpha_split.unwrap_or(inner.len());
    if start < main_end { parts.push(inner[start..main_end].trim()); }
    parts.retain(|p| !p.is_empty());
    let alpha = alpha_split.and_then(|idx| parse_alpha(inner[idx+1..].trim()));
    (parts, alpha)
}

/// Vrati (color_token, zbytek) - color je prvni token (mozna funkcni s zavorkami).
fn split_first_color_token(s: &str) -> Option<(&str, &str)> {
    let s = s.trim();
    let mut depth = 0i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ' ' | '\t' if depth == 0 => return Some((&s[..i], s[i..].trim())),
            _ => {}
        }
    }
    Some((s, ""))
}

/// RGB -> HSL: H 0..360, S 0..1, L 0..1.
fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-6 { return (0.0, 0.0, l); }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == rf { ((gf - bf) / d) + (if gf < bf { 6.0 } else { 0.0 }) }
            else if max == gf { ((bf - rf) / d) + 2.0 }
            else { ((rf - gf) / d) + 4.0 };
    (h * 60.0, s, l)
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
/// CSS Color L5 contrast() function - vyhrava pres comparing colory s background.
/// Format: contrast(<bg> vs <c1>, <c2>, ...) - vrati color s nejvyssim contrast vs bg.
/// Pouziva relative luminance.
fn parse_contrast(inner: &str) -> Option<[u8; 4]> {
    let trimmed = inner.trim();
    let (bg_str, candidates_str) = if let Some(idx) = trimmed.find(" vs ") {
        (&trimmed[..idx], &trimmed[idx+4..])
    } else {
        // Forma "contrast(bg)" -> vrati black/white podle bg luma
        let bg = parse_color(trimmed)?;
        let luma = (0.2126 * bg[0] as f32 + 0.7152 * bg[1] as f32 + 0.0722 * bg[2] as f32) / 255.0;
        return Some(if luma > 0.5 { [0, 0, 0, 255] } else { [255, 255, 255, 255] });
    };
    let bg = parse_color(bg_str.trim())?;
    let bg_luma = (0.2126 * bg[0] as f32 + 0.7152 * bg[1] as f32 + 0.0722 * bg[2] as f32) / 255.0;
    let mut best: Option<[u8; 4]> = None;
    let mut best_contrast = -1.0_f32;
    for cand_str in candidates_str.split(',') {
        if let Some(c) = parse_color(cand_str.trim()) {
            let cl = (0.2126 * c[0] as f32 + 0.7152 * c[1] as f32 + 0.0722 * c[2] as f32) / 255.0;
            let l1 = bg_luma.max(cl);
            let l2 = bg_luma.min(cl);
            let contrast = (l1 + 0.05) / (l2 + 0.05);
            if contrast > best_contrast {
                best_contrast = contrast;
                best = Some(c);
            }
        }
    }
    best
}

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

/// Konvertuje cislo na rimske cislice (1..3999).
pub fn to_roman(n: i32) -> String {
    let pairs: &[(i32, &str)] = &[
        (1000, "M"), (900, "CM"), (500, "D"), (400, "CD"),
        (100,  "C"), (90,  "XC"), (50,  "L"), (40,  "XL"),
        (10,   "X"), (9,   "IX"), (5,   "V"), (4,   "IV"),
        (1,    "I"),
    ];
    let mut out = String::new();
    let mut n = n.max(0);
    for (val, sym) in pairs {
        while n >= *val {
            out.push_str(sym);
            n -= val;
        }
    }
    out
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
    /// background-clip: text - bg renderovan jen pres glyfy textu.
    Text,
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
        "text"        => BgBox::Text,
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

/// 4x4 identity matrix (row-major).
#[inline]
fn mat4_identity() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ]
}

/// Multiply two 4x4 row-major matrices: out = a * b.
fn mat4_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut out = [0.0_f32; 16];
    for r in 0..4 {
        for c in 0..4 {
            let mut s = 0.0;
            for k in 0..4 {
                s += a[r * 4 + k] * b[k * 4 + c];
            }
            out[r * 4 + c] = s;
        }
    }
    out
}

/// Vrati matrix pro jeden TransformOp.
fn transform_op_matrix(op: &TransformOp) -> [f32; 16] {
    match op {
        TransformOp::Translate(tx, ty) => [
            1.0, 0.0, 0.0, *tx,
            0.0, 1.0, 0.0, *ty,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ],
        TransformOp::Translate3D { x, y, z } => [
            1.0, 0.0, 0.0, *x,
            0.0, 1.0, 0.0, *y,
            0.0, 0.0, 1.0, *z,
            0.0, 0.0, 0.0, 1.0,
        ],
        TransformOp::Scale(sx, sy) => [
            *sx, 0.0, 0.0, 0.0,
            0.0, *sy, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ],
        TransformOp::Scale3D { x, y, z } => [
            *x,  0.0, 0.0, 0.0,
            0.0, *y,  0.0, 0.0,
            0.0, 0.0, *z,  0.0,
            0.0, 0.0, 0.0, 1.0,
        ],
        TransformOp::Rotate(rad) => {
            let c = rad.cos();
            let s = rad.sin();
            [
                c,   -s,  0.0, 0.0,
                s,   c,   0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]
        }
        TransformOp::Rotate3D { x, y, z, angle_rad } => {
            // Rodrigues axis-angle. Predpoklad: osa normalizovana.
            let len = (x*x + y*y + z*z).sqrt();
            let (ux, uy, uz) = if len > 1e-6 {
                (x / len, y / len, z / len)
            } else {
                (0.0, 0.0, 1.0)
            };
            let c = angle_rad.cos();
            let s = angle_rad.sin();
            let one_c = 1.0 - c;
            [
                c + ux*ux*one_c,    ux*uy*one_c - uz*s, ux*uz*one_c + uy*s, 0.0,
                uy*ux*one_c + uz*s, c + uy*uy*one_c,    uy*uz*one_c - ux*s, 0.0,
                uz*ux*one_c - uy*s, uz*uy*one_c + ux*s, c + uz*uz*one_c,    0.0,
                0.0,                0.0,                0.0,                1.0,
            ]
        }
        TransformOp::Matrix3D(m) => *m,
        TransformOp::Perspective(d) => {
            let inv = if d.abs() > 1e-6 { -1.0 / d } else { 0.0 };
            [
                1.0, 0.0, 0.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, inv, 1.0,
            ]
        }
        TransformOp::None => mat4_identity(),
    }
}

/// Compose vsechny TransformOp do jedne 4x4 matrix.
/// CSS spec: `transform: T1 T2 T3` znamena P' = T1 * T2 * T3 * P
/// (zacina prvni ops zvenku - vlozeny posledni do mat multiplication).
pub fn compute_transform_matrix(ops: &[TransformOp], parent_perspective: Option<f32>) -> [f32; 16] {
    let mut m = mat4_identity();
    // Apply ops in order (left-multiply each)
    for op in ops {
        let opm = transform_op_matrix(op);
        m = mat4_mul(&m, &opm);
    }
    // Parent perspective wraps cely transform: P_persp * T = result
    if let Some(d) = parent_perspective {
        let persp = transform_op_matrix(&TransformOp::Perspective(d));
        m = mat4_mul(&persp, &m);
    }
    m
}

/// True pokud transform vyzaduje 3D pipeline (rotate3d X/Y, perspective,
/// matrix3d s non-zero z, translate3d s nonzero z).
/// Pure 2D transformy (Translate/Scale/Rotate Z) nepotrebuji RT pipeline.
pub fn needs_3d_pipeline(ops: &[TransformOp], parent_perspective: Option<f32>) -> bool {
    if parent_perspective.is_some() {
        // Perspective wrapper trebuje 3D jen pokud transform aspon nejak meni Z
        for op in ops {
            match op {
                TransformOp::Rotate3D { x, y, .. } if x.abs() > 1e-3 || y.abs() > 1e-3 => return true,
                TransformOp::Translate3D { z, .. } if z.abs() > 1e-3 => return true,
                TransformOp::Scale3D { z, .. } if (z - 1.0).abs() > 1e-3 => return true,
                TransformOp::Matrix3D(_) => return true,
                _ => {}
            }
        }
        return false;
    }
    for op in ops {
        match op {
            TransformOp::Rotate3D { x, y, .. } if x.abs() > 1e-3 || y.abs() > 1e-3 => return true,
            TransformOp::Perspective(_) => return true,
            TransformOp::Matrix3D(m) => {
                // Detekce 3D matice: m[8]/m[9]/m[2]/m[6]/m[14]/m[11] nenulove
                if m[2].abs() > 1e-3 || m[6].abs() > 1e-3
                    || m[8].abs() > 1e-3 || m[9].abs() > 1e-3
                    || m[11].abs() > 1e-3 || m[14].abs() > 1e-3 {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Vypocita 4x5 row-major color matrix z filter chain.
/// Format: vystup[i] = sum_j(m[i*5 + j] * input[j]) + m[i*5 + 4]; j=0..3 (rgba).
/// Vraci identity (1,0,0,0,0; 0,1,0,0,0; 0,0,1,0,0; 0,0,0,1,0) pro prazdny chain.
/// Blur a DropShadow se ignoruji (jine fazy pipeline).
pub fn compute_color_matrix(chain: &[FilterOp]) -> [f32; 20] {
    // Identity
    let mut m: [f32; 20] = [
        1.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 1.0, 0.0,
    ];
    // Compose: new = filter_matrix * current
    let compose = |m: &mut [f32; 20], f: [f32; 20]| {
        let mut r = [0.0_f32; 20];
        for row in 0..4 {
            for col in 0..4 {
                let mut s = 0.0;
                for k in 0..4 {
                    s += f[row * 5 + k] * m[k * 5 + col];
                }
                r[row * 5 + col] = s;
            }
            // Offset: f[row*5+4] + sum_k(f[row*5+k] * m[k*5+4])
            let mut s = f[row * 5 + 4];
            for k in 0..4 {
                s += f[row * 5 + k] * m[k * 5 + 4];
            }
            r[row * 5 + 4] = s;
        }
        *m = r;
    };
    let identity = || -> [f32; 20] {
        [
            1.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 1.0, 0.0,
        ]
    };
    for op in chain {
        match op {
            FilterOp::Brightness(v) => {
                let b = *v;
                let mut f = identity();
                f[0] = b; f[6] = b; f[12] = b;
                compose(&mut m, f);
            }
            FilterOp::Contrast(c) => {
                let off = 0.5 * (1.0 - c);
                let mut f = identity();
                f[0] = *c; f[4] = off;
                f[6] = *c; f[9] = off;
                f[12] = *c; f[14] = off;
                compose(&mut m, f);
            }
            FilterOp::Invert(i) => {
                let coef = 1.0 - 2.0 * i;
                let mut f = identity();
                f[0] = coef; f[4] = *i;
                f[6] = coef; f[9] = *i;
                f[12] = coef; f[14] = *i;
                compose(&mut m, f);
            }
            FilterOp::Grayscale(amount) => {
                let a = *amount;
                let inv = 1.0 - a;
                // SVG luminance coeffs 0.2126/0.7152/0.0722
                let lr = 0.2126_f32; let lg = 0.7152_f32; let lb = 0.0722_f32;
                let f: [f32; 20] = [
                    inv + a*lr, a*lg,       a*lb,       0.0, 0.0,
                    a*lr,       inv + a*lg, a*lb,       0.0, 0.0,
                    a*lr,       a*lg,       inv + a*lb, 0.0, 0.0,
                    0.0,        0.0,        0.0,        1.0, 0.0,
                ];
                compose(&mut m, f);
            }
            FilterOp::Sepia(amount) => {
                let a = *amount;
                let inv = 1.0 - a;
                // Sepia coeffs from W3C
                let f: [f32; 20] = [
                    inv + a*0.393, a*0.769,       a*0.189,       0.0, 0.0,
                    a*0.349,       inv + a*0.686, a*0.168,       0.0, 0.0,
                    a*0.272,       a*0.534,       inv + a*0.131, 0.0, 0.0,
                    0.0,           0.0,           0.0,           1.0, 0.0,
                ];
                compose(&mut m, f);
            }
            FilterOp::Saturate(s) => {
                // Standard SVG saturate matrix
                let inv = 1.0 - s;
                let lr = 0.213_f32; let lg = 0.715_f32; let lb = 0.072_f32;
                let f: [f32; 20] = [
                    lr*inv + s, lg*inv,     lb*inv,     0.0, 0.0,
                    lr*inv,     lg*inv + s, lb*inv,     0.0, 0.0,
                    lr*inv,     lg*inv,     lb*inv + s, 0.0, 0.0,
                    0.0,        0.0,        0.0,        1.0, 0.0,
                ];
                compose(&mut m, f);
            }
            FilterOp::HueRotate(deg) => {
                let rad = deg.to_radians();
                let c = rad.cos();
                let s = rad.sin();
                // SVG hue-rotate matrix (luma-preserving)
                let f: [f32; 20] = [
                    0.213 + c*0.787  + s*-0.213, 0.715 + c*-0.715 + s*-0.715, 0.072 + c*-0.072 + s*0.928,  0.0, 0.0,
                    0.213 + c*-0.213 + s*0.143,  0.715 + c*0.285  + s*0.140,  0.072 + c*-0.072 + s*-0.283, 0.0, 0.0,
                    0.213 + c*-0.213 + s*-0.787, 0.715 + c*-0.715 + s*0.715,  0.072 + c*0.928  + s*0.072,  0.0, 0.0,
                    0.0,                         0.0,                         0.0,                         1.0, 0.0,
                ];
                compose(&mut m, f);
            }
            FilterOp::Opacity(o) => {
                let mut f = identity();
                f[18] = *o;
                compose(&mut m, f);
            }
            FilterOp::Blur(_) | FilterOp::DropShadow { .. } => {
                // Jine faze pipeline - skip
            }
        }
    }
    m
}

/// True pokud color matrix neni identity (do epsilonu).
pub fn is_identity_matrix(m: &[f32; 20]) -> bool {
    let id: [f32; 20] = [
        1.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 1.0, 0.0,
    ];
    for i in 0..20 {
        if (m[i] - id[i]).abs() > 1e-4 { return false; }
    }
    true
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
/// Parsuje cely transform chain ("translate(10px) rotate(45deg) scale(1.5)").
/// Vyvazene zavorky pri tokenize.
pub fn parse_transform_chain(s: &str) -> Vec<TransformOp> {
    let mut out = Vec::new();
    let s = s.trim();
    if s == "none" || s.is_empty() { return out; }
    let mut chars = s.char_indices().peekable();
    while let Some(&(_, c)) = chars.peek() {
        if c.is_whitespace() { chars.next(); continue; }
        // Najdi name + (...)
        let start = chars.peek().map(|&(i, _)| i).unwrap_or(0);
        // Read az do '('
        while let Some(&(_, c)) = chars.peek() {
            if c == '(' { break; }
            if c.is_whitespace() { break; }
            chars.next();
        }
        // Pokracuj az '('
        while let Some(&(_, c)) = chars.peek() {
            if c == '(' { break; }
            chars.next();
        }
        // Sosa do matching ')' - vyvazene
        let mut end = start;
        if let Some(&(i, _)) = chars.peek() {
            chars.next(); // '('
            let mut depth = 1;
            while let Some(&(j, c)) = chars.peek() {
                end = j;
                if c == '(' { depth += 1; }
                if c == ')' { depth -= 1; if depth == 0 { chars.next(); break; } }
                chars.next();
            }
            let _ = i;
        }
        let name_args = &s[start..=end.min(s.len()-1)];
        if let Some(op) = parse_transform(name_args) {
            out.push(op);
        }
    }
    out
}

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
    // Dynamic / small / large viewport units - musi byt PRED vw/vh aby suffix match
    if let Some(num) = s.strip_suffix("dvw").or_else(|| s.strip_suffix("svw")).or_else(|| s.strip_suffix("lvw")) {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vw / 100.0;
    }
    if let Some(num) = s.strip_suffix("dvh").or_else(|| s.strip_suffix("svh")).or_else(|| s.strip_suffix("lvh")) {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vh / 100.0;
    }
    if let Some(num) = s.strip_suffix("dvi").or_else(|| s.strip_suffix("svi")).or_else(|| s.strip_suffix("lvi")) {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vw / 100.0;
    }
    if let Some(num) = s.strip_suffix("dvb").or_else(|| s.strip_suffix("svb")).or_else(|| s.strip_suffix("lvb")) {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vh / 100.0;
    }
    if let Some(num) = s.strip_suffix("vi") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vw / 100.0;
    }
    if let Some(num) = s.strip_suffix("vb") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * vh / 100.0;
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
    // Character / line-height units
    // ch = sirka 0 (zero glyph) - aproximace 0.5 * font-size (default 16px)
    // lh = line-height current element - aproximace 1.2 * font-size
    // rlh = root lh
    if let Some(num) = s.strip_suffix("rlh") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 16.0 * 1.2;
    }
    if let Some(num) = s.strip_suffix("lh") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 16.0 * 1.2;
    }
    if let Some(num) = s.strip_suffix("ch") {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 16.0 * 0.5;
    }
    if let Some(num) = s.strip_suffix("ex") {
        // ex = vyska x-height - aproximace 0.5 * font-size
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 16.0 * 0.5;
    }
    // Absolutni jednotky
    if let Some(num) = s.strip_suffix("cm") {
        // 1cm = 96/2.54 px ~ 37.795
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 37.795;
    }
    if let Some(num) = s.strip_suffix("mm") {
        // 1mm = 1cm/10
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 3.7795;
    }
    if let Some(num) = s.strip_suffix("Q") {
        // 1Q = 0.25mm
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 0.9449;
    }
    if let Some(num) = s.strip_suffix("in") {
        // 1in = 96px
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 96.0;
    }
    if let Some(num) = s.strip_suffix("pc") {
        // 1pc = 12pt = 16px
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * 16.0;
    }
    if let Some(num) = s.strip_suffix('%') {
        let v: f32 = num.trim().parse().unwrap_or(0.0);
        return v * parent_size / 100.0;
    }
    s.parse().unwrap_or(0.0)
}
