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

## Co je hotove navic (od posledni revize)

### Rendering / paint
- **GPU image rendering** - mode 4 textureSample z RGBA atlasu (4096x4096 shelf-packed po 8cf1f8a).
- **wgpu 0.20 -> 29 upgrade** (0ebf297): RenderPipelineDescriptor +cache+multiview_mask, RenderPassDescriptor +multiview_mask, RenderPassColorAttachment +depth_slice, PipelineLayoutDescriptor -push_constant_ranges +immediate_size, bind_group_layouts &[Option<&BGL>], ShaderModule entry_point Option<&str>, MipmapFilterMode, ImageCopyTexture->TexelCopyTextureInfo, surface.get_current_texture() vraci CurrentSurfaceTexture enum.
- **HiDPI scale_factor tracking** (a720a63) - Renderer.scale_factor z winit, vp uniform = config.width / (zoom * scale_factor). CSS px -> physical fb mapping spravne pri HiDPI.
- **3D transform NDC z fix** (f6309cd) - compose_transform shader nz=0.5 konstantni. Driv `clamp(tz*inv_w, -1, 1)` clipoval pulku quadu (wgpu NDC z range = [0,1] ne [-1,+1]). Po fixu rotateX/Y/Z full visible.
- **CSS animation runtime tick**, **Radial+Conic gradient** (mode 6/7), **Canvas 2D API**, **@font-face**, **SVG support**, **Box-shadow inset**, **CSS clip-path**, **WebGL** (1308 lines).
- **WOFF2 glyf transform reverse** - real Google Fonts Roboto subsets pass.
- **Form submit** - JS API + native button[type=submit].
- **Polygon edge AA** (ff598b5) - winding-aware outward normal (CW = vpravo od edge), 1px feather strip.
- **Crisp glyph rasterization at zoom** (fd31f04) - atlas key = (font_size * zoom).round(), metrics scale dle inv_z, integer physical pixel snap.
- **Image atlas re-raster pri zoomu** (fb29b71) - source bytes cached, resample na target physical size.
- **LCD subpixel atlas storage** (2f05b48 -> 817b41d) - fontdue rasterize_subpixel pri size<24 dela 3x sirku, shader avg ze 3 sub-pixelu = grayscale fallback (proper LCD vyzaduje dual-source blend, neni implemented).
- **SDF AA range zoom-aware** (e5fd596) - aa_range = 1/zoom logical (= 1 phys px), zachovava sharp edges pri zoomu.
- **Real bold + italic font variants** - timesbd.ttf, timesi.ttf, timesbi.ttf, fallback fake skew/smear.
- **Default font Times New Roman** (d03ad51) - match Chrome UA default.
- **Glyph atlas 4096x4096** (8cf1f8a) - vetsi kapacita pri zoomu.
- **Animations smooth scroll inertia** (625477e) - lerp 25% per frame na target.

### Layout
- **Per-element layout cache** (05d09bb) - subtree fingerprint = hash(node_ptr + tag + text + sorted style + child fingerprints). Thread-local LAYOUT_CACHE pri build, child rebuild skip pri match.
- **Cascade hover/focus state hash** (0aba5e5) - cascade_hash zahrnul hovered_node + focused_node, ale skip pokud CSS bez :hover/:focus selektoru.
- **@media + @container queries pres viewport** (45f5c4b) - cascade_with_viewport pasuje vw/vh.
- **SVG child LayoutBox rect** (c9bb81e) - z SVG attrs (rect/circle/ellipse/line/text). Devtools highlight + hit-test funguje pres SVG shapes.
- **Inline replaced explicit_height ovlivnuje line_height** (0e4de94) - SVG/img s height attr drzi spravnou cursor_y advance v parent flow. Section content_h zahrnuje shapes.
- **Inline-block element_h ovlivnuje line_height** (0e4de94) - button s padding 8+text+8 = 32 px line, cursor_y advance pokryje cely button bbox vc paddingu. Section bottom padding viditelny.
- **Text vertical center via inner_h** (e1e3391 -> 817b41d) - v_offset = (inner_h - 1.5*fs)/2, bez clampu. Visible glyph (0.9*fs) center v inner_h.
- **Inline element baseline shift** (eab6562) - pro smaller font (small/sub/sup) rect.y = cursor_y + (parent_fs - el_fs).max(0).
- **Bold-aware width measure** (186f359) - measure_text_width_styled(text, size, bold) prefer bold font / +1px fake-bold pad. Carka po `<strong>` neoverlapuje.
- **Text wrap multi-line via \n** (e45836a) - flux_inline insertne newline na break point, render handluje pen_x reset + pen_y advance.
- **line_height default 1.2** (ad12a0f) - CSS spec normal, predtim 1.4 = prilis aggressivni.
- **layout_block bound s asymmetric padding** (ad12a0f) - pad_t/pad_b namisto bx.padding shorthand.
- **Text node rect span pres vsech words** (817b41d) - rect.width = cursor_x - rect.x, height = lines * advance_h. Hit-test funguje pres cely text run, cursor I-beam visible.
- **Default font sans serif** vs serif Times New Roman (d03ad51).

### Browser shell / UI
- **Zoom support** (4905e7b) - Ctrl++/-/0 (1.1x kroky, 25%-500%). Layout viewport = window/zoom = reflow.
- **Vertical + Horizontal scrollbars at zoom** (bd5ad42) - scroll_y/x v logical px, viewport_w/h logical, scrollbar emit visible.
- **Smooth scroll inertia** (625477e) - lerp scroll_y -> target.
- **Keyboard scroll** (2ea9e70) - PageUp/Down (0.9 viewport), Arrow (60px), Home/End, Space.
- **Find on page (Ctrl+F)** (90ddc25) - overlay UI s query + counter, Enter next/Shift+Enter prev, Esc close.
- **Address bar (Ctrl+L)** (a4380df) - URL input + Enter navigate (http/file/path).
- **Print to PDF (Ctrl+P)** (8b0ab3a) - printpdf walk LayoutBox tree, Times Roman font, save .pdf.
- **Text selection** (3e39e85) - mouse drag rect, Ctrl+A select all, Ctrl+C copy text. arboard clipboard.
- **Cursor icon** (b42d4d1) - I-beam over text, Pointer over a/button/input/select.
- **Form input typing** (b42d4d1) - focused input/textarea kapture char + Backspace -> set value attr.
- **Devtools console Ctrl+V paste** (fbc9f85).

### Deps bumps
- arboard 3.4 -> 3.6, pollster 0.3 -> 0.4, bytemuck 1.16 -> 1.25, fancy-regex 0.13 -> 0.18, brotli 6 -> 8, tungstenite 0.24 -> 0.29, ureq 2.10 -> 2.12 (3.x = API rewrite skip), selectors 0.25 -> 0.38 (CSS L4), cssparser 0.34 -> 0.37, html5ever 0.27 stays (rcdom 0.5+ +unofficial only), icu 1.5 stays (2.0 major rewrite).

## Co zbyva (TODO pro dalsi vlakno)

Viz HANDOFF.md.

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

Aktualne 0 warnings.
