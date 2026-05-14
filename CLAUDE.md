# RustWebEngine - Projektove instrukce

## Co to je

Rust implementace **JS enginu + browseru od nuly**. Cilem je funkcni prohlizec - lexer/parser/interpreter pro JavaScript + HTML/CSS engine + GPU rendering pres wgpu.

Inspirace Servo (html5ever, selectors, cssparser) ale interpreter, layout pomocnik (taffy obal), paint, rendering jsou vlastni.

## Workspace layout (od Session N+21)

Cargo workspace s 2 crates:

```
crates/engine/  -> lib `rwe_engine` + bin `rwe-engine` (renderer + JS interp + DOM)
crates/shell/   -> lib `rwe_shell` + bin `rwe-shell` (host: vlastni Window + chrome UI)
static/         -> test fixtures (workspace root)
```

`default-members = ["crates/engine"]` v root Cargo.toml -> `cargo run` bez `-p` = engine bin (zachovava puvodni CLI rezimy).

Embeddable API contract v `crates/engine/src/embed/`:
- `Engine` (shared `Arc<Device>`/`Arc<Queue>` + atlas placeholders + `EngineSettings`)
- `WebView` (per-tab DOM/CSS/JS interp/layout/scroll/offscreen RT)
- `InputEvent` / `EventResponse` neutralni typy (no winit dep)
- `loader::load_page(url)` http/file dispatch

Edge/CEF model: shell crate je samostatny host enginu. WebView renderuje do offscreen texture (`render_via(&mut Renderer)`), shell kompozituje pres `Renderer::present_external_to_swap_chain`.

Detaily v HANDOFF.md Session N+21.

## Globalni preference (z user CLAUDE.md)

- **Cesky** v komunikaci a komentarich. Diakritika OK (a/c/e/...).
- **Ciste ASCII v kodu** - zadne `->` Unicode sipky, em-dash, "smart quotes". Pouzivat `->`, `-`, `"..."`, `...`, `<=`, `>=`, `!=`. Vyjimka jen kdyz se test/feature znaku samych tyka.
- **Pri nejistote se zeptat** drive nez psat kod (A/B/C varianty).
- CAVEMAN MODE: terse Czech v komunikaci, kod normalne.

## Adresarova struktura

```
src/
  main.rs                  - Entry point. CLI rezimy: debug / devtools / browser / window / default.
                              #![allow(dead_code)] (test-expose API + future-pub variants);
                              unused_imports/unused_variables aktivni jako warning.
  tokens.rs                - TokenKind enum (Punctuator, Keyword, Ident, NumericLiteral, ...)
  ast.rs                   - AST node definice (Expression, Statement, Program)
  utils/
    mod.rs
    utf8_cursor.rs         - UTF-8 cursor s undo() pro multi-byte CP
    macros/                - debug! a podobne makra
  specifications/          - ECMA262 spec referencni tabulky (number_literal, lexer_errors)

  lexer/                   - JavaScript tokenizer
    base.rs                - Lexer struct, parse_str, debug_print_tokens, read_identifier_continue
    numeric.rs             - Number literal (decimal/hex/bin/oct/BigInt/scientific)
    string.rs              - String literal vc. template literals (`${...}`)
    regex.rs               - Regex literal disambiguation
    debug.rs               - debug_print_tokens
    tests/                 - extracted unit tests (base/numeric/string/regex)

  parser/
    mod.rs                 - Recursive descent parser - JS expressions a statements
    tests.rs               - extracted unit tests

  interpreter/             - Tree-walking JS interpreter (mod.rs split na 6 sub-modulu)
    mod.rs                 - Interpreter struct, JsValue/JsObject/JsMap/JsSet/JsFunc, Environment,
                              run(), event loop, drain_timers/workers/websockets, load_module,
                              dispatch_event, iterator_helper_method
    eval_call.rs           - eval_call - massive call dispatch (callee match, native + JS fn + class + method)
    eval_expr.rs           - eval (dispatcher) + eval_unary/binary/logical/assign + assign_to +
                              destructure_bind + bind_target_expr
    eval_member.rs         - eval_member + get_prop (obj.prop, obj[key], prototype chain)
    exec_stmt.rs           - exec_stmts + exec_stmt (if/for/while/return/throw/try/...)
    class.rs               - make_class_func, construct_class, run_super_constructor, bind_params
    call_machinery.rs      - call_function dispatch + call_new + construct_map/set/date/error/promise + call_generator
    builtins.rs            - setup_builtins (4800 LOC giant fn): console, Math, JSON, Date, Intl,
                              fetch, setTimeout, Worker, storage, observers, navigator, crypto, ...
    builtins_helpers.rs    - run_worker_thread, make_message_port, build_search_params, make_object_store
    builtins_reflect.rs    - Reflect API (get/set/has/deleteProperty/ownKeys/...)
    builtins_atomics.rs    - Atomics API stubs
    builtins_temporal.rs   - Temporal API stubs (Plain Date/Time/DateTime/...)
    string_methods.rs      - String prototype methods (charAt/concat/slice/...)
    bytecode.rs            - Bytecode VM (opt-in via console_eval_via_vm; tree-walker is authoritative)
    canvas.rs              - Canvas2D context API
    webgl.rs               - WebGL state + draw queue
    helpers.rs             - native(), make_settled_promise, helper closures
    dom_props.rs           - parse_url_parts + url_encode (location.* properties)
    js_value_impl.rs       - JsValue Display/Debug impls
    serialize.rs           - structuredClone glue
    tests/                 - unit + integration tests batches

  browser/                 - HTML/CSS engine + rendering
    devtools_panel.rs      - Inline DevTools panel frontend (paint nad DevToolsState)
                              + paint_element_highlight (Chrome-like overlay)
                              + devtools_hit_test + find_box_rect_by_id + pick_node_at_screen_pos
    html_parser.rs         - HTML5 parsing pres html5ever -> nas DOM tree
    dom.rs                 - DOM node, get_elements_by_tag, text_content
    css_parser.rs          - CSS pres cssparser -> StyleSheet, Rule, Declaration; @media, @keyframes, var()
    cascade.rs             - Selector matching, specificity, kaskada, viewport queries, hover/focus state hash
    devtools_panel.rs      - In-window devtools paint (F12 toggle)
    woff.rs                - WOFF/WOFF2 font decompression (brotli + glyf transform reverse)
    variable_fonts.rs      - Variable font axes detection
    emoji_fonts.rs         - Color emoji font (COLR/CPAL/CBDT/SBIX/SVG) detection
    webgl_helpers.rs       - WebGL serialize uniforms + attrib format conversions

    layout/                - layout split (mod.rs + 9 sub-modules; viz layout/mod.rs comment)
      mod.rs               - LayoutBox struct, layout_tree, build_box, layout_block, flush_inline,
                              measure_text_width, build_pseudo_box, animations, sticky/anchor positioning
      length.rs            - parse_length / parse_length_ctx (px/em/rem/vw/vh/%)
      shadows.rs           - parse_text_shadow + parse_box_shadow
      shape_fn.rs          - ShapeFunction enum + parse_shape_function
      transform.rs         - mat4 math + transform_op_matrix + compute_transform_matrix + needs_3d_pipeline
      transform_parse.rs   - parse_transform_chain + parse_transform tokenize
      filter.rs            - FilterOp + parse_filter_chain + apply_filter_chain + compute_color_matrix
      backgrounds.rs       - BgGradient/BgLayer/BgPosition/BgSize/BgRepeat/BgBox/BgAttachment + ClipPath
      gradients.rs         - parse_any/radial/conic/linear_gradient
      color.rs             - parse_color (CSS L4 superset): hex/named/rgb/hsl/hwb/oklab/lab/color()/
                              color-mix()/contrast()/relative

    layout_engine/         - vlastni flex / grid layout
      mod.rs               - dispatch + helpers
      flex.rs              - flex algoritmus per CSS Flexbox L1 spec 9.7
      grid.rs              - grid algoritmus per CSS Grid L1 spec
      flex_tests.rs / flex_spec_tests.rs / grid_tests.rs / grid_spec_tests.rs / taffy_compliance.rs

    paint.rs               - ComputedStyle + LayoutBox -> DisplayList (Rect/Text/Image/Gradient/Shadow/Border/Filter)

    render/                - render split (mod.rs + 10 sub-modules)
      mod.rs               - Vertex struct, build_vertices (display list -> verts), Renderer struct + impl,
                              run_window_with_options (winit ApplicationHandler), apply_paint_animations,
                              console_eval_via_vm
      url.rs               - fetch_text_url, fetch_image_bytes, resolve_url, decode_base64
      forms.rs             - find_ancestor_form, build_form_request, post_form, url_encode
      dirty.rs             - DirtyRegion (inkrementalni render)
      segments.rs          - Seg + partition_filter_segments + shift_command_x/y
      polygon.rs           - polygon math: signed_area, triangulate, clip, point_in_triangle
      atlas.rs             - GlyphAtlas + ImageAtlas + try_load_default_font (4096x4096 shelf-pack)
      shaders.rs           - WGSL shader strings: BLUR / TRANSFORM / COMPOSE / RECT
      primitives.rs        - push_rect/gradient/shadow/image/polygon vertex helpers
      canvas_paint.rs      - paint_canvas_ops (Canvas2D ops -> DisplayCommand)
      webgl_paint.rs       - paint_webgl_canvases (WebGL queue drain stub)

    tests/                 - integration tests (cascade/css/dom/html/layout/paint/render/devtools_panel/woff/emoji_fonts/variable_fonts)

  devtools/                - DevTools state + model (sjednoceny pro inline + static frontends)
    mod.rs                 - DevToolsState (theme, tab, panel_h, focus, frame_counter, ...)
    theme.rs               - ThemeMode + ThemeFlavor + Palette + OS dark mode detection
    focus.rs               - FocusTarget enum (keyboard input dispatcher)
    context_menu.rs        - MenuItem + MenuAction + per-tab builders
    search.rs              - tag/class/id/CSS selector/XPath element search
    model/
      elements.rs          - ElementRow + RowKind + build_rows (s collapsed HashSet)
      console.rs           - LogEntry + ConsoleInput (cursor/selection/history/clipboard)
      network.rs           - NetworkEntry + NetworkResourceType + NetworkFilter
      sources.rs           - SourceFile + SourcesState + Breakpoint + parse_source_map (V3 + VLQ decode)
      performance.rs       - FrameSample + 240-frame ring buffer
      styles.rs            - MatchedRule + RuleSource + StylesState
    tests/
      console_input_tests.rs / search_tests.rs / sources_tests.rs

  debug_view/              - HTML diagnosticke nahledy (statics)
    mod.rs                 - generate_debug_html (tokeny + AST)
    tokens_view.rs         - Tokeny jako barevne badge + tooltip
    ast_view.rs            - AST tree (collapsible)
    page.rs                - HTML wrapper (CSS + JS embedded)
    devtools.rs            - DevTools-like static HTML export (DEPRECATED, F11 stale funguje)

static/                    - Test HTML/CSS/JS pro browser/devtools
  test.html, test.css, basic_test.js, engine-test.html

DOKUMENTACE.md             - Vyssi-uroven dokumentace projektu (cesky)
HANDOFF.md                 - Stav projektu + TODO + posledni session changes
TODO.md / TODO_CSS.md      - Otevrene tasks
```

## CLI rezimy

```bash
# Engine bin (default cargo run = engine = -p rwe-engine implicit)
cargo run                                        # JS demo: tokenize + parse + interpret inline JS
cargo run -- debug [src.js] [out.html]           # Debug viewer: tokeny + AST do HTML
cargo run -- devtools [src.html] [out.html]      # DevTools-like nahled (4 panely) + spusti <script> a zachyti console+network
cargo run -- browser [src.html] [--devtools]     # Render do okna pres wgpu (default static/test.html). --devtools auto-otevri devtools.html
cargo run -- window [src.html]                   # Alias pro browser --no-shell
cargo run -- dump [src.html] [--selector=.foo]   # Layout/cascade dump

# Shell bin (Phase 4c+ runtime - vlastni Window + WebView pres embed API)
cargo run -p rwe-shell                           # WebView render path (no chrome bar)
cargo run -p rwe-shell -- static/test.html       # spec source HTML
# Chrome bar (tabs/addr/find/bookmarks) NIKDE dostupny od Session N+22 -
# odrezany z engine, Phase 99 ho doda do shell crate.
```

Engine entry point (browser rezim): `embed::Engine::run_standalone` (wrapper okolo `browser::render::run_window_with_options`).
Shell entry point: `rwe_shell::run_window(html, css, base_url, local_path)`.

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
- Test soubory: dva patterny:
  - `<modul>/tests/` adresar + `mod.rs` listing (integration-style, browser/interpreter/lexer/debug_view/parser)
  - `#[cfg(test)] #[path = "tests/X.rs"] mod tests;` v source souboru pro pristup k privates (lexer/base, lexer/numeric, browser/woff, browser/emoji_fonts, browser/variable_fonts)
  - Inline `#[cfg(test)] mod tests { ... }` jen pri malych testech (< ~60 LOC) kde extrakce nestoji za to (utf8_cursor, render mod.rs, layout_engine/grid+flex)
- Cargo.toml ma kazda dependency komentar **proc** je tam.
- Po kazde feature: build + test + commit. Commit message strucny, popis "co + proc".
- Rc<RefCell<>> pro sdileny mutable state (interpreter scope, console_log, document).
- **Splittovani velkych souboru:** modul s vetsi koncentraci kodu rozdelit do `<name>/mod.rs` + sub-soubory. Pattern:
  - Pro free fns: `mod x;` + `pub use x::{...}` v mod.rs
  - Pro struct methods: kazdy sub-soubor ma `impl Type { ... }` block, metody `pub(super)` volane z mod.rs / dalsich sub-modulu
  - Sdilene helpery promoted na `pub(super)` v mod.rs

## Build / test

```bash
cargo build              # Engine bin (default member)
cargo build --workspace  # Engine + shell bins
cargo test --workspace   # Vsechny unit testy (2706 pass aktualne)
cargo run -- browser     # Engine browser rezim (chrome bar - puvodni)
cargo run -p rwe-shell   # Shell crate runtime (WebView pipeline, no chrome)
```

Aktualne 0 warnings, 2706 testu pass.
