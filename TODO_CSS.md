# CSS specifikace - implementacni tracker

Kompletni mapa CSS modulu (W3C). Stav per feature. Postupujem **shora dolu**
v sekci "Priority", pak prochazime jednotlive moduly.

Konvence stavu:
- [x] hotovo
- [/] castecne (popis chybejiciho)
- [ ] chybi cele
- [-] vynechano (out of scope)

---

## Priorita / next batches

### Batch 1 - dotazeni stavajicich modulu (highest ROI)
- [x] Selectors L4: `:is()`, `:where()`, `:not()`, `:has()`, `~` general sibling, `:nth-child/-of-type`, `:nth-last-*`, `:empty`, `:first/last/only-of-type`, `:only-child` (`:focus-visible/-within` zatim no-op kvuli runtime stavu)
- [/] Values L4: `min()`, `max()`, `clamp()`, `env()` hotovo, `attr()` chybi, math funkce (round/sin/cos/...) chybi
- [/] Color L4: `oklch`, `oklab`, `lab`, `lch`, `hsl`, `hwb`, `color-mix(in srgb|oklab|oklch)`, modern rgb syntax, hex 4/8 hotovo. Relative color syntax + `color()` namespace + system colors zatim chybi.
- [/] Backgrounds L3: position/size/repeat/clip/origin/attachment parser hotovy. Multiple backgrounds (carkove) + paint integrace TODO.
- [x] Animations L1: `animation-fill-mode` (none/forwards/backwards/both), `animation-play-state` (running/paused), arbitrary `cubic-bezier(...)`, `steps(n, jump-*)`

### Batch 2 - chybejici "must-have" moduly
- [x] CSS Transitions L1 (parser + state diff per-frame, transitionend dispatch hotov)
- [x] CSS Logical Properties L1 (`margin/padding-block/-inline`, `border-*-block/-inline-*`, `inset-*`, `block-size/inline-size`, `border-start-end-radius` rohy) - mapovani LTR + horizontal-tb
- [x] CSS Nesting L1 (`&` selector + nested rulesets, implicit descendant pri ne-amp prefix, kombinace `.parent.nested` pres `&`)
- [/] CSS Container Queries L1 (`@container [name] (cond)` parser, cqw/cqh/cqi/cqb/cqmin/cqmax units, evaluation pres viewport - per-element ancestor lookup TODO)
- [x] CSS Filter Effects L1 (`filter: blur/brightness/contrast/grayscale/sepia/invert/saturate/hue-rotate/opacity/drop-shadow` - blur 2-pass gauss RT pipeline + color matrix compose shader hotove)

### Batch 3 - dalsi
- [x] @font-face (CSS Fonts L4) - FS load, FontFace API, document.fonts
- [/] Cascade Layers (`@layer`) - parser hotov, runtime layer ordering TODO
- [x] Position L3: `sticky` (apply_sticky post-pass pri scroll)
- [x] Masking L1: `clip-path` (inset/circle/ellipse hotove, polygon ear-clipping triangulace)
- [ ] Subgrid L2
- [ ] Shapes L1: `shape-outside`
- [x] Transforms L2: 3D (`perspective`, `transform-style`, `rotateX/Y/3d`, matrix3d) - 4x4 matrix shader pipeline

### Batch 4 - exoticke / draft
- [x] Anchor Positioning L1 (`anchor-name`, `position-anchor` runtime layout post-pass)
- [x] Scroll-driven Animations L1 (`animation-timeline: scroll()`)
- [x] View Transitions L1 (parser + `document.startViewTransition()` stub)
- [-] Houdini (Paint/Layout/Properties API) - vynechano (out of scope, browser internals)
- [ ] Color L5 (advanced color manipulation)

---

## Detail per modul

### CSS Selectors Level 4 ([CR](https://www.w3.org/TR/selectors-4/))
- [x] Type, universal, class, id selektory
- [x] Descendant, child (`>`), adjacent sibling (`+`)
- [x] Attribute selektory (`[attr]`, `[attr=v]`, `[attr~=v]`)
- [x] `:hover`, `:active`, `:focus` (pseudo-classes - parsing OK, runtime stavu chybi)
- [x] `~` general sibling combinator
- [x] `:is()` - matches-any pseudo-class
- [x] `:where()` - jako :is ale specificita 0
- [x] `:not(<selector-list>)` (vc. selector list)
- [x] `:has()` - relacni pseudo-class (descendant only)
- [/] `:focus-visible`, `:focus-within` - parsing OK, runtime stav chybi (no-op match)
- [x] `:empty`, `:first-of-type`, `:last-of-type`, `:only-of-type`, `:only-child`
- [x] `:nth-of-type(an+b)`, `:nth-last-child(an+b)`, `:nth-last-of-type(an+b)`, `:nth-child(an+b)` (vc. odd/even)
- [ ] `:lang()`, `:dir()`
- [x] `:any-link`, `:scope` (no-op match - bez navigation context)
- [x] `:required`, `:optional` (z required attribut)
- [x] `:disabled`, `:enabled` (z disabled attribut)
- [x] `:checked` (z checked attribut)
- [x] `:read-only`, `:read-write` (z readonly/disabled)
- [x] `:placeholder-shown` (placeholder + empty value)
- [ ] `:valid`, `:invalid` - vyzaduje runtime form validation
- [ ] `||` column combinator (table cells)
- [ ] `&` nesting selector (CSS Nesting L1)

### CSS Color L3/L4/L5
- [x] `#hex`, `#hexa` (3/4/6/8)
- [x] `rgb()`, `rgba()` legacy + modern (mezery + `/` alpha)
- [x] `hsl()`, `hsla()` legacy + modern
- [x] Named colors
- [x] `currentColor` (parsing)
- [x] `hwb(h w b)` (L4)
- [x] `lab(l a b)` (L4) - D65 illuminant
- [x] `lch(l c h)` (L4) - polar varianta lab
- [x] `oklab(l a b)` (L4) - Bjorn Ottosson algoritmus
- [x] `oklch(l c h)` (L4)
- [ ] `color(colorspace c1 c2 c3)` (L4) - sRGB, display-p3, rec2020, ...
- [x] `color-mix(in space, c1 X%, c2 Y%)` (L5) - srgb / oklab / oklch
- [ ] Relative color: `rgb(from <c> r g b)` (L5)
- [ ] `system-color` keywords L4
- [ ] `device-cmyk()` (L4)

### CSS Values & Units L4
- [x] Length: `px`, `em`, `rem`, `%`, `vw`, `vh`, `vmin`, `vmax`, `pt`
- [x] `calc()`
- [x] `var()` + fallback
- [x] `min(a, b, ...)`
- [x] `max(a, b, ...)`
- [x] `clamp(min, val, max)`
- [ ] `attr(name <type>, fallback)`
- [x] `env(name, fallback)` (no-op s fallback)
- [x] Math fci L4: `round`, `floor`, `ceil`, `mod`, `rem`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `pow`, `sqrt`, `hypot`, `log`, `exp`, `abs`, `sign`
- [ ] Container queries units: `cqw`, `cqh`, `cqi`, `cqb`, `cqmin`, `cqmax`
- [ ] `lh`, `rlh` (line-height units)
- [ ] `ch`, `ex` (font units)
- [ ] `vi`, `vb` (logical viewport units)
- [ ] `dvw`, `dvh`, `lvw`, `lvh`, `svw`, `svh` (dynamic viewport)
- [ ] `Q`, `cm`, `mm`, `in`, `pc` (absolute units - low priority)

### CSS Box Model L3
- [x] `width`, `height`, `min-width`, `min-height`, `max-width`, `max-height`
- [x] `padding`, `margin`, `border`
- [x] `box-sizing: content-box | border-box`
- [x] `aspect-ratio: <w> / <h>` / `aspect-ratio: <ratio>` (parser + LayoutBox.aspect_ratio)
- [ ] `min-content`, `max-content`, `fit-content` keywords

### CSS Backgrounds & Borders L3/L4
- [x] `background-color`
- [x] `background-image: linear-gradient(...)`
- [x] `background-image: url(...)`
- [x] `border-width`, `border-style`, `border-color`, `border-radius`
- [x] `box-shadow` (drop)
- [x] Multiple backgrounds (kazdy oddelen carkou) - vsechny props comma-split, layery v Vec<BgLayer>
- [x] `background-position` (parser - keywords/length/% mix)
- [x] `background-size: cover|contain|<length>|<%>` (parser)
- [x] `background-repeat: repeat/-x/-y/no-repeat/space/round` (parser)
- [x] `background-attachment: scroll|fixed|local` (parser)
- [x] `background-clip: border-box|padding-box|content-box` (parser)
- [x] `background-origin: border-box|padding-box|content-box` (parser)
- [ ] `background-clip: text` (text mask - vyzaduje RT)
- [ ] `background-blend-mode`
- [ ] `border-image-*` (L4)
- [x] `box-shadow inset` varianta (mode 5 SDF shader, fade smerem dovnitr od okraju)
- [ ] Asymetricke `border-radius` (`/` syntax)

### CSS Fonts L4/L5
- [x] `font-family`, `font-size`, `font-weight`, `font-style`
- [x] `font` shorthand (parsing)
- [x] `@font-face { font-family; src: url(); }` parser + runtime FS load + GlyphAtlas extra_fonts + per-text font lookup pri rasterize. HTTP load TODO.
- [/] `font-display: auto|block|swap|fallback|optional` parsing OK
- [ ] `font-variation-settings`
- [ ] `font-feature-settings`
- [ ] `font-stretch`
- [ ] `font-size-adjust`
- [ ] `font-variant-*` (caps, ligatures, numeric, ...)
- [ ] System UI fonts: `system-ui`, `ui-serif`, ...

### CSS Text L3/L4
- [x] `color`
- [x] `text-align`
- [x] `line-height`
- [/] `white-space` (parsing OK?, layout?)
- [x] `text-indent` (parser, paint apply TODO)
- [x] `text-transform: uppercase|lowercase|capitalize|none` (paint apply)
- [x] `letter-spacing` parser (paint apply TODO)
- [x] `word-spacing` parser (paint apply TODO)
- [ ] `tab-size`
- [ ] `word-break: normal|break-all|keep-all`
- [ ] `overflow-wrap` / `word-wrap`
- [ ] `hyphens`
- [ ] `text-wrap: wrap|nowrap|balance|pretty` (L4)
- [ ] `line-break`
- [ ] `text-justify`

### CSS Text Decoration L3/L4
- [x] `text-decoration: underline|overline|line-through`
- [x] `text-decoration-color` (parser, render apply TODO)
- [x] `text-decoration-style: solid|double|dotted|dashed|wavy` (parser, render TODO)
- [x] `text-decoration-thickness` (parser)
- [x] `text-underline-offset` (parser)
- [x] `text-shadow` (offset_x offset_y blur color, paint emit pred main text)
- [ ] `text-emphasis`

### CSS Flexbox L1
- [x] `display: flex`, `flex-direction`, `flex-wrap`, `justify-content`, `align-items`, `align-self`, `gap`, `flex` shorthand (pres taffy)
- [x] `flex-grow`, `flex-shrink`, `flex-basis`
- [x] `align-content`
- [x] `place-content`, `place-items`, `place-self` shorthandy (expand do align-/justify-)
- [ ] `order`
- [ ] Edge cases: aspect-ratio interakce, intrinsic sizing

### CSS Grid L1/L2
- [x] `display: grid`, `grid-template-columns`, `grid-template-rows`, `gap`, `grid-area` (pres taffy)
- [ ] `grid-template-areas` (string syntax)
- [ ] `grid-auto-flow`, `grid-auto-columns`, `grid-auto-rows`
- [ ] `subgrid` (L2)
- [ ] Named grid lines, named areas
- [ ] `masonry` layout (L3 draft)

### CSS Position L3/L4
- [x] `position: static|relative|absolute|fixed`
- [x] `top`, `right`, `bottom`, `left`, `z-index`
- [ ] `position: sticky` + `inset` shorthand
- [ ] Anchor positioning L1 (`anchor-name`, `position-anchor`, `inset-area`)

### CSS Transforms L1/L2
- [x] `transform: translate(...)`, `rotate(...)`, `scale(...)`, `skew(...)`, `matrix(...)`
- [x] `transform-origin`
- [x] 3D transformy: `translate3d`, `translateZ`, `rotate3d`, `rotateX/Y/Z`, `scale3d`, `matrix3d`, `perspective()` (parser; render aktualne 2D approximace)
- [ ] `perspective` property + `perspective-origin` (render 3D pipeline)
- [ ] `transform-style: flat | preserve-3d`
- [ ] `backface-visibility: hidden`
- [ ] `transform-box`

### CSS Animations L1/L2
- [x] `@keyframes name { 0% {} 100% {} }`
- [x] `animation` shorthand (name, duration, timing-function, iteration-count, direction, delay, fill-mode, play-state)
- [x] Easing: linear, ease, ease-in, ease-out, ease-in-out (cubic-bezier)
- [x] `step-start`, `step-end`
- [x] `animation-fill-mode: none|forwards|backwards|both`
- [x] `animation-play-state: running|paused`
- [x] `cubic-bezier(x1, y1, x2, y2)` arbitrary
- [x] `steps(n, jump-start|jump-end|jump-both|jump-none|start|end)`
- [ ] `animation-composition` (L2)
- [ ] `animation-timeline` (scroll-driven, L2 draft)

### CSS Transitions L1
- [x] `transition-property`
- [x] `transition-duration`
- [x] `transition-timing-function`
- [x] `transition-delay`
- [x] `transition` shorthand (vc. multiple comma-separated)
- [ ] `transitionrun`/`transitionstart`/`transitionend`/`transitioncancel` events
- [x] State diff detection (interpoluje pri zmene stylu) - per-frame v render loopu

### CSS Custom Properties L1
- [x] `--name: value` definice
- [x] `var(--name, fallback)` pouziti
- [ ] `@property --name { syntax: ...; inherits: ...; initial-value: ... }` registrace
- [ ] Animatable vlastni properties
- [ ] `inherits: false`

### CSS Cascading & Inheritance L5
- [x] Specificita selektoru
- [x] `!important`
- [x] User-agent default styly per tag
- [x] `@layer name { ... }` cascade layers (parser + cascade prio)
- [x] `@layer name1, name2;` order declaration
- [ ] `@import url(...) layer(name);` - vyzaduje @import support
- [ ] `revert`, `revert-layer`, `unset` keywords
- [ ] Origin importance (user vs author)

### CSS Logical Properties L1
- [x] `margin-block-start/end`, `margin-inline-start/end`
- [x] `margin-block`, `margin-inline` shorthandy
- [x] `padding-block-*`, `padding-inline-*` + shorthandy
- [x] `border-block-*-width/-style/-color`, `border-inline-*-width/-style/-color`
- [x] `border-start-start-radius` ... `border-end-end-radius` (4 logicke rohy)
- [x] `inset-block-*`, `inset-inline-*` + shorthandy
- [x] `inset` shorthand (top right bottom left)
- [x] `block-size`, `inline-size`
- [x] `min-block-size`, `min-inline-size`, `max-block-size`, `max-inline-size`
- [ ] `text-align: start | end` (mapping na left/right v LTR)
- [ ] `float: inline-start | inline-end`
- [ ] `direction: ltr | rtl` - aktualne predpoklad LTR
- [ ] `writing-mode: horizontal-tb | vertical-rl | vertical-lr` - aktualne predpoklad horizontal-tb

### CSS Containment L3
- [x] `contain: layout | paint | size | style | content | strict` (parser + bitfield)
- [ ] `contain-intrinsic-size`
- [ ] `content-visibility: auto | hidden | visible`

### CSS Container Queries L1
- [/] `container-type: normal | inline-size | size` (parsing OK, runtime detection TODO)
- [/] `container-name` (parsing OK)
- [ ] `container` shorthand
- [x] `@container [name] (condition) { rules }` parsing + viewport-fallback evaluation
- [x] `cqw`, `cqh`, `cqi`, `cqb`, `cqmin`, `cqmax` jednotky (aproximace pres viewport)
- [ ] Style queries `@container style(--name: val)`
- [ ] Per-element container ancestor lookup (correct CQ implementation)

### CSS Nesting L1
- [x] `&` selector
- [ ] Nested at-rules (`@media`, `@supports` uvnitr ruleset)
- [x] Implicit descendant pri ne-amp prefix (`tag`, `.class`)

### CSS @media L4/L5
- [x] `@media (max-width: ...)`, `@media (min-width: ...)`, `@media (max/min-height)`
- [x] `@media screen`, `@media print` (parsing)
- [x] `@media (prefers-color-scheme: dark|light)` - env var override RUST_WEB_ENGINE_DARK
- [x] `@media (prefers-reduced-motion: reduce|no-preference)` - env var override RUST_WEB_ENGINE_REDUCED_MOTION
- [x] `@media (hover: hover|none)` - default hover available
- [x] `@media (pointer: fine|coarse|none)` - default fine
- [x] `@media (any-hover|any-pointer)` - default match
- [x] `@media (display-mode: browser|fullscreen)` - default browser
- [x] `@media (forced-colors: active|none)` - default none
- [x] `@media (color: 0|n)` - default n>0 (8 bit)
- [x] `@media (orientation: landscape|portrait)`
- [ ] Range syntax: `@media (400px <= width <= 800px)` (L4)

### CSS Filter Effects L1
- [x] `filter: blur(<r>)` - 2-pass separable gauss WGSL + RT pipeline (compose shader)
- [x] `filter: brightness(<n>)` - color matrix shader
- [x] `filter: contrast(<n>)` - color matrix shader
- [x] `filter: grayscale(<%>)` - color matrix shader (luma basis 0.2126/0.7152/0.0722)
- [x] `filter: hue-rotate(<deg>)` - color matrix shader (luma-preserving)
- [x] `filter: invert(<n>)` - color matrix shader
- [x] `filter: opacity(<%>)` - color matrix shader (alpha kanal)
- [x] `filter: saturate(<n>)` - color matrix shader
- [x] `filter: sepia(<%>)` - color matrix shader (W3C sepia coefs)
- [x] `filter: drop-shadow(...)` - shadow command emit pred bg
- [x] Vice filtru chained: `filter: blur(2px) brightness(1.2) hue-rotate(45deg)` - parser + RT pipeline subtree
- [ ] `backdrop-filter` (samples scene background za elementem)
- **Pristup**: subtree filter pres FilterBegin/End markery v paint, render
  capture inner do offscreen RT, blur 2-pass + color matrix compose, composit RT
  do swap chain pres scissor.

### CSS Masking L1
- [x] `clip-path: inset()|circle()|ellipse()` - parser + CPU paint apply (rect modify + radius)
- [x] `clip-path: polygon(...)` - ear-clipping triangulace (convex i concave) + emit triangles
- [ ] `mask-image`
- [ ] `mask-mode`, `mask-repeat`, `mask-position`, `mask-size`, `mask-origin`, `mask-clip`
- [ ] `mask-composite`

### CSS Shapes L1
- [ ] `shape-outside: circle()|ellipse()|polygon()|inset()|url()`
- [ ] `shape-margin`
- [ ] `shape-image-threshold`

### CSS Overflow L3
- [x] `overflow`, `overflow-x`, `overflow-y` (clip, scroll - parsing)
- [ ] `overflow-clip-margin`
- [x] `overscroll-behavior` (parser, runtime TODO)
- [x] `scroll-behavior: auto | smooth` (parser, runtime TODO)
- [x] `scroll-snap-type`, `scroll-snap-align`, `scroll-padding`, `scroll-margin` (parser, runtime TODO)
- [x] `scrollbar-width: auto | thin | none` (parser)
- [x] `scrollbar-color: <thumb> <track>` (parser)
- [ ] `scrollbar-gutter`

### CSS Pseudo-Elements L4
- [x] `::before`, `::after` + `content` property (string + attr() + counter() runtime)
- [x] Legacy `:before` / `:after` syntax (CSS2 fallback)
- [x] Specificita kaskady na pseudo-elementech (cascade_pseudo)
- [x] Layout integrace - virtualni pseudo LayoutBox vlozeny pred/po children
- [x] `::first-line`, `::first-letter` (text split + pseudo box prepended)
- [x] `::marker` (list-style markers - 8 stylu disc/circle/square/decimal/decimal-leading-zero/upper-roman/lower-roman/upper-alpha/lower-alpha)
- [ ] `::placeholder`
- [ ] `::file-selector-button`
- [ ] `::backdrop`
- [ ] `::selection`, `::target-text`
- [x] Counter API (`counter-reset`, `counter-increment`, `counter()`) - runtime resolve v ::before/::after content

### CSS Color Adjust L1
- [x] `color-scheme: light | dark | light dark | normal` (parser, ulozeno v LayoutBox.color_scheme)
- [ ] `forced-color-adjust`
- [x] `accent-color: <color>` (parser, ulozeno v LayoutBox.accent_color)

### CSS Anchor Positioning L1 (draft)
- [x] `anchor-name` - layout post-pass collect_anchors map
- [x] `position-anchor` - apply_anchor_positioning lookup
- [/] `anchor()` function - basic positioning
- [ ] `inset-area`

### CSS Scroll-driven Animations L1 (draft)
- [x] `animation-timeline: scroll(<scroller>, <axis>)` - scroll_progress aplikovan misto elapsed
- [ ] `animation-timeline: view()`
- [ ] `scroll-timeline-name`, `view-timeline-name`

### CSS View Transitions L1
- [/] `view-transition-name` (parser)
- [ ] `::view-transition`, `::view-transition-group`, `::view-transition-image-pair`
- [x] `document.startViewTransition()` API (stub - vola callback)

### CSS Houdini (low priority - out of scope)
- [-] CSS Properties and Values API (`@property`)
- [-] CSS Painting API (`paintWorklet`)
- [-] CSS Typed OM
- [-] CSS Layout API (worklet)

---

## Test stranky

Per modul vytvorime `static/css_modules/<modul>/index.html` + `index.css`.

Hlavni `static/test.html` aktualizujeme po batchich (komplexni univerzal).

```
static/css_modules/
  selectors_l4/        :is, :where, :not, :has, ...
  values_l4/           min, max, clamp, ...
  color_l4/            oklch, color-mix, ...
  transitions/         transition + change trigger
  filter/              blur, brightness, ...
  container_queries/   @container
  nesting/             & selector
  ...
```

---

## Performance focus

User pozadavek: vysoky vykon. Pri kazde feature implementaci checkovat:
- Cascade resolution: O(rules x elements)? Cache?
- Layout reflow: incremental, ne always-from-scratch
- Paint: dirty rect tracking, ne full DisplayList per frame
- VBuf: pool/reuse, ne alocate per frame
- Atlas: batch upload, jen dirty regions

Pri review kazdeho commitu zmerit `cargo build --release` + run a sledovat
allocations / frame time.

---

Last updated: 2026-05-04
