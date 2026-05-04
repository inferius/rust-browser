# Prechodovy plan - nove vlakno

Cti **driv nez zacnes**. Plus `CLAUDE.md`, `README.md`, `TODO_CSS.md`.

## Stav

- Build: **OK**, 0 errors (par non-fatal duplicate match arms warnings).
- Tests: **805 passed, 0 failed, 3 ignored** (z 639 puv, +166 v session).
- Posledni commit: `98d0c5a Batch 13: Storage + Headers + navigator`.
- Tree: ciste.
- Branch master, ~170 commitu pred origin/master.

## Test runner

```bash
powershell -ExecutionPolicy Bypass -File run_tests.ps1   # Win
./run_tests.sh                                            # Linux/Mac
```

## Co bylo posledni session hotovo

**CSS - vsechno krome filter blur RT:**
- Selectors L4, Values L4 (vc Math L4 round/sqrt/sin/cos/pow/hypot/...),
  Color L4 (oklch/oklab/lab/lch/hsl/hwb/color-mix), Logical Properties,
  Animations L1+L2 (fill-mode/play-state/cubic-bezier/steps),
  Nesting, Container Queries, Box-shadow inset, Radial+conic gradients,
  Transitions L1, Filter Effects (parser+CPU render), Pseudo-elements
  ::before/::after (vc Counter API runtime), Backgrounds L3 (vc multi-layer),
  @font-face (parser+FS runtime+per-text lookup), SVG basic shapes,
  Canvas tag layout, clip-path (parser+CPU render), Cascade Layers @layer,
  text-shadow, @media L4 (prefers-*/hover/pointer/range syntax),
  Form pseudo-classes (vc :valid/:invalid/:default), Color Adjust + Containment,
  scroll/scrollbar/scroll-snap, place-* + gap, scroll-snap parser,
  3D transforms parsing + render (single matrix + chain),
  text-decoration L4, text-indent, text-transform, aspect-ratio,
  @scope/@supports/@starting-style/@page/@property/@import/@namespace/
  @counter-style/@font-feature-values/@document/@view-transition parsers,
  Anchor Positioning L1 parser (anchor-name/position-anchor/inset-area),
  Scroll-driven anims parser (animation-timeline/scroll-timeline-*/view-timeline-*),
  View Transitions parser (view-transition-name),
  Subgrid L2 (Display enum + Grid fallback),
  outline shorthand, list-style-image, font-stretch/-variant/-feature/
  -variation/-display/-kerning/-language-override/-optical-sizing,
  text-orientation/ruby-position/quotes,
  mask-image/shape-outside/direction/writing-mode/content-visibility,
  contain-intrinsic-size, will-change, isolation, mix-blend-mode,
  pointer-events, user-select, caret-color, resize, touch-action,
  hyphens, tab-size, word-break, overflow-wrap, text-wrap,
  text-align-last, transform-style, perspective, backface-visibility,
  page-break-*/break-*/orphans/widows, counter-set,
  print-color-adjust/forced-color-adjust, math-style/math-depth,
  speak/speak-as, bookmark-*/string-set, float/clear,
  object-fit/object-position, background-blend-mode, image-rendering,
  table-layout/border-collapse/border-spacing/caption-side/empty-cells,
  vertical-align, Display extensions (Contents/ListItem/Table*/Inline*/
  Subgrid/Ruby).

**JS API:**
- HTMLFormElement (action/method/elements/submit() + form data + url_encode + real POST)
- innerHTML/outerHTML (getters + setter pres parse_html_fragment)
- font-family parser + GlyphAtlas refactor (per-text font lookup)
- Canvas API JS (getContext + 2D + paths: beginPath/moveTo/lineTo/arc/stroke/fill)
- HTMLElement.style (setProperty/getPropertyValue/removeProperty)
- Element.classList (add/remove/toggle/contains)
- Element.dataset (data-* + kebab->camelCase)
- Element.matches(sel), Element.closest(sel)
- WebGL stub (canvas.getContext('webgl') - constants + 40+ no-op methods)
- HTMLImageElement (naturalWidth/-Height/complete)
- offsetWidth/-Height/clientWidth/-Height/scrollWidth/-Height
- hidden/contentEditable/draggable/tabIndex
- getBoundingClientRect()
- toggleAttribute, cloneNode, contains
- append/prepend/before/after/replaceWith/remove
- insertAdjacentHTML
- HTMLDialogElement (show/showModal/close), HTMLDetailsElement.open
- HTMLMediaElement (play/pause/load/currentTime/duration/paused/muted/volume)
- HTMLInputElement (select/setSelectionRange/checkValidity/...)
- ResizeObserver / IntersectionObserver / MutationObserver / PerformanceObserver stuby
- requestAnimationFrame / cancelAnimationFrame / queueMicrotask
- customElements registry (define/get/whenDefined/upgrade)
- new CSSStyleSheet() stub
- new URL("...") - real parsing s host/port/origin
- new URLSearchParams - get/set/has/append/toString
- localStorage / sessionStorage (in-memory + length update)
- new Headers() - get/set/append/has/delete (case-insensitive)
- navigator (userAgent/language/platform/onLine/cookieEnabled/
  hardwareConcurrency/geolocation/clipboard)

**Test runner**: PowerShell + bash skripty.

## TODO (priorita zbyle)

### Velke (vyzaduji wgpu RT pipeline / shader uniformy)
1. **Filter blur + drop-shadow render** (RT pipeline) - 2-pass gauss + offscreen RT
2. **Filter na cely subtree** - render-to-texture pipeline
3. **3D perspective shader** - rotate3d s matrix uniform per-vertex
4. **Polygon clip-path render** - shader stencil
5. **WebGL real render** - aktualne jen stub
6. **HTTP @font-face** - aktualne jen FS

### Menstruvalo
- Pseudo-elements ::first-line / ::first-letter layout
- transition / animation events dispatch (pri end emit DOM event)
- Houdini APIs (Paint/Layout/Properties)
- Anchor positioning runtime layout
- Scroll-driven animations runtime
- View transitions runtime
- text-decoration render style (wavy/dashed/double)
- mask-image render
- backdrop-filter render
- shape-outside runtime layout
- direction: rtl runtime
- position: sticky runtime
- table layout (real table-layout: fixed)
- Real custom elements upgrade (callback at element instantiation)
- HTML parser fragment (proxylepsi cleanup)

### TypeScript kompilator
**User pozadoval**: po kompletu prokonzultujeme.

## Pracovni flow

- Po fici: build + test (run_tests.ps1) + commit
- Commit cesky, ASCII, "co + proc"
- Komunikace cesky CAVEMAN MODE

## Klicove soubory

- `src/main.rs` - CLI
- `src/browser/cascade.rs` (~2100) - cascade + animations + transitions + Math L4
  + cascade_pseudo + form pseudo + Anchor positioning eval
- `src/browser/css_parser.rs` - Stylesheet (vsechny at-rules + range queries)
- `src/browser/layout.rs` (~3300) - LayoutBox (140+ fields) + parsers
- `src/browser/render.rs` (~1700) - winit+wgpu, GlyphAtlas family lookup,
  ImageAtlas, font_registry, canvas paint_canvas_ops
- `src/browser/paint.rs` - DisplayList (vc CanvasOp + 3D transform aplikace)
- `src/interpreter/mod.rs` (~4100) - Interpreter, JsValue, DomNode dispatch
  (130+ properties + methods)
- `src/interpreter/builtins.rs` (~2300) - globals (Math/JSON/Date/Intl/
  fetch/Worker/setTimeout/setInterval/raf/localStorage/sessionStorage/
  customElements/URL/URLSearchParams/Headers/navigator/observers/...)

## Dalsi krok pri pokracovani

User: "vsechno krome TS, pak prokonzultujeme TS".
Zbyle velke (RT/shader heavy):
- **A)** Filter blur RT pipeline (offscreen RT + 2-pass gauss)
- **B)** 3D perspective shader (matrix uniform per-vertex)
- **C)** Filter subtree pipeline
- **D)** Polygon clip-path shader
- **E)** WebGL real render (wgpu mapping)
- **F)** Pseudo-elements ::first-line/::first-letter layout
- **G)** transition/animation events dispatch
- **H)** TypeScript kompilator design konzultace
