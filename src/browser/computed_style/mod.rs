//! Typed ComputedStyle per CSS spec.
//!
//! L5 refactor: nahrazuje `HashMap<String, String>` cascade output typed
//! struct. Cascade pln-parsuje hodnoty pri build, ne pri kazdem cteni v
//! build_box_inner.
//!
//! Vyhody:
//! - Perf: zadny hash lookup + re-parse per node read
//! - Type safety: PropertyId enum + typed value (compile-time check)
//! - Pamet: pole struct < HashMap + String alloc per property
//! - Cleaner code: `cs.background_color` vs `s.get("background-color").and_then(parse_color)`
//!
//! Naming: snake_case 1:1 mapping CSS kebab-case property names.
//! (CSS `background-color` -> Rust `background_color`). Pro JS
//! getComputedStyle: snake -> camel mapper (background_color -> backgroundColor).
//!
//! Stage 1 (current): struct kostry + zakladni typy. Naplnovani postupne
//! z cascade.rs (dual-write s puvodni HashMap).

pub mod color;
pub mod length;
pub mod property;
pub mod cascade_decl;

pub use color::Color;
pub use length::Length;
pub use property::{PropertyId, PROPERTY_ID_BITSET_WORDS};
pub use cascade_decl::{CascadeDecl, CascadeOrigin, Specificity};

/// L5 step 4 Phase 3 Step G: bitset replace HashSet<PropertyId>. Stack-allocated
/// [u64; 8] = 512 bits. 246 current variants + headroom. O(1) ops via bit shift.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PropertySet([u64; PROPERTY_ID_BITSET_WORDS]);

impl PropertySet {
    #[inline]
    pub fn new() -> Self { Self([0u64; PROPERTY_ID_BITSET_WORDS]) }
    #[inline]
    pub fn insert(&mut self, prop: PropertyId) {
        let idx = prop.as_index();
        let word = idx / 64;
        let bit = idx % 64;
        if word < PROPERTY_ID_BITSET_WORDS {
            self.0[word] |= 1u64 << bit;
        }
    }
    #[inline]
    pub fn contains(&self, prop: PropertyId) -> bool {
        let idx = prop.as_index();
        let word = idx / 64;
        let bit = idx % 64;
        if word >= PROPERTY_ID_BITSET_WORDS { return false; }
        (self.0[word] & (1u64 << bit)) != 0
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|w| *w == 0)
    }
}

/// Per-element typed computed style. Node ptr (Rc::as_ptr usize) -> resolved
/// ComputedStyle. L5 stage 2c: definovan, naplnovan v stage 3 dual-write
/// z cascade. layout/paint zatim cte z StyleMap (HashMap<String,String>).
pub type ComputedStyleMap = std::collections::HashMap<usize, ComputedStyle>;

/// Per-element collected declarations (vsechny, vc. invalid). Pro devtools
/// strikethrough display - layout neuses, ulozeno cisto pro devtools UI
/// (L5 stage 5).
pub type DeclarationsMap = std::collections::HashMap<usize, Vec<CascadeDecl>>;

/// Cascade output bundle. L5 step 4 Phase G: style_map field DROPPED.
/// Render pipeline kompletne typed (cs + declarations). Tests stale mohou
/// volat cascade_with_viewport pro legacy StyleMap pristup separately.
#[derive(Debug, Default)]
pub struct CascadeOutput {
    /// Typed computed styles. Primary cascade output.
    pub computed: ComputedStyleMap,
    /// All declarations vc. invalid. Pro devtools (stage 5).
    pub declarations: DeclarationsMap,
}

/// Resolved computed style per element (CSS Cascade L4 §4.1 specified->
/// computed value mapping).
///
/// Pole jsou pre-parsed typed values. Pro hodnoty zavisle na kontextu
/// (% delky, em/rem) zachovavame Length enum a resolver volame az pri
/// layout dispatch s realnym parent/viewport sizem.
#[derive(Debug, Clone)]
pub struct ComputedStyle {
    // ─── Color / Background ────────────────────────────────────────────
    pub color: Color,                          // CSS `color` (text)
    pub background_color: Color,               // CSS `background-color`
    // (background_image, position, size, repeat etc. zatim HashMap fallback)

    // ─── Font ──────────────────────────────────────────────────────────
    pub font_size: Length,                     // computed length, default 16px
    pub font_weight: u32,                      // 1..1000 (normal=400, bold=700)
    pub font_style_italic: bool,               // italic | oblique = true
    pub line_height: LineHeight,
    pub font_family: Vec<FontFamily>,

    // ─── Box model (Length keep unit for percent resolution) ──────────
    pub margin_top: Length,
    pub margin_right: Length,
    pub margin_bottom: Length,
    pub margin_left: Length,
    pub padding_top: Length,
    pub padding_right: Length,
    pub padding_bottom: Length,
    pub padding_left: Length,
    pub width: Length,                         // Auto = no explicit
    pub height: Length,
    pub min_width: Length,
    pub min_height: Length,
    pub max_width: Length,
    pub max_height: Length,

    // ─── Position offset ──────────────────────────────────────────────
    pub top: Length,
    pub right: Length,
    pub bottom: Length,
    pub left: Length,

    // ─── Visual ─────────────────────────────────────────────────────────
    pub opacity: f32,                          // 0..1, default 1
    pub visibility: Visibility,
    pub cursor: Cursor,

    // ─── Display / Position (batch 2) ─────────────────────────────────
    pub display: Display,
    pub position: PositionKind,
    pub z_index: ZIndex,

    // ─── Text properties (batch 9) ────────────────────────────────────
    pub text_align: TextAlign,
    pub white_space: WhiteSpace,
    pub word_break: WordBreak,
    pub overflow_wrap: OverflowWrap,

    // ─── Writing / Box (batch 10) ─────────────────────────────────────
    pub writing_mode: WritingMode,
    pub direction: Direction,
    pub box_sizing: BoxSizing,
    pub pointer_events: PointerEvents,

    // ─── Overflow / Float (batch 11) ──────────────────────────────────
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,
    pub float: Float,
    pub clear: Clear,

    // ─── Flex (batch 12) ──────────────────────────────────────────────
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
    pub flex_grow: f32,
    pub flex_shrink: f32,

    // ─── Flex/Grid alignment (batch 13) ───────────────────────────────
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub align_content: AlignContent,
    pub align_self: AlignSelf,

    // ─── Flex item + gap (batch 14) ───────────────────────────────────
    pub flex_basis: FlexBasis,
    pub order: i32,
    pub row_gap: Length,
    pub column_gap: Length,

    // ─── Border widths (batch 15) ─────────────────────────────────────
    pub border_top_width: Length,
    pub border_right_width: Length,
    pub border_bottom_width: Length,
    pub border_left_width: Length,

    // ─── Border colors (batch 16) ─────────────────────────────────────
    pub border_top_color: Color,
    pub border_right_color: Color,
    pub border_bottom_color: Color,
    pub border_left_color: Color,

    // ─── Border styles (batch 17) ─────────────────────────────────────
    pub border_top_style: BorderStyle,
    pub border_right_style: BorderStyle,
    pub border_bottom_style: BorderStyle,
    pub border_left_style: BorderStyle,

    // ─── Border radius (batch 18) ─────────────────────────────────────
    pub border_top_left_radius: Length,
    pub border_top_right_radius: Length,
    pub border_bottom_right_radius: Length,
    pub border_bottom_left_radius: Length,

    // ─── Outline (batch 19) ───────────────────────────────────────────
    pub outline_width: Length,
    pub outline_style: BorderStyle,
    pub outline_color: Color,
    pub outline_offset: Length,

    // ─── Text decoration (batch 20) ───────────────────────────────────
    pub text_decoration_line: TextDecorationLine,
    pub text_decoration_style: TextDecorationStyle,
    pub text_decoration_color: Color,
    pub text_decoration_thickness: Length,

    // ─── Text misc (batch 21) ─────────────────────────────────────────
    pub text_indent: Length,
    pub text_transform: TextTransform,
    pub text_overflow: TextOverflow,
    pub vertical_align: VerticalAlign,

    // ─── List + tab (batch 22) ────────────────────────────────────────
    pub list_style_type: ListStyleType,
    pub list_style_position: ListStylePosition,
    pub list_style_image: ListStyleImage,
    pub tab_size: f32,

    // ─── Table (batch 23) ─────────────────────────────────────────────
    pub border_collapse: BorderCollapse,
    pub border_spacing_h: Length,
    pub border_spacing_v: Length,
    pub table_layout: TableLayout,
    pub caption_side: CaptionSide,

    // ─── Replaced + resize + ratio (batch 24) ─────────────────────────
    pub object_fit: ObjectFit,
    pub object_position_x: Length,
    pub object_position_y: Length,
    pub aspect_ratio: Option<f32>,    // None = auto
    pub resize: Resize,

    // ─── Transitions (batch 25) ───────────────────────────────────────
    pub transition_property: String,         // raw CSS comma-list (all|none|<ident>)
    pub transition_duration: Vec<f32>,        // seconds
    pub transition_timing_function: Vec<TimingFunction>,
    pub transition_delay: Vec<f32>,           // seconds

    // ─── Animations: timing (batch 26) ────────────────────────────────
    pub animation_name: Vec<String>,
    pub animation_duration: Vec<f32>,
    pub animation_timing_function: Vec<TimingFunction>,
    pub animation_delay: Vec<f32>,

    // ─── Animations: control (batch 27) ───────────────────────────────
    pub animation_iteration_count: Vec<f32>,     // f32 = inf via f32::INFINITY
    pub animation_direction: Vec<AnimationDirection>,
    pub animation_fill_mode: Vec<AnimationFillMode>,
    pub animation_play_state: Vec<AnimationPlayState>,

    // ─── Transform (batch 28) ─────────────────────────────────────────
    pub transform: String,           // raw CSS chain (parser v layout/transform_parse)
    pub transform_origin_x: Length,
    pub transform_origin_y: Length,
    pub perspective: Length,         // None = no perspective; px > 0 = vanishing dist

    // ─── Filter + blend (batch 29) ────────────────────────────────────
    pub filter: String,              // raw chain (parser v layout/filter)
    pub backdrop_filter: String,     // raw chain
    pub mix_blend_mode: BlendMode,
    pub isolation: Isolation,

    // ─── Grid template (batch 30) ─────────────────────────────────────
    pub grid_template_columns: String,    // raw track list (parser v layout_engine/grid)
    pub grid_template_rows: String,
    pub grid_template_areas: String,
    pub grid_auto_flow: GridAutoFlow,

    // ─── Grid item placement (batch 31) ───────────────────────────────
    pub grid_column_start: GridLine,
    pub grid_column_end: GridLine,
    pub grid_row_start: GridLine,
    pub grid_row_end: GridLine,

    // ─── Grid implicit + justify (batch 32) ───────────────────────────
    pub grid_auto_columns: String,    // raw track
    pub grid_auto_rows: String,
    pub justify_items: JustifyItems,
    pub justify_self: JustifySelf,

    // ─── Shadows + clip + scroll (batch 33) ───────────────────────────
    pub box_shadow: String,          // raw chain (parser v layout/shadows)
    pub text_shadow: String,         // raw chain
    pub clip_path: String,           // raw shape (parser v layout/shape_fn)
    pub scroll_behavior: ScrollBehavior,

    // ─── Background layers (batch 34) ─────────────────────────────────
    pub background_image: String,    // raw multi-layer (parser v layout/backgrounds)
    pub background_position: String,
    pub background_size: String,
    pub background_repeat: String,

    // ─── Background extras + caret (batch 35) ─────────────────────────
    pub background_attachment: BackgroundAttachment,
    pub background_clip: BackgroundClip,
    pub background_origin: BackgroundClip,    // stejny enum (border/padding/content-box)
    pub caret_color: Color,

    // ─── Appearance + counters (batch 36) ─────────────────────────────
    pub appearance: Appearance,
    pub content: String,                 // raw (pseudo-element content)
    pub counter_reset: String,           // raw "name N" list
    pub counter_increment: String,       // raw "name N" list

    // ─── Multi-column (batch 37) ──────────────────────────────────────
    pub column_count: ColumnCount,       // Auto | Integer(u32)
    pub column_width: Length,            // Auto | <length>
    pub column_fill: ColumnFill,
    pub column_span: ColumnSpan,

    // ─── Scroll margin (batch 38) ─────────────────────────────────────
    pub scroll_margin_top: Length,
    pub scroll_margin_right: Length,
    pub scroll_margin_bottom: Length,
    pub scroll_margin_left: Length,

    // ─── Scroll padding (batch 39) ────────────────────────────────────
    pub scroll_padding_top: Length,
    pub scroll_padding_right: Length,
    pub scroll_padding_bottom: Length,
    pub scroll_padding_left: Length,

    // ─── Scroll snap + overscroll (batch 40) ──────────────────────────
    pub scroll_snap_type: String,         // raw "<axis> <strictness>"
    pub scroll_snap_align: ScrollSnapAlign,
    pub overscroll_behavior_x: OverscrollBehavior,
    pub overscroll_behavior_y: OverscrollBehavior,

    // ─── L5 step 4 batch 18: chybejici pole pro Phase 3 unblock ──────
    pub letter_spacing: LetterSpacing,
    pub word_spacing: LetterSpacing,             // stejna shape (Normal | Length)
    pub accent_color: AccentColor,               // Auto | Color
    pub scrollbar_width: ScrollbarWidth,
    pub scrollbar_color: ScrollbarColor,         // Auto | (thumb, track)
    pub image_rendering: ImageRendering,
    pub text_underline_offset: Length,           // Auto = Length::Auto
    pub line_height_step: Length,
    pub speak: SpeakKind,
    pub mask_image: String,                      // raw multi-layer
    pub shape_outside: ShapeOutsideVal,
    pub shape_margin: Length,
    pub shape_image_threshold: f32,              // 0..1
    pub contain_intrinsic_block_size: Length,
    pub contain_intrinsic_inline_size: Length,
    pub contain_intrinsic_size: Length,
    pub orphans: u32,
    pub widows: u32,
    pub border_image_source: String,             // raw url() / gradient
    pub border_image_slice: String,
    pub border_image_width: String,
    pub text_emphasis: String,                   // shorthand raw
    pub text_emphasis_color: Color,

    // ─── EXPERIMENTAL: CSS L4/L5 / nove specs (Phase B) ───────────────
    // Tyto pole jsou pripraveny pro budouci typed migraci. Cascade NAPLNUJE
    // raw string z deklarace, layout/paint zatim NEMA implementaci. Hodnota
    // existuje pro getComputedStyle + devtools display; rendering ignoruje.
    // POZN: pri pridavani implementace zkontrolovat CS field type + odebrat
    // EXPERIMENTAL marker.

    /// CSS Anchor Positioning L1 (CR 2024): anchor-name.
    /// PARTIAL: storage + devtools display ANO. Layout anchor() fn + position
    /// resolution NOT impl - pole pro cilek elementu.
    pub anchor_name: String,
    /// CSS Anchor Positioning L1: position-anchor. PARTIAL: storage only.
    pub position_anchor: String,
    /// CSS Anchor Positioning L1: inset-area. PARTIAL: storage only.
    pub inset_area: String,

    /// CSS View Transitions L1: view-transition-name. PARTIAL: storage only.
    /// Pri navigation triggeruje cross-document transition (BROWSER impl required).
    pub view_transition_name: String,

    /// CSS Scroll-Driven Animations L1: view-timeline-name. PARTIAL: storage only.
    pub view_timeline_name: String,
    /// CSS Scroll-Driven Animations L1: view-timeline-axis. PARTIAL.
    pub view_timeline_axis: String,
    /// CSS Scroll-Driven Animations L1: scroll-timeline-name. PARTIAL.
    pub scroll_timeline_name: String,
    /// CSS Scroll-Driven Animations L1: scroll-timeline-axis. PARTIAL.
    pub scroll_timeline_axis: String,
    /// CSS Scroll-Driven Animations L1: animation-timeline. IMPL: "scroll"
    /// trigger scroll-driven keyframe interp via apply_scroll_animations_typed.
    /// Named timelines / view-timeline (element entry/exit) NOT impl.
    pub animation_timeline_l5: String,

    /// CSS Containment L3: content-visibility (partial impl: hidden hides
    /// element. auto = visible (viewport-relative optimization NOT impl)).
    pub content_visibility: ContentVisibility,
    /// CSS Containment L3: container shorthand (raw - cascade expands).
    /// PARTIAL: @container queries cascade-side ANO; runtime container size
    /// invalidation NOT impl.
    pub container: String,
    /// CSS Containment L3: container-type. normal | inline-size | size.
    pub container_type: ContainerType,
    /// CSS Containment L3: container-name. Custom ident (raw).
    pub container_name: String,

    /// CSS Text L4: text-wrap (partial impl: nowrap/wrap mapped na white-space).
    /// balance/pretty/stable advanced line-break NOT implemented = behaves as wrap.
    pub text_wrap: TextWrap,
    /// CSS Text L4: text-wrap-style (typed). auto | balance | pretty | stable.
    /// PARTIAL: text-wrap-style algorithms (balance/pretty) NOT impl.
    pub text_wrap_style: TextWrapStyle,
    /// CSS Text L4: text-wrap-mode (typed). wrap | nowrap. Mirror text-wrap subset.
    pub text_wrap_mode: TextWrapMode,

    /// CSS Inline L3: text-box-trim (typed). none | trim-start | trim-end | trim-both.
    /// PARTIAL: line-box trimming NOT impl (no metric adjustment).
    pub text_box_trim: TextBoxTrim,
    /// CSS Inline L3: text-box-edge (typed). leading | text | cap | ex | ideographic | ideographic-ink.
    /// PARTIAL: edge adjustments NOT impl.
    pub text_box_edge: TextBoxEdge,

    /// CSS Forms L1: field-sizing. content = auto-size to text.
    pub field_sizing: FieldSizing,

    /// CSS Color L4: print-color-adjust (typed). PARTIAL: layout/paint
    /// pri tisku NOT implemented; field zachycen pres cascade + devtools.
    pub print_color_adjust: PrintColorAdjust,
    /// CSS Color L4: forced-color-adjust (typed). PARTIAL: Windows high
    /// contrast detection NOT implemented; respect future toggle.
    pub forced_color_adjust: ForcedColorAdjust,
    /// CSS Color L4: color-scheme (typed). PARTIAL: scrollbar / form control
    /// dark rendering NOT implemented; passed na browser shell hint.
    pub color_scheme: ColorScheme,

    /// CSS Math L3: math-style. compact | normal. PARTIAL: math layout NOT impl.
    pub math_style: MathStyle,
    /// CSS Math L3: math-depth (raw - keyword/integer/relative). PARTIAL.
    pub math_depth: String,

    /// CSS Ruby L1: ruby-position (typed). PARTIAL: ruby layout NOT fully impl.
    pub ruby_position: RubyPosition,
    /// CSS Ruby L1: ruby-align. PARTIAL.
    pub ruby_align: RubyAlign,

    /// EXPERIMENTAL CSS Pseudo-classes L4: @starting-style support (no field needed,
    /// jen marker ze parser dokaze rozpoznat. Cascade prozatim ignoruje.)
    /// Marker komentar - skutecna podpora vyzaduje samostatny cascade pass.

    /// EXPERIMENTAL CSS Cascading L6: @scope - cascade ignores.

    /// EXPERIMENTAL CSS Overflow L4: scroll-marker / scroll-button-controls.
    /// CSS Overflow L4: scroll-marker-group. PARTIAL: storage only.
    pub scroll_marker_group: String,

    // ─── L5 step 4 Phase 3: raw shorthand storage pro shorthand sites v build_box_inner.
    // Cascade dovni shorthand expansion pro gap/inset/border-radius/place-*/atd,
    // ale NEPCHODI: flex, columns, column-rule, scroll-padding, scroll-margin.
    // Pro tyto musi layout parse raw shorthand. CS drzi raw string pro reach.
    /// CSS `flex` shorthand: <grow> <shrink> <basis>.
    pub flex_shorthand: String,
    /// CSS `columns` shorthand: <width> <count>.
    pub columns_shorthand: String,
    /// CSS `column-rule` shorthand: <width> <style> <color>.
    pub column_rule_shorthand: String,
    /// CSS `scroll-padding` shorthand: 1-4 hodnoty.
    pub scroll_padding_shorthand: String,
    /// CSS `scroll-margin` shorthand: 1-4 hodnoty.
    pub scroll_margin_shorthand: String,
    /// CSS `gap` shorthand: row + column. (cascade expands ale layout obcas re-parse.)
    pub gap_shorthand: String,
    /// CSS `inset` shorthand: top/right/bottom/left.
    pub inset_shorthand: String,
    /// CSS `text-decoration` shorthand: line/style/color/thickness.
    pub text_decoration_shorthand: String,
    /// CSS `overflow` shorthand: x + y.
    pub overflow_shorthand: String,
    /// CSS `border-width` / `border-style` / `border-color` shorthand (4 sides).
    pub border_width_shorthand: String,
    pub border_style_shorthand: String,
    pub border_color_shorthand: String,
    /// CSS `background` shorthand (color + image + position + size + repeat + ...).
    /// Layout dela gradient detection pres tento raw string.
    pub background_shorthand: String,
    /// CSS `contain` shorthand.
    pub contain_shorthand: String,
    /// CSS counter shorthandy.
    pub counter_reset_raw: String,
    pub counter_increment_raw: String,
    pub counter_set_raw: String,
    /// CSS `object-position` raw (cs ma typed x/y, ale layout drzi raw).
    pub object_position_raw: String,
    /// CSS `grid-column`/`grid-row` shorthandy (cs ma start/end typed).
    pub grid_column_raw: String,
    pub grid_row_raw: String,
    /// Background multi-layer raw storage (cs ma single typed; tady raw pro layout split).
    pub background_clip_raw: String,
    pub background_origin_raw: String,
    pub background_attachment_raw: String,
    /// Border-image multi-component raw.
    pub border_image_outset_raw: String,
    pub border_image_repeat_raw: String,
    /// Column-rule longhandy raw (cs nema typed fields pro tyto).
    pub column_rule_width_raw: String,
    pub column_rule_style_raw: String,
    pub column_rule_color_raw: String,

    /// L5 step 4 Phase G+G: bitset PropertyId set. Pouziva PropertySet
    /// [u64; 8] = 512 bits. Stack-alloc, O(1) ops. Saves ~100B per cs vs HashSet.
    pub explicit_set: PropertySet,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self::initial()
    }
}

impl ComputedStyle {
    /// CSS spec initial value pro vsechny properties.
    ///
    /// Inherited properties (color, font-*, line-height) maji "inherit"
    /// jako vychozi pri cascade resolution; tady vracime spec initial
    /// pro root element (drive nez cascade inheritance).
    pub fn initial() -> Self {
        Self {
            color: Color::Rgba { r: 0, g: 0, b: 0, a: 255 },
            background_color: Color::Rgba { r: 0, g: 0, b: 0, a: 0 }, // transparent
            font_size: Length::Px(16.0),
            font_weight: 400,
            font_style_italic: false,
            line_height: LineHeight::Normal,
            font_family: Vec::new(),
            margin_top: Length::Px(0.0),
            margin_right: Length::Px(0.0),
            margin_bottom: Length::Px(0.0),
            margin_left: Length::Px(0.0),
            padding_top: Length::Px(0.0),
            padding_right: Length::Px(0.0),
            padding_bottom: Length::Px(0.0),
            padding_left: Length::Px(0.0),
            width: Length::Auto,
            height: Length::Auto,
            min_width: Length::Px(0.0),
            min_height: Length::Px(0.0),
            max_width: Length::None,
            max_height: Length::None,
            top: Length::Auto,
            right: Length::Auto,
            bottom: Length::Auto,
            left: Length::Auto,
            opacity: 1.0,
            visibility: Visibility::Visible,
            cursor: Cursor::Auto,
            // background-color initial = transparent. Stejna hodnota jako
            // ComputedStyle.background_color drive (rgba 0,0,0,0).
            display: Display::Inline,           // CSS spec initial pro non-replaced
            position: PositionKind::Static,
            z_index: ZIndex::Auto,
            text_align: TextAlign::Start,
            white_space: WhiteSpace::Normal,
            word_break: WordBreak::Normal,
            overflow_wrap: OverflowWrap::Normal,
            writing_mode: WritingMode::HorizontalTb,
            direction: Direction::Ltr,
            box_sizing: BoxSizing::ContentBox,
            pointer_events: PointerEvents::Auto,
            overflow_x: Overflow::Visible,
            overflow_y: Overflow::Visible,
            float: Float::None,
            clear: Clear::None,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Nowrap,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Stretch,
            align_content: AlignContent::Normal,
            align_self: AlignSelf::Auto,
            flex_basis: FlexBasis::Auto,
            order: 0,
            row_gap: Length::Px(0.0),
            column_gap: Length::Px(0.0),
            // CSS spec: border-*-width initial = `medium` = 3px. Engine
            // pouziva 0 default (UA stylesheet) - bez border declaration
            // engine ne-vykresluje border. Spravne: pouzit 0 jako "no
            // explicit border" sentinel; border-style:none neg overrides.
            border_top_width: Length::Px(0.0),
            border_right_width: Length::Px(0.0),
            border_bottom_width: Length::Px(0.0),
            border_left_width: Length::Px(0.0),
            // CSS spec: border-*-color initial = currentColor (= text color).
            // Engine drz currentColor sentinel, paint resolve proti cs.color.
            border_top_color: Color::CurrentColor,
            border_right_color: Color::CurrentColor,
            border_bottom_color: Color::CurrentColor,
            border_left_color: Color::CurrentColor,
            border_top_style: BorderStyle::None,
            border_right_style: BorderStyle::None,
            border_bottom_style: BorderStyle::None,
            border_left_style: BorderStyle::None,
            border_top_left_radius: Length::Px(0.0),
            border_top_right_radius: Length::Px(0.0),
            border_bottom_right_radius: Length::Px(0.0),
            border_bottom_left_radius: Length::Px(0.0),
            outline_width: Length::Px(3.0), // medium
            outline_style: BorderStyle::None,
            outline_color: Color::CurrentColor,
            outline_offset: Length::Px(0.0),
            text_decoration_line: TextDecorationLine::NONE,
            text_decoration_style: TextDecorationStyle::Solid,
            text_decoration_color: Color::CurrentColor,
            text_decoration_thickness: Length::Auto,
            text_indent: Length::Px(0.0),
            text_transform: TextTransform::None,
            text_overflow: TextOverflow::Clip,
            vertical_align: VerticalAlign::Baseline,
            list_style_type: ListStyleType::Disc,
            list_style_position: ListStylePosition::Outside,
            list_style_image: ListStyleImage::None,
            tab_size: 8.0,
            border_collapse: BorderCollapse::Separate,
            border_spacing_h: Length::Px(0.0),
            border_spacing_v: Length::Px(0.0),
            table_layout: TableLayout::Auto,
            caption_side: CaptionSide::Top,
            object_fit: ObjectFit::Fill,
            object_position_x: Length::Percent(50.0),
            object_position_y: Length::Percent(50.0),
            aspect_ratio: None,
            resize: Resize::None,
            transition_property: "all".into(),
            transition_duration: vec![0.0],
            transition_timing_function: vec![TimingFunction::Ease],
            transition_delay: vec![0.0],
            animation_name: vec!["none".into()],
            animation_duration: vec![0.0],
            animation_timing_function: vec![TimingFunction::Ease],
            animation_delay: vec![0.0],
            animation_iteration_count: vec![1.0],
            animation_direction: vec![AnimationDirection::Normal],
            animation_fill_mode: vec![AnimationFillMode::None],
            animation_play_state: vec![AnimationPlayState::Running],
            transform: "none".into(),
            transform_origin_x: Length::Percent(50.0),
            transform_origin_y: Length::Percent(50.0),
            perspective: Length::None,
            filter: "none".into(),
            backdrop_filter: "none".into(),
            mix_blend_mode: BlendMode::Normal,
            isolation: Isolation::Auto,
            grid_template_columns: String::new(),
            grid_template_rows: String::new(),
            grid_template_areas: String::new(),
            grid_auto_flow: GridAutoFlow::Row,
            grid_column_start: GridLine::Auto,
            grid_column_end: GridLine::Auto,
            grid_row_start: GridLine::Auto,
            grid_row_end: GridLine::Auto,
            grid_auto_columns: "auto".into(),
            grid_auto_rows: "auto".into(),
            justify_items: JustifyItems::Normal,
            justify_self: JustifySelf::Auto,
            box_shadow: "none".into(),
            text_shadow: "none".into(),
            clip_path: "none".into(),
            scroll_behavior: ScrollBehavior::Auto,
            background_image: "none".into(),
            background_position: "0% 0%".into(),
            background_size: "auto".into(),
            background_repeat: "repeat".into(),
            background_attachment: BackgroundAttachment::Scroll,
            background_clip: BackgroundClip::BorderBox,
            background_origin: BackgroundClip::PaddingBox,
            caret_color: Color::CurrentColor,    // auto = currentColor
            appearance: Appearance::Auto,
            content: "normal".into(),
            counter_reset: "none".into(),
            counter_increment: "none".into(),
            column_count: ColumnCount::Auto,
            column_width: Length::Auto,
            column_fill: ColumnFill::Balance,
            column_span: ColumnSpan::None,
            scroll_margin_top: Length::Px(0.0),
            scroll_margin_right: Length::Px(0.0),
            scroll_margin_bottom: Length::Px(0.0),
            scroll_margin_left: Length::Px(0.0),
            scroll_padding_top: Length::Auto,
            scroll_padding_right: Length::Auto,
            scroll_padding_bottom: Length::Auto,
            scroll_padding_left: Length::Auto,
            scroll_snap_type: "none".into(),
            scroll_snap_align: ScrollSnapAlign::None,
            overscroll_behavior_x: OverscrollBehavior::Auto,
            overscroll_behavior_y: OverscrollBehavior::Auto,
            // L5 step 4 batch 18 defaults.
            letter_spacing: LetterSpacing::Normal,
            word_spacing: LetterSpacing::Normal,
            accent_color: AccentColor::Auto,
            scrollbar_width: ScrollbarWidth::Auto,
            scrollbar_color: ScrollbarColor::Auto,
            image_rendering: ImageRendering::Auto,
            text_underline_offset: Length::Auto,
            line_height_step: Length::Px(0.0),
            speak: SpeakKind::Normal,
            mask_image: String::new(),
            shape_outside: ShapeOutsideVal::None,
            shape_margin: Length::Px(0.0),
            shape_image_threshold: 0.0,
            contain_intrinsic_block_size: Length::None,
            contain_intrinsic_inline_size: Length::None,
            contain_intrinsic_size: Length::None,
            orphans: 2,
            widows: 2,
            border_image_source: "none".into(),
            border_image_slice: "100%".into(),
            border_image_width: "1".into(),
            text_emphasis: String::new(),
            text_emphasis_color: Color::CurrentColor,
            // EXPERIMENTAL defaults
            anchor_name: String::new(),
            position_anchor: String::new(),
            inset_area: String::new(),
            view_transition_name: "none".into(),
            view_timeline_name: "none".into(),
            view_timeline_axis: "block".into(),
            scroll_timeline_name: "none".into(),
            scroll_timeline_axis: "block".into(),
            animation_timeline_l5: "auto".into(),
            content_visibility: ContentVisibility::Visible,
            container: String::new(),
            container_type: ContainerType::Normal,
            container_name: String::new(),
            text_wrap: TextWrap::Wrap,
            text_wrap_style: TextWrapStyle::Auto,
            text_wrap_mode: TextWrapMode::Wrap,
            text_box_trim: TextBoxTrim::None,
            text_box_edge: TextBoxEdge::Leading,
            field_sizing: FieldSizing::Fixed,
            print_color_adjust: PrintColorAdjust::Economy,
            forced_color_adjust: ForcedColorAdjust::Auto,
            color_scheme: ColorScheme::Normal,
            math_style: MathStyle::Normal,
            math_depth: "auto".into(),
            ruby_position: RubyPosition::Alternate,
            ruby_align: RubyAlign::SpaceAround,
            scroll_marker_group: "none".into(),
            // L5 step 4 Phase 3 raw shorthand storage.
            flex_shorthand: String::new(),
            columns_shorthand: String::new(),
            column_rule_shorthand: String::new(),
            scroll_padding_shorthand: String::new(),
            scroll_margin_shorthand: String::new(),
            gap_shorthand: String::new(),
            inset_shorthand: String::new(),
            text_decoration_shorthand: String::new(),
            overflow_shorthand: String::new(),
            border_width_shorthand: String::new(),
            border_style_shorthand: String::new(),
            border_color_shorthand: String::new(),
            background_shorthand: String::new(),
            contain_shorthand: String::new(),
            counter_reset_raw: String::new(),
            counter_increment_raw: String::new(),
            counter_set_raw: String::new(),
            object_position_raw: String::new(),
            grid_column_raw: String::new(),
            grid_row_raw: String::new(),
            background_clip_raw: String::new(),
            background_origin_raw: String::new(),
            background_attachment_raw: String::new(),
            border_image_outset_raw: String::new(),
            border_image_repeat_raw: String::new(),
            column_rule_width_raw: String::new(),
            column_rule_style_raw: String::new(),
            column_rule_color_raw: String::new(),
            explicit_set: PropertySet::new(),
        }
    }

    /// L5 step 4 Phase G: kontrola zda byla property explicitne nastavena.
    /// Bitset O(1) check.
    #[inline]
    pub fn is_set(&self, prop: PropertyId) -> bool {
        self.explicit_set.contains(prop)
    }

    /// L5 step 4 Phase G: oznac property jako explicitne nastavenou.
    #[inline]
    pub fn mark_set(&mut self, prop: PropertyId) {
        self.explicit_set.insert(prop);
    }

    /// L5 step 4 Phase D: Devtools adapter. Serializuj typed ComputedStyle do
    /// `Vec<(name, value)>` pro Devtools "Styles" / "Computed" panel display.
    /// Pouziva css_string() metody kde dostupne, raw String fields prepojuje
    /// primo. Nezahrnuje vsechna pole - jen ta s alphanumeric mapping
    /// (subset pro UX, neni round-trip-able zpet do CSS).
    pub fn to_devtools_entries(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::with_capacity(160);
        // ─── Color / Background ─
        out.push(("color".into(), color_css_string(self.color)));
        out.push(("background-color".into(), color_css_string(self.background_color)));
        // ─── Font ─
        out.push(("font-size".into(), length_css_string(&self.font_size)));
        out.push(("font-weight".into(), self.font_weight.to_string()));
        out.push(("font-style".into(), if self.font_style_italic { "italic".into() } else { "normal".into() }));
        out.push(("line-height".into(), match &self.line_height {
            LineHeight::Normal => "normal".into(),
            LineHeight::Multiplier(m) => m.to_string(),
            LineHeight::Length(l) => length_css_string(l),
        }));
        // ─── Box model ─
        out.push(("margin-top".into(), length_css_string(&self.margin_top)));
        out.push(("margin-right".into(), length_css_string(&self.margin_right)));
        out.push(("margin-bottom".into(), length_css_string(&self.margin_bottom)));
        out.push(("margin-left".into(), length_css_string(&self.margin_left)));
        out.push(("padding-top".into(), length_css_string(&self.padding_top)));
        out.push(("padding-right".into(), length_css_string(&self.padding_right)));
        out.push(("padding-bottom".into(), length_css_string(&self.padding_bottom)));
        out.push(("padding-left".into(), length_css_string(&self.padding_left)));
        out.push(("width".into(), length_css_string(&self.width)));
        out.push(("height".into(), length_css_string(&self.height)));
        out.push(("min-width".into(), length_css_string(&self.min_width)));
        out.push(("max-width".into(), length_css_string(&self.max_width)));
        out.push(("min-height".into(), length_css_string(&self.min_height)));
        out.push(("max-height".into(), length_css_string(&self.max_height)));
        out.push(("top".into(), length_css_string(&self.top)));
        out.push(("right".into(), length_css_string(&self.right)));
        out.push(("bottom".into(), length_css_string(&self.bottom)));
        out.push(("left".into(), length_css_string(&self.left)));
        // ─── Visual ─
        out.push(("opacity".into(), self.opacity.to_string()));
        out.push(("visibility".into(), self.visibility.css_string().into()));
        out.push(("display".into(), self.display.css_string().into()));
        out.push(("position".into(), self.position.css_string().into()));
        out.push(("z-index".into(), self.z_index.css_string()));
        out.push(("overflow-x".into(), self.overflow_x.css_string().into()));
        out.push(("overflow-y".into(), self.overflow_y.css_string().into()));
        out.push(("text-align".into(), self.text_align.css_string().into()));
        out.push(("white-space".into(), self.white_space.css_string().into()));
        out.push(("word-break".into(), self.word_break.css_string().into()));
        out.push(("overflow-wrap".into(), self.overflow_wrap.css_string().into()));
        out.push(("writing-mode".into(), self.writing_mode.css_string().into()));
        out.push(("direction".into(), self.direction.css_string().into()));
        out.push(("text-transform".into(), self.text_transform.css_string().into()));
        out.push(("cursor".into(), self.cursor.css_string()));
        // ─── EXPERIMENTAL (raw strings, marker prefix v devtools UI) ─
        if !self.anchor_name.is_empty() {
            out.push(("anchor-name".into(), self.anchor_name.clone()));
        }
        if !self.position_anchor.is_empty() {
            out.push(("position-anchor".into(), self.position_anchor.clone()));
        }
        if !self.inset_area.is_empty() {
            out.push(("inset-area".into(), self.inset_area.clone()));
        }
        if self.view_transition_name != "none" {
            out.push(("view-transition-name".into(), self.view_transition_name.clone()));
        }
        if !matches!(self.content_visibility, ContentVisibility::Visible) {
            out.push(("content-visibility".into(), self.content_visibility.css_string().to_string()));
        }
        if !matches!(self.text_wrap, TextWrap::Wrap) {
            out.push(("text-wrap".into(), self.text_wrap.css_string().to_string()));
        }
        if !matches!(self.field_sizing, FieldSizing::Fixed) {
            out.push(("field-sizing".into(), self.field_sizing.css_string().to_string()));
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }
}

/// Helper: Length -> CSS string pro devtools display + transitions roundtrip.
pub fn length_css_string(l: &Length) -> String {
    match l {
        Length::Px(v) => format!("{}px", v),
        Length::Em(v) => format!("{}em", v),
        Length::Rem(v) => format!("{}rem", v),
        Length::Vw(v) => format!("{}vw", v),
        Length::Vh(v) => format!("{}vh", v),
        Length::Vmin(v) => format!("{}vmin", v),
        Length::Vmax(v) => format!("{}vmax", v),
        Length::Percent(v) => format!("{}%", v),
        Length::Ch(v) => format!("{}ch", v),
        Length::Ex(v) => format!("{}ex", v),
        Length::Auto => "auto".into(),
        Length::None => "none".into(),
        Length::MinContent => "min-content".into(),
        Length::MaxContent => "max-content".into(),
        Length::FitContent(_) => "fit-content".into(),
        Length::Calc(_) => "calc(...)".into(),
    }
}

/// Helper: Color -> CSS string pro devtools + transitions roundtrip.
pub fn color_css_string(c: Color) -> String {
    let [r, g, b, a] = c.to_rgba_u8();
    if a == 255 {
        format!("rgb({}, {}, {})", r, g, b)
    } else {
        format!("rgba({}, {}, {}, {})", r, g, b, a as f32 / 255.0)
    }
}

// ─── L5 step 4 Phase I: nove typed enums pro PARTIAL implementaci experimental specs ───

/// CSS Text L4: `text-wrap`. balance/pretty/stable = advanced line-break
/// strategy (NOT YET implementovany - chovaji se jako wrap). nowrap/wrap
/// mapped 1:1 na white-space nowrap/normal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextWrap {
    Wrap,
    Nowrap,
    /// PARTIAL: zatim chovani jako Wrap; advanced balance algo neni implemented.
    Balance,
    /// PARTIAL: chovani jako Wrap.
    Pretty,
    /// PARTIAL: chovani jako Wrap.
    Stable,
}

impl TextWrap {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "nowrap" => Self::Nowrap,
            "balance" => Self::Balance,
            "pretty" => Self::Pretty,
            "stable" => Self::Stable,
            _ => Self::Wrap,
        }
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Wrap => "wrap",
            Self::Nowrap => "nowrap",
            Self::Balance => "balance",
            Self::Pretty => "pretty",
            Self::Stable => "stable",
        }
    }
    /// Pro mapping na white-space-style behavior. Nowrap = block break.
    /// Balance/Pretty/Stable = wrap (advanced not impl).
    pub fn is_nowrap(self) -> bool { matches!(self, Self::Nowrap) }
}

/// CSS Containment L3: `content-visibility`. hidden = no render + skip layout.
/// auto = visible kdyz blizko viewport (NOT implemented; treated as visible).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentVisibility {
    Visible,
    Hidden,
    /// PARTIAL: chovani jako Visible (viewport-relative optimization not impl).
    Auto,
}

impl ContentVisibility {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "hidden" => Self::Hidden,
            "auto" => Self::Auto,
            _ => Self::Visible,
        }
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Visible => "visible",
            Self::Hidden => "hidden",
            Self::Auto => "auto",
        }
    }
}

/// CSS Text L4: text-wrap-style. auto/balance/pretty/stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextWrapStyle {
    Auto,
    Balance,
    Pretty,
    Stable,
}
impl TextWrapStyle {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "balance" => Self::Balance,
            "pretty" => Self::Pretty,
            "stable" => Self::Stable,
            _ => Self::Auto,
        }
    }
    pub fn css_string(self) -> &'static str {
        match self { Self::Auto=>"auto", Self::Balance=>"balance", Self::Pretty=>"pretty", Self::Stable=>"stable" }
    }
}

/// CSS Text L4: text-wrap-mode. wrap | nowrap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextWrapMode {
    Wrap,
    Nowrap,
}
impl TextWrapMode {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "nowrap" => Self::Nowrap,
            _ => Self::Wrap,
        }
    }
    pub fn css_string(self) -> &'static str {
        match self { Self::Wrap=>"wrap", Self::Nowrap=>"nowrap" }
    }
}

/// CSS Inline L3: text-box-trim. line-box trimming directive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextBoxTrim {
    None,
    TrimStart,
    TrimEnd,
    TrimBoth,
}
impl TextBoxTrim {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "trim-start" => Self::TrimStart,
            "trim-end" => Self::TrimEnd,
            "trim-both" => Self::TrimBoth,
            _ => Self::None,
        }
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::None=>"none", Self::TrimStart=>"trim-start",
            Self::TrimEnd=>"trim-end", Self::TrimBoth=>"trim-both",
        }
    }
}

/// CSS Inline L3: text-box-edge. leading | text | cap | ex | ideographic | ideographic-ink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextBoxEdge {
    Leading,
    Text,
    Cap,
    Ex,
    Ideographic,
    IdeographicInk,
}
impl TextBoxEdge {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "text" => Self::Text,
            "cap" => Self::Cap,
            "ex" => Self::Ex,
            "ideographic" => Self::Ideographic,
            "ideographic-ink" => Self::IdeographicInk,
            _ => Self::Leading,
        }
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Leading=>"leading", Self::Text=>"text", Self::Cap=>"cap", Self::Ex=>"ex",
            Self::Ideographic=>"ideographic", Self::IdeographicInk=>"ideographic-ink",
        }
    }
}

/// CSS Containment L3: container-type. normal | inline-size | size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerType {
    Normal,
    InlineSize,
    Size,
}

impl ContainerType {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "inline-size" => Self::InlineSize,
            "size" => Self::Size,
            _ => Self::Normal,
        }
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::InlineSize => "inline-size",
            Self::Size => "size",
        }
    }
}

/// CSS Math L3: math-style. compact reduces vertical size of math elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathStyle {
    Normal,
    Compact,
}

impl MathStyle {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "compact" => Self::Compact,
            _ => Self::Normal,
        }
    }
    pub fn css_string(self) -> &'static str {
        match self { Self::Normal => "normal", Self::Compact => "compact" }
    }
}

/// CSS Ruby L1: ruby-position. Over (above) | Under | InterCharacter | Alternate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RubyPosition {
    Over,
    Under,
    InterCharacter,
    Alternate,
}

impl RubyPosition {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "over" => Self::Over,
            "under" => Self::Under,
            "inter-character" => Self::InterCharacter,
            _ => Self::Alternate,
        }
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Over => "over",
            Self::Under => "under",
            Self::InterCharacter => "inter-character",
            Self::Alternate => "alternate",
        }
    }
}

/// CSS Ruby L1: ruby-align. Start | Center | SpaceBetween | SpaceAround.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RubyAlign {
    Start,
    Center,
    SpaceBetween,
    SpaceAround,
}

impl RubyAlign {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "start" => Self::Start,
            "center" => Self::Center,
            "space-between" => Self::SpaceBetween,
            _ => Self::SpaceAround,
        }
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Center => "center",
            Self::SpaceBetween => "space-between",
            Self::SpaceAround => "space-around",
        }
    }
}

/// CSS Color L4: print-color-adjust. economy (UA strip backgrounds for print)
/// | exact (preserve all colors). Partial impl - browser print pipeline TBD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintColorAdjust {
    Economy,
    Exact,
}

impl PrintColorAdjust {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "exact" => Self::Exact,
            _ => Self::Economy,
        }
    }
    pub fn css_string(self) -> &'static str {
        match self { Self::Economy => "economy", Self::Exact => "exact" }
    }
}

/// CSS Color L4: forced-color-adjust. auto = honor system colors v high contrast;
/// none = ignore. Partial impl - high contrast mode detection TBD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForcedColorAdjust {
    Auto,
    None,
}

impl ForcedColorAdjust {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" => Self::None,
            _ => Self::Auto,
        }
    }
    pub fn css_string(self) -> &'static str {
        match self { Self::Auto => "auto", Self::None => "none" }
    }
}

/// CSS Color L4: color-scheme. light | dark | normal | "light dark" (both).
/// Partial impl - native form/scrollbar dark rendering TBD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorScheme {
    Normal,
    Light,
    Dark,
    /// `light dark` shorthand - browser picks based on user pref.
    LightDark,
}

impl ColorScheme {
    pub fn parse(s: &str) -> Self {
        let t = s.trim().to_ascii_lowercase();
        let parts: Vec<&str> = t.split_whitespace().collect();
        match parts.as_slice() {
            ["light"] => Self::Light,
            ["dark"] => Self::Dark,
            ["light", "dark"] | ["dark", "light"] => Self::LightDark,
            _ => Self::Normal,
        }
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Light => "light",
            Self::Dark => "dark",
            Self::LightDark => "light dark",
        }
    }
}

/// CSS Forms L1: `field-sizing`. content = auto-size to content. fixed = standard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldSizing {
    Fixed,
    Content,
}

impl FieldSizing {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "content" => Self::Content,
            _ => Self::Fixed,
        }
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::Content => "content",
        }
    }
}

// ─── L5 step 4 batch 18: nove typed enums + value types ───────────────────

/// CSS `letter-spacing` / `word-spacing` (CSS Text L4 §10).
/// `normal` -> 0 podle font metrics; jinak <length>.
#[derive(Debug, Clone, PartialEq)]
pub enum LetterSpacing {
    Normal,
    Length(Length),
}

impl LetterSpacing {
    pub fn parse(s: &str) -> Self {
        let t = s.trim();
        if t.eq_ignore_ascii_case("normal") { return Self::Normal; }
        Length::parse(t).map(Self::Length).unwrap_or(Self::Normal)
    }
}

/// CSS `accent-color` (CSS UI L4 §10). `auto` | <color>.
#[derive(Debug, Clone, PartialEq)]
pub enum AccentColor {
    Auto,
    Color(Color),
}

impl AccentColor {
    pub fn parse(s: &str) -> Self {
        let t = s.trim();
        if t.eq_ignore_ascii_case("auto") { return Self::Auto; }
        Color::parse(t).map(Self::Color).unwrap_or(Self::Auto)
    }
}

/// CSS `scrollbar-width` (CSS Scrollbars L1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollbarWidth {
    Auto,
    Thin,
    None,
}

impl ScrollbarWidth {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "thin" => Self::Thin,
            "none" => Self::None,
            _ => Self::Auto,
        }
    }
    pub fn css_string(self) -> &'static str {
        match self { Self::Auto => "auto", Self::Thin => "thin", Self::None => "none" }
    }
}

/// CSS `scrollbar-color` (CSS Scrollbars L1). `auto` | <color> <color>.
#[derive(Debug, Clone, PartialEq)]
pub enum ScrollbarColor {
    Auto,
    Pair { thumb: Color, track: Color },
}

impl ScrollbarColor {
    pub fn parse(s: &str) -> Self {
        let t = s.trim();
        if t.eq_ignore_ascii_case("auto") { return Self::Auto; }
        let parts: Vec<&str> = t.split_whitespace().collect();
        if parts.len() >= 2 {
            if let (Some(thumb), Some(track)) = (Color::parse(parts[0]), Color::parse(parts[1])) {
                return Self::Pair { thumb, track };
            }
        }
        Self::Auto
    }
}

/// CSS `image-rendering` (CSS Images L3 §5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageRendering {
    Auto,
    Smooth,
    HighQuality,
    CrispEdges,
    Pixelated,
}

impl ImageRendering {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "smooth" => Self::Smooth,
            "high-quality" => Self::HighQuality,
            "crisp-edges" => Self::CrispEdges,
            "pixelated" => Self::Pixelated,
            _ => Self::Auto,
        }
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Smooth => "smooth",
            Self::HighQuality => "high-quality",
            Self::CrispEdges => "crisp-edges",
            Self::Pixelated => "pixelated",
        }
    }
}

/// CSS `speak` (CSS Speech L1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeakKind {
    Normal,
    None,
    Always,
}

impl SpeakKind {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" => Self::None,
            "always" => Self::Always,
            _ => Self::Normal,
        }
    }
    pub fn css_string(self) -> &'static str {
        match self { Self::Normal => "normal", Self::None => "none", Self::Always => "always" }
    }
}

/// CSS `shape-outside` (CSS Shapes L1 §3). Float wrapping shape.
#[derive(Debug, Clone, PartialEq)]
pub enum ShapeOutsideVal {
    None,
    /// Raw shape ftring (basic-shape, image, box) - parser layout dela.
    Raw(String),
}

impl ShapeOutsideVal {
    pub fn parse(s: &str) -> Self {
        let t = s.trim();
        if t.eq_ignore_ascii_case("none") || t.is_empty() { return Self::None; }
        Self::Raw(t.to_string())
    }
}


/// CSS `scroll-snap-align` (CSS Scroll Snap L1 §6.2). `<block> <inline>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollSnapAlign {
    None,
    Start,
    End,
    Center,
}

impl ScrollSnapAlign {
    pub fn parse(s: &str) -> Option<Self> {
        // Pri 2-value (block+inline) bere prvni token.
        Some(match s.split_whitespace().next().unwrap_or("").to_lowercase().as_str() {
            "none" => Self::None,
            "start" => Self::Start,
            "end" => Self::End,
            "center" => Self::Center,
            _ => return None,
        })
    }
}

/// CSS `overscroll-behavior` (CSS Overscroll L1 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverscrollBehavior {
    Auto,
    Contain,
    None,
}

impl OverscrollBehavior {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "auto" => Self::Auto,
            "contain" => Self::Contain,
            "none" => Self::None,
            _ => return None,
        })
    }
}

/// CSS `column-count` (CSS Multi-column L1 §3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnCount {
    Auto,
    Integer(u32),
}

impl ColumnCount {
    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim().to_lowercase();
        if t == "auto" { return Some(Self::Auto); }
        t.parse::<u32>().ok().map(|n| Self::Integer(n.max(1)))
    }
}

/// CSS `column-fill` (CSS Multi-column L1 §5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnFill {
    Auto,
    Balance,
    BalanceAll,
}

impl ColumnFill {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "auto" => Self::Auto,
            "balance" => Self::Balance,
            "balance-all" => Self::BalanceAll,
            _ => return None,
        })
    }
}

/// CSS `column-span` (CSS Multi-column L1 §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnSpan {
    None,
    All,
}

impl ColumnSpan {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "none" => Self::None,
            "all" => Self::All,
            _ => return None,
        })
    }
}

/// CSS `appearance` (CSS UI L4 §3.3). Subset: Auto/None/Custom.
#[derive(Debug, Clone, PartialEq)]
pub enum Appearance {
    Auto,
    None,
    Button,
    Textfield,
    MenuList,
    Checkbox,
    Radio,
    SearchField,
    PushButton,
    SquareButton,
    Listbox,
    Custom(String),
}

impl Appearance {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "auto" => Self::Auto,
            "none" => Self::None,
            "button" => Self::Button,
            "textfield" => Self::Textfield,
            "menulist" => Self::MenuList,
            "checkbox" => Self::Checkbox,
            "radio" => Self::Radio,
            "searchfield" => Self::SearchField,
            "push-button" => Self::PushButton,
            "square-button" => Self::SquareButton,
            "listbox" => Self::Listbox,
            other => Self::Custom(other.to_string()),
        }
    }
}

/// CSS `background-attachment` (CSS Backgrounds L3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundAttachment {
    Scroll,
    Fixed,
    Local,
}

impl BackgroundAttachment {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "scroll" => Self::Scroll,
            "fixed" => Self::Fixed,
            "local" => Self::Local,
            _ => return None,
        })
    }
}

/// CSS `background-clip` + `background-origin` (CSS Backgrounds L3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundClip {
    BorderBox,
    PaddingBox,
    ContentBox,
    Text,
}

impl BackgroundClip {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "border-box" => Self::BorderBox,
            "padding-box" => Self::PaddingBox,
            "content-box" => Self::ContentBox,
            "text" => Self::Text,
            _ => return None,
        })
    }
}

/// CSS `scroll-behavior` (CSS OM View L4 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollBehavior {
    Auto,
    Smooth,
}

impl ScrollBehavior {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "auto" => Self::Auto,
            "smooth" => Self::Smooth,
            _ => return None,
        })
    }
}

/// CSS `justify-items` (CSS Box Alignment L3 §6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JustifyItems {
    Normal,
    Stretch,
    Start,
    End,
    Center,
    FlexStart,
    FlexEnd,
    Left,
    Right,
    Baseline,
    SelfStart,
    SelfEnd,
    Legacy,
}

impl JustifyItems {
    pub fn parse(s: &str) -> Option<Self> {
        // "legacy left" / "legacy right" / "legacy center" -> Legacy.
        let t = s.trim().to_lowercase();
        if t.starts_with("legacy") { return Some(Self::Legacy); }
        Some(match t.as_str() {
            "normal" => Self::Normal,
            "stretch" => Self::Stretch,
            "start" => Self::Start,
            "end" => Self::End,
            "center" => Self::Center,
            "flex-start" => Self::FlexStart,
            "flex-end" => Self::FlexEnd,
            "left" => Self::Left,
            "right" => Self::Right,
            "baseline" => Self::Baseline,
            "self-start" => Self::SelfStart,
            "self-end" => Self::SelfEnd,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Stretch => "stretch",
            Self::Start => "start",
            Self::End => "end",
            Self::Center => "center",
            Self::FlexStart => "flex-start",
            Self::FlexEnd => "flex-end",
            Self::Left => "left",
            Self::Right => "right",
            Self::Baseline => "baseline",
            Self::SelfStart => "self-start",
            Self::SelfEnd => "self-end",
            Self::Legacy => "legacy",
        }
    }
}

/// CSS `justify-self` (CSS Box Alignment L3 §6.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JustifySelf {
    Auto,
    Normal,
    Stretch,
    Start,
    End,
    Center,
    FlexStart,
    FlexEnd,
    Left,
    Right,
    Baseline,
    SelfStart,
    SelfEnd,
}

impl JustifySelf {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "auto" => Self::Auto,
            "normal" => Self::Normal,
            "stretch" => Self::Stretch,
            "start" => Self::Start,
            "end" => Self::End,
            "center" => Self::Center,
            "flex-start" => Self::FlexStart,
            "flex-end" => Self::FlexEnd,
            "left" => Self::Left,
            "right" => Self::Right,
            "baseline" => Self::Baseline,
            "self-start" => Self::SelfStart,
            "self-end" => Self::SelfEnd,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Normal => "normal",
            Self::Stretch => "stretch",
            Self::Start => "start",
            Self::End => "end",
            Self::Center => "center",
            Self::FlexStart => "flex-start",
            Self::FlexEnd => "flex-end",
            Self::Left => "left",
            Self::Right => "right",
            Self::Baseline => "baseline",
            Self::SelfStart => "self-start",
            Self::SelfEnd => "self-end",
        }
    }
}

/// CSS grid-line value (CSS Grid L1 §8.3). Auto | <int> | span <int> | <ident>.
#[derive(Debug, Clone, PartialEq)]
pub enum GridLine {
    Auto,
    Line(i32),                  // integer index (1-based, neg = from-end)
    Span(i32),                  // span N tracks
    Named(String),              // named line
    SpanNamed(String),          // span ident
}

impl GridLine {
    pub fn parse(s: &str) -> Self {
        let t = s.trim();
        if t.eq_ignore_ascii_case("auto") || t.is_empty() { return Self::Auto; }
        if let Some(rest) = t.strip_prefix("span ").or_else(|| t.strip_prefix("span\t")) {
            let r = rest.trim();
            if let Ok(n) = r.parse::<i32>() { return Self::Span(n.max(1)); }
            return Self::SpanNamed(r.to_string());
        }
        if let Ok(n) = t.parse::<i32>() { return Self::Line(n); }
        Self::Named(t.to_string())
    }
}

/// CSS `grid-auto-flow` (CSS Grid L1 §6.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridAutoFlow {
    Row,
    Column,
    RowDense,
    ColumnDense,
}

impl GridAutoFlow {
    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim().to_lowercase();
        let parts: Vec<&str> = t.split_whitespace().collect();
        let has_dense = parts.iter().any(|p| *p == "dense");
        let is_col = parts.iter().any(|p| *p == "column");
        let is_row = parts.iter().any(|p| *p == "row");
        if !has_dense && !is_col && !is_row { return None; }
        Some(match (is_col, has_dense) {
            (true, true) => Self::ColumnDense,
            (true, false) => Self::Column,
            (false, true) => Self::RowDense,
            (false, false) => Self::Row,
        })
    }
}

/// CSS `mix-blend-mode` (CSS Compositing L1 §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl BlendMode {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "normal" => Self::Normal,
            "multiply" => Self::Multiply,
            "screen" => Self::Screen,
            "overlay" => Self::Overlay,
            "darken" => Self::Darken,
            "lighten" => Self::Lighten,
            "color-dodge" => Self::ColorDodge,
            "color-burn" => Self::ColorBurn,
            "hard-light" => Self::HardLight,
            "soft-light" => Self::SoftLight,
            "difference" => Self::Difference,
            "exclusion" => Self::Exclusion,
            "hue" => Self::Hue,
            "saturation" => Self::Saturation,
            "color" => Self::Color,
            "luminosity" => Self::Luminosity,
            _ => return None,
        })
    }
}

/// CSS `isolation` (CSS Compositing L1 §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Isolation {
    Auto,
    Isolate,
}

impl Isolation {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "auto" => Self::Auto,
            "isolate" => Self::Isolate,
            _ => return None,
        })
    }
}

/// CSS `animation-direction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationDirection {
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

impl AnimationDirection {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "normal" => Self::Normal,
            "reverse" => Self::Reverse,
            "alternate" => Self::Alternate,
            "alternate-reverse" => Self::AlternateReverse,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Reverse => "reverse",
            Self::Alternate => "alternate",
            Self::AlternateReverse => "alternate-reverse",
        }
    }
}

/// CSS `animation-fill-mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationFillMode {
    None,
    Forwards,
    Backwards,
    Both,
}

impl AnimationFillMode {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "none" => Self::None,
            "forwards" => Self::Forwards,
            "backwards" => Self::Backwards,
            "both" => Self::Both,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Forwards => "forwards",
            Self::Backwards => "backwards",
            Self::Both => "both",
        }
    }
}

/// CSS `animation-play-state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationPlayState {
    Running,
    Paused,
}

impl AnimationPlayState {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "running" => Self::Running,
            "paused" => Self::Paused,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Paused => "paused",
        }
    }
}

/// CSS `<easing-function>` (CSS Easing L1).
#[derive(Debug, Clone, PartialEq)]
pub enum TimingFunction {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    StepStart,
    StepEnd,
    Steps(u32, StepPosition),
    CubicBezier(f32, f32, f32, f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepPosition {
    Start,
    End,
    JumpStart,
    JumpEnd,
    JumpNone,
    JumpBoth,
}

impl TimingFunction {
    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim().to_lowercase();
        match t.as_str() {
            "linear" => return Some(Self::Linear),
            "ease" => return Some(Self::Ease),
            "ease-in" => return Some(Self::EaseIn),
            "ease-out" => return Some(Self::EaseOut),
            "ease-in-out" => return Some(Self::EaseInOut),
            "step-start" => return Some(Self::StepStart),
            "step-end" => return Some(Self::StepEnd),
            _ => {}
        }
        if let Some(inner) = t.strip_prefix("cubic-bezier(").and_then(|x| x.strip_suffix(')')) {
            let parts: Vec<f32> = inner.split(',').filter_map(|p| p.trim().parse().ok()).collect();
            if parts.len() == 4 {
                return Some(Self::CubicBezier(parts[0], parts[1], parts[2], parts[3]));
            }
        }
        if let Some(inner) = t.strip_prefix("steps(").and_then(|x| x.strip_suffix(')')) {
            let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
            if let Ok(n) = parts.first().map_or("", |v| v).parse::<u32>() {
                let pos = match parts.get(1).copied().unwrap_or("end") {
                    "start" | "jump-start" => StepPosition::JumpStart,
                    "end" | "jump-end" => StepPosition::JumpEnd,
                    "jump-none" => StepPosition::JumpNone,
                    "jump-both" => StepPosition::JumpBoth,
                    _ => StepPosition::JumpEnd,
                };
                return Some(Self::Steps(n, pos));
            }
        }
        None
    }
    pub fn css_string(&self) -> String {
        match self {
            Self::Linear => "linear".into(),
            Self::Ease => "ease".into(),
            Self::EaseIn => "ease-in".into(),
            Self::EaseOut => "ease-out".into(),
            Self::EaseInOut => "ease-in-out".into(),
            Self::StepStart => "step-start".into(),
            Self::StepEnd => "step-end".into(),
            Self::Steps(n, pos) => {
                let p = match pos {
                    StepPosition::Start | StepPosition::JumpStart => "jump-start",
                    StepPosition::End | StepPosition::JumpEnd => "jump-end",
                    StepPosition::JumpNone => "jump-none",
                    StepPosition::JumpBoth => "jump-both",
                };
                format!("steps({}, {})", n, p)
            }
            Self::CubicBezier(a, b, c, d) => format!("cubic-bezier({},{},{},{})", a, b, c, d),
        }
    }
}

/// Parse `<time>` value (s/ms). 1.5s, 200ms.
fn parse_time(s: &str) -> Option<f32> {
    let t = s.trim().to_lowercase();
    if let Some(num) = t.strip_suffix("ms") {
        return num.trim().parse::<f32>().ok().map(|n| n / 1000.0);
    }
    if let Some(num) = t.strip_suffix('s') {
        return num.trim().parse::<f32>().ok();
    }
    None
}

/// Parse comma-separated time list. "1s, 2s, 200ms" -> [1.0, 2.0, 0.2].
pub fn parse_time_list(s: &str) -> Vec<f32> {
    s.split(',').filter_map(|p| parse_time(p)).collect()
}

/// Parse comma-separated timing-function list.
pub fn parse_timing_function_list(s: &str) -> Vec<TimingFunction> {
    // Simple split by top-level comma (cubic-bezier obsahuje vnitrni carky).
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b',' if depth == 0 => {
                if let Some(t) = TimingFunction::parse(&s[start..i]) { out.push(t); }
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < s.len() {
        if let Some(t) = TimingFunction::parse(&s[start..]) { out.push(t); }
    }
    out
}

/// CSS `object-fit` (CSS Images L3 §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectFit {
    Fill,
    Contain,
    Cover,
    None,
    ScaleDown,
}

impl ObjectFit {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "fill" => Self::Fill,
            "contain" => Self::Contain,
            "cover" => Self::Cover,
            "none" => Self::None,
            "scale-down" => Self::ScaleDown,
            _ => return None,
        })
    }
}

/// CSS `resize` (CSS UI L4 §6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resize {
    None,
    Both,
    Horizontal,
    Vertical,
    Block,
    Inline,
}

impl Resize {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "none" => Self::None,
            "both" => Self::Both,
            "horizontal" => Self::Horizontal,
            "vertical" => Self::Vertical,
            "block" => Self::Block,
            "inline" => Self::Inline,
            _ => return None,
        })
    }
}

/// CSS `border-collapse` (CSS Tables L3 §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderCollapse {
    Separate,
    Collapse,
}

impl BorderCollapse {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "separate" => Self::Separate,
            "collapse" => Self::Collapse,
            _ => return None,
        })
    }
}

/// CSS `table-layout` (CSS Tables L3 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableLayout {
    Auto,
    Fixed,
}

impl TableLayout {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "auto" => Self::Auto,
            "fixed" => Self::Fixed,
            _ => return None,
        })
    }
}

/// CSS `caption-side` (CSS Tables L3 §10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptionSide {
    Top,
    Bottom,
}

impl CaptionSide {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "top" => Self::Top,
            "bottom" => Self::Bottom,
            _ => return None,
        })
    }
}

/// CSS `list-style-type` (CSS Lists L3). Subset; rare types -> Custom.
#[derive(Debug, Clone, PartialEq)]
pub enum ListStyleType {
    None,
    Disc,
    Circle,
    Square,
    Decimal,
    DecimalLeadingZero,
    LowerAlpha,
    UpperAlpha,
    LowerRoman,
    UpperRoman,
    LowerGreek,
    LowerLatin,
    UpperLatin,
    Armenian,
    Georgian,
    Custom(String),
}

impl ListStyleType {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "none" => Self::None,
            "disc" => Self::Disc,
            "circle" => Self::Circle,
            "square" => Self::Square,
            "decimal" => Self::Decimal,
            "decimal-leading-zero" => Self::DecimalLeadingZero,
            "lower-alpha" => Self::LowerAlpha,
            "upper-alpha" => Self::UpperAlpha,
            "lower-roman" => Self::LowerRoman,
            "upper-roman" => Self::UpperRoman,
            "lower-greek" => Self::LowerGreek,
            "lower-latin" => Self::LowerLatin,
            "upper-latin" => Self::UpperLatin,
            "armenian" => Self::Armenian,
            "georgian" => Self::Georgian,
            other => Self::Custom(other.to_string()),
        }
    }
}

/// CSS `list-style-position` (CSS Lists L3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListStylePosition {
    Inside,
    Outside,
}

impl ListStylePosition {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "inside" => Self::Inside,
            "outside" => Self::Outside,
            _ => return None,
        })
    }
}

/// CSS `list-style-image` (CSS Lists L3). None | url().
#[derive(Debug, Clone, PartialEq)]
pub enum ListStyleImage {
    None,
    Url(String),
}

impl ListStyleImage {
    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim();
        if t.eq_ignore_ascii_case("none") { return Some(Self::None); }
        // url(...) extrakt.
        if let Some(inner) = t.strip_prefix("url(").and_then(|x| x.strip_suffix(')')) {
            let url = inner.trim().trim_matches('"').trim_matches('\'').to_string();
            return Some(Self::Url(url));
        }
        None
    }
}

/// CSS `text-transform` (CSS Text L3 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextTransform {
    None,
    Capitalize,
    Uppercase,
    Lowercase,
    FullWidth,
    FullSizeKana,
}

impl TextTransform {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "none" => Self::None,
            "capitalize" => Self::Capitalize,
            "uppercase" => Self::Uppercase,
            "lowercase" => Self::Lowercase,
            "full-width" => Self::FullWidth,
            "full-size-kana" => Self::FullSizeKana,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Capitalize => "capitalize",
            Self::Uppercase => "uppercase",
            Self::Lowercase => "lowercase",
            Self::FullWidth => "full-width",
            Self::FullSizeKana => "full-size-kana",
        }
    }
}

/// CSS `text-overflow` (CSS UI L4 §6.2). Subset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextOverflow {
    Clip,
    Ellipsis,
    Custom(String),
}

impl TextOverflow {
    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim();
        Some(match t.to_lowercase().as_str() {
            "clip" => Self::Clip,
            "ellipsis" => Self::Ellipsis,
            _ => {
                // String literal '...' nebo unquoted treat jako Custom.
                if t.is_empty() { return None; }
                Self::Custom(t.to_string())
            }
        })
    }
    pub fn css_string(&self) -> String {
        match self {
            Self::Clip => "clip".into(),
            Self::Ellipsis => "ellipsis".into(),
            Self::Custom(s) => s.clone(),
        }
    }
}

/// CSS `vertical-align` (CSS Inline L3 §3.2).
#[derive(Debug, Clone, PartialEq)]
pub enum VerticalAlign {
    Baseline,
    Sub,
    Super,
    Top,
    TextTop,
    Middle,
    Bottom,
    TextBottom,
    Length(Length),
}

impl VerticalAlign {
    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim();
        match t.to_lowercase().as_str() {
            "baseline" => Some(Self::Baseline),
            "sub" => Some(Self::Sub),
            "super" => Some(Self::Super),
            "top" => Some(Self::Top),
            "text-top" => Some(Self::TextTop),
            "middle" => Some(Self::Middle),
            "bottom" => Some(Self::Bottom),
            "text-bottom" => Some(Self::TextBottom),
            _ => Length::parse(t).map(Self::Length),
        }
    }
    pub fn css_string(&self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Sub => "sub",
            Self::Super => "super",
            Self::Top => "top",
            Self::TextTop => "text-top",
            Self::Middle => "middle",
            Self::Bottom => "bottom",
            Self::TextBottom => "text-bottom",
            Self::Length(_) => "<length>",
        }
    }
}

/// CSS `text-decoration-line` bitflag (CSS Text Decoration L3 §2.2).
/// `none` = bitflags::empty(). Combinations: underline + line-through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextDecorationLine(pub u8);

impl TextDecorationLine {
    pub const NONE: Self = TextDecorationLine(0);
    pub const UNDERLINE: Self = TextDecorationLine(1);
    pub const OVERLINE: Self = TextDecorationLine(2);
    pub const LINE_THROUGH: Self = TextDecorationLine(4);
    pub const BLINK: Self = TextDecorationLine(8);

    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim().to_lowercase();
        if t == "none" { return Some(Self::NONE); }
        let mut bits = 0u8;
        for tok in t.split_whitespace() {
            match tok {
                "underline" => bits |= 1,
                "overline" => bits |= 2,
                "line-through" => bits |= 4,
                "blink" => bits |= 8,
                _ => return None,
            }
        }
        Some(TextDecorationLine(bits))
    }
    pub fn has_underline(self) -> bool { (self.0 & 1) != 0 }
    pub fn has_overline(self) -> bool { (self.0 & 2) != 0 }
    pub fn has_line_through(self) -> bool { (self.0 & 4) != 0 }
    pub fn css_string(self) -> String {
        if self.0 == 0 { return "none".into(); }
        let mut parts = Vec::new();
        if self.has_underline() { parts.push("underline"); }
        if self.has_overline() { parts.push("overline"); }
        if self.has_line_through() { parts.push("line-through"); }
        if (self.0 & 8) != 0 { parts.push("blink"); }
        parts.join(" ")
    }
}

/// CSS `text-decoration-style` (CSS Text Decoration L3 §2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDecorationStyle {
    Solid,
    Double,
    Dotted,
    Dashed,
    Wavy,
}

impl TextDecorationStyle {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "solid" => Self::Solid,
            "double" => Self::Double,
            "dotted" => Self::Dotted,
            "dashed" => Self::Dashed,
            "wavy" => Self::Wavy,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Solid => "solid",
            Self::Double => "double",
            Self::Dotted => "dotted",
            Self::Dashed => "dashed",
            Self::Wavy => "wavy",
        }
    }
}

/// CSS `border-*-style` (CSS Backgrounds L3 §3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderStyle {
    None,
    Hidden,
    Dotted,
    Dashed,
    Solid,
    Double,
    Groove,
    Ridge,
    Inset,
    Outset,
}

impl BorderStyle {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "none" => Self::None,
            "hidden" => Self::Hidden,
            "dotted" => Self::Dotted,
            "dashed" => Self::Dashed,
            "solid" => Self::Solid,
            "double" => Self::Double,
            "groove" => Self::Groove,
            "ridge" => Self::Ridge,
            "inset" => Self::Inset,
            "outset" => Self::Outset,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Hidden => "hidden",
            Self::Dotted => "dotted",
            Self::Dashed => "dashed",
            Self::Solid => "solid",
            Self::Double => "double",
            Self::Groove => "groove",
            Self::Ridge => "ridge",
            Self::Inset => "inset",
            Self::Outset => "outset",
        }
    }
}

/// CSS `flex-basis` (CSS Flexbox L1 §7.2). Auto | Content | <length>.
#[derive(Debug, Clone, PartialEq)]
pub enum FlexBasis {
    Auto,
    Content,
    Length(Length),
}

impl FlexBasis {
    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim();
        if t.eq_ignore_ascii_case("auto") { return Some(Self::Auto); }
        if t.eq_ignore_ascii_case("content") { return Some(Self::Content); }
        Length::parse(t).map(Self::Length)
    }
    pub fn css_string(&self) -> String {
        match self {
            Self::Auto => "auto".into(),
            Self::Content => "content".into(),
            Self::Length(l) => length_css_string(l),
        }
    }
}

/// CSS `justify-content` (CSS Box Alignment L3 §6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JustifyContent {
    Normal,
    FlexStart,
    FlexEnd,
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Stretch,
    Left,
    Right,
}

impl JustifyContent {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "normal" => Self::Normal,
            "flex-start" => Self::FlexStart,
            "flex-end" => Self::FlexEnd,
            "start" => Self::Start,
            "end" => Self::End,
            "center" => Self::Center,
            "space-between" => Self::SpaceBetween,
            "space-around" => Self::SpaceAround,
            "space-evenly" => Self::SpaceEvenly,
            "stretch" => Self::Stretch,
            "left" => Self::Left,
            "right" => Self::Right,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::FlexStart => "flex-start",
            Self::FlexEnd => "flex-end",
            Self::Start => "start",
            Self::End => "end",
            Self::Center => "center",
            Self::SpaceBetween => "space-between",
            Self::SpaceAround => "space-around",
            Self::SpaceEvenly => "space-evenly",
            Self::Stretch => "stretch",
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

/// CSS `align-items` (CSS Box Alignment L3 §5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignItems {
    Normal,
    Stretch,
    FlexStart,
    FlexEnd,
    Start,
    End,
    Center,
    Baseline,
    SelfStart,
    SelfEnd,
}

impl AlignItems {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "normal" => Self::Normal,
            "stretch" => Self::Stretch,
            "flex-start" => Self::FlexStart,
            "flex-end" => Self::FlexEnd,
            "start" => Self::Start,
            "end" => Self::End,
            "center" => Self::Center,
            "baseline" => Self::Baseline,
            "self-start" => Self::SelfStart,
            "self-end" => Self::SelfEnd,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Stretch => "stretch",
            Self::FlexStart => "flex-start",
            Self::FlexEnd => "flex-end",
            Self::Start => "start",
            Self::End => "end",
            Self::Center => "center",
            Self::Baseline => "baseline",
            Self::SelfStart => "self-start",
            Self::SelfEnd => "self-end",
        }
    }
}

/// CSS `align-content` (CSS Box Alignment L3 §5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignContent {
    Normal,
    FlexStart,
    FlexEnd,
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Stretch,
    Baseline,
}

impl AlignContent {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "normal" => Self::Normal,
            "flex-start" => Self::FlexStart,
            "flex-end" => Self::FlexEnd,
            "start" => Self::Start,
            "end" => Self::End,
            "center" => Self::Center,
            "space-between" => Self::SpaceBetween,
            "space-around" => Self::SpaceAround,
            "space-evenly" => Self::SpaceEvenly,
            "stretch" => Self::Stretch,
            "baseline" => Self::Baseline,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::FlexStart => "flex-start",
            Self::FlexEnd => "flex-end",
            Self::Start => "start",
            Self::End => "end",
            Self::Center => "center",
            Self::SpaceBetween => "space-between",
            Self::SpaceAround => "space-around",
            Self::SpaceEvenly => "space-evenly",
            Self::Stretch => "stretch",
            Self::Baseline => "baseline",
        }
    }
}

/// CSS `align-self` (CSS Box Alignment L3 §5.4). Auto = follow align-items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignSelf {
    Auto,
    Normal,
    Stretch,
    FlexStart,
    FlexEnd,
    Start,
    End,
    Center,
    Baseline,
    SelfStart,
    SelfEnd,
}

impl AlignSelf {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "auto" => Self::Auto,
            "normal" => Self::Normal,
            "stretch" => Self::Stretch,
            "flex-start" => Self::FlexStart,
            "flex-end" => Self::FlexEnd,
            "start" => Self::Start,
            "end" => Self::End,
            "center" => Self::Center,
            "baseline" => Self::Baseline,
            "self-start" => Self::SelfStart,
            "self-end" => Self::SelfEnd,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Normal => "normal",
            Self::Stretch => "stretch",
            Self::FlexStart => "flex-start",
            Self::FlexEnd => "flex-end",
            Self::Start => "start",
            Self::End => "end",
            Self::Center => "center",
            Self::Baseline => "baseline",
            Self::SelfStart => "self-start",
            Self::SelfEnd => "self-end",
        }
    }
}

/// CSS `flex-direction` (CSS Flexbox L1 §5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

impl FlexDirection {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "row" => Self::Row,
            "row-reverse" => Self::RowReverse,
            "column" => Self::Column,
            "column-reverse" => Self::ColumnReverse,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Row => "row",
            Self::RowReverse => "row-reverse",
            Self::Column => "column",
            Self::ColumnReverse => "column-reverse",
        }
    }
    pub fn is_row(self) -> bool {
        matches!(self, Self::Row | Self::RowReverse)
    }
}

/// CSS `flex-wrap` (CSS Flexbox L1 §6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexWrap {
    Nowrap,
    Wrap,
    WrapReverse,
}

impl FlexWrap {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "nowrap" => Self::Nowrap,
            "wrap" => Self::Wrap,
            "wrap-reverse" => Self::WrapReverse,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Nowrap => "nowrap",
            Self::Wrap => "wrap",
            Self::WrapReverse => "wrap-reverse",
        }
    }
}

/// CSS `overflow` (CSS Overflow L3 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    Visible,
    Hidden,
    Scroll,
    Auto,
    Clip,
}

impl Overflow {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "visible" => Self::Visible,
            "hidden" => Self::Hidden,
            "scroll" => Self::Scroll,
            "auto" => Self::Auto,
            "clip" => Self::Clip,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Visible => "visible",
            Self::Hidden => "hidden",
            Self::Scroll => "scroll",
            Self::Auto => "auto",
            Self::Clip => "clip",
        }
    }
    /// Pri Auto/Scroll/Hidden je box scrollable nebo clipped.
    pub fn is_scrollable(self) -> bool {
        matches!(self, Self::Auto | Self::Scroll)
    }
}

/// CSS `float` (CSS Floats L1 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Float {
    None,
    Left,
    Right,
    InlineStart,
    InlineEnd,
}

impl Float {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "none" => Self::None,
            "left" => Self::Left,
            "right" => Self::Right,
            "inline-start" => Self::InlineStart,
            "inline-end" => Self::InlineEnd,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Left => "left",
            Self::Right => "right",
            Self::InlineStart => "inline-start",
            Self::InlineEnd => "inline-end",
        }
    }
}

/// CSS `clear` (CSS Floats L1 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clear {
    None,
    Left,
    Right,
    Both,
    InlineStart,
    InlineEnd,
}

impl Clear {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "none" => Self::None,
            "left" => Self::Left,
            "right" => Self::Right,
            "both" => Self::Both,
            "inline-start" => Self::InlineStart,
            "inline-end" => Self::InlineEnd,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Left => "left",
            Self::Right => "right",
            Self::Both => "both",
            Self::InlineStart => "inline-start",
            Self::InlineEnd => "inline-end",
        }
    }
}

/// CSS `writing-mode` (CSS Writing Modes L3 §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WritingMode {
    HorizontalTb,
    VerticalRl,
    VerticalLr,
    SidewaysRl,
    SidewaysLr,
}

impl WritingMode {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "horizontal-tb" => Self::HorizontalTb,
            "vertical-rl" => Self::VerticalRl,
            "vertical-lr" => Self::VerticalLr,
            "sideways-rl" => Self::SidewaysRl,
            "sideways-lr" => Self::SidewaysLr,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::HorizontalTb => "horizontal-tb",
            Self::VerticalRl => "vertical-rl",
            Self::VerticalLr => "vertical-lr",
            Self::SidewaysRl => "sideways-rl",
            Self::SidewaysLr => "sideways-lr",
        }
    }
}

/// CSS `direction` (CSS Writing Modes L3 §2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Ltr,
    Rtl,
}

impl Direction {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "ltr" => Self::Ltr,
            "rtl" => Self::Rtl,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Ltr => "ltr",
            Self::Rtl => "rtl",
        }
    }
}

/// CSS `box-sizing` (CSS UI L4 §6.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxSizing {
    ContentBox,
    BorderBox,
}

impl BoxSizing {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "content-box" => Self::ContentBox,
            "border-box" => Self::BorderBox,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::ContentBox => "content-box",
            Self::BorderBox => "border-box",
        }
    }
}

/// CSS `pointer-events` (CSS UI L4 §3.1). Subset - mostly Auto vs None.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerEvents {
    Auto,
    None,
}

impl PointerEvents {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "auto" => Self::Auto,
            "none" => Self::None,
            // SVG values (visiblePainted, visibleFill, etc.) treat as Auto
            // pro non-SVG context. Vsechny ostatni neznamy -> None match
            // failure.
            "visible" | "visiblepainted" | "visiblefill" | "visiblestroke"
                | "painted" | "fill" | "stroke" | "all" => Self::Auto,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::None => "none",
        }
    }
}

/// CSS `text-align` (CSS Text L4 §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Start,
    End,
    Left,
    Right,
    Center,
    Justify,
    MatchParent,
}

impl TextAlign {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "start" => Self::Start,
            "end" => Self::End,
            "left" => Self::Left,
            "right" => Self::Right,
            "center" => Self::Center,
            "justify" => Self::Justify,
            "match-parent" => Self::MatchParent,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::End => "end",
            Self::Left => "left",
            Self::Right => "right",
            Self::Center => "center",
            Self::Justify => "justify",
            Self::MatchParent => "match-parent",
        }
    }
}

/// CSS `white-space` (CSS Text L4 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhiteSpace {
    Normal,
    Nowrap,
    Pre,
    PreWrap,
    PreLine,
    BreakSpaces,
}

impl WhiteSpace {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "normal" => Self::Normal,
            "nowrap" => Self::Nowrap,
            "pre" => Self::Pre,
            "pre-wrap" => Self::PreWrap,
            "pre-line" => Self::PreLine,
            "break-spaces" => Self::BreakSpaces,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Nowrap => "nowrap",
            Self::Pre => "pre",
            Self::PreWrap => "pre-wrap",
            Self::PreLine => "pre-line",
            Self::BreakSpaces => "break-spaces",
        }
    }
}

/// CSS `word-break` (CSS Text L4 §5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordBreak {
    Normal,
    BreakAll,
    KeepAll,
    BreakWord,
}

impl WordBreak {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "normal" => Self::Normal,
            "break-all" => Self::BreakAll,
            "keep-all" => Self::KeepAll,
            "break-word" => Self::BreakWord,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::BreakAll => "break-all",
            Self::KeepAll => "keep-all",
            Self::BreakWord => "break-word",
        }
    }
}

/// CSS `overflow-wrap` / `word-wrap` (CSS Text L4 §5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowWrap {
    Normal,
    BreakWord,
    Anywhere,
}

impl OverflowWrap {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "normal" => Self::Normal,
            "break-word" => Self::BreakWord,
            "anywhere" => Self::Anywhere,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::BreakWord => "break-word",
            Self::Anywhere => "anywhere",
        }
    }
}

/// CSS `visibility` (CSS Display L3 §11). Inherited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Visible,
    Hidden,
    Collapse,
}

impl Visibility {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "visible" => Self::Visible,
            "hidden" => Self::Hidden,
            "collapse" => Self::Collapse,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Visible => "visible",
            Self::Hidden => "hidden",
            Self::Collapse => "collapse",
        }
    }
}

/// CSS `display` (CSS Display L3). Subset reflektujici layout backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    None,
    Block,
    Inline,
    InlineBlock,
    Flex,
    InlineFlex,
    Grid,
    InlineGrid,
    Contents,
    Table,
    TableRow,
    TableCell,
    TableHeaderCell,
    TableRowGroup,
    TableHeaderGroup,
    TableFooterGroup,
    TableColumn,
    TableColumnGroup,
    TableCaption,
    ListItem,
    Ruby,
}

impl Display {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "none" => Self::None,
            "block" => Self::Block,
            "inline" => Self::Inline,
            "inline-block" => Self::InlineBlock,
            "flex" => Self::Flex,
            "inline-flex" => Self::InlineFlex,
            "grid" => Self::Grid,
            "inline-grid" => Self::InlineGrid,
            "contents" => Self::Contents,
            "table" => Self::Table,
            "table-row" => Self::TableRow,
            "table-cell" => Self::TableCell,
            "table-header-cell" => Self::TableHeaderCell,
            "table-row-group" => Self::TableRowGroup,
            "table-header-group" => Self::TableHeaderGroup,
            "table-footer-group" => Self::TableFooterGroup,
            "table-column" => Self::TableColumn,
            "table-column-group" => Self::TableColumnGroup,
            "table-caption" => Self::TableCaption,
            "list-item" => Self::ListItem,
            "ruby" => Self::Ruby,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Block => "block",
            Self::Inline => "inline",
            Self::InlineBlock => "inline-block",
            Self::Flex => "flex",
            Self::InlineFlex => "inline-flex",
            Self::Grid => "grid",
            Self::InlineGrid => "inline-grid",
            Self::Contents => "contents",
            Self::Table => "table",
            Self::TableRow => "table-row",
            Self::TableCell => "table-cell",
            Self::TableHeaderCell => "table-header-cell",
            Self::TableRowGroup => "table-row-group",
            Self::TableHeaderGroup => "table-header-group",
            Self::TableFooterGroup => "table-footer-group",
            Self::TableColumn => "table-column",
            Self::TableColumnGroup => "table-column-group",
            Self::TableCaption => "table-caption",
            Self::ListItem => "list-item",
            Self::Ruby => "ruby",
        }
    }
}

/// CSS `position` (CSS Position L3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionKind {
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

impl PositionKind {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "static" => Self::Static,
            "relative" => Self::Relative,
            "absolute" => Self::Absolute,
            "fixed" => Self::Fixed,
            "sticky" => Self::Sticky,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Relative => "relative",
            Self::Absolute => "absolute",
            Self::Fixed => "fixed",
            Self::Sticky => "sticky",
        }
    }
}

/// CSS `z-index` (CSS Position L3 §9.9). `auto` ne stack context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZIndex {
    Auto,
    Value(i32),
}

impl ZIndex {
    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim();
        if t.eq_ignore_ascii_case("auto") { return Some(Self::Auto); }
        t.parse::<i32>().ok().map(Self::Value)
    }
    pub fn css_string(self) -> String {
        match self {
            Self::Auto => "auto".into(),
            Self::Value(n) => n.to_string(),
        }
    }
}

/// CSS `cursor` (CSS UI L4 §8.1). Inherited. Subset typed; ostatni Custom.
#[derive(Debug, Clone, PartialEq)]
pub enum Cursor {
    Auto,
    Default,
    Pointer,
    Text,
    Move,
    NotAllowed,
    Grab,
    Grabbing,
    Wait,
    Help,
    Crosshair,
    Progress,
    Custom(String),     // url() nebo neznamy keyword
}

impl Cursor {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "auto" => Self::Auto,
            "default" => Self::Default,
            "pointer" => Self::Pointer,
            "text" => Self::Text,
            "move" => Self::Move,
            "not-allowed" => Self::NotAllowed,
            "grab" => Self::Grab,
            "grabbing" => Self::Grabbing,
            "wait" => Self::Wait,
            "help" => Self::Help,
            "crosshair" => Self::Crosshair,
            "progress" => Self::Progress,
            other => Self::Custom(other.to_string()),
        }
    }
    pub fn css_string(&self) -> String {
        match self {
            Self::Auto => "auto".into(),
            Self::Default => "default".into(),
            Self::Pointer => "pointer".into(),
            Self::Text => "text".into(),
            Self::Move => "move".into(),
            Self::NotAllowed => "not-allowed".into(),
            Self::Grab => "grab".into(),
            Self::Grabbing => "grabbing".into(),
            Self::Wait => "wait".into(),
            Self::Help => "help".into(),
            Self::Crosshair => "crosshair".into(),
            Self::Progress => "progress".into(),
            Self::Custom(s) => s.clone(),
        }
    }
}

/// CSS `line-height`: `normal` | <number> | <length>.
/// Number = multiplier on font-size; Length = absolute computed.
#[derive(Debug, Clone, PartialEq)]
pub enum LineHeight {
    Normal,
    Multiplier(f32),
    Length(Length),
}

impl LineHeight {
    pub fn resolve(&self, font_size_px: f32) -> f32 {
        match self {
            LineHeight::Normal => font_size_px * 1.2,
            LineHeight::Multiplier(m) => font_size_px * m,
            LineHeight::Length(l) => l.resolve_or(0.0, font_size_px, font_size_px, 1024.0, 768.0),
        }
    }
}

/// CSS `font-family` token: konkretni font nebo generic alias.
#[derive(Debug, Clone, PartialEq)]
pub enum FontFamily {
    Named(String),                              // "Arial", "Helvetica"
    Generic(GenericFamily),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GenericFamily {
    Serif,
    SansSerif,
    Monospace,
    Cursive,
    Fantasy,
    SystemUi,
    UiSerif,
    UiSansSerif,
    UiMonospace,
    UiRounded,
    Emoji,
    Math,
    Fangsong,
}

impl GenericFamily {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "serif" => Self::Serif,
            "sans-serif" => Self::SansSerif,
            "monospace" => Self::Monospace,
            "cursive" => Self::Cursive,
            "fantasy" => Self::Fantasy,
            "system-ui" => Self::SystemUi,
            "ui-serif" => Self::UiSerif,
            "ui-sans-serif" => Self::UiSansSerif,
            "ui-monospace" => Self::UiMonospace,
            "ui-rounded" => Self::UiRounded,
            "emoji" => Self::Emoji,
            "math" => Self::Math,
            "fangsong" => Self::Fangsong,
            _ => return None,
        })
    }
}
