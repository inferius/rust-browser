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
- [ ] Backgrounds L3: multiple backgrounds, `background-clip`, `background-origin`, `background-attachment`
- [x] Animations L1: `animation-fill-mode` (none/forwards/backwards/both), `animation-play-state` (running/paused), arbitrary `cubic-bezier(...)`, `steps(n, jump-*)`

### Batch 2 - chybejici "must-have" moduly
- [ ] CSS Transitions L1 (cely modul)
- [x] CSS Logical Properties L1 (`margin/padding-block/-inline`, `border-*-block/-inline-*`, `inset-*`, `block-size/inline-size`, `border-start-end-radius` rohy) - mapovani LTR + horizontal-tb
- [x] CSS Nesting L1 (`&` selector + nested rulesets, implicit descendant pri ne-amp prefix, kombinace `.parent.nested` pres `&`)
- [ ] CSS Container Queries L1 (`@container`, `cqw`, `cqh`)
- [ ] CSS Filter Effects L1 (`filter: blur/brightness/...`)

### Batch 3 - dalsi
- [ ] @font-face (CSS Fonts L4)
- [ ] Cascade Layers (`@layer`)
- [ ] Position L3: `sticky`
- [ ] Masking L1: `clip-path`
- [ ] Subgrid L2
- [ ] Shapes L1: `shape-outside`
- [ ] Transforms L2: 3D (`perspective`, `transform-style`)

### Batch 4 - exoticke / draft
- [ ] Anchor Positioning L1
- [ ] Scroll-driven Animations L1
- [ ] View Transitions L1
- [ ] Houdini (Paint/Layout/Properties API)
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
- [ ] `:placeholder-shown`, `:read-only`, `:read-write`, `:required`, `:optional`, `:valid`, `:invalid`
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
- [ ] Math fci L4: `round()`, `mod()`, `rem()`, `sin()`, `cos()`, `tan()`, `asin()`, `acos()`, `atan()`, `atan2()`, `pow()`, `sqrt()`, `hypot()`, `log()`, `exp()`, `abs()`, `sign()`
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
- [ ] `aspect-ratio`
- [ ] `min-content`, `max-content`, `fit-content` keywords

### CSS Backgrounds & Borders L3/L4
- [x] `background-color`
- [x] `background-image: linear-gradient(...)`
- [x] `background-image: url(...)`
- [x] `border-width`, `border-style`, `border-color`, `border-radius`
- [x] `box-shadow` (drop)
- [ ] Multiple backgrounds (kazdy oddelen carkou)
- [ ] `background-position`
- [ ] `background-size: cover|contain|<length>`
- [ ] `background-repeat`
- [ ] `background-attachment: scroll|fixed|local`
- [ ] `background-clip: border-box|padding-box|content-box|text`
- [ ] `background-origin`
- [ ] `background-blend-mode`
- [ ] `border-image-*` (L4)
- [ ] `box-shadow inset` varianta
- [ ] Asymetricke `border-radius` (`/` syntax)

### CSS Fonts L4/L5
- [x] `font-family`, `font-size`, `font-weight`, `font-style`
- [x] `font` shorthand (parsing)
- [ ] `@font-face { font-family; src: url(); }`
- [ ] `font-display: auto|block|swap|fallback|optional`
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
- [ ] `text-indent`
- [ ] `text-transform: uppercase|lowercase|capitalize|none`
- [ ] `letter-spacing`
- [ ] `word-spacing`
- [ ] `tab-size`
- [ ] `word-break: normal|break-all|keep-all`
- [ ] `overflow-wrap` / `word-wrap`
- [ ] `hyphens`
- [ ] `text-wrap: wrap|nowrap|balance|pretty` (L4)
- [ ] `line-break`
- [ ] `text-justify`

### CSS Text Decoration L3/L4
- [x] `text-decoration: underline|overline|line-through`
- [ ] `text-decoration-color`
- [ ] `text-decoration-style: solid|double|dotted|dashed|wavy`
- [ ] `text-decoration-thickness`
- [ ] `text-underline-offset`
- [ ] `text-shadow`
- [ ] `text-emphasis`

### CSS Flexbox L1
- [x] `display: flex`, `flex-direction`, `flex-wrap`, `justify-content`, `align-items`, `align-self`, `gap`, `flex` shorthand (pres taffy)
- [x] `flex-grow`, `flex-shrink`, `flex-basis`
- [x] `align-content`
- [ ] `place-content`, `place-items`, `place-self` shorthandy
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
- [ ] 3D transformy: `translate3d`, `rotate3d`, `scale3d`, `matrix3d`, `perspective()`
- [ ] `perspective` property + `perspective-origin`
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
- [ ] `transition-property`
- [ ] `transition-duration`
- [ ] `transition-timing-function`
- [ ] `transition-delay`
- [ ] `transition` shorthand
- [ ] `transitionrun`/`transitionstart`/`transitionend`/`transitioncancel` events
- [ ] State diff detection (interpoluje pri zmene stylu)

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
- [ ] `@layer name { ... }` cascade layers
- [ ] `@import url(...) layer(name);`
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
- [ ] `contain: layout | paint | size | style | content | strict`
- [ ] `contain-intrinsic-size`
- [ ] `content-visibility: auto | hidden | visible`

### CSS Container Queries L1
- [ ] `container-type: normal | inline-size | size`
- [ ] `container-name`
- [ ] `container` shorthand
- [ ] `@container [name] (condition) { rules }`
- [ ] `cqw`, `cqh`, `cqi`, `cqb`, `cqmin`, `cqmax` jednotky
- [ ] Style queries `@container style(--name: val)`

### CSS Nesting L1
- [x] `&` selector
- [ ] Nested at-rules (`@media`, `@supports` uvnitr ruleset)
- [x] Implicit descendant pri ne-amp prefix (`tag`, `.class`)

### CSS @media L4/L5
- [x] `@media (max-width: ...)`, `@media (min-width: ...)`
- [x] `@media screen`, `@media print` (parsing)
- [ ] `@media (prefers-color-scheme: dark)`
- [ ] `@media (prefers-reduced-motion)`
- [ ] `@media (hover: hover|none)`
- [ ] `@media (pointer: coarse|fine|none)`
- [ ] Range syntax: `@media (400px <= width <= 800px)` (L4)
- [ ] `@media (display-mode: ...)`
- [ ] `@media (forced-colors: active)`

### CSS Filter Effects L1
- [ ] `filter: blur(<r>)`
- [ ] `filter: brightness(<n>)`
- [ ] `filter: contrast(<n>)`
- [ ] `filter: grayscale(<%>)`
- [ ] `filter: hue-rotate(<deg>)`
- [ ] `filter: invert(<n>)`
- [ ] `filter: opacity(<%>)`
- [ ] `filter: saturate(<n>)`
- [ ] `filter: sepia(<%>)`
- [ ] `filter: drop-shadow(...)`
- [ ] Vice filtru chained: `filter: blur(2px) brightness(1.2)`
- [ ] `backdrop-filter`

### CSS Masking L1
- [ ] `clip-path: inset()|circle()|ellipse()|polygon()|path()|url()`
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
- [ ] `overscroll-behavior`
- [ ] `scroll-behavior: smooth`
- [ ] `scroll-snap-type`, `scroll-snap-align`, `scroll-padding`, `scroll-margin`
- [ ] `scrollbar-width`, `scrollbar-color`, `scrollbar-gutter`

### CSS Pseudo-Elements L4
- [ ] `::before`, `::after` + `content` property
- [ ] `::first-line`, `::first-letter`
- [ ] `::marker` (list-style)
- [ ] `::placeholder`
- [ ] `::file-selector-button`
- [ ] `::backdrop`
- [ ] `::selection`, `::target-text`

### CSS Color Adjust L1
- [ ] `color-scheme: light | dark | light dark`
- [ ] `forced-color-adjust`
- [ ] `accent-color`

### CSS Anchor Positioning L1 (draft)
- [ ] `anchor-name`
- [ ] `position-anchor`
- [ ] `anchor()` function
- [ ] `inset-area`

### CSS Scroll-driven Animations L1 (draft)
- [ ] `animation-timeline: scroll(<scroller>, <axis>)`
- [ ] `animation-timeline: view()`
- [ ] `scroll-timeline-name`, `view-timeline-name`

### CSS View Transitions L1
- [ ] `view-transition-name`
- [ ] `::view-transition`, `::view-transition-group`, `::view-transition-image-pair`
- [ ] `document.startViewTransition()` API

### CSS Houdini (low priority)
- [ ] CSS Properties and Values API (`@property`)
- [ ] CSS Painting API (`paintWorklet`)
- [ ] CSS Typed OM
- [ ] CSS Layout API (worklet)

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
