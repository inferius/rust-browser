# Prechodovy plan - nove vlakno

Toto cti **driv nez zacnes**. Plus `CLAUDE.md`, `README.md` a `TODO_CSS.md`.

## Stav

- Build: **OK** (cargo build cisty, 0 warnings).
- Tests: **774 passed, 0 failed, 3 ignored** (z 639 puvodnich, +135 v session).
- Posledni commit: `f91788d scroll-snap parser`.
- Working tree: ciste.
- Branch master, ~110 commitu pred origin/master (nepushovano).

## Test runner

```bash
# Windows
powershell -ExecutionPolicy Bypass -File run_tests.ps1

# Linux/Mac
./run_tests.sh
```

Vystup: `test_logs/test-<ts>.log` + `test_logs/failures-<ts>.log`.
Exit 0 OK, exit 1 fail.

## Co bylo posledni session hotovo

Po inicialnim Console+Network capture + GPU image atlas + Animation runtime + README,
sled CSS feature batches + dalsich:

CSS Selectors L4, Values L4, Color L4, Logical Properties, Animations rozsireni,
Nesting, Container Queries, Box-shadow inset, Radial+conic gradients,
Transitions L1, Filter Effects, Pseudo-elements ::before/::after,
Backgrounds L3 parser+paint+multi, @font-face parser, SVG basic shapes,
Canvas tag, clip-path parser, Cascade Layers @layer, text-shadow,
@media L4 prefers-*, Math fci L4, text-transform/aspect-ratio,
Form pseudo-classes, Color Adjust + Containment, scroll/scrollbar properties,
place-* + gap shorthandy, scroll-snap parser.

Test runner skripty (run_tests.ps1 + .sh).

## TODO (priorita shora dolu)

### Velke (vyzaduji vetsi prace)
1. **Filter blur + drop-shadow render** - vyzaduje 2-pass gauss + offscreen RT
2. **Filter na cely subtree** - vyzaduje render-to-texture pipeline (ne single element)
3. **clip-path render shader** - SDF mode + per-fragment polygon test
4. **Canvas API JS bindings** - `<canvas>` tag uz layout, getContext('2d') + 2D
   methods (fillRect, fillText, beginPath, drawImage, ...) - velky modul
5. **@font-face runtime** - load font + fontdue Font registry per family
6. **Form submit + JS API** - form.submit(), form.elements, submit event,
   URL-encode + ureq POST
7. **WebGL** - po Canvas. wgpu-based GL context emulation
8. **Pseudo-elements ::first-line / ::first-letter layout integrace**
9. **Counter API** - counter-reset, counter-increment, counter()
10. **Anchor positioning L1** (Chrome experimental)
11. **Scroll-driven animations** (Chrome experimental)
12. **View transitions L1**
13. **Houdini APIs** (Paint/Layout/Properties)

### Mensi
- :valid / :invalid (form validation)
- @scope (Cascade L6)
- @starting-style
- range syntax @media (400px <= width <= 800px)
- contain-intrinsic-size, content-visibility
- @import + url(...) layer(name)
- revert / revert-layer / unset keywords
- transition events (transitionrun/-start/-end/-cancel)
- animation-composition L2, animation-timeline L2
- Subgrid L2 (taffy-side)
- Transforms L2: 3D (perspective, transform-style)
- Asymetric border-radius (`/` syntax)
- mask-image / mask-mode (CSS Masking)
- shape-outside (CSS Shapes)
- direction: rtl + writing-mode runtime per-element

### TypeScript kompilator
**User pozadoval**: dotahnu prohlizec, pak prokonzultujem.
- Otazky:
  - Scope: full TSC superset vs subset
  - Type checking vs jen strip types -> JS
  - Integrace s lexer/parser nebo vlastni front-end
  - Vystup: JS string vs primy AST

## Pracovni flow

- Po kazde fici: **build + test (run_tests.ps1) + commit**.
- Commit message cesky, ASCII, strucny popis "co + proc".
- Pred psanim kodu pri nejasnosti se ptat (A/B/C varianty).
- Komunikace cesky, CAVEMAN MODE aktivni (terse), kod normalne.
- Ke kazdemu CSS modulu: unit testy + test stranky v `static/css_modules/`.
- Aktualizovat TODO_CSS.md checkboxy.

## Klicove soubory

- `src/main.rs` - CLI rezimy.
- `src/browser/cascade.rs` (~1700 lines) - cascade + animations + transitions
  + Logical + Values L4 (vc. math fci) + cascade_pseudo + AnimationSpec/
  TransitionSpec/ActiveTransition + apply_transitions/animations + @media.
- `src/browser/css_parser.rs` - CSS -> Stylesheet, vc. selectors L4 + nesting +
  container queries + keyframes + pseudo-elements + @font-face + cascade layers.
- `src/browser/layout.rs` (~2200 lines) - LayoutBox + parse_color (Color L4)
  + parse_*_gradient + parse_filter_chain + apply_filter_chain + parse_box_shadow
  + parse_text_shadow + parse_clip_path + parse_length + BgLayer + TextTransform
  + Containment + Color-adjust + scroll-snap.
- `src/browser/render.rs` (~1500 lines) - winit + wgpu, App struct, frame loop
  + transitions state diff, image atlas, WGSL shader (8 modu).
- `src/browser/paint.rs` - DisplayList emission z LayoutBox (filter aplikace,
  pseudo box, SVG shapes, multi-layer backgrounds, text-shadow, text-transform).
- `static/test.html` + `static/test.css` - hlavni testovaci stranka (rozsirena).
- `static/css_modules/<modul>/` - 17+ per-feature test stranek.

## Co necist hned (velke soubory)

- `src/interpreter/builtins.rs` (>2000 lines).
- `src/browser/cascade.rs` (~1700 lines).
- `src/browser/layout.rs` (~2200 lines).
- `src/browser/render.rs` (~1500 lines).
- `src/debug_view/devtools.rs` (>500 lines).

## Dalsi krok pri pokracovani

User pozadoval: "Vsechno - pokracuj. Komplet prohlizec, pak TypeScript".
Doporucene volby:
- **A)** Filter blur + drop-shadow (RT pipeline)
- **B)** Canvas API JS bindings (getContext + methods)
- **C)** @font-face runtime (Font registry)
- **D)** Form submit + JS form API
- **E)** clip-path render shader

Pri nejasnosti zeptat se A/B/C/D/E.
