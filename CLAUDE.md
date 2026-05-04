# RustWebEngine - Projektove instrukce

## Co to je

Rust implementace **JS enginu + browseru od nuly**. Cilem je funkcni prohlizec - lexer/parser/interpreter pro JavaScript + HTML/CSS engine + GPU rendering pres wgpu.

Inspirace Servo (html5ever, selectors, cssparser) ale interpreter, layout pomocnik (taffy obal), paint, rendering jsou vlastni.

## Globalni preference (z user CLAUDE.md)

- **Cesky** v komunikaci a komentarich. Diakritika OK (a/c/e/...).
- **Ciste ASCII v kodu** - zadne `->` Unicode sipky, em-dash, "smart quotes". Pouzivat `->`, `-`, `"..."`, `...`, `<=`, `>=`, `!=`. Vyjimka jen kdyz se test/feature znaku samych tyka.
- **Pri nejistote se zeptat** drive nez psat kod (A/B/C varianty).
- CAVEMAN MODE: terse Czech v komunikaci, kod normalne.

## Adresarova struktura

```
src/
  main.rs                  - Entry point. CLI rezimy: debug / devtools / browser / window / default
  tokens.rs                - TokenKind enum (Punctuator, Keyword, Ident, NumericLiteral, ...)
  ast.rs                   - AST node definice (Expression, Statement, Program)
  evaluator.rs             - (legacy / pomocny eval)
  utils/
    mod.rs                 - utf8_cursor, string_utils
    macros/                - debug! a podobne makra
    string_utils.rs
    utf8_cursor.rs
  specifications/          - Konstanty z ECMA262 / spec referencni tabulky (number_literal, lexer_errors)

  lexer/                   - JavaScript tokenizer
    base.rs                - Lexer struct, parse_str, debug_print_tokens
    identifier.rs          - Ident + keyword recognition
    numeric.rs             - Number literal (decimal/hex/bin/oct/BigInt/scientific)
    string.rs              - String literal vc. template literals (`${...}`)
    regex.rs               - Regex literal disambiguation
    debug.rs               - Debug pretty-print

  parser/
    mod.rs                 - Recursive descent parser - JS expressions a statements

  interpreter/             - Tree-walking JS interpreter
    mod.rs                 - Interpreter, JsValue, scope, JsObject, run(), event loop, task queue, workers
    builtins.rs            - Globalni objekty: console, Math, JSON, Date, Intl (ICU4X), fetch (ureq), setTimeout, Worker
    string_methods.rs      - String prototype metody
    helpers.rs

  browser/                 - HTML/CSS engine + rendering
    html_parser.rs         - HTML5 parsing pres html5ever -> nas DOM tree
    dom.rs                 - DOM node, get_elements_by_tag, text_content, atd.
    css_parser.rs          - CSS pres cssparser -> StyleSheet, Rule, Declaration. @media, @keyframes, var().
    cascade.rs             - Selector matching, specificity, kaskada, ruleset -> ComputedStyle
    layout.rs              - Box model + taffy flex/grid + inline (word wrap, line boxes)
    paint.rs               - ComputedStyle + LayoutBox -> DisplayList (Rect, Text, Image, Gradient, Shadow, Border)
    render.rs              - winit + wgpu. WGSL shadery (solid/text/gradient/shadow/SDF). Glyph atlas. Hit test + click dispatch.
    tests/

  debug_view/              - HTML diagnosticke nahledy
    mod.rs                 - generate_debug_html (tokeny + AST)
    tokens_view.rs         - Tokeny jako barevne badge + tooltip
    ast_view.rs            - AST tree (collapsible)
    page.rs                - HTML wrapper (CSS + JS embedded)
    devtools.rs            - DevTools-like panel (Elements / Console / Network / Performance)

static/                    - Test HTML/CSS/JS pro browser/devtools
  test.html, test.css, basic_test.js

DOKUMENTACE.md             - Vyssi-uroven dokumentace projektu (cesky)
```

## CLI rezimy (main.rs)

```bash
cargo run                                        # Default: tokenize + parse + interpret inline JS
cargo run -- debug [src.js] [out.html]           # Debug viewer: tokeny + AST do HTML
cargo run -- devtools [src.html] [out.html]      # DevTools-like nahled (4 panely) + spusti <script> a zachyti console+network
cargo run -- browser [src.html]                  # Render do okna pres wgpu (default static/test.html)
cargo run -- window [src.html]                   # Alias browser, pres run_window_with_html
```

## Architektonicke volby + duvody

- **wgpu (ne Skia/Cairo)**: cross-platform GPU - obali Vulkan/Metal/DX12/WebGPU. WGSL shadery. Lze naportovat na WebGPU.
- **taffy (ne vlastni flex)**: Servo-grade flex/grid implementace. Spec compliance lepsi nez psat od nuly. Inline layout zustava nas (word wrap, line boxes).
- **html5ever + selectors + cssparser**: stejne crates co Servo, na ne se da spolehnout. Parser stage neni zajimavy problem - chceme cas na engine/render.
- **fontdue (ne ttf-parser+raqote)**: rasterizer + glyph atlas. SDF text mode.
- **ICU4X pro Intl**: compiled_data, real CLDR. Lepsi nez fake/stub.
- **ureq sync (ne reqwest+tokio)**: fetch() ma sync interpreter, async runtime by komplikoval. Blocking call OK.
- **fancy-regex**: lookbehind/backref - co `regex` crate nepodporuje.
- **Tree-walking interpreter (ne bytecode VM)**: jednoduchost. Performance neni cil; correctness + visibility ano.
- **JsValue: Number(f64), String, Bool, Null, Undefined, BigInt, Object(Rc<RefCell<JsObject>>)**. Rc<RefCell> stejne semantika jako JS reference.
- **Single-threaded interpreter, Workers spawn novy Interpreter v threadu**: !Send constraint - zadne sdileni JsValue cross-thread, message passing pres channels.
- **Console + Network log capture**: `Rc<RefCell<Vec<...>>>` v Interpreter struct, sdileno do native closures pres clone. DevTools to pak borrow().clone().

## Co je hotove (high level)

- JS lexer (ECMA262 superset coverage)
- JS parser (vyrazy + statements + funkce + arrow + async/await + destructuring + spread)
- JS interpreter (scopes, closures, prototype chain, this binding, eventual loop, microtasks, timers)
- Builtins: Math, JSON, Date, Intl (ICU4X), fetch (ureq sync), Worker (real thread + script eval), setTimeout/setInterval, console
- DOM bridge (document.querySelector, getElementById, addEventListener, dispatchEvent)
- HTML5 parsing (html5ever)
- CSS: parser (cssparser), kaskada (specificity), @media, @keyframes, var()/calc(), shorthand
- Layout: box model + taffy flex/grid + inline (word wrap, line boxes), viewport units
- Paint: bg color, border (vc. radius pres SDF), text, linear gradient, box-shadow, image (cache), opacity
- Render: wgpu + WGSL multi-mode shader, glyph atlas, mouse scroll, click hit-test + event dispatch
- Animations: @keyframes parser + tick interpolace
- Debug viewer: tokeny barevne badge + AST tree
- DevTools panel: Elements / Console (live capture) / Network (live capture) / Performance
- Form value sync, img tag, animation tick

## Co zbyva (TODO - prioritizovane)

1. **GPU image rendering** - cache existuje, ale rendering placeholder. Multi-texture binding nebo RGBA atlas.
2. **CSS animation runtime application** - @keyframes parsuju, interpoluju, ale `animation: name 2s` na elementu se nepouziva pro paint. Time-based redraw pres App.start_time.
3. **Radial + conic gradient** (linear hotovy)
4. **Canvas API** - `<canvas>` tag + 2D context (fillRect, fillText, beginPath, ...)
5. **@font-face** custom fonty
6. **SVG support** - shapes -> display list
7. **Box-shadow inset** varianta
8. **CSS clip-path**
9. **WebGL** (po canvas)
10. **Form submit handling** (value sync uz je, submit ne)

## Konvence

- Komentare cesky, ASCII only.
- Errory cesky (`"Nelze nacist {path}: {e}"`).
- Test soubory v `<modul>/tests/` adresari nebo `mod tests` inline.
- Cargo.toml ma kazda dependency komentar **proc** je tam.
- Po kazde feature: build + test + commit. Commit message strucny, popis "co + proc".
- Rc<RefCell<>> pro sdileny mutable state (interpreter scope, console_log, document).

## Build / test

```bash
cargo build              # Dev profile, debuginfo, no opt
cargo test               # Vsechny unit testy (lexer, parser, browser, debug_view)
cargo run -- browser     # Otevri okno s static/test.html
```

Aktualne 1 warning: `suspicious_double_ref_op` v `debug_view/devtools.rs:108` - sort_by_key na &&String. Drobnost, fixnout pri nejblizsim doteku souboru.
