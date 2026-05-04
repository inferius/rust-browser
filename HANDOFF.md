# Prechodovy plan - nove vlakno

Cti **driv nez zacnes**. Plus `CLAUDE.md`, `README.md`, `TODO_CSS.md`.

## Stav

- Build: **OK**, 0 errors (par non-fatal warnings z duplicate match arms).
- Tests: **803 passed, 0 failed, 3 ignored** (z 639 puv, +164 v session).
- Posledni commit: `8887329 Batch 6: DOM append/prepend/before/after/...`.
- Tree: ciste.
- Branch master, ~155 commitu pred origin/master.

## Test runner

```bash
powershell -ExecutionPolicy Bypass -File run_tests.ps1   # Win
./run_tests.sh                                            # Linux/Mac
```

## Co bylo posledni session hotovo

**CSS - vsechno krome filter blur RT:**
- Selectors L4, Values L4, Color L4, Logical Properties, Animations L1+L2,
  Nesting, Container Queries, Box-shadow inset, Radial+conic gradients,
  Transitions L1, Filter Effects (parser+CPU render), Pseudo-elements,
  Backgrounds L3 (parser+paint+multi), @font-face (parser+FS runtime+per-text lookup),
  SVG basic shapes, Canvas tag layout, clip-path (parser+CPU render),
  Cascade Layers @layer, text-shadow, @media L4 (prefers-*/hover/pointer/range),
  Math fci L4, text-transform/aspect-ratio, Form pseudo-classes (vc :valid/:invalid),
  Color Adjust + Containment, scroll/scrollbar/scroll-snap, place-* + gap,
  3D transforms parsing + render (single matrix), text-decoration L4, text-indent,
  @scope/@supports/@starting-style/@page/@property parsers,
  Counter API runtime (counter-reset/increment/counter()),
  outline shorthand, list-style-image, font-stretch/-variant/-feature/-variation,
  text-orientation/ruby-position/quotes, mask-image/shape-outside/direction/
  writing-mode/content-visibility, contain-intrinsic-size, will-change,
  isolation, mix-blend-mode, pointer-events, user-select, caret-color,
  resize, touch-action, hyphens, tab-size, word-break, overflow-wrap,
  text-wrap, text-align-last, transform-style, perspective, backface-visibility.

**JS API:**
- HTMLFormElement (action/method/elements/submit() + form data + url_encode + real POST)
- innerHTML/outerHTML getters + setter (parse_html_fragment)
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

**Test runner**: PowerShell + bash skripty.

## TODO (priorita)

### Velke zbyle
1. **Filter blur + drop-shadow render** (RT pipeline) - vyzaduje wgpu offscreen
   render target + 2-pass gauss shader. Posledni velky kus.
2. **Filter na cely subtree** - render-to-texture pipeline pro vsechny filtry
   per CSS spec.
3. **Polygon clip-path render** - shader stencil pipeline.
4. **WebGL real render** - aktualne jen stub. Implementace via wgpu.
5. **3D perspective render** - rotate3d + perspective vyzaduje shader matrix uniform.
6. **HTTP @font-face** - aktualne jen FS load.
7. **Pseudo-elements ::first-line / ::first-letter layout**.
8. **Anchor positioning L1** (Chrome experimental).
9. **Scroll-driven animations**.
10. **View transitions L1**.
11. **Houdini APIs**.
12. **Subgrid L2**.

### Mensi zbyle
- transition events (transitionrun/-start/-end/-cancel dispatch)
- animation events (animationstart/-end/-iteration)
- @import + url(...) layer(name)
- revert / revert-layer / unset / inherit keywords runtime
- direction: rtl runtime per-element layout
- position: sticky runtime
- shape-outside runtime layout
- backdrop-filter render
- mask-image render
- text-emphasis render
- ruby layout
- text-wrap balance/pretty layout

### TypeScript kompilator
**User pozadoval**: vse krome TS. Po kompletu prokonzultujeme.

## Pracovni flow

- Po fici: build + test (run_tests.ps1) + commit
- Commit cesky, ASCII
- Komunikace cesky CAVEMAN MODE

## Klicove soubory

- `src/main.rs` - CLI
- `src/browser/cascade.rs` (~2000) - cascade + animations + transitions + Math L4
  + cascade_pseudo + pseudo-classes (vc form pseudo)
- `src/browser/css_parser.rs` - Stylesheet (selectors L4, nesting, container,
  keyframes, pseudo-elements, @font-face, @layer, @scope/@supports/@starting-style,
  @media range)
- `src/browser/layout.rs` (~3000) - LayoutBox (90+ fields) + parsers
- `src/browser/render.rs` (~1700) - winit+wgpu, GlyphAtlas family lookup,
  ImageAtlas, font_registry, canvas paint_canvas_ops
- `src/browser/paint.rs` - DisplayList (vc CanvasOp + 3D transform aplikace)
- `src/interpreter/mod.rs` (~3900) - Interpreter, JsValue, DomNode dispatch
  (90+ properties + methods)

## Dalsi krok pri pokracovani

User: "vsechno krome TS, pak prokonzultujeme TS".
Zbyle velke:
- **A)** Filter blur RT pipeline (offscreen RT + 2-pass gauss)
- **B)** WebGL real render (wgpu mapping)
- **C)** 3D perspective shader (matrix uniform per-vertex)
- **D)** Filter subtree pipeline
- **E)** Pseudo-elements ::first-line/::first-letter

Po kompletu prohlizec -> TS kompilator design konzultace.
