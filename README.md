# RustWebEngine

Webovy prohlizec a JavaScript engine **napsane v Rustu od nuly**. Cilem je
funkcni prohlizec se vlastni implementaci JS interpretu + HTML/CSS engine
+ GPU rendering.

Inspirace Servo (sdileme html5ever, selectors, cssparser), ale interpreter,
layout, paint a rendering vrstva jsou vlastni.

## Co je hotove

- **JS lexer** (ECMA262 superset coverage)
- **JS parser** (vyrazy, statements, funkce, arrow, async/await, destructuring, spread)
- **JS interpreter** (scopes, closures, prototype chain, this binding, event loop, microtasks, timers)
- **Builtins**: Math, JSON, Date, Intl (real CLDR pres ICU4X), fetch (sync ureq), Worker (real thread), setTimeout/setInterval, console
- **DOM bridge**: document.querySelector, getElementById, addEventListener, dispatchEvent
- **HTML5 parsing** (html5ever)
- **CSS**: parser (cssparser), kaskada (specificity), @media, @keyframes, var()/calc(), shorthand
- **Layout**: box model + flex/grid (taffy) + inline (word wrap, line boxes), viewport units
- **Paint**: bg color, border (vc. radius pres SDF), text, linear gradient, box-shadow, image, opacity
- **Render**: wgpu + WGSL shader (4 mody: solid/text/gradient/shadow/image), glyph atlas, **RGBA image atlas**, mouse scroll, click hit-test
- **Animations**: @keyframes parser + runtime interpolace + cubic-bezier easing
- **Debug viewer**: tokeny barevne badge + AST tree (HTML)
- **DevTools panel**: Elements / Console (live capture) / Network (live capture) / Performance

## Co zbyva (TODO)

1. Radial + conic gradient (linear hotovy)
2. Canvas API (`<canvas>` + 2D context)
3. @font-face custom fonty
4. SVG support
5. TypeScript kompilator (front-end pres parser, k diskuzi)
6. Box-shadow inset, clip-path, WebGL, form submit handling

## Pozadavky

- **Rust 1.78+** (edition 2024, viz `Cargo.toml`)
- **System font** - hleda Arial/Segoe UI/Helvetica/DejaVu/Liberation
  v standardnich umistenich Windows/macOS/Linux. Pripadne override:
  ```
  set RUST_WEB_ENGINE_FONT_PATH=C:\path\to\font.ttf
  ```
- **GPU s Vulkan/Metal/DX12** (wgpu vyber backend automaticky)

## Build

```bash
cargo build           # Dev profile (debuginfo, no opt) - rychly compile
cargo build --release # Optimalizovany build pro performance test
cargo test            # Vsechny unit testy (lexer, parser, interpreter, browser, render)
```

## CLI rezimy

### 1. Default - JS engine showcase

```bash
cargo run
```

Spusti inline JS source: tokenizer -> parser -> interpreter. Vypise tokeny,
strukturu AST a vystup z `console.log`. Slouzi jako sanity check JS frontu.

### 2. Debug viewer (tokeny + AST)

```bash
cargo run -- debug                          # Default ukazka
cargo run -- debug myfile.js                # Vlastni JS soubor
cargo run -- debug myfile.js out.html       # Vlastni vystup
```

Vygeneruje **self-contained HTML** s:
- tokeny jako barevne badge (typ, lexeme, line:col, hodnota v tooltipu)
- AST tree (collapsible - klik rozbali/zabali uzly)

Otevri vystup v prohlizeci.

### 3. DevTools-like panel

```bash
cargo run -- devtools                       # static/test.html
cargo run -- devtools page.html             # Vlastni stranka
cargo run -- devtools page.html dt.html     # Vlastni vystup
```

4 panely (Elements / Console / Network / Performance):
- **Elements** - DOM tree + computed styles per element
- **Console** - live capture `console.log/error/warn/info/debug` z `<script>` tagu
- **Network** - live capture `fetch()` volani (URL + status code)
- **Performance** - placeholder pro budouci profile data

JS uvnitr stranky se realne spusti v interpreteru a logy zachycene.

### 4. Browser - render do okna

```bash
cargo run -- browser                        # static/test.html
cargo run -- browser stranka.html           # Vlastni HTML
```

Otevre **wgpu okno** se skutecnym renderingem. Podporuje:
- mys scroll
- click (hit-test + JS event dispatch)
- @keyframes animace (frame loop redraw)
- obrazky (RGBA atlas)

CSS se hleda automaticky podle nazvu (`stranka.html` -> `stranka.css`).

### 5. Window - alias browser

```bash
cargo run -- window stranka.html
```

Stejne jako `browser`, jen jine entry funkce.

## Adresarova struktura

```
src/
  main.rs              CLI entry point + rezimy
  lexer/               JS tokenizer
  parser/              JS recursive descent parser
  ast.rs               AST node definice
  tokens.rs            TokenKind enum
  interpreter/         Tree-walking JS interpreter + builtins
  browser/             HTML/CSS engine + wgpu rendering
    html_parser.rs     HTML5 parsing (html5ever)
    dom.rs             DOM tree
    css_parser.rs      CSS -> Stylesheet (cssparser)
    cascade.rs         Selector matching, specificity, animation runtime
    layout.rs          Box model + taffy + inline layout
    paint.rs           ComputedStyle -> DisplayList
    render.rs          winit + wgpu, WGSL shader, glyph + image atlas
  debug_view/          HTML diagnosticke nahledy
    devtools.rs        DevTools panel generator
  utils/               utf8_cursor, makra
  specifications/      ECMA262 referencni tabulky

static/                Test HTML/CSS/JS (default cilove cesty)
  test.html
  test.css
  basic_test.js

CLAUDE.md              Instrukce pro Claude Code (LLM agenta)
HANDOFF.md             Prechodovy plan mezi vlakny
DOKUMENTACE.md         Vyssi-uroven projektova dokumentace
README.md              Tento soubor
```

## Architektonicke volby

- **wgpu** - cross-platform GPU API (obali Vulkan/Metal/DX12/WebGPU). WGSL shadery.
- **taffy** - Servo-grade flex/grid layout. Inline layout (word wrap) je vlastni.
- **html5ever + selectors + cssparser** - stejne crates co Servo.
- **fontdue** - rasterizer + glyph atlas. SDF text mode.
- **ICU4X** - real CLDR locale data pro `Intl.*`. compiled_data feature.
- **ureq sync** - fetch je sync, neni potreba tokio.
- **fancy-regex** - lookbehind/backref support.
- **Tree-walking interpreter** (ne bytecode VM) - jednoduchost + visibility.
- **JsValue: Number(f64), String, Bool, Null, Undefined, BigInt, Object(Rc<RefCell<JsObject>>)**.
- **Workers spawn novy Interpreter v threadu** - !Send constraint. Komunikace pres channels.

## Rendering pipeline

1. HTML5 parse -> DOM tree (`dom::Node`)
2. CSS parse -> Stylesheet (`css_parser::Stylesheet`)
3. Cascade (selector match + specificity) -> `StyleMap` (per-element computed styles)
4. **Animation runtime** - aplikace @keyframes pri elapsed time
5. Layout tree (`layout::layout_tree`) - box model, flex/grid, inline
6. Paint -> `DisplayCommand` list (Rect, Border, Text, Gradient, Shadow, Image)
7. Vertex builder -> `Vec<Vertex>` (mode-tagged: 0=solid, 1=text, 2=gradient, 3=shadow, 4=image)
8. wgpu draw call (1 pipeline, 1 bind group, 1 vertex buffer per frame)

## Konvence

- Komentare cesky, **ASCII only** v kodu (`->`, `-`, `"..."`, `<=`, `>=`).
- Errory cesky.
- Po kazde fici: build + test + commit.
- `Rc<RefCell<>>` pro sdileny mutable state.

## Dokumentace

- **DOKUMENTACE.md** - Vyssi-uroven popis architektury (cesky)
- **CLAUDE.md** - Instrukce pro LLM agenta + struktura
- **HANDOFF.md** - Prechodovy plan, TODO, klicove soubory

## Licence

Bez specifikovane licence (TBD).
