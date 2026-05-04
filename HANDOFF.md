# Prechodovy plan - nove vlakno

Toto cti **driv nez zacnes**. Plus `CLAUDE.md`, `README.md` a `TODO_CSS.md`.

## Stav

- Build: **OK** (cargo build cisty, 0 warnings).
- Tests: **729 passed, 0 failed, 3 ignored**.
- Posledni commit: `3d3ef3c Backgrounds L3 - parser + struct + 8 testu`.
- Working tree: ciste.
- Branch master, ~85 commitu pred origin/master (nepushovano).

## Test runner

```bash
# Windows
powershell -ExecutionPolicy Bypass -File run_tests.ps1

# Linux/Mac
./run_tests.sh
```

Vystup: `test_logs/test-<ts>.log` + `test_logs/failures-<ts>.log`.
Exit 0 OK, exit 1 fail. Failure log obsahuje "---- name stdout ----" bloky
+ panicked at + assertion lines.

## Co bylo posledni session hotovo

Po inicialnim Console+Network capture + GPU image atlas + Animation runtime + README:

CSS feature batches (kazdy: parser + cascade/layout/paint/render kde aplikovatelne,
unit testy, test stranky v `static/css_modules/<modul>/`):

1. Selectors L4 (8 testu)
2. Values L4: min/max/clamp/env (9 testu)
3. Color L4: oklch/oklab/lab/lch/hsl/hwb/color-mix/modern rgb (11 testu)
4. Logical Properties L1 (5 testu)
5. Animations L1 rozsireni: fill-mode/play-state/cubic-bezier/steps (4 testy)
6. Nesting L1 (4 testy)
7. Container Queries L1 (5 testu)
8. Box-shadow inset (1 test)
9. Radial + conic gradients (5 testu)
10. Transitions L1: parser + state diff + per-frame interpolace (6 testu)
11. Filter Effects L1 - parser only (5 testu)
12. **Test runner (PowerShell + bash)**
13. **C: Pseudo-Elements ::before / ::after** (7 testu) - cascade + layout
14. **A: Filter Effects render** (5 testu) - CPU color matrix
15. **B: Backgrounds L3** (8 testu) - parser + BgLayer struct

## TODO (priorita shora dolu)

### Velke
1. **Backgrounds L3 - paint integrace** - position/size/repeat aplikace
   pri image render + multiple bg (comma-separated layers).
2. **Canvas API** - `<canvas>` tag + getContext('2d'). 2D context metody:
   fillRect, fillText, beginPath, moveTo, lineTo, stroke, fill, arc, drawImage.
   Vlastni display list per-canvas -> render to texture.
3. **CSS clip-path** - inset()/circle()/ellipse()/polygon() - SDF clipping.
4. **@font-face** - parser, font fetch (HTTP/FS), pridat do fontdue Font
   registry (multi-font support).
5. **SVG support** - shapes (rect, circle, path) -> display list.
6. **Filter blur + drop-shadow** - vyzaduje render-to-texture pipeline
   (offscreen RT + composit).
7. **WebGL** - po Canvas. Vlastni GL context simulating cez wgpu.
8. **Form submit handling** - submit event + URL encode + ureq POST.

### Mensi
- Cascade Layers `@layer` (parser + priority).
- Position sticky.
- ::first-line / ::first-letter pseudo-elementy (layout integrace).
- Counter API (`counter-reset`, `counter-increment`, `counter()`).
- text-decoration L4 (text-shadow, wavy/dashed/double styles).
- Overflow L3 (scroll-snap, overscroll-behavior).
- @import / @supports parser.
- :focus-visible / :focus-within runtime stav.
- Subgrid L2.
- Transforms L2 - 3D (perspective, transform-style).
- Masking - mask-image, mask-mode.
- @media: prefers-color-scheme, prefers-reduced-motion, hover, pointer.
- 'attr()' v ne-content kontextech (length, color values).

### TypeScript kompilator
**User pozadoval**: dotahnu CSS, pak prokonzultujem design.
- Otazky pred implementaci:
  - Scope: full TSC superset vs subset
  - Type checking vs jen strip types -> JS
  - Integrace s lexer/parser (extend nebo vlastni front-end)
  - Vystup: JS string vs primy AST

## Pracovni flow

- Po kazde fici: **build + test (run_tests.ps1) + commit**.
- Commit message cesky, ASCII, strucny popis "co + proc".
- Pred psanim kodu pri nejasnosti se ptat (A/B/C varianty).
- Komunikace cesky, CAVEMAN MODE aktivni (terse), kod normalne.
- Ke kazdemu CSS modulu: unit testy + static/css_modules/<modul>/ test stranka.
- Aktualizovat TODO_CSS.md checkboxy.

## Klicove soubory

- `src/main.rs` - CLI rezimy.
- `src/browser/cascade.rs` - cascade + animations + transitions + Logical
  + Values L4 resolver + cascade_pseudo + AnimationSpec/TransitionSpec/ActiveTransition
  + apply_transitions + apply_animations.
- `src/browser/css_parser.rs` - CSS -> Stylesheet, vc. selectors L4 + nesting +
  container queries + keyframes + pseudo-elements.
- `src/browser/layout.rs` - box model + parse_color (vc. Color L4) +
  parse_*_gradient + parse_filter_chain + apply_filter_chain + parse_box_shadow +
  parse_length + BgLayer/BgPosition/BgSize/BgRepeat parsers + LayoutBox + build_pseudo_box.
- `src/browser/render.rs` - winit + wgpu, App struct, frame loop (+ transitions
  state diff), image atlas, WGSL shader (8 modu: 0 solid, 1 text, 2 linear gradient,
  3 shadow, 4 image, 5 inset shadow, 6 radial gradient, 7 conic gradient).
- `src/browser/paint.rs` - DisplayList emission z LayoutBox (filter aplikace).
- `static/test.html` + `static/test.css` - hlavni testovaci stranka.
- `static/css_modules/<modul>/` - 14 per-feature test stranek.

## Co necist hned (velke soubory)

- `src/interpreter/builtins.rs` (>2000 lines).
- `src/browser/cascade.rs` (~1500 lines).
- `src/browser/layout.rs` (~2000 lines).
- `src/browser/render.rs` (~1400 lines).
- `src/debug_view/devtools.rs` (>500 lines).

## Dalsi krok pri pokracovani

User pozadoval: "Nejdrive chci dotahnout komplet prohlizec, pak TypeScript".
Doporucene volby:
- **A)** Backgrounds L3 paint integrace - position/size/repeat na image render
- **B)** Canvas API - velky novy modul, vyzaduje rozhodnuti o pristupu
  (per-canvas DisplayList -> texture vs primy render)
- **C)** CSS clip-path (SDF clipping)
- **D)** @font-face (multi-font registry)
- **E)** SVG support

Pokud nejsi jisty, zeptat se A/B/C/D/E.
