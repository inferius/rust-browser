# Prechodovy plan - nove vlakno

Cti **driv nez zacnes**. Plus `CLAUDE.md`, `README.md`, `TODO_CSS.md`.

## Stav

- Build: **OK**, 0 errors.
- Tests: **1113 passed, 0 failed, 3 ignored** (+308 v teto session, +38.3%).
- Posledni commit: `a8c0ecd WebGL phase 3c2 - vertex layout helpers + buffer upload`.
- Tree: ciste.
- Branch master, ~239 commitu pred origin/master (NEPUSHOVAT bez vyzvy).

## Recent session highlights

1. **Filter blur subtree orchestration** (commit cc1c531) - paint emit FilterBegin/End,
   render::draw_segments rozdeli display list na Main/Filter, vola RT pre-pass +
   run_blur_passes + compose_offscreen.
2. **Filter color matrix subtree** (commit 3ac8ea6) - rozsireni z blur-only na
   obecne color matrix filtry (hue/saturate/grayscale/sepia/invert/contrast/
   brightness/opacity). Compose shader s 4x5 row-major matrix. +159 testu.
3. **Cascade + DOM testy** (commit c6a3b6d) - +20 testu cascade + DOM.
4. **3D perspective shader pipeline** (commit 2602957) - real 4x4 matrix
   transform pres TRANSFORM_SHADER WGSL. compute_transform_matrix v layout,
   TransformBegin/End markery v paint, transform_pipeline + compose_transform
   v render. +31 testu pro matrix compose + paint markery + partition.
5. **Polygon clip-path** (commit b3f5c37) - fan triangulace pro convex
   polygons. ClippedRect display command. +5 testu.
6. **WebGL phase 1** (commit 514d0c1) - state machine + handle objects +
   clear color + buffer/shader/texture/program management. +26 testu.
7. **WebGL phase 2** (commit 91969a4) - GLSL -> WGSL transpile pres naga
   crate. preprocess_glsl_es1_to_es3 (attribute->in, varying->out/in,
   gl_FragColor->_gl_FragColor, version 450 core). compileShader real
   parse. linkProgram vyrobi WGSL strings. +5 testu.
8. **Concave polygon ear-clipping** (commit 6ca3b39) - real triangulace
   pro concave polygons (sipky, L-shape, hvezdy). polygon_signed_area
   shoelace, triangulate_polygon ear-clipping s convex check + point-in-
   triangle. Fallback fan pri stuck. +11 testu.
9. **WebGL phase 3a** (commit cce4ef1) - command queue + state recording.
   WebGLAttribSlot/UniformValue/DrawCmd structs. vertexAttribPointer/
   enableVertexAttribArray/uniform*/uniformMatrix*fv/drawArrays/drawElements
   ted naplnuji state + push do queue. +12 testu.
10. **WebGL phase 3b** (commits b1b6197 + ff2787f) - Interpreter sdili
    `webgl_states: Rc<RefCell<HashMap<canvas_ptr, Rc<RefCell<WebGLState>>>>>`.
    paint_webgl_canvases() drainuje queue per canvas a emituje:
    - Clear color jako solid Rect bbox.
    - DrawArrays/Elements jako stripe overlay (placeholder phase 3c).
    Test stranka #webgl section s blue clear demo. +10 testu.
11. **WebGL phase 3c1** (commit d325d2b) - pipeline + shader module cache
    infrastructure v Renderer (webgl_shader_modules, webgl_pipelines,
    webgl_buffers HashMaps). build_webgl_shader_modules helper
    (idempotent cache).
12. **WebGL phase 3c2** (commit a8c0ecd) - vertex layout helpers:
    webgl_attrib_to_vertex_format mapper (FLOAT/INT/UINT x size 1-4 ->
    wgpu::VertexFormat), webgl_compute_stride (explicit nebo tightly
    packed). Renderer::upload_webgl_buffer pro real GPU buffer cache.
    +9 testu.

## Velke remaining work

- **WebGL phase 3c3**: Connect dohromady - vertex layout pres VertexBufferLayout
  z helpers, build pipeline z modules + layout, real wgpu draw call. Vyzaduje:
  - Refactor paint_webgl_canvases na Renderer metodu (self.device + queue access).
  - Pri DrawArrays/Elements: lookup buffer + pipeline cache; pokud miss,
    build z webgl_shader_modules + helpers.
  - Per-canvas offscreen RT (Vec<wgpu::Texture> per canvas_ptr).
  - Composit canvas RT do swap chain pres image_atlas / new compose pass.
  - Bind group pro uniform buffer (kazdy DrawArrays write_buffer pred draw).
  Scope: 400-700 radku, prevazne refactor existing paint_webgl_canvases
  + render-pass encoding logiky.
- **Filter v Transform RT (nested)**: aktualne filter inside transform
  je inner cmds bez efektu - lepsi pristup vyzaduje rekursi v draw_segments.
- **Filter v Transform RT (nested)**: aktualne filter inside transform
  je inner cmds bez efektu - lepsi pristup vyzaduje rekursi v draw_segments.
- **TypeScript kompilator** - design konzultace stale otevrena.

## Test runner

```bash
powershell -ExecutionPolicy Bypass -File run_tests.ps1   # Win
./run_tests.sh                                            # Linux/Mac
```

Vystup `test_logs/test-<ts>.log` + `failures-<ts>.log`. Exit 0 OK, 1 fail.

## Spusteni prohlizece (testovaci browser)

```bash
cargo run -- browser                                # default static/test.html
cargo run -- browser stranka.html                   # vlastni HTML
cargo run -- window stranka.html                    # alias
cargo run -- devtools stranka.html out.html         # DevTools panel HTML
cargo run -- debug skript.js out.html               # Token+AST viewer

# Test stranky per modul:
cargo run -- browser static/css_modules/filter_effects/index.html
cargo run -- browser static/css_modules/animations_l1/index.html
cargo run -- browser static/css_modules/svg_basic/index.html
cargo run -- browser static/css_modules/gradients/index.html
```

## Hlavni stav prohlizece

**Real implementovane render/runtime** (NE jen parser):
- WGSL shader 9 modu: solid, text, linear gradient, shadow, image, inset shadow,
  radial grad, conic grad, blurred (mode 8 smoothstep edge)
- Filter drop-shadow render
- Filter blur shader-side mode 8 (single element edge blur)
- **Filter blur 2-pass gauss pipeline** - 9-tap separable, 2 offscreen RTs
  ping-pong, blur shader vs_main fullscreen triangle + fs_main 9-tap.
  `run_blur_passes(radius)` hotova. Orchestrace s filter element TODO.
- Real 2D rotation post-process (cos/sin matrix kolem centroid)
- 3D rotate aproximace (axis Z = 2D, X/Y = scale-based squeeze)
- Translate/Scale single + chain
- Counter API runtime (counter() resolve)
- ::before/::after pseudo (s content/attr/counter)
- ::first-letter + ::first-line text split
- list-style-type marker (8 stylu: disc/circle/square/decimal/decimal-leading-zero/
  upper-roman/lower-roman/upper-alpha/lower-alpha)
- list-style-image (url marker)
- text-decoration solid/double/dotted/dashed/wavy
- outline render (mimo border)
- Position: sticky runtime
- Anchor positioning runtime
- Scroll-driven animations runtime
- pointer-events: none hit-test skip
- direction: rtl text-align default = right
- Per-text font lookup (GlyphAtlas (family, char, size))
- @font-face FS load + Font registry
- Glyph + Image atlas rendering
- transition events dispatch (transitionend)
- animation events (animationstart/-end/-iteration)
- Form submit real POST (ureq)
- Multiple backgrounds tiling
- Multi-layer cascade (Layers / Pseudo / Container Queries)

**Parser-only** (vyzaduji wgpu RT pipeline orchestration):
- Filter blur na cely subtree - **RT setup + blur shader + run_blur_passes hotov**,
  zbyva: capture scene region do RT pri filter blur element, run blur, composit
- Filter na cely subtree obecne (RT capture)
- 3D perspective shader - matrix uniform per-vertex
- Polygon clip-path - shader stencil/SDF
- Hue-rotate / saturate / contrast filtry na cely subtree
- WebGL real render (jen stub - velky modul, cele OpenGL ES 2.0 mapping)

**JS API kompletni:**

DOM (130+ properties + methods):
- element/document append/prepend/before/after/replaceWith/remove/insertAdjacentHTML
- cloneNode, contains, getBoundingClientRect, hasAttribute/removeAttribute/toggleAttribute
- classList (add/remove/toggle/contains), dataset (kebab->camel)
- matches/closest, namespaceURI/localName/prefix
- previousElementSibling/nextElementSibling, firstElementChild/lastElementChild
- childElementCount, isConnected, ownerDocument
- HTMLAnchorElement url parts (protocol/host/port/origin/...)
- HTMLLabelElement.control + htmlFor
- HTMLOptionElement.text/label/defaultSelected
- HTMLSelectElement.options/selectedIndex/selectedOptions
- HTMLTableElement.rows + tr.cells
- HTMLDialogElement (show/showModal/close), HTMLDetailsElement.open
- HTMLMediaElement (play/pause/load/currentTime/duration/paused/muted/volume)
- HTMLInputElement (validity/select/setSelectionRange/...)
- HTMLTemplateElement.content
- HTMLElement.style (setProperty/getPropertyValue/removeProperty)
- innerHTML/outerHTML getter+setter
- form-controls.form / .labels / submit() real POST

Modern Web APIs:
- Canvas 2D (getContext + fillRect/strokeRect/clearRect/fillText +
  beginPath/moveTo/lineTo/arc/closePath/stroke/fill)
- WebGL stub (canvas.getContext('webgl') - constants + 40+ no-op methods)
- ResizeObserver/IntersectionObserver/MutationObserver/PerformanceObserver stuby
- requestAnimationFrame/cancelAnimationFrame/queueMicrotask
- customElements (define/get/whenDefined/upgrade)
- new CSSStyleSheet(), new URL(), new URLSearchParams()
- new Headers(), new FormData(), new Blob()
- localStorage / sessionStorage (in-memory + length)
- navigator (userAgent/language/platform/clipboard/geolocation/...)
- TextEncoder/TextDecoder
- crypto (randomUUID/getRandomValues/subtle stubs)
- performance (now/timeOrigin/mark/measure)
- AbortController + AbortSignal
- history (pushState/replaceState/back/forward/state/length)
- WebSocket / EventSource / BroadcastChannel stuby
- IndexedDB (open/deleteDatabase/databases stubs)
- new FontFace(family, src) + document.fonts (FontFaceSet)
- document.startViewTransition + 16+ document props (readyState/visibilityState/
  hidden/title/URL/dir/...)

CSS:
- Selectors L4 (:is/:where/:not/:has/~/nth-*/of-type/empty)
- Form pseudo (:required/:optional/:disabled/:enabled/:checked/:read-only/
  :read-write/:placeholder-shown/:valid/:invalid/:default)
- Color L4 (oklch/oklab/lab/lch/hsl/hwb/color-mix/modern syntax)
- Values L4 (min/max/clamp/env + math fci L4)
- Logical Properties L1
- Animations L1+L2 (fill-mode/play-state/cubic-bezier/steps/iteration)
- Nesting L1 (& selector)
- Container Queries L1 (cq* units)
- Cascade Layers @layer
- Box-shadow inset
- Radial+conic gradients
- Transitions L1 (parser + state diff + interpolace)
- Filter Effects parser + drop-shadow + CPU color matrix + blur shader
- Pseudo-Elements ::before/::after/::first-letter/::first-line
- Backgrounds L3 (multi-layer + position/size/repeat/clip/origin)
- @font-face (FS runtime)
- SVG basic shapes (rect/circle/ellipse/line/text)
- Canvas tag default
- clip-path parser + CPU render (inset/circle/ellipse)
- text-shadow
- @media L4 (prefers-*/hover/pointer/range syntax)
- Math fci L4
- text-transform/aspect-ratio/text-decoration L4
- @scope/@supports/@starting-style/@page/@property/@import/@namespace/
  @counter-style/@font-feature-values/@document/@view-transition
- Anchor Positioning L1 (parser + runtime)
- Scroll-driven anims (parser + runtime)
- View Transitions parser
- Subgrid L2 (Display enum)
- Outline shorthand
- 100+ dalsi properties (font-stretch/-variant/-feature/-variation/
  ruby-position/quotes/mask-image/shape-outside/direction/writing-mode/
  content-visibility/contain-intrinsic-size/will-change/isolation/
  mix-blend-mode/pointer-events/user-select/caret-color/resize/
  touch-action/hyphens/tab-size/word-break/overflow-wrap/text-wrap/
  text-align-last/transform-style/perspective/backface-visibility/
  page-break-*/break-*/orphans/widows/counter-set/print-color-adjust/
  forced-color-adjust/math-style/math-depth/speak/speak-as/bookmark-*/
  string-set/object-fit/object-position/background-blend-mode/
  image-rendering/table-layout/border-collapse/border-spacing/
  caption-side/empty-cells/vertical-align/list-style-image/...)
- Display L3: Contents/ListItem/Table*/InlineFlex/InlineGrid/Subgrid/Ruby

## TODO zbyle

### Velke (vyzaduji wgpu pipeline orchestration)

1. **Filter blur orchestration** - integrace s filter blur element:
   - RT pipeline + shader + run_blur_passes ALREADY HOTOVA v render.rs
   - Zbyva:
     - V paint emit specialni FilteredGroup marker pri filter blur element
     - V render: mezi normal pass scoutat marker, switch encoder na RT_a
     - Po vykresleni elementu skupiny, zavolat run_blur_passes(radius)
     - Composit RT_a do hlavni swap chain (jen v regionu)
   - Slozite kvuli per-element render encoder switching

2. **Filter na cely subtree** (general) - same RT pipeline (blur, brightness,
   contrast, hue-rotate, saturate). Color matrix na RT_a sample.

3. **3D perspective shader** - matrix uniform per-vertex:
   - Pridat shader uniform 4x4 matrix
   - Vertex shader aplikuje pred clip-space transform
   - LayoutBox ulozi matrix per element
   - Per-element render with matrix uniform

4. **Polygon clip-path** - shader stencil:
   - Vyzaduje stencil buffer setup
   - Render polygon do stencil bufferu
   - Render element s stencil test enabled

5. **WebGL real render** - VELMI VELKE:
   - Track GL state (programs/buffers/textures/uniforms)
   - drawArrays/drawElements -> emit Vertex commands?
   - Compile WebGL shaders na WGSL? (HARD - WGSL syntax different)
   - Stack multiple GL contexts per canvas
   - Reasonable scope: jen stub pro JS code compatibility (uz mam)

### Mensi runtime
- Filter subtree (blur + color matrix) - vyzaduje #1 hotov
- text-emphasis render
- ruby layout (CJK)
- Subgrid real layout (taffy support)
- shape-outside real layout (text wrap)
- mask-image render
- backdrop-filter render (vyzaduje filter pipeline)
- Real custom elements upgrade callback
- transition events s detail (computed start/end values)

### TypeScript kompilator
**User pozadoval**: po dokoncenem prohlizeci prokonzultujeme.

Otazky:
- Scope: full TSC superset vs subset (jen parser + strip types)
- Type checking vs jen strip types -> JS
- Integrace s lexer/parser nebo vlastni front-end
- Vystup: JS string vs primy AST do interpreteru

## Pracovni flow

- Po fici: build + test (run_tests.ps1) + commit
- Commit cesky, ASCII, "co + proc"
- Pri nejasnosti: zeptat se A/B/C
- Komunikace cesky CAVEMAN MODE
- CSS modul: testy + static/css_modules/<name>/
- Aktualizovat TODO_CSS.md

## Klicove soubory

- `src/main.rs` - CLI rezimy
- `src/browser/cascade.rs` (~2200 lines) - cascade + animations + transitions
  + Math L4 + cascade_pseudo + form pseudo + apply_scroll_animations
- `src/browser/css_parser.rs` - Stylesheet (vsechny at-rules + range queries +
  parse_selectors pub)
- `src/browser/layout.rs` (~3500 lines) - LayoutBox (140+ fields) +
  build_box_inner (counter state) + pseudo virtuals (before/after/first-letter/
  first-line/marker) + apply_anchor_positioning + apply_sticky + parsers
  vsech CSS properties + parse_color (Color L4) + parse_*_gradient +
  parse_filter_chain + apply_filter_chain + parse_clip_path + parse_box_shadow +
  parse_text_shadow + transform chain (3D ops) + Display L3 enum
- `src/browser/render.rs` (~2000 lines) - winit + wgpu, GlyphAtlas family lookup,
  ImageAtlas, font_registry, canvas paint_canvas_ops, **2 offscreen RTs +
  BLUR_SHADER WGSL + blur_pipeline + run_blur_passes**, transition events
  dispatch, animation events + iteration, find_node_by_ptr
- `src/browser/paint.rs` - DisplayList (vc CanvasOp + 3D transform aplikace +
  rotate_cmd/scale_cmd/shift_cmd) + emit_svg_children + filter chain + clip
  rect compute + drop-shadow emit + outline emit
- `src/interpreter/mod.rs` (~4400 lines) - Interpreter, JsValue, DomNode dispatch
  (130+ properties + methods), style/classList/dataset/canvas/form helpers,
  parse_url_parts, dispatch_event pub method
- `src/interpreter/builtins.rs` (~2700 lines) - globals (Math/JSON/Date/Intl/
  fetch/Worker/setTimeout/setInterval/raf/queueMicrotask/localStorage/
  sessionStorage/customElements/URL/URLSearchParams/Headers/navigator/
  observers/TextEncoder/TextDecoder/crypto/performance/FormData/Blob/
  AbortController/history/WebSocket/EventSource/BroadcastChannel/IndexedDB/
  FontFace/document.fonts/document.startViewTransition + 16 doc props)

## Co necist hned (velke soubory)

- `src/interpreter/builtins.rs` (~2700)
- `src/browser/cascade.rs` (~2200)
- `src/browser/layout.rs` (~3500)
- `src/browser/render.rs` (~2000)
- `src/interpreter/mod.rs` (~4400)
- `src/debug_view/devtools.rs` (>500)

## Test stranky

- `static/test.html` + `test.css` - hlavni univerzalni demo (13+ sekci:
  typografie/barvy/animace/pseudo/filter/color L4/gradients/SVG/rotation/
  text-decoration styles/lists/outline)
- `static/css_modules/<modul>/index.html` - 19+ per-feature stranek

## Dalsi krok pri pokracovani

User: "vsechno krome TS, pak prokonzultujeme TS".

Doporucene volby pro velke zbyle:
- **A)** Filter blur orchestration (capture scene -> RT, blur, composit zpet)
  RT pipeline UZ HOTOVA v render.rs
- **B)** Filter na cely subtree (general) - po A, color matrix na RT
- **C)** 3D perspective shader (matrix uniform per-vertex)
- **D)** Polygon clip-path stencil
- **E)** WebGL real render (velmi velke - cely GL state mapping)
- **F)** TypeScript kompilator design konzultace (po prohlizeci hotov)

Po dokonceni RT-heavy zbylych = prohlizec **kompletni**. Pak TS.
