# Prechodovy plan - nove vlakno

Cti **driv nez zacnes**. Plus `CLAUDE.md`, `README.md`, `TODO_CSS.md`.

## Stav

- Build: **OK**, 0 errors, 0 warnings.
- Tests: **2181 unit testu**, 0 failed, 3 ignored.
- **Layout engine pod nasi kontrolou** - vlastni flex/grid v `src/browser/layout_engine/`.
- **168 layout unit testu** (flex_tests + flex_spec_tests + grid_tests + grid_spec_tests).
- **4108 taffy XML test fixtures** prevzato (MIT licence) v tests/fixtures/taffy_*/:
  - flex: 2212, grid: 1076, block: 820
- **Compliance harness** v src/browser/layout_engine/taffy_compliance.rs:
  - XML parser + LayoutBox converter + run_directory + compare_layout
  - 4 testy spousteji vsechny fixtury, vypocitavaji pass-rate
  - **Aktualni pass-rate: 1942/1988 (97.7%)**
    * BLOCK:  385/392 (98.2%)
    * GRID:   491/512 (95.9%)
    * FLEX:   1066/1084 (98.3%)
  - Iter 208 win: pre-pass: block s flex-direction treat as flex (+2 flex)
  - Iter 195-207 wins:
    * Grid auto-flow=column (column-major auto cursor + col extension)
    * Grid item h pri auto margin: intrinsic
    * Grid min-content track: pricti fixed margins
    * Stop skipping overflow tests
    * overflow-x/y + scrollbar-width parsing
    * Scrollbar takes space in flex/grid/block inner area
    * Flex container_cross subtract scrollbar_h
    * Block content height: pricti scrollbar
    * Abs item containing block subtract scrollbar
    * Block BFC blocks margin collapse with descendants
    * Block scrollbar takes space in auto-height
    * Flex item overflow!=visible -> auto-min-content=0
    * Flex container overflow!=visible: neexpanduje na content
    * Grid auto-track sizing item_min: overflow!=visible -> 0
    * Grid span item overflow hidden: item_min=0
    * Block BFC blocks empty-passthrough collapse
  - Iter 191-194 wins:
    * Grid placement negative: implicit cols PRED explicit (col_prepend)
    * Grid placement multi-pass per CSS Grid §8.5
    * Grid row prepend pro negative grid-row-start
    * Grid impl-before rows cycle order (reverze pres formula)
    * Grid item h pri auto margin: intrinsic (ne stretch)
    * Grid min-content track: pricti fixed margins (ne percent)
    Total +12 grid (447 -> 459), 1826 -> 1838
  - Iter 186-190 wins:
    * Track::MaxContent + MinContent variants v parse (rozliseni keywordu)
    * Span items distribute extra space (CSS §11.5.5):
      - Step 1: min_content do auto-class tracks
      - Step 2: max_content extra do tier1 (MaxContent), tier2 (FitContent), tier3 (Auto)
      - Run PRED redistribute leftover (jinak tracky uz inflated)
    * Implicit rows (rows nad explicit count) treat jako auto pro sizing
    * Span row > 1: distribute h do prvni auto recipient row v spanu
    Total +26 grid (421 -> 447)
  - Iter 162-185 wins (this session):
    * Flex cross intrinsic clamp by max-height (+2)
    * Pre-pass include own padding/border (+2)
    * Flex column needed h s margins (+2)
    * Skip percent-derived intrinsic mode (+6)
    * Row direction container_h override (+1)
    * inner_w/h floor by min-w/h v intrinsic (+2)
    * Recursive child_baseline walk (+2)
    * Baseline column = FlexStart fallback (+4)
    * Content-box conversion + min/max-w/h (+5)
    * Item stretch !pri baseline + line cross expand (+6)
    * Grid baseline post-pass recursive + row expand (+4)
    * Grid row track include vert margins (+4)
    * Grid baseline preserve relative offset (+2)
    * Grid baseline use pad_t (+2)
    * Grid minmax max/min-content sentinely (+6)
    * Grid row aspect-ratio dopocet z width (+2)
    * Grid text wrap pri non-stretch align-self (+2)
    * Block text wrap pri max-width (+2)
    * Flex column text wrap (+2)
    * Grid recursive deep_min_content (+2)
    * Grid row 0 fallback fix (+4)
    * Grid rows count vc. spans (+4)
    * Flex baseline first-child v line 1 (+2)
  - Iter 156-160 wins:
    * Pre-pass set rect to explicit + intrinsic_mode flag
    * grid no-template col -> auto track sizing (+2)
    * specified min applied pred wrap (incl. explicit basis) (+2)
    * grid baseline alignment per-row (+2)
    * grid stretch rows pri baseline + definite height (+16!)
  - Iter 148-155 wins:
    * Text intrinsic height v block layout (+2)
    * Aspect-ratio + text: max-h/w wins (+4)
    * Block s flex-wrap -> Flex heuristika (+2)
    * abs prepass block fallback flex-basis (+2)
    * Text content blocks empty passthrough (+4)
    * Text intrinsic pro abs items (+2)
    * Min applied pred wrap (no explicit basis) (+2)
  - Iter 143-147 wins:
    * Pre-pass include child margins v intrinsic (+4)
    * cross_offset = 0 pri item flex-wrap stretch (+4)
    * column_gap pct re-resolve proti inner_w (+2)
    * pseudo_flex flag pro baseline rozliseni (+2)
    * Per-item baseline first-child rule (+6)
  - Iter 134-142 wins:
    * Percent margin re-resolve v gridu (+12)
    * minmax maximize pred fr distribute (+2)
    * Justify negative free (overflow center) (+10)
    * Aspect-ratio clamp est_w/h pred derivation (+4)
    * Skip percent-derived widths v intrinsic propagation (+6)
    * Descendant_min jen pri flex-grow=0 (+6)
    * Descendant_min skip pri overflow (+2)
    * Percent row-gap = 0 pri indefinite parent (+2)
    * Flex-wrap item stretch cross axis (+2)
  - Iterace 0-133 progress: 18 -> 1017 (50%) -> 1392 (70%) -> 1490 (74.9%) ->
    1492 (75.0%) -> 1516 (76.3%) -> 1548 (77.9%) -> 1566 (78.8%) ->
    1578 (79.4%) -> 1588 (79.9%) -> 1592 (80.1%) -> 1602 (80.6%) ->
    1608 (80.9%) -> 1616 (81.3%) -> 1620 (81.5%) -> 1628 (81.9%)
  - Iter 125-133 wins:
    * Baseline alignment v flex (synth bottom + first-child fallback heuristika)
    * Block s align-items=baseline -> implicit flex
    * Grid auto margin + text intrinsic (centruje text v cell)
    * Grid auto-fit collapse empty tracks + space-evenly active count
    * Descendant max-width contributes do flex item min_main
    * minmax(min-content/max-content, ...) v gridu (NaN sentinel)
    * fit-content() track sizing s clamp(min, max(min, arg), max)
    * Minmax leftover redistribute respects item-driven min
  - Iter 100-123 wins:
    * inset percent top/bottom = 0 pri auto parent height
    * abs auto margin LTR over-constrained: left=0
    * block intrinsic v abs pre-pass: max child w, sum heights
    * taffy_mode flag: skip 20px default v leaf empty divech
    * grid fr-track expansion (item explicit_w > track -> expand)
    * grid fr multi-span items (distribute extra)
    * grid 0fr s span items (equal split)
    * grid auto margins v gridu (center/push override)
    * flex pre-pass block fallback recurzy (gc flex/grid/block)
    * flex wrap-reverse cross axis flip
    * taffy_mode block: nepresahnout pre-set parent height
    * block sibling margin collapse: max(pos)+min(neg)
    * block first/all-children chain margin collapse
    * flex abs static wrap-reverse cross flip
    * flex abs default cross Stretch -> FlexStart
    * grid pre_compute_fixed: handle repeat(N,...)
    * abs display:none -> zero out (flex + grid)
    * grid-auto-rows/columns/flow parsed
    * Text intrinsic v taffy_mode: 10px/char (parser sbira text content)
    * Flex est_w/est_h pres text bx.text + 10/char
    * Flex min_main floor: longest unbreakable segment z text (CSS auto-min-content)
    * text-align na block children: Right -> free_x, Center -> free_x/2
  - Implementovano:
    * Position absolute/fixed (CB padding-box, top/left/right/bottom + inset)
    * Asymmetric padding/border/margin per side
    * Aspect-ratio override v abs sizing
    * Min/max width/height v abs (min wins nad max)
    * Margin auto v block + flex (centrovani)
    * Flex flex-grow/shrink CSS spec algoritmus (freeze violators)
    * Flex flex-basis (px/percent/auto/content)
    * Flex align-items default Stretch (CSS spec)
    * Flex single-line cross = container_cross
    * Flex multi-line align-content stretch + packing
    * Flex align-self per item override
    * Flex item margins (main + cross axis)
    * Flex abs static position podle justify/align/align-self
    * Grid track sizing s explicit + implicit rows
    * Grid placement (grid-row/column-start/end + span)
    * Grid auto-flow row order s tracking occupied cells
    * Grid item self alignment (justify-self, align-self)
    * Grid abs s self alignment
    * Grid negative free space (overflow rendering)
    * Box-sizing content-box (default) vs border-box
    * <text> tag jako node v parseru
    * Display:none -> 0x0
    * Default display per directory (taffy_flex/grid/block)
  - Co zbyva pro >70%: intrinsic content sizes (shrink-to-fit pro abs bez size),
    baseline alignment, RTL direction, percent v all contexts (margin %),
    flex item shrink se vsemi spec edge cases, text measurement.
- Tree: ciste.
- Branch master, ~290 commitu pred origin/master (NEPUSHOVAT bez vyzvy).
- **WebGL pipeline DOKONCEN** + **CSS L4-L6 KOMPLET** + **JS DOM kompletni**:
  - CustomElements lifecycle, MutationObserver real callback
  - File / FileList / form.elements / form.length
  - form submit event + preventDefault chain
  - ResizeObserver / IntersectionObserver real targets tracking
  - Date arithmetic, Date.UTC, Date.parse, multi-arg constructor
  - String.isWellFormed / toWellFormed (ES2024)
  - @scope real scoping, @function runtime, container queries clean
  - 17 modernich CSS units (dvw/dvh/svw/lvh/vi/vb/ch/lh/rlh/cm/mm/Q/in/pc)
  - Color L5 relative + L4 color() namespace
  - Canvas 2D API rozsireni: save/restore + transforms (translate/rotate/scale/
    setTransform/transform/resetTransform) + curves (quadraticCurveTo/bezierCurveTo/
    arcTo) + paths (rect/roundRect/ellipse) + clip/strokeText/measureText +
    setLineDash/getLineDash + drawImage (3/5/9-arg) + createLinearGradient/
    createRadialGradient s addColorStop + createImageData/getImageData/putImageData +
    isPointInPath/isPointInStroke. CanvasOp enum + render paint_canvas_ops update.
  - SVG support kompletni: rect/circle/ellipse/line/text + polygon/polyline/path/g.
    parse_svg_path s M/L/H/V/Z/C/Q (relative + absolute), Bezier endpoint emit,
    group recursion. parse_svg_points helper.
  - Gradient render: radial (mode 6) + conic (mode 7) + linear (mode 2) WGSL.
    Inset box-shadow (mode 5) full SDF impl.
  - GPU image rendering: ImageAtlas RGBA s shelf packing, 2048x2048 atlas.
    HTTP fetch pres ureq sync (10s timeout), data: URI base64 dekoder
    (self-contained), FS fallback. WGSL mode 4 sample image_tex.
    push_image emit + UV bounds tracking.
  - DOM/JS modern APIs: EventTarget constructor, MessageChannel/MessagePort,
    Notification + permission, ServiceWorker container stub, navigator.locks,
    requestIdleCallback + IdleDeadline, AbortSignal.timeout/any/abort statics,
    document.adoptedStyleSheets pool.
  - JS Iterator helpers (ES2025): Iterator.prototype.toArray/map/filter/take/
    drop/reduce/forEach/some/every/find/flatMap pres __iterator_helpers__ flag.
  - JS Temporal API stub (TC39 Stage 3): Temporal.Now (instant/plainDateISO/
    plainTimeISO/zonedDateTimeISO), PlainDate.from, Duration.from,
    Instant.fromEpochMilliseconds.
  - DOM Event classes (22): Event/CustomEvent/MouseEvent/PointerEvent/
    KeyboardEvent/TouchEvent/WheelEvent/InputEvent/FocusEvent/DragEvent/
    SubmitEvent/ProgressEvent/MessageEvent/ErrorEvent/HashChangeEvent/
    PopStateEvent/StorageEvent/AnimationEvent/TransitionEvent/ClipboardEvent/
    BeforeUnloadEvent/PageTransitionEvent. preventDefault skutecne nastavi
    defaultPrevented=true.
  - DOM HTMLDialogElement.close(returnValue) + dispatch close event.
  - DOM Range API real: setStart/setEnd ulozi state, collapse/cloneRange/
    selectNode/selectNodeContents real impl.
  - DOM Selection API real: addRange/removeRange/removeAllRanges/collapse/
    selectAllChildren state tracking.
  - DOM Clipboard API stub: writeText/readText (in-memory) Promise-based.
  - DOM Geolocation API stub: getCurrentPosition/watchPosition/clearWatch.
  - JS String.matchAll(regex) -> iterator nad matches.
  - Web Crypto SubtleCrypto: digest (FNV-1a x4), encrypt/decrypt/sign/verify/
    generate/import/export/derive/wrap/unwrap stuby.
  - Shadow DOM: element.attachShadow({mode}) + shadowRoot getter.
  - Web Animations API: element.animate(keyframes, options) -> Animation s
    play/pause/cancel/finish/reverse + finished/ready Promise.
  - Web Platform APIs: Permissions / WakeLock / Vibration / Gamepad /
    Battery / Sensors (Accelerometer/Gyroscope/Orientation/Magnetometer/
    AmbientLightSensor) / WebAuthn (PublicKeyCredential, credentials) /
    TrustedTypes / FileSystemAccess (showOpenFilePicker/showSaveFilePicker/
    showDirectoryPicker) / WebMIDI / SpeechSynthesis + Recognition /
    Bluetooth/USB/HID/Serial requestDevice.
  - JS ArrayBuffer ES2024: transfer / resize / slice / detached / maxByteLength.
  - JS DataView: getUint8/setUint8/getInt8/getUint16(LE+BE).
  - Web Streams: ReadableStream/WritableStream/TransformStream s reader/writer
    lifecycle + pipeTo/pipeThrough/tee.
  - Compression Streams: CompressionStream/DecompressionStream stuby.
  - Cookie Store API real: get/set/delete/getAll s in-memory storage.
  - Typed Arrays kompletni: Uint8/Int8/Uint8Clamped/Uint16/Int16/Uint32/Int32/
    Float32/Float64/BigInt64/BigUint64 s BYTES_PER_ELEMENT + byteLength.
  - HTML form elements: HTMLProgressElement (value/max/position),
    HTMLMeterElement (value/min/max/low/high/optimum), HTMLDataListElement.options,
    HTMLSelectElement.selectedIndex, HTMLAnchorElement.relList, Element.popover.
  - Popover API: showPopover/hidePopover/togglePopover.
  - Atomics extras: wait/waitAsync/notify/pause/isLockFree/load/store/exchange/
    and/or/xor (real impl s SharedArrayBuffer __bytes__).
  - URL.canParse / URL.parse (ES2024+).
  - TextDecoder s {fatal, ignoreBOM} options + accepts ArrayBuffer.
  - TextEncoderStream / TextDecoderStream stubs.
  - Performance API real: mark/measure entries (HashMap + Vec), getEntries/
    getEntriesByType/getEntriesByName, clearMarks/clearMeasures s name filter.
  - FormData real: append/set/get/getAll/has/delete/keys/values/entries iterators.
  - Headers (Fetch API): get/set/append (combine ", ")/has/delete/entries case-insens.
  - Request constructor: url/method/body/cache/credentials/mode/redirect/referrer.
  - DOM Geometry: DOMRect/DOMRectReadOnly/DOMPoint/DOMPointReadOnly/DOMMatrix/
    DOMMatrixReadOnly/DOMQuad s a..f / m11..m44 / multiply/inverse/translate/scale.
  - Console extras: trace/table/group/groupCollapsed/groupEnd/time/timeEnd/timeLog/
    count/countReset/assert/dir/dirxml/clear/profile/timeStamp s real timer + counter state.
  - DOMException: name/message/code mapping (NotFoundError=8, QuotaExceededError=22, atd.).
  - ImageData/OffscreenCanvas/createImageBitmap/Path2D constructors.
  - Element extras: checkVisibility/requestFullscreen/requestPointerLock/
    attachInternals (ElementInternals s validity)/computedStyleMap.
  - Visual Viewport / Web Share / Badging / Contacts / Background Sync /
    Push Manager / ReportingObserver / WebTransport stubs.
  - DOM constructors: DocumentFragment / Comment / Text / CDATASection /
    Node interface (13 NODE_TYPE constants), MutationRecord / HTMLCollection /
    NodeList / DOMTokenList (real add/remove/toggle/contains state).
  - SharedWorker constructor s port (MessagePort-like).
  - DOM Element ctors: Image (width, height) / Audio (src) / Option (text, value).
  - DataTransfer (drag-drop): setData/getData/clearData/types/files.
  - StorageManager: estimate/persist/persisted Promise.
  - PerformanceObserver / PerformanceEntry constructors.
  - Symbol well-known: iterator/asyncIterator/toPrimitive/hasInstance/
    isConcatSpreadable/match/matchAll/replace/search/split/species/toStringTag/
    unscopables/dispose/asyncDispose/metadata.
  - call_new fix: native ctors mohou vratit DomNode/Array/Map/Set (ne jen Object).
  - Crypto SHA-1 / SHA-256 real Rust impl (FIPS 180-4) + SHA-384 / SHA-512 derived.
    crypto.subtle.digest("SHA-256", data) -> ArrayBuffer real.
  - Typed Array methods: subarray/set/fill/slice/copyWithin/indexOf/includes/
    reverse/join + buffer view + byteOffset.
  - CSS Color L5: contrast(<bg> vs <list>) / contrast-color() s relative luminance,
    light-dark() function.
  - DataView complete: getUint16/getInt16/getUint32/getInt32/getFloat32/getFloat64 +
    setUint16/setUint32/setFloat32/setFloat64 (little/big endian).
  - Refactor: dom_api_tests.rs split do dom_api_tests + dom_api_modern_tests.

## Test runner

```bash
powershell -ExecutionPolicy Bypass -File run_tests.ps1   # Win
./run_tests.sh                                            # Linux/Mac
```

Vystup `test_logs/test-<ts>.log` + `failures-<ts>.log`.

## Spusteni prohlizece

```bash
cargo run -- browser                            # default static/test.html
cargo run -- browser stranka.html               # vlastni HTML
cargo run -- devtools stranka.html out.html     # DevTools panel HTML
cargo run -- debug skript.js out.html           # Token+AST viewer
```

## Posledni dokoncene faze (recent batch CSS L4-L6 + JS DOM)

### Commit b7dec02 -> 64d12e8: CSS L4-L6 hromadne + DOM CustomElements lifecycle (+178 testu)

**JS DOM CustomElements lifecycle** (b7dec02):
- `connectedCallback / disconnectedCallback / attributeChangedCallback`
- shared `custom_elements` registry, instances per node ptr
- `document.createElement` volá konstruktor, ulozi instance
- `appendChild / removeChild / setAttribute` spousti lifecycle metody
- run_super_constructor: native funkce (HTMLElement) jako super = no-op

**CSS Color L5 relative + L4 color() namespace** (b7dec02):
- `rgb(from c r g b)` / `hsl(from c h s l)` s keyword substitution
- calc(r * 0.5), proenta, none podpora
- `color(srgb r g b)` / display-p3 / rec2020 / a98-rgb / prophoto-rgb / xyz / xyz-d50 / xyz-d65
- XYZ -> sRGB matice transformace

**CSS Units L4** (714e72d):
- dvw/dvh, svw/svh, lvw/lvh dynamic viewport
- vi/vb logical viewport
- ch/lh/rlh/ex character/line-height
- cm/mm/Q/in/pc absolutni jednotky

**CSS at-rules** (714e72d, f8c2bc7, 4e8279c, 64d12e8):
- @scope (root) [to (limit)] { rules } - ScopeRule struct
- @starting-style { rules } - parsovan do starting_style_rules
- @property --name { syntax/inherits/initial-value } + cascade integrace
- @font-palette-values --name { font-family/base-palette/override-colors }
- @counter-style name { system/symbols/suffix/prefix/range/pad/fallback/negative }
- @view-transition { navigation } global config
- @page { ... } per-page declarations
- @function --name(<args>) returns <type> { ... } (CSS Functions L1)
- @supports condition: selector(...), font-tech(...), font-format(...),
  not, and, or operatory s top-level paren handling

**CSS pseudo-classes** (b7dec02, 4e8279c):
- :user-valid / :user-invalid (Selectors L5) s data-* attribute
- :popover-open / :open / :closed / :modal / :fullscreen / :blank
- ::placeholder / ::selection / ::backdrop matching tests

**CSS values + functions** (64d12e8):
- if(<test>, <if-true>, <if-false>) (Values L5) - literal test
- attr(name <type>, fallback) typed (uz drive)

**CSS properties batch** (714e72d, f8c2bc7, 4e8279c, 64d12e8):
- background-clip: text (paint potlaci box bg)
- border-image-source/slice/width/repeat
- mix-blend-mode / background-blend-mode
- text-emphasis style/color shorthand
- text-decoration-skip-ink, text-spacing, text-autospace, initial-letter
- field-sizing (Forms L1), interpolate-size (Animations L2)
- grid-template-columns/rows s named lines, grid-template-areas
- grid-area / grid-column / grid-row / grid-auto-*
- shape-outside / shape-margin / shape-image-threshold
- scrollbar-gutter, marker-start/mid/end (SVG)
- background-position-x/y, image-orientation
- hyphenate-character, hyphenate-limit-chars, text-box-trim/edge
- inset shorthand 1/2/3/4 hodnot
- position-area, position-try-fallbacks, anchor-default
- ruby-overhang, ruby-merge, math-shift
- transition-behavior: allow-discrete
- animation-composition: replace/add/accumulate
- color-interpolation
- timeline-scope, animation-range-start/end, scroll-marker-group
- contain-intrinsic-block-size / -inline-size

**Container Queries L1** (b7dec02):
- cascade_with_container_sizes pro per-element ancestor lookup
- container_sizes mapa node ptr -> (w, h)

## Posledni dokoncene faze (predchozi)

### WebGL kompletni pipeline (phases 1-3c9)
JS gl.* calls -> WebGLState -> execute_webgl_canvas extract -> upload
buffers/textures + shader modules + full resources -> serialize uniforms ->
ensure pipeline (s BGL pres bindings) -> build bind group (uniform/textures/
samplers entries) -> encode draw / draw_indexed -> compose canvas RT do
swap chain bbox.

Components:
- WebGLState (uniforms HashMap, texture_units, programs, buffers, draw_queue)
- naga GLSL frontend + WGSL backend (preprocess ES1 -> 450 core)
- extract_uniform_layout + extract_resource_bindings z naga IR
- webgl_serialize_uniforms (Float/Vec/Mat std140 layout)
- Per-program pipeline + uniform buffer + bind group layout cache
- Per-texture wgpu Texture + view cache
- Per-canvas offscreen RT + composit pres transform_pipeline

### Render features hotove
- Filter blur 2-pass gauss WGSL + RT pipeline
- Filter color matrix subtree (hue/saturate/grayscale/sepia/invert/contrast/
  brightness/opacity) shader-based
- Drop-shadow paint
- 3D perspective shader (4x4 matrix + perspective divide)
- Polygon clip-path ear-clipping triangulace (convex i concave)
- text-decoration 5 stylu
- list-style-type 8 stylu + list-style-image
- outline render
- ::first-letter / ::first-line / ::marker / ::before / ::after
- Counter API runtime (counter() v content)
- Anchor positioning + position:sticky + scroll-driven animations
- direction:rtl text-align default

### CSS Hotove (TODO_CSS.md)
- Selectors L4 (vetsina pseudoclasses, :is/:where/:not/:has, ~ general sibling)
- Values L4 (calc/min/max/clamp/env, var() s fallback)
- Color L4 (oklch/oklab/lab/lch/hsl/hwb/color-mix, modern rgb)
- Animations L1 + Transitions L1 (transitionend dispatch)
- Logical Properties L1
- Nesting L1 (& selector)
- Container Queries L1 (parser, viewport approximation)
- Filter Effects L1 (vsechny filtry shader-based)
- @font-face / FontFace API
- Cascade Layers @layer (parser hotov, runtime ordering missing)
- Position L3 sticky
- Masking L1 clip-path (vc polygon)
- Transforms L2 3D
- Anchor Positioning L1
- Scroll-driven Animations L1
- View Transitions L1 (stub)

## CSS PRIORITY pro dalsi session

### Top priority (max ROI)
1. **backdrop-filter** - reuse existing blur RT pipeline, samples scene background
2. **mask-image** + mask-* family (RT alpha mask)
3. **writing-mode: vertical-rl/lr** - layout sirka/vyska swap, text rotace
4. **::placeholder, ::selection, ::backdrop** pseudo-elementy
5. **Cascade Layers @layer runtime ordering** (parser hotov)
6. **attr(name <type>, fallback)** - typed attr resolution

### Mensi
7. **system-color keywords** L4
8. **relative color**: `rgb(from c r g b)` L5
9. **ch/lh/rlh/vi/vb/dvw/dvh/svw/lvh** units
10. **Q/cm/mm/in/pc** absolute units
11. **min-content/max-content/fit-content** keywords
12. **|| column combinator** (table cells)
13. **:lang(), :dir()** pseudo-classes
14. **:valid, :invalid** runtime form validation
15. **forced-color-adjust**
16. **scrollbar-gutter**, **content-visibility**, **contain-intrinsic-size**
17. **@property registrace** + animatable custom properties
18. **@import url() layer(name)** s layer support
19. **revert / revert-layer / unset** keywords
20. **Container queries per-element ancestor lookup** (aktualne viewport)

### Velke (deferred)
- **Subgrid L2**
- **Shapes L1: shape-outside**
- **Masonry layout L3 draft**
- **Color L4 color() namespace** (display-p3/rec2020)
- **Houdini** (Paint/Layout/Properties/Typed OM) - out of scope

## JS interpreter chybejici (impl gaps z testu)

- Math.sign / cbrt / log2 / log10 / exp / tan / atan2 / trunc / hypot
- Date.getUTCFullYear / getUTCMonth / Date arithmetic difference
- String.prototype.concat / substr
- Symbol() callable constructor (aktualne objekt)
- Array spread/object spread v expression literalu
- Array.length assignment truncation
- Array negative index, Array.includes(NaN)
- for-in proper iteration order
- Object key order (numeric ascending sort)
- Array destructure swap
- String.length UTF-16 code units pro emoji
- parseFloat trailing garbage
- Math.floor(-1.5) correctness
- JSON.stringify circular reference
- Object.isExtensible / isFrozen
- Error.toString format

## DOM API chybejici

- Form validation runtime (:valid/:invalid)
- Custom Elements lifecycle (connectedCallback/disconnectedCallback)
- Shadow DOM (attachShadow + slot)
- IntersectionObserver real (aktualne stub)
- ResizeObserver real
- MutationObserver real
- localStorage FS persistence
- Selection / Range API
- document.cookie domain/path
- HTMLCanvasElement.toDataURL real
- HTMLInputElement.files (FileList)

## Render chybejici

- HTTP @font-face / image load (FS only aktualne)
- Vertical text layout (writing-mode)
- Subpixel antialiasing
- Multiple texture binding mimo single image atlas
- Multi-monitor / DPI scaling
- Print rendering (page-break)

## Architektura

- Filter v Transform RT (nested) - rekurze v draw_segments TODO
- TypeScript kompilator (design konzultace neproběhla - max scope)
- Performance benchmarks
- Dirty rect tracking pro paint
- Incremental layout reflow (aktualne always-from-scratch)

## Test pokriti

1578 testu, 0 failed, 3 ignored. Pure-logic 100% (color matrix, filter
chain, polygon, lexer/parser, naga extract). GPU integration testy
vyzaduji headless wgpu adapter setup (TODO).

Test soubory:
- src/lexer/base.rs (mod tests inline) - 35+ testu
- src/parser/mod.rs (mod tests inline) - 80+ testu
- src/browser/render.rs (mod tests inline) - 4 testu
- src/browser/tests/ - 6 souboru s ~150 browser testy
- src/interpreter/tests/ - 30+ souboru s ~1300+ JS/DOM/WebGL testy

## Workflow

- Cesky komentare + commits
- ASCII only v kodu (bez `->`, `…`, `–`, smart quotes)
- Build + test + commit per feature
- Cargo.toml deps maji komentar **proc**
- Pri nejistote zeptat se A/B/C variant pred kodom
- CAVEMAN MODE komunikace - terse fragments, drop articles/filler

## Klicove soubory

```
src/main.rs              - CLI rezimy (debug/devtools/browser/window/default)
src/tokens.rs            - TokenKind enum
src/ast.rs               - AST nodes
src/lexer/               - JS lexer
src/parser/mod.rs        - JS parser (recursive descent)
src/interpreter/mod.rs   - Tree-walking interpreter (~7000 radku)
  - WebGLState, WebGLProgram, UniformSlot, extract_uniform_layout
  - extract_resource_bindings, extract_texture_sampler_counts
src/interpreter/builtins.rs - globalni objekty (Math, JSON, Promise, ...)
src/browser/html_parser.rs  - html5ever wrapper
src/browser/dom.rs           - DOM nodes
src/browser/css_parser.rs    - cssparser wrapper + at-rules
src/browser/cascade.rs       - selector matching + var/calc resolve
src/browser/layout.rs (~3500 radku) - box model + taffy + filter parsing
  - compute_color_matrix, compute_transform_matrix
  - parse_clip_path / FilterOp / TransformOp / ClipPath
src/browser/paint.rs (~900 radku) - DisplayList builder
  - DisplayCommand enum (Rect/Border/Text/Gradient/Shadow/Image/
    BlurredRect/FilterBegin-End/TransformBegin-End/ClippedRect)
  - paint_box recursive emit
src/browser/render.rs (~3200 radku) - wgpu Renderer
  - draw_full_frame (single-frame webgl integration)
  - draw_segments_into_view (Main/Filter/Transform3D)
  - run_webgl_frame -> walk_webgl -> execute_webgl_canvas
  - webgl_serialize_uniforms / extract_resource_bindings
  - build_webgl_bind_group / ensure_webgl_full_resources
  - WGSL shaders: BLUR, COMPOSE, TRANSFORM, RECT
```

## Build / test

```bash
cargo build              # Dev profile, debuginfo
cargo test               # Vsechny unit testy (1578 passed)
cargo run -- browser     # static/test.html
```
