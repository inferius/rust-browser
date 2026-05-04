# Prechodovy plan - nove vlakno

Cti **driv nez zacnes**. Plus `CLAUDE.md`, `README.md`, `TODO_CSS.md`.

## Stav

- Build: **OK**, 0 warnings.
- Tests: **800 passed, 0 failed, 3 ignored** (z 639 puv, +161 v session).
- Posledni commit: `3032772 Element.matches + closest`.
- Tree: ciste.
- Branch master, ~140 commitu pred origin/master.

## Test runner

```bash
powershell -ExecutionPolicy Bypass -File run_tests.ps1   # Win
./run_tests.sh                                            # Linux/Mac
```

## Co bylo posledni session hotovo

CSS:
- Selectors L4, Values L4, Color L4, Logical Properties, Animations rozsireni,
  Nesting, Container Queries, Box-shadow inset, Radial+conic gradients,
  Transitions L1, Filter Effects (parser+CPU render), Pseudo-elements
  ::before/::after, Backgrounds L3 (parser+paint+multi),
  @font-face (parser+FS runtime+per-text font lookup), SVG basic shapes,
  Canvas tag layout, clip-path (parser+CPU render), Cascade Layers @layer,
  text-shadow, @media L4 (prefers-*/hover/pointer), Math fci L4,
  text-transform/aspect-ratio, Form pseudo-classes, Color Adjust + Containment,
  scroll/scrollbar properties, place-* + gap, scroll-snap parser,
  3D transforms parsing, transform chain, text-decoration L4, text-indent

JS API:
- HTMLFormElement (action/method/elements/submit() + form data + url_encode)
- innerHTML / outerHTML getters
- font-family parser + GlyphAtlas refactor (per-text font lookup)
- Canvas API JS bindings + paths (fillRect/strokeRect/clearRect/fillText/
  beginPath/moveTo/lineTo/arc/closePath/stroke/fill) + render emit
- HTMLElement.style.setProperty/getPropertyValue/removeProperty
- Element.classList (add/remove/toggle/contains)
- Element.dataset (data-* atributy, kebab->camelCase)
- Element.matches(selector), Element.closest(selector)

Test runner skripty (run_tests.ps1 + .sh).

## TODO (priorita shora dolu)

### Velke
1. **Filter blur + drop-shadow render** - 2-pass gauss + offscreen RT
2. **Filter na cely subtree** - render-to-texture pipeline
3. **Polygon clip-path** - shader stencil pipeline
4. **3D transform render pipeline** - perspective + matrix multiply
5. **Form submit fetch POST** - aktualne jen log, real POST pres ureq
6. **innerHTML setter** - HTML parser + DOM mutation
7. **Pseudo-elements ::first-line / ::first-letter layout**
8. **Counter API** (counter-reset/-increment/counter())
9. **WebGL** - po Canvas, vlastni GL context emulace
10. **HTTP @font-face load** - aktualne jen FS

### Mensi
- :valid/:invalid (form validation)
- @scope (Cascade L6)
- @starting-style
- @media range syntax (400px <= width <= 800px)
- contain-intrinsic-size, content-visibility
- @import + url(...) layer(name)
- revert / revert-layer / unset
- transition events (transitionrun/-start/-end/-cancel)
- animation-composition L2, animation-timeline L2
- Subgrid L2
- text-emphasis, line-break, text-justify
- mask-image / mask-mode
- shape-outside
- direction: rtl + writing-mode runtime
- Anchor positioning L1 (Chrome experimental)
- Scroll-driven animations
- View transitions L1
- Houdini APIs

### TypeScript kompilator
**User pozadoval**: dotahnu prohlizec, pak prokonzultujem.

Otazky:
- Scope: full TSC superset vs subset
- Type checking vs jen strip types -> JS
- Integrace s lexer/parser
- Vystup: JS string vs primy AST

## Pracovni flow

- Po fici: build + test (run_tests.ps1) + commit
- Commit cesky, ASCII, "co + proc"
- Pri nejasnosti: zeptat se A/B/C
- Komunikace cesky CAVEMAN MODE
- CSS modul: testy + static/css_modules/<name>/
- Aktualizovat TODO_CSS.md

## Klicove soubory

- `src/main.rs` - CLI rezimy
- `src/browser/cascade.rs` (~2000 lines) - cascade + animations + transitions
- `src/browser/css_parser.rs` - CSS -> Stylesheet (pub parse_selectors)
- `src/browser/layout.rs` (~2700 lines) - LayoutBox + parsers + 3D transforms
- `src/browser/render.rs` (~1700 lines) - winit + wgpu, GlyphAtlas s family
  lookup, ImageAtlas, font_registry, canvas paint_canvas_ops
- `src/browser/paint.rs` - DisplayList emit + CanvasOp enum
- `src/interpreter/mod.rs` (~3700 lines) - Interpreter, JsValue, DomNode
  property dispatch, style/classList/dataset/canvas/form helpers
- `static/test.html` + .css - hlavni test page
- `static/css_modules/<modul>/` - 19+ test stranek

## Dalsi krok pri pokracovani

User: "vsechno, pokracuj. Komplet prohlizec, pak TypeScript".
Doporucene:
- **A)** Filter blur RT pipeline (offscreen RT, multi-pass gauss)
- **B)** 3D transform render pipeline (perspective + matrix)
- **C)** innerHTML setter (HTML parser + DOM mutation)
- **D)** Counter API (counter-reset/increment/counter())
- **E)** Form submit real fetch POST
- **F)** TypeScript kompilator design konzultace

Pri nejasnosti zeptat se.
