# Master TODO

Souhrnny TODO pres celou enginu. CSS specificity v `TODO_CSS.md`.

Konvence:
- [x] hotovo
- [/] castecne (popis chybejiciho)
- [ ] chybi cele
- [-] vynechano (out of scope)

---

## Media

### Obrazky (raster)
Pres `image` crate (pure-Rust, bez C deps).

- [x] PNG (vc. APNG?)
- [x] JPEG (baseline + progressive)
- [x] GIF (animace ne, jen prvni frame)
- [x] BMP
- [x] WebP (lossy + lossless dekoder cisty Rust)
- [x] TIFF
- [x] ICO
- [x] TGA
- [x] EXR (HDR)
- [x] QOI
- [x] Farbfeld
- [x] HDR (Radiance)
- [x] PNM (PBM/PGM/PPM)
- [x] DDS
- [ ] **AVIF** - vyzaduje dav1d AV1 dekoder (C library, NASM build).
  - Pure-Rust dekoder zatim neexistuje (AV1 je velky kodek).
  - Reseni: feature flag `avif-native` v image crate -> linkuje libavif/dav1d.
  - Decision: ASIS pure-Rust priorita -> AVIF zatim TODO, mozno later s system-dav1d.
- [ ] HEIC/HEIF (proprietarni Apple/Nokia)

### Obrazky (vektor)
- [ ] **SVG** - parser + paint integration. Velky modul - shapes, paths, gradients,
  text-on-path, filters. Mozno pres `usvg` crate (resvg ekosystem).

### Animovane obrazky
- [ ] APNG (multi-frame PNG) - image crate ho neumi, treba `apng-rs`.
- [ ] Animovany GIF (multi-frame iterace + delay).
- [ ] Animovany WebP.

### Video
- [ ] **`<video>` tag** - parser + layout box.
- [ ] Decoder: H.264, VP8, VP9, AV1.
  - Pure-Rust AV1 dekoder neexistuje. H.264 podobne.
  - Reseni: ffmpeg-rs (C dep) nebo nic.
- [ ] Audio sync.
- [ ] Controls UI.
- [ ] `<source>` tag pro multiple formats.
- [ ] HTMLMediaElement API (play/pause/currentTime/duration/...).
- [ ] HLS / DASH streaming.

### Audio
- [ ] **`<audio>` tag** - parser + layout box.
- [ ] Decoder: MP3, AAC, OGG/Vorbis, FLAC, Opus, WAV.
  - `symphonia` crate je pure-Rust, podpora vsech tech outpus.
- [ ] Web Audio API (AudioContext, OscillatorNode, GainNode, ...).
- [ ] Audio output: cpal/rodio.

### Canvas
- [x] `<canvas>` tag layout box.
- [/] 2D context: fillRect/fillText/beginPath - basic primitiva.
- [ ] Komplexni Canvas2D (Path2D, transformace, gradients, patterns, ImageData).
- [ ] WebGL 1.0 (parser stub).
- [ ] WebGL 2.0.
- [ ] WebGPU API (browser-side, ne nas wgpu render).

### Fonts
- [x] System fonty pres OS API (DirectWrite/Core Text/fontconfig).
- [x] @font-face (FS load, FontFace API, document.fonts).
- [x] WOFF/WOFF2 (zatim ne, jen TTF/OTF).
- [ ] Font subsetting.
- [ ] Color emoji (CBDT/CBLC, COLR/CPAL).
- [ ] Variable fonts (axes).

---

## CSS gaps (z TODO_CSS.md)

Vsechny moduly viz `TODO_CSS.md`. Hlavni chybejici:

### Velke nedotazene
- [ ] **CSS Subgrid L2** - grid item display=grid s `grid-template-rows: subgrid`.
  Vyzaduje track-share s parent grid. Slozite.
- [ ] **CSS Shapes L1**: `shape-outside`, `shape-margin`, `shape-image-threshold`.
  Float-aware text wrap. Bez flow text wrap (nas inline jen line boxes).
- [ ] **CSS Color L5** - advanced color manipulation, color-mix variants.
- [ ] **CSS Masking L1**: `mask-image`, `mask-mode`, `mask-position/size/repeat/composite`.
- [ ] **CSS writing-mode L4**: `vertical-lr`, `vertical-rl`, `sideways-lr/-rl`.
  Swap main/cross axes ve flex/grid. 10 taffy fixtures je kvuli tomu skip.
- [ ] **CSS Pseudo-Elements L4**: `::placeholder`, `::file-selector-button`,
  `::backdrop`, `::selection`, `::target-text`.
- [ ] **CSS Forms L4**: form validation `:valid`/`:invalid` runtime.
- [ ] **CSS Tables L3**: `||` column combinator (table cell selektor).

### Drobnosti
- [ ] `:lang()`, `:dir()`
- [ ] `:focus-visible`, `:focus-within` runtime state (parser hotov).
- [ ] `forced-color-adjust`
- [ ] `scrollbar-gutter`
- [ ] `overflow-clip-margin`
- [ ] Multiple backgrounds (carkova syntax + paint integrace).
- [ ] Relative color syntax + `color()` namespace + system colors.
- [ ] Container Queries: per-element ancestor lookup (zatim viewport).
- [ ] Anchor Positioning: `inset-area`.
- [ ] Scroll-driven Animations: `animation-timeline: view()`.
- [ ] View Transitions: `::view-transition*` pseudo-elements.

### Out of scope
- [-] Houdini (Paint/Layout/Properties API).

---

## Layout engine

### Hotove
- [x] Box model + flex (full L1 spec) + grid (full L1 spec)
- [x] Inline (word wrap, line boxes)
- [x] Position absolute/fixed
- [x] Margin collapse (vc. collapse-through chain pres pos+neg)
- [x] BFC (overflow non-visible)
- [x] Scrollbar takes space
- [x] Aspect-ratio
- [x] Fr units + iterativni clamp re-resolution
- [x] Span items distribute extra space (CSS §11.5.5)
- [x] Auto-flow column
- [x] Negative grid lines

### Chybi / castecne
- [ ] **Subgrid L2**
- [ ] **Writing mode vertical-lr** (10 taffy fixtures, swap axes)
- [ ] **Inline-block** (mame jen block + inline)
- [ ] **Float** (CSS2.1 layout) - bez flow tedy bez `float: left/right`.
  - Souvisi se shape-outside.
- [ ] **CSS Tables** auto-layout / fixed-layout (zatim block fallback).
- [ ] **Multi-column** (`column-count`, `column-width`).
- [ ] **Direction RTL** runtime (parser parsing OK, layout swap chybi).
- [ ] **Bidirectional text** (Unicode BiDi).

---

## Renderer (wgpu)

### Hotove
- [x] WGSL shadery: solid, text (SDF), gradient (linear), shadow, image
- [x] Glyph atlas + SDF text mode
- [x] Mouse scroll, click hit-test
- [x] CSS animation tick + redraw loop
- [x] Filter effects: blur 2-pass + color matrix
- [x] Backdrop-filter
- [x] 3D transforms 4x4 matrix
- [x] Clip-path (inset/circle/ellipse/polygon ear-clipping)

### Chybi / castecne
- [ ] **Radial gradient**
- [ ] **Conic gradient**
- [ ] **Box-shadow inset**
- [ ] **Image rendering** - cache existuje, paint pres atlas, ALE pri velke obrazky
  downscale lossy (ne mip-mapping). Multi-texture binding alternativa.
- [ ] **Anti-aliasing edges** (MSAA?)
- [ ] **Subpixel text rendering**
- [ ] **Color emoji rendering** (COLR/CBDT)
- [ ] **GPU clip-path** pro polygon (CPU triangulation OK ale shader-based by byl rychlejsi).
- [ ] **Hardware mip-maps** pro image atlas.

---

## JS interpreter

### Hotove
- [x] ECMA262 lexer (full superset)
- [x] Parser (vyrazy + statements + funkce + arrow + async/await + destructuring + spread)
- [x] Tree-walking interpreter
- [x] Builtins: Math, JSON, Date, Intl (ICU4X), fetch (ureq sync), Worker (real thread), setTimeout/setInterval, console
- [x] DOM bridge (querySelector, getElementById, addEventListener, dispatchEvent)
- [x] Prototype chain, this binding, closures
- [x] Microtasks, timers
- [x] Real Worker (own thread + script eval)
- [x] BigInt
- [x] Symbol, Map, Set, WeakMap, WeakSet
- [x] Iterators, generators, for-of, for-in
- [x] try/catch/finally, throw
- [x] Async/await (transform to promises)
- [x] Promise A+ (then/catch/finally)
- [x] ES Modules (import/export, dynamic import)

### Chybi / castecne
- [ ] **Bytecode VM** (zatim tree-walking - mensi vykon).
- [ ] **JIT** (out of scope - stejne).
- [ ] **WeakRef**, **FinalizationRegistry**.
- [ ] **SharedArrayBuffer**, **Atomics** (vyzaduje cross-origin isolation).
- [ ] **TypedArray** vsechny varianty (mame jen Uint8Array? cek).
- [ ] **Temporal API** (Stage 3 proposal, ICU4X je hotov).
- [ ] **Decorators** (Stage 3).
- [ ] **Records & Tuples** (Stage 2, ne yet).

---

## Networking

### Hotove
- [x] `fetch()` sync pres ureq.
- [x] CORS preflight (basic).
- [x] Cookies (basic Cookie/Set-Cookie).

### Chybi
- [ ] **HTTP/2**, **HTTP/3** (ureq je 1.1).
- [ ] **WebSocket** (manual implementation potrebny).
- [ ] **Service Workers**.
- [ ] **IndexedDB**.
- [ ] **localStorage**, **sessionStorage** (mame? zkontrolovat).
- [ ] **Cache API**.

---

## Forms & Input

### Hotove
- [x] `<input>` (text, checkbox, radio, button) - layout + value sync.
- [x] Focus state (parser).
- [x] Click event dispatch.

### Chybi
- [ ] **Form submit** (value sync je, submit ne).
- [ ] `<input type="file">` file picker.
- [ ] `<input type="date|time|month|week">` native picker.
- [ ] `<select>` dropdown rendering.
- [ ] `<textarea>` multi-line editing.
- [ ] `<datalist>` autocomplete.
- [ ] Form validation runtime (`:valid`/`:invalid`/`:user-valid`).
- [ ] **IME** (Input Method Editor) - asijske jazyky.
- [ ] **Keyboard input** events (uz parser, runtime dispatch chybi).
- [ ] Selection API (`window.getSelection()`).
- [ ] Clipboard API.

---

## Accessibility

### Chybi
- [ ] ARIA attributes runtime.
- [ ] Screen reader support (UIA Windows, AX macOS).
- [ ] Focus management.
- [ ] Tab order navigation.

---

## Browser shell

### Hotove
- [x] Single window per HTML soubor.
- [x] DevTools panel (4 panely) - HTML output + open v default browser.
- [x] **Drag-drop** HTML soubory do okna -> reload.
- [x] **F5** reload current.
- [x] **F12** regen+open devtools.
- [x] CLI: `cargo run -- browser [path] [--devtools]`.

### Chybi
- [ ] Multi-tab support (tabs UI).
- [ ] URL bar / navigation history (back/forward).
- [ ] Bookmarks.
- [ ] Settings panel.
- [ ] Find-in-page (Ctrl+F).
- [ ] Print preview.
- [ ] Save page as HTML.
- [ ] **DevTools v NASEM browseru** (zatim HTML otevreny v default OS browseru):
  - Otvirat devtools jako split-pane v hlavnim okne (resp. side panel).
  - **Two-way binding** jako Chrome:
    - Hover element v Elements panelu -> highlight v render area (overlay rect).
    - Click element v render area -> select v Elements panelu (Inspect Element).
    - Edit attribut/style v panel -> live update DOM + reflow + redraw.
    - Edit text content -> propagace do DOM.
    - Toggle pseudo-class state (`:hover`, `:focus`).
    - Add/remove classes v classes panel.
  - Console: live REPL (typed -> eval v interpreter -> output).
  - Network: real-time stream fetch calls (uz mame log capture).
  - Performance: frame timing graf.
  - Sources: edit JS + reload runtime.
  - Computed styles s links na zdrojovy ruleset.
- [ ] Right-click context menu (Inspect Element / View Source / Save As).
- [ ] Keyboard shortcuts (Ctrl+L address bar, Ctrl+T new tab, Ctrl+W close, atd.).
- [ ] Window-level zoom (Ctrl++ / Ctrl+-).

---

## Engine architectural

### Recursion vs iteration
- [x] Linker stack 64 MB (Windows main thread default = 1 MB).
- [x] Stacker crate auto-grow na hot recursion paths (dom::walk, layout::build_box_inner,
  layout::layout_dispatch_inner, paint::paint_box, html_parser::convert_handle,
  dom::collect_text, dom::find_inner).
- [x] Iterativni `Drop` impl na NodeData - prevenci recursive drop chain pri dropnuti
  hlubokeho DOM tree.
- [ ] **html5ever RcDom Drop recursion** - externi crate ma vlastni recursive Drop
  na svuj NodeData. Pri DOMech > ~500 urovni stack overflow pri konci `parse_html`.
  - Reseni: detach children z RcDom progressively v `convert_handle` (pred drop ujistit
    ze RcDom strom je prazdny / shallow).
  - Nebo: fork rcdom + iterativni Drop tam.
- [ ] Performance: ASM/SIMD pro hot paths (text shaping, layout traversal,
  paint primitives). Mozno autovektorizace + intrinsic.

---

## Test coverage

### Hotove
- [x] 2181 unit testu (lexer, parser, interpreter, browser, debug_view).
- [x] 168 layout unit testu.
- [x] 1978/1988 (99.5%) taffy XML compliance, 0 FAIL.

### Chybi
- [ ] WPT (Web Platform Tests) integrace.
- [ ] CSS WG test suites runner.
- [ ] JS conformance tests (test262 subset).
- [ ] Snapshot rendering tests (pixel diff).
- [ ] Fuzzing (HTML/CSS/JS parsers).

---

Last updated: 2026-05-06
