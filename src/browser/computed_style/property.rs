//! PropertyId enum - typed CSS property names.
//!
//! Misto stringly `s.get("background-color")` po L5 migrace typed
//! `PropertyId::BackgroundColor`. Compile-time check, autocomplete v IDE,
//! cleaner pattern matching.
//!
//! Mapping: CSS kebab-case <-> Rust PascalCase.
//! - CSS `background-color` <-> `PropertyId::BackgroundColor`
//! - JS DOM `backgroundColor` (camel) reseno separate serialize krok.
//!
//! Coverage stage 2: jen subset CSS props (vsechny co cascade aktualne
//! podporuje). Pri dalsim feature work pridat.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertyId {
    // ─── Color / Background ───────────────────────────────────────────
    Color,
    BackgroundColor,
    BackgroundImage,
    BackgroundPosition,
    BackgroundSize,
    BackgroundRepeat,
    BackgroundAttachment,
    BackgroundClip,
    BackgroundOrigin,
    Background,                          // shorthand

    // ─── Font ─────────────────────────────────────────────────────────
    FontFamily,
    FontSize,
    FontWeight,
    FontStyle,
    FontStretch,
    FontVariant,
    FontFeatureSettings,
    FontVariationSettings,
    Font,                                // shorthand
    LineHeight,
    LetterSpacing,
    WordSpacing,

    // ─── Text ─────────────────────────────────────────────────────────
    TextAlign,
    TextDecoration,
    TextDecorationLine,
    TextDecorationStyle,
    TextDecorationColor,
    TextDecorationThickness,
    TextIndent,
    TextTransform,
    TextOverflow,
    TextShadow,
    VerticalAlign,
    WhiteSpace,
    WordBreak,
    OverflowWrap,
    Direction,
    WritingMode,
    UnicodeBidi,

    // ─── Box model: margin / padding / border ────────────────────────
    Margin,
    MarginTop,
    MarginRight,
    MarginBottom,
    MarginLeft,
    Padding,
    PaddingTop,
    PaddingRight,
    PaddingBottom,
    PaddingLeft,
    Border,
    BorderWidth,
    BorderStyle,
    BorderColor,
    BorderTop,
    BorderTopWidth,
    BorderTopStyle,
    BorderTopColor,
    BorderRight,
    BorderRightWidth,
    BorderRightStyle,
    BorderRightColor,
    BorderBottom,
    BorderBottomWidth,
    BorderBottomStyle,
    BorderBottomColor,
    BorderLeft,
    BorderLeftWidth,
    BorderLeftStyle,
    BorderLeftColor,
    BorderRadius,
    BorderTopLeftRadius,
    BorderTopRightRadius,
    BorderBottomLeftRadius,
    BorderBottomRightRadius,
    BoxSizing,
    BoxShadow,
    Outline,
    OutlineWidth,
    OutlineStyle,
    OutlineColor,
    OutlineOffset,

    // ─── Size / Position ──────────────────────────────────────────────
    Width,
    Height,
    MinWidth,
    MinHeight,
    MaxWidth,
    MaxHeight,
    Top,
    Right,
    Bottom,
    Left,
    Inset,
    Position,
    Float,
    Clear,
    ZIndex,

    // ─── Display / Layout ─────────────────────────────────────────────
    Display,
    Visibility,
    Overflow,
    OverflowX,
    OverflowY,
    Opacity,
    Cursor,
    PointerEvents,

    // ─── Flex ─────────────────────────────────────────────────────────
    Flex,
    FlexDirection,
    FlexWrap,
    FlexFlow,
    FlexGrow,
    FlexShrink,
    FlexBasis,
    JustifyContent,
    AlignItems,
    AlignContent,
    AlignSelf,
    JustifyItems,
    JustifySelf,
    Order,
    Gap,
    RowGap,
    ColumnGap,

    // ─── Grid ─────────────────────────────────────────────────────────
    GridTemplateColumns,
    GridTemplateRows,
    GridTemplateAreas,
    GridTemplate,
    GridAutoColumns,
    GridAutoRows,
    GridAutoFlow,
    Grid,
    GridColumn,
    GridRow,
    GridColumnStart,
    GridColumnEnd,
    GridRowStart,
    GridRowEnd,
    GridArea,

    // ─── Multi-column ─────────────────────────────────────────────────
    ColumnCount,
    ColumnWidth,
    Columns,
    ColumnGapMulticol,
    ColumnRule,
    ColumnRuleWidth,
    ColumnRuleStyle,
    ColumnRuleColor,
    ColumnSpan,
    BreakInside,
    BreakBefore,
    BreakAfter,

    // ─── Transform / Filter / Animation ──────────────────────────────
    Transform,
    TransformOrigin,
    TransformStyle,
    Perspective,
    PerspectiveOrigin,
    Filter,
    BackdropFilter,
    Transition,
    TransitionProperty,
    TransitionDuration,
    TransitionTimingFunction,
    TransitionDelay,
    Animation,
    AnimationName,
    AnimationDuration,
    AnimationTimingFunction,
    AnimationDelay,
    AnimationIterationCount,
    AnimationDirection,
    AnimationFillMode,
    AnimationPlayState,
    WillChange,

    // ─── Misc ─────────────────────────────────────────────────────────
    Content,                              // ::before / ::after
    Counter,
    CounterReset,
    CounterIncrement,
    ListStyle,
    ListStyleType,
    ListStylePosition,
    ListStyleImage,
    Quotes,
    TabSize,
    Resize,
    UserSelect,
    AspectRatio,
    MixBlendMode,
    Isolation,
    Clip,
    ClipPath,
    Mask,

    /// Catch-all pro neuvedene property names. Pri L5 pri parse stage
    /// neznamy property zmizi do `Unknown(String)` - cascade ignoruje
    /// (CSS spec: unknown declaration je invalid -> discard).
    Unknown,
}

impl PropertyId {
    /// Z CSS kebab-case nazvu na PropertyId.
    /// Vraci `Unknown` pri neznamem property (cascade ignoruje).
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "color" => Self::Color,
            "background-color" => Self::BackgroundColor,
            "background-image" => Self::BackgroundImage,
            "background-position" => Self::BackgroundPosition,
            "background-size" => Self::BackgroundSize,
            "background-repeat" => Self::BackgroundRepeat,
            "background-attachment" => Self::BackgroundAttachment,
            "background-clip" => Self::BackgroundClip,
            "background-origin" => Self::BackgroundOrigin,
            "background" => Self::Background,
            "font-family" => Self::FontFamily,
            "font-size" => Self::FontSize,
            "font-weight" => Self::FontWeight,
            "font-style" => Self::FontStyle,
            "font-stretch" => Self::FontStretch,
            "font-variant" => Self::FontVariant,
            "font-feature-settings" => Self::FontFeatureSettings,
            "font-variation-settings" => Self::FontVariationSettings,
            "font" => Self::Font,
            "line-height" => Self::LineHeight,
            "letter-spacing" => Self::LetterSpacing,
            "word-spacing" => Self::WordSpacing,
            "text-align" => Self::TextAlign,
            "text-decoration" => Self::TextDecoration,
            "text-decoration-line" => Self::TextDecorationLine,
            "text-decoration-style" => Self::TextDecorationStyle,
            "text-decoration-color" => Self::TextDecorationColor,
            "text-decoration-thickness" => Self::TextDecorationThickness,
            "text-indent" => Self::TextIndent,
            "text-transform" => Self::TextTransform,
            "text-overflow" => Self::TextOverflow,
            "text-shadow" => Self::TextShadow,
            "vertical-align" => Self::VerticalAlign,
            "white-space" => Self::WhiteSpace,
            "word-break" => Self::WordBreak,
            "overflow-wrap" | "word-wrap" => Self::OverflowWrap,
            "direction" => Self::Direction,
            "writing-mode" => Self::WritingMode,
            "unicode-bidi" => Self::UnicodeBidi,
            "margin" => Self::Margin,
            "margin-top" => Self::MarginTop,
            "margin-right" => Self::MarginRight,
            "margin-bottom" => Self::MarginBottom,
            "margin-left" => Self::MarginLeft,
            "padding" => Self::Padding,
            "padding-top" => Self::PaddingTop,
            "padding-right" => Self::PaddingRight,
            "padding-bottom" => Self::PaddingBottom,
            "padding-left" => Self::PaddingLeft,
            "border" => Self::Border,
            "border-width" => Self::BorderWidth,
            "border-style" => Self::BorderStyle,
            "border-color" => Self::BorderColor,
            "border-top" => Self::BorderTop,
            "border-top-width" => Self::BorderTopWidth,
            "border-top-style" => Self::BorderTopStyle,
            "border-top-color" => Self::BorderTopColor,
            "border-right" => Self::BorderRight,
            "border-right-width" => Self::BorderRightWidth,
            "border-right-style" => Self::BorderRightStyle,
            "border-right-color" => Self::BorderRightColor,
            "border-bottom" => Self::BorderBottom,
            "border-bottom-width" => Self::BorderBottomWidth,
            "border-bottom-style" => Self::BorderBottomStyle,
            "border-bottom-color" => Self::BorderBottomColor,
            "border-left" => Self::BorderLeft,
            "border-left-width" => Self::BorderLeftWidth,
            "border-left-style" => Self::BorderLeftStyle,
            "border-left-color" => Self::BorderLeftColor,
            "border-radius" => Self::BorderRadius,
            "border-top-left-radius" => Self::BorderTopLeftRadius,
            "border-top-right-radius" => Self::BorderTopRightRadius,
            "border-bottom-left-radius" => Self::BorderBottomLeftRadius,
            "border-bottom-right-radius" => Self::BorderBottomRightRadius,
            "box-sizing" => Self::BoxSizing,
            "box-shadow" => Self::BoxShadow,
            "outline" => Self::Outline,
            "outline-width" => Self::OutlineWidth,
            "outline-style" => Self::OutlineStyle,
            "outline-color" => Self::OutlineColor,
            "outline-offset" => Self::OutlineOffset,
            "width" => Self::Width,
            "height" => Self::Height,
            "min-width" => Self::MinWidth,
            "min-height" => Self::MinHeight,
            "max-width" => Self::MaxWidth,
            "max-height" => Self::MaxHeight,
            "top" => Self::Top,
            "right" => Self::Right,
            "bottom" => Self::Bottom,
            "left" => Self::Left,
            "inset" => Self::Inset,
            "position" => Self::Position,
            "float" => Self::Float,
            "clear" => Self::Clear,
            "z-index" => Self::ZIndex,
            "display" => Self::Display,
            "visibility" => Self::Visibility,
            "overflow" => Self::Overflow,
            "overflow-x" => Self::OverflowX,
            "overflow-y" => Self::OverflowY,
            "opacity" => Self::Opacity,
            "cursor" => Self::Cursor,
            "pointer-events" => Self::PointerEvents,
            "flex" => Self::Flex,
            "flex-direction" => Self::FlexDirection,
            "flex-wrap" => Self::FlexWrap,
            "flex-flow" => Self::FlexFlow,
            "flex-grow" => Self::FlexGrow,
            "flex-shrink" => Self::FlexShrink,
            "flex-basis" => Self::FlexBasis,
            "justify-content" => Self::JustifyContent,
            "align-items" => Self::AlignItems,
            "align-content" => Self::AlignContent,
            "align-self" => Self::AlignSelf,
            "justify-items" => Self::JustifyItems,
            "justify-self" => Self::JustifySelf,
            "order" => Self::Order,
            "gap" => Self::Gap,
            "row-gap" => Self::RowGap,
            "column-gap" => Self::ColumnGap,
            "grid-template-columns" => Self::GridTemplateColumns,
            "grid-template-rows" => Self::GridTemplateRows,
            "grid-template-areas" => Self::GridTemplateAreas,
            "grid-template" => Self::GridTemplate,
            "grid-auto-columns" => Self::GridAutoColumns,
            "grid-auto-rows" => Self::GridAutoRows,
            "grid-auto-flow" => Self::GridAutoFlow,
            "grid" => Self::Grid,
            "grid-column" => Self::GridColumn,
            "grid-row" => Self::GridRow,
            "grid-column-start" => Self::GridColumnStart,
            "grid-column-end" => Self::GridColumnEnd,
            "grid-row-start" => Self::GridRowStart,
            "grid-row-end" => Self::GridRowEnd,
            "grid-area" => Self::GridArea,
            "column-count" => Self::ColumnCount,
            "column-width" => Self::ColumnWidth,
            "columns" => Self::Columns,
            "column-rule" => Self::ColumnRule,
            "column-rule-width" => Self::ColumnRuleWidth,
            "column-rule-style" => Self::ColumnRuleStyle,
            "column-rule-color" => Self::ColumnRuleColor,
            "column-span" => Self::ColumnSpan,
            "break-inside" => Self::BreakInside,
            "break-before" => Self::BreakBefore,
            "break-after" => Self::BreakAfter,
            "transform" => Self::Transform,
            "transform-origin" => Self::TransformOrigin,
            "transform-style" => Self::TransformStyle,
            "perspective" => Self::Perspective,
            "perspective-origin" => Self::PerspectiveOrigin,
            "filter" => Self::Filter,
            "backdrop-filter" => Self::BackdropFilter,
            "transition" => Self::Transition,
            "transition-property" => Self::TransitionProperty,
            "transition-duration" => Self::TransitionDuration,
            "transition-timing-function" => Self::TransitionTimingFunction,
            "transition-delay" => Self::TransitionDelay,
            "animation" => Self::Animation,
            "animation-name" => Self::AnimationName,
            "animation-duration" => Self::AnimationDuration,
            "animation-timing-function" => Self::AnimationTimingFunction,
            "animation-delay" => Self::AnimationDelay,
            "animation-iteration-count" => Self::AnimationIterationCount,
            "animation-direction" => Self::AnimationDirection,
            "animation-fill-mode" => Self::AnimationFillMode,
            "animation-play-state" => Self::AnimationPlayState,
            "will-change" => Self::WillChange,
            "content" => Self::Content,
            "counter-reset" => Self::CounterReset,
            "counter-increment" => Self::CounterIncrement,
            "list-style" => Self::ListStyle,
            "list-style-type" => Self::ListStyleType,
            "list-style-position" => Self::ListStylePosition,
            "list-style-image" => Self::ListStyleImage,
            "quotes" => Self::Quotes,
            "tab-size" => Self::TabSize,
            "resize" => Self::Resize,
            "user-select" => Self::UserSelect,
            "aspect-ratio" => Self::AspectRatio,
            "mix-blend-mode" => Self::MixBlendMode,
            "isolation" => Self::Isolation,
            "clip" => Self::Clip,
            "clip-path" => Self::ClipPath,
            "mask" => Self::Mask,
            _ => Self::Unknown,
        }
    }

    /// Zpet na CSS kebab-case nazov (pro devtools display + JS getComputedStyle).
    pub fn css_name(self) -> &'static str {
        match self {
            Self::Color => "color",
            Self::BackgroundColor => "background-color",
            Self::BackgroundImage => "background-image",
            Self::BackgroundPosition => "background-position",
            Self::BackgroundSize => "background-size",
            Self::BackgroundRepeat => "background-repeat",
            Self::BackgroundAttachment => "background-attachment",
            Self::BackgroundClip => "background-clip",
            Self::BackgroundOrigin => "background-origin",
            Self::Background => "background",
            Self::FontFamily => "font-family",
            Self::FontSize => "font-size",
            Self::FontWeight => "font-weight",
            Self::FontStyle => "font-style",
            Self::FontStretch => "font-stretch",
            Self::FontVariant => "font-variant",
            Self::FontFeatureSettings => "font-feature-settings",
            Self::FontVariationSettings => "font-variation-settings",
            Self::Font => "font",
            Self::LineHeight => "line-height",
            Self::LetterSpacing => "letter-spacing",
            Self::WordSpacing => "word-spacing",
            Self::TextAlign => "text-align",
            Self::TextDecoration => "text-decoration",
            Self::TextDecorationLine => "text-decoration-line",
            Self::TextDecorationStyle => "text-decoration-style",
            Self::TextDecorationColor => "text-decoration-color",
            Self::TextDecorationThickness => "text-decoration-thickness",
            Self::TextIndent => "text-indent",
            Self::TextTransform => "text-transform",
            Self::TextOverflow => "text-overflow",
            Self::TextShadow => "text-shadow",
            Self::VerticalAlign => "vertical-align",
            Self::WhiteSpace => "white-space",
            Self::WordBreak => "word-break",
            Self::OverflowWrap => "overflow-wrap",
            Self::Direction => "direction",
            Self::WritingMode => "writing-mode",
            Self::UnicodeBidi => "unicode-bidi",
            Self::Margin => "margin",
            Self::MarginTop => "margin-top",
            Self::MarginRight => "margin-right",
            Self::MarginBottom => "margin-bottom",
            Self::MarginLeft => "margin-left",
            Self::Padding => "padding",
            Self::PaddingTop => "padding-top",
            Self::PaddingRight => "padding-right",
            Self::PaddingBottom => "padding-bottom",
            Self::PaddingLeft => "padding-left",
            Self::Border => "border",
            Self::BorderWidth => "border-width",
            Self::BorderStyle => "border-style",
            Self::BorderColor => "border-color",
            Self::BorderTop => "border-top",
            Self::BorderTopWidth => "border-top-width",
            Self::BorderTopStyle => "border-top-style",
            Self::BorderTopColor => "border-top-color",
            Self::BorderRight => "border-right",
            Self::BorderRightWidth => "border-right-width",
            Self::BorderRightStyle => "border-right-style",
            Self::BorderRightColor => "border-right-color",
            Self::BorderBottom => "border-bottom",
            Self::BorderBottomWidth => "border-bottom-width",
            Self::BorderBottomStyle => "border-bottom-style",
            Self::BorderBottomColor => "border-bottom-color",
            Self::BorderLeft => "border-left",
            Self::BorderLeftWidth => "border-left-width",
            Self::BorderLeftStyle => "border-left-style",
            Self::BorderLeftColor => "border-left-color",
            Self::BorderRadius => "border-radius",
            Self::BorderTopLeftRadius => "border-top-left-radius",
            Self::BorderTopRightRadius => "border-top-right-radius",
            Self::BorderBottomLeftRadius => "border-bottom-left-radius",
            Self::BorderBottomRightRadius => "border-bottom-right-radius",
            Self::BoxSizing => "box-sizing",
            Self::BoxShadow => "box-shadow",
            Self::Outline => "outline",
            Self::OutlineWidth => "outline-width",
            Self::OutlineStyle => "outline-style",
            Self::OutlineColor => "outline-color",
            Self::OutlineOffset => "outline-offset",
            Self::Width => "width",
            Self::Height => "height",
            Self::MinWidth => "min-width",
            Self::MinHeight => "min-height",
            Self::MaxWidth => "max-width",
            Self::MaxHeight => "max-height",
            Self::Top => "top",
            Self::Right => "right",
            Self::Bottom => "bottom",
            Self::Left => "left",
            Self::Inset => "inset",
            Self::Position => "position",
            Self::Float => "float",
            Self::Clear => "clear",
            Self::ZIndex => "z-index",
            Self::Display => "display",
            Self::Visibility => "visibility",
            Self::Overflow => "overflow",
            Self::OverflowX => "overflow-x",
            Self::OverflowY => "overflow-y",
            Self::Opacity => "opacity",
            Self::Cursor => "cursor",
            Self::PointerEvents => "pointer-events",
            Self::Flex => "flex",
            Self::FlexDirection => "flex-direction",
            Self::FlexWrap => "flex-wrap",
            Self::FlexFlow => "flex-flow",
            Self::FlexGrow => "flex-grow",
            Self::FlexShrink => "flex-shrink",
            Self::FlexBasis => "flex-basis",
            Self::JustifyContent => "justify-content",
            Self::AlignItems => "align-items",
            Self::AlignContent => "align-content",
            Self::AlignSelf => "align-self",
            Self::JustifyItems => "justify-items",
            Self::JustifySelf => "justify-self",
            Self::Order => "order",
            Self::Gap => "gap",
            Self::RowGap => "row-gap",
            Self::ColumnGap => "column-gap",
            Self::GridTemplateColumns => "grid-template-columns",
            Self::GridTemplateRows => "grid-template-rows",
            Self::GridTemplateAreas => "grid-template-areas",
            Self::GridTemplate => "grid-template",
            Self::GridAutoColumns => "grid-auto-columns",
            Self::GridAutoRows => "grid-auto-rows",
            Self::GridAutoFlow => "grid-auto-flow",
            Self::Grid => "grid",
            Self::GridColumn => "grid-column",
            Self::GridRow => "grid-row",
            Self::GridColumnStart => "grid-column-start",
            Self::GridColumnEnd => "grid-column-end",
            Self::GridRowStart => "grid-row-start",
            Self::GridRowEnd => "grid-row-end",
            Self::GridArea => "grid-area",
            Self::ColumnCount => "column-count",
            Self::ColumnWidth => "column-width",
            Self::Columns => "columns",
            Self::ColumnGapMulticol => "column-gap",
            Self::ColumnRule => "column-rule",
            Self::ColumnRuleWidth => "column-rule-width",
            Self::ColumnRuleStyle => "column-rule-style",
            Self::ColumnRuleColor => "column-rule-color",
            Self::ColumnSpan => "column-span",
            Self::BreakInside => "break-inside",
            Self::BreakBefore => "break-before",
            Self::BreakAfter => "break-after",
            Self::Transform => "transform",
            Self::TransformOrigin => "transform-origin",
            Self::TransformStyle => "transform-style",
            Self::Perspective => "perspective",
            Self::PerspectiveOrigin => "perspective-origin",
            Self::Filter => "filter",
            Self::BackdropFilter => "backdrop-filter",
            Self::Transition => "transition",
            Self::TransitionProperty => "transition-property",
            Self::TransitionDuration => "transition-duration",
            Self::TransitionTimingFunction => "transition-timing-function",
            Self::TransitionDelay => "transition-delay",
            Self::Animation => "animation",
            Self::AnimationName => "animation-name",
            Self::AnimationDuration => "animation-duration",
            Self::AnimationTimingFunction => "animation-timing-function",
            Self::AnimationDelay => "animation-delay",
            Self::AnimationIterationCount => "animation-iteration-count",
            Self::AnimationDirection => "animation-direction",
            Self::AnimationFillMode => "animation-fill-mode",
            Self::AnimationPlayState => "animation-play-state",
            Self::WillChange => "will-change",
            Self::Content => "content",
            Self::Counter => "counter",
            Self::CounterReset => "counter-reset",
            Self::CounterIncrement => "counter-increment",
            Self::ListStyle => "list-style",
            Self::ListStyleType => "list-style-type",
            Self::ListStylePosition => "list-style-position",
            Self::ListStyleImage => "list-style-image",
            Self::Quotes => "quotes",
            Self::TabSize => "tab-size",
            Self::Resize => "resize",
            Self::UserSelect => "user-select",
            Self::AspectRatio => "aspect-ratio",
            Self::MixBlendMode => "mix-blend-mode",
            Self::Isolation => "isolation",
            Self::Clip => "clip",
            Self::ClipPath => "clip-path",
            Self::Mask => "mask",
            Self::Unknown => "<unknown>",
        }
    }

    /// CSS DOM convention - JS getComputedStyle().backgroundColor.
    /// kebab "background-color" -> camel "backgroundColor".
    /// Vraci String (allocate) protoze conversion na runtime.
    pub fn js_dom_name(self) -> String {
        let name = self.css_name();
        if !name.contains('-') { return name.to_string(); }
        let mut out = String::with_capacity(name.len());
        let mut up = false;
        for c in name.chars() {
            if c == '-' { up = true; continue; }
            if up { out.extend(c.to_uppercase()); up = false; }
            else { out.push(c); }
        }
        out
    }

    /// True pokud property je inherited per CSS spec (cascade default
    /// inherit z parent kdyz none specified).
    pub fn is_inherited(self) -> bool {
        matches!(self,
            Self::Color | Self::FontFamily | Self::FontSize | Self::FontWeight
            | Self::FontStyle | Self::FontStretch | Self::FontVariant
            | Self::FontFeatureSettings | Self::FontVariationSettings
            | Self::Font | Self::LineHeight | Self::LetterSpacing
            | Self::WordSpacing | Self::TextAlign | Self::TextIndent
            | Self::TextTransform | Self::WhiteSpace | Self::WordBreak
            | Self::OverflowWrap | Self::Direction | Self::WritingMode
            | Self::UnicodeBidi | Self::Visibility | Self::Cursor
            | Self::ListStyle | Self::ListStyleType | Self::ListStylePosition
            | Self::ListStyleImage | Self::Quotes | Self::TabSize
            | Self::TextDecoration | Self::TextDecorationColor
            | Self::PointerEvents
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_known() {
        assert_eq!(PropertyId::parse("background-color"), PropertyId::BackgroundColor);
        assert_eq!(PropertyId::parse("FONT-SIZE"), PropertyId::FontSize);
        assert_eq!(PropertyId::parse(" margin "), PropertyId::Margin);
    }

    #[test]
    fn parse_unknown() {
        assert_eq!(PropertyId::parse("totally-fake-prop"), PropertyId::Unknown);
        assert_eq!(PropertyId::parse(""), PropertyId::Unknown);
    }

    #[test]
    fn css_name_roundtrip() {
        let props = [
            PropertyId::BackgroundColor, PropertyId::FontSize, PropertyId::MarginTop,
            PropertyId::FlexDirection, PropertyId::GridTemplateColumns,
        ];
        for p in props {
            let name = p.css_name();
            assert_eq!(PropertyId::parse(name), p, "roundtrip failed for {}", name);
        }
    }

    #[test]
    fn js_dom_name_camelcase() {
        assert_eq!(PropertyId::BackgroundColor.js_dom_name(), "backgroundColor");
        assert_eq!(PropertyId::MarginTop.js_dom_name(), "marginTop");
        assert_eq!(PropertyId::GridTemplateColumns.js_dom_name(), "gridTemplateColumns");
        assert_eq!(PropertyId::Color.js_dom_name(), "color");
    }

    #[test]
    fn inherited_flag() {
        assert!(PropertyId::Color.is_inherited());
        assert!(PropertyId::FontFamily.is_inherited());
        assert!(PropertyId::LetterSpacing.is_inherited());
        assert!(!PropertyId::BackgroundColor.is_inherited());
        assert!(!PropertyId::Margin.is_inherited());
        assert!(!PropertyId::Width.is_inherited());
    }

    #[test]
    fn word_wrap_alias() {
        // CSS Text L3: overflow-wrap je modern jmeno, word-wrap legacy alias.
        assert_eq!(PropertyId::parse("overflow-wrap"), PropertyId::OverflowWrap);
        assert_eq!(PropertyId::parse("word-wrap"), PropertyId::OverflowWrap);
    }
}
