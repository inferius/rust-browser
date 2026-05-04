# Prechodovy plan - nove vlakno

Toto cti **driv nez zacnes**. Plus `CLAUDE.md`, `README.md` a `TODO_CSS.md` v rootu.

## Stav

- Build: **OK** (cargo build cisty, 0 warnings).
- Tests: **709 passed, 0 failed, 3 ignored**.
- Posledni commit: `a5717b6 CSS Filter Effects L1 - parser + 5 testu (render TODO)`.
- Working tree: ciste.
- Branch master, ~70 commitu pred origin/master (nepushovano - **nepushovat bez vyzvy uzivatele**).

## Co bylo posledni session hotovo

V tomto rozjezdu doplneno (po Console+Network capture):

1. **CSS animation runtime application** - apply_animations per-frame v render loopu
2. **GPU image rendering pres RGBA atlas** (2048x2048, shelf packing, 16 MB)
3. **README.md** - cesky popis projektu + jak spustit (5 CLI rezimu)
4. **TODO_CSS.md** - kompletni mapa CSS modulu W3C s checkboxy + prioritou

CSS feature batches (vsechny inkluduji parser, runtime [pokud relevantni],
unit testy a static/css_modules/<modul>/ test stranky):

5. **Selectors L4** (8 testu) - :is/:where/:not(list)/:has, ~ general sibling,
   :nth-child(an+b)/:nth-of-type/:nth-last-*, :first/last/only-of-type,
   :only-child, :empty
6. **Values L4** (9 testu) - min(), max(), clamp(), env() - iterativni
   resolution do fixed pointu pres 10 prochodu (var/calc/min/max/clamp/env)
7. **Color L4** (11 testu) - oklch/oklab/lab/lch/hsl/hwb, modern rgb syntax,
   hex 4/8 digit, color-mix(in srgb|oklab|oklch). Bjorn Ottosson algoritmus
   pro OkLab, D65 CIELAB, gamma encoding.
8. **Logical Properties L1** (5 testu) - margin/padding-block/-inline,
   border-block/-inline-*, border-start-end-radius rohy, inset/-block/-inline,
   block-size/inline-size. LTR + horizontal-tb default.
9. **Animations L1 rozsireni** (4 testy) - animation-fill-mode (none/forwards/
   backwards/both), animation-play-state (running/paused), arbitrary
   cubic-bezier(...), steps(n, jump-*).
10. **Nesting L1** (4 testy) - `&` selector + nested rulesets, implicit
    descendant pri ne-amp prefix, `.parent.nested` kombinace.
11. **Container Queries L1** (5 testu) - @container [name] (cond) parsing,
    cqw/cqh/cqi/cqb/cqmin/cqmax units (aproximace pres viewport).
12. **Box-shadow inset** (1 test) - mode 5 SDF shader, fade dovnitr od okraju.
13. **Radial + conic gradients** (5 testu) - mode 6/7 SDF shader, GradientKind
    enum, parser pro "circle at top left" / "from 90deg at center".
14. **Transitions L1** (6 testu) - shorthand + longhand parser, ActiveTransition,
    detect_transitions (state diff), apply_transitions (interpolace),
    integrace v render App loopu.
15. **Filter Effects L1 - parser only** (5 testu) - FilterOp enum, parse_filter_chain
    pro blur/brightness/contrast/grayscale/hue-rotate/invert/saturate/sepia/
    opacity/drop-shadow + multiple chained. Render zatim placeholder.

## TODO (priorita shora dolu)

### Velke balky
1. **Filter Effects L1 - render** (slozite)
   - Color matrix filtry (brightness/contrast/grayscale/sepia/invert/saturate/
     hue-rotate/opacity) lze pridat single-pass do shaderu jako mode 8 - aplikace
     na bg/text na elementu. Per CSS spec ale potreba na cely subtree -> render-
     to-texture pipeline (offscreen RT + composit pass).
   - Blur: 2-pass gaussian (horizontal + vertical render passes).
   - drop-shadow: blur + offset compositing.

2. **Backgrounds L3** - multiple backgrounds (comma-separated layers),
   background-position/-size/-repeat/-clip/-origin/-attachment, border-image L4.

3. **Canvas API** - `<canvas>` tag + getContext('2d'). 2D context metody:
   fillRect, fillText, beginPath, moveTo, lineTo, stroke, fill, arc, drawImage.
   Vlastni display list per-canvas -> render to texture -> draw v hlavni RT.

4. **CSS clip-path** - inset()/circle()/ellipse()/polygon() - SDF clipping
   v shaderu nebo stencil buffer.

5. **@font-face** - parser, font binary fetch (HTTP/FS), pridat do fontdue
   Font registry (multi-font support).

6. **SVG support** - shapes (rect, circle, path) -> display list. Path parser.

7. **Cascade Layers @layer** - parser-only zacatek. Layer priority < unlayered.

8. **Position sticky** - layout changes (track scroll position vs original).

9. **WebGL** - po Canvas. Vlastni GL context simulating cez wgpu.

10. **Form submit handling** - JS bezi, form value sync hotovy, submit event
    + URL encode + ureq POST.

### Mensi inkrementy
- Pseudo-elements ::before / ::after + content property (vyzaduje virtualni
  layout boxy).
- text-decoration L4 - text-shadow, text-decoration-style wavy/dashed.
- Overflow L3 - scroll-snap, overscroll-behavior.
- @import / @supports parser support.
- :focus-visible / :focus-within runtime stav.
- transition events (transitionrun/-start/-end/-cancel).

### TypeScript kompilator
**User pozadoval**: dotahnu CSS, pak prokonzultujem design.
- Otazky pred implementaci:
  - Scope: full TSC superset vs subset (jen common features)
  - Type checking vs jen strip types -> JS
  - Integrace s lexer/parser (extend nebo vlastni front-end)
  - Vystup: JS string vs primy AST do interpreteru

## Pracovni flow (uzivatel ocekava)

- Po kazde fici: **build + test + commit**.
- Commit message cesky, ASCII, strucny popis "co + proc".
- Pred psanim kodu pri nejasnosti se ptat (A/B/C varianty).
- Komunikace cesky, CAVEMAN MODE aktivni (terse), kod normalne.
- Ke kazdemu CSS modulu: unit testy + static/css_modules/<modul>/ test stranka.
- Aktualizovat TODO_CSS.md checkboxy.

## Klicove soubory pro orientaci

- `src/main.rs` - CLI rezimy, dobry vstupni bod.
- `src/browser/cascade.rs` - cascade + animations + transitions + Logical
  + Values L4 resolver. **Velky, hodne dulezity.**
- `src/browser/css_parser.rs` - CSS -> Stylesheet, vc. selectors L4 +
  nesting + container queries + keyframes.
- `src/browser/layout.rs` - box model + parse_color (vc. Color L4) +
  parse_*_gradient + parse_filter_chain + parse_box_shadow + parse_length.
- `src/browser/render.rs` - winit + wgpu, App struct, frame loop, image atlas,
  WGSL shader (8 modu: 0 solid, 1 text, 2 linear gradient, 3 shadow, 4 image,
  5 inset shadow, 6 radial gradient, 7 conic gradient).
- `src/browser/paint.rs` - DisplayList emission z LayoutBox.
- `static/test.html` + `static/test.css` - hlavni testovaci stranka.
- `static/css_modules/<modul>/` - per-feature test stranky.

## Co necist hned (velke soubory)

- `src/interpreter/builtins.rs` (>2000 lines) - cti az kdyz potrebujes konkretni builtin.
- `src/browser/cascade.rs` (~1300 lines) - cti po sekcich.
- `src/browser/layout.rs` (~1500 lines) - cti po sekcich.
- `src/browser/render.rs` (~1300 lines) - cti po sekcich.
- `src/debug_view/devtools.rs` (>500 lines) - cti az kdyz upravujes DevTools panel.

## Dalsi krok pri pokracovani

Uzivatel pravdepodobne rekne "pokracuj". Doporucene volby:
- **A)** Filter Effects render (color matrix + RT pipeline)
- **B)** Backgrounds L3 (multiple bg, position, size)
- **C)** Pseudo-elements ::before/::after
- **D)** TypeScript kompilator - prokonzultovat design

Pokud nejsi jisty, zeptat se A/B/C/D.
