# Prechodovy plan - nove vlakno

Cti **driv nez zacnes**. Plus `CLAUDE.md`, `README.md`, `TODO_CSS.md`.

## Stav

- Build: **OK**, 0 warnings.
- Tests: **788 passed, 0 failed, 3 ignored** (z 639 puv, +149 v session).
- Posledni commit: `617556c innerHTML / outerHTML getters`.
- Tree: ciste.
- Branch master, ~125 commitu pred origin/master.

## Test runner

```bash
powershell -ExecutionPolicy Bypass -File run_tests.ps1   # Win
./run_tests.sh                                            # Linux/Mac
```

## Co bylo posledni session hotovo

Velky CSS feature stream + JS API rozsireni:

CSS Selectors L4, Values L4, Color L4, Logical Properties, Animations rozsireni,
Nesting, Container Queries, Box-shadow inset, Radial+conic gradients,
Transitions L1, Filter Effects (parser+CPU render), Pseudo-elements ::before/::after,
Backgrounds L3 (parser+paint+multi), @font-face (parser+FS runtime),
SVG basic shapes, Canvas tag layout, clip-path (parser+CPU render),
Cascade Layers @layer, text-shadow, @media L4 (prefers-*/hover/pointer),
Math fci L4 (round/sqrt/sin/cos/pow/hypot...), text-transform/aspect-ratio,
Form pseudo-classes (:required/:disabled/:checked/:read-only/:placeholder-shown),
Color Adjust + Containment, scroll/scrollbar properties,
place-* + gap shorthandy, scroll-snap parser,
3D transforms parsing (translate3d/rotate3d/scale3d/matrix3d/perspective),
transform chain, text-decoration L4, text-indent,
HTMLFormElement props (action/method/elements), innerHTML/outerHTML,
font-family parser.

Test runner skripty (run_tests.ps1 + .sh).

## TODO (priorita shora dolu)

### Velke
1. **Filter blur + drop-shadow render** - 2-pass gauss + offscreen RT
2. **Filter na cely subtree** - render-to-texture pipeline
3. **Polygon clip-path** - shader stencil pipeline
4. **Canvas API JS bindings** - canvas.getContext('2d') + 2D methods
5. **Per-text font lookup z registry** - GlyphAtlas refactor (family, char, size)
6. **Form submit() method** + form data POST
7. **3D transform render pipeline** - perspective + 3D matrix multiply
8. **innerHTML setter** - HTML parser + DOM mutation
9. **WebGL**
10. **Pseudo-elements ::first-line / ::first-letter layout**
11. **Counter API**
12. **Anchor positioning L1** (Chrome experimental)
13. **Scroll-driven animations**
14. **View transitions L1**
15. **Houdini APIs**

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
- dataset property (data-* attributes)
- classList (add/remove/toggle/contains)
- HTMLElement.style.setProperty/getPropertyValue
- text-decoration render (style wavy/dashed/double)

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
- `src/browser/cascade.rs` (~2000 lines) - cascade + animations + transitions +
  Logical + Values L4 (math fci) + cascade_pseudo + apply_*
- `src/browser/css_parser.rs` - CSS -> Stylesheet (selectors L4, nesting,
  container queries, keyframes, pseudo-elements, @font-face, @layer)
- `src/browser/layout.rs` (~2700 lines) - LayoutBox + parsers (color L4,
  gradient *, filter, shadow, clip-path, transform chain 3D, BgLayer,
  text-decoration L4, scroll-snap)
- `src/browser/render.rs` (~1500 lines) - winit + wgpu, App, frame loop,
  image atlas, font_registry, WGSL 8 modu shader
- `src/browser/paint.rs` - DisplayList emit (filter, pseudo, SVG, multi-bg,
  clip-path apply, text-shadow, text-transform)
- `src/interpreter/mod.rs` (~3500 lines) - Interpreter, JsValue, DomNode
  property dispatch (form props, innerHTML/outerHTML)
- `src/interpreter/builtins.rs` (>2000 lines)
- `static/test.html` + .css - hlavni test page
- `static/css_modules/<modul>/` - 17+ per-feature stranek

## Co necist hned (velke soubory)

- `src/interpreter/builtins.rs` (>2000)
- `src/browser/cascade.rs` (~2000)
- `src/browser/layout.rs` (~2700)
- `src/browser/render.rs` (~1500)
- `src/interpreter/mod.rs` (~3500)
- `src/debug_view/devtools.rs` (>500)

## Dalsi krok pri pokracovani

User: "vsechno, pokracuj. Komplet prohlizec, pak TypeScript".
Doporucene volby:
- **A)** Filter blur + RT pipeline
- **B)** Canvas API JS bindings
- **C)** Per-text font lookup (GlyphAtlas refactor)
- **D)** 3D transform render pipeline
- **E)** form submit() + form data POST

Pri nejasnosti zeptat se.
