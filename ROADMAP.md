# RustWebEngine Roadmap

Stav kazde produkcni feature + odhadovana prace + references.

## Session 2026-05-20 status

**2993 testu pass.** 22+ novych foundation modulu pridanych:

| Modul | Lines | Tests | Status |
|-------|-------|-------|--------|
| `scroll_anim.rs` | 230 | 10 | accumulative retarget hotovo, real fix bug |
| `interpreter/gc.rs` | 175 | 5 | cycle collector foundation |
| `paint.rs` blend markers | +30 | - | mix-blend-mode emit |
| `testing/wpt.rs` | 215 | 4 | WPT harness + testharness.js subset |
| `testing/reftest.rs` | 115 | 4 | pixel diff utility |
| `security/csp.rs` | 215 | 8 | CSP3 parser + enforce |
| `security/cors.rs` | 145 | 8 | CORS preflight + headers |
| `security/hsts.rs` | 130 | 5 | HSTS upgrade store |
| `security/cookies.rs` | 175 | 6 | SameSite Strict/Lax/None |
| `embed/pointer_events.rs` | 110 | 3 | unified pointer model |
| `embed/sandbox.rs` | 115 | 4 | per-OS sandbox foundation |
| `interpreter/service_worker.rs` | 115 | 5 | SW registry + lifecycle |
| `interpreter/wasm.rs` | 175 | 4 | WAAPI surface |
| `interpreter/streams.rs` | 155 | 6 | Readable/Writable/Transform |
| `interpreter/broadcast_channel.rs` | 115 | 4 | per-origin pub/sub |
| `interpreter/push_api.rs` | 95 | 3 | Push permissions + subscribe |
| `interpreter/webrtc.rs` | 165 | 4 | RTCPeerConnection + DataChannel |
| `browser/view_transitions.rs` | 110 | 4 | startViewTransition API |
| `browser/tree_walker.rs` | 175 | 3 | DOM Traversal L1 |
| `browser/lcd_aa.rs` | 75 | 3 | sub-pixel AA foundation |
| `browser/a11y.rs` | 205 | 4 | ARIA tree builder |
| `browser/a11y_prefs.rs` | 95 | 3 | prefers-reduced-motion/color-scheme |
| `browser/atlas_multipage.rs` | 145 | 4 | shelf-pack multi-page |
| `devtools/memory.rs` | 105 | 4 | snapshot Rc + heap walk |
| `devtools/debugger.rs` | 135 | 5 | breakpoint registry + step |
| `devtools/source_map_lookup.rs` | 100 | 5 | binary search V3 mapping |

**Real bug fix**: smooth scroll accumulative retarget (user reported: "cim rychleji
scrollu tim pomalejsi rozjezd"). Predtim cubic-bezier reset pri kazdem wheel =
rapid wheel = ease-in lag accumulates. Nyni Chromium pattern (UpdateTarget):
impulse += k prev.target, start_value/start_time zachovany, velocity preserved.

## Skupina A - Audit fixes hotovo (session)

- ✅ Flex `order` property
- ✅ Hit-test visibility/opacity
- ✅ Position:fixed scroll skip
- ✅ Wheel JS event dispatch
- ✅ addEventListener capture/passive/once

## Skupina B - Audit medium hotovo

- ✅ Hit-test transform inverse
- ✅ Smooth scroll cubic bezier (+ velocity retarget accumulative)
- ✅ Stacking context 4-bucket

## Skupina C - Audit large hotovo

- ✅ Specificity (id, class, type) tuple lex compare
- ✅ ResizeObserver / IntersectionObserver fire

## Skupina D - Compositor pipeline

- ✅ D1 Layer damage tracking (fingerprint diff)
- ✅ D2 Per-layer paint isolation
- ✅ D3 Per-layer paint cache (CPU)
- ✅ D4 Per-layer GPU pipeline (RWE_LAYER_GPU=1 opt-in)
- ✅ D5 Structural fingerprint (compositor-only no damage)
- ✅ D6 Layer transform matrix compose
- ✅ D7 Tile-based cache (foundation - per-tile fingerprint, dirty flag, diag)
- ✅ D8 Compositor thread foundation (mpsc channel, worker stub)
- ✅ D9 Multi-process renderer foundation (cmd/event protocol, in-thread)

## Foundation hotovo, prod-quality blockers

### D7 plne tile cache - per-tile GPU texture
**Aktualne:** Tile struct + fingerprint + dirty flag, NO per-tile texture
**Chybi:**
- wgpu::Texture pool s recycling (tile drop pri layer remove)
- Tile invalidation sub-rect (partial repaint v tile)
- Composite per tile blit
**Prace:** 3-5 dni
**Reference:** `reference/firefox/gfx/wr/webrender/src/tile_cache.rs`

### D8 plne compositor thread - real GPU work
**Aktualne:** mpsc channel + thread + cmd loop bez GPU
**Chybi:**
- `Arc<wgpu::Device> + Arc<wgpu::Queue>` cross-thread (Send/Sync Windows D3D12 tested)
- Swap chain ownership presun (DXGI thread affinity!)
- Synchronization barriers/fences pro layer texture write z main
- Real composite pass beho na thread
- Input low-latency forwarding (wheel→scroll bez main blok)
**Prace:** 7-10 dni (Windows pain point)
**Reference:** `reference/chromium/cc/trees/proxy_main.cc`,
              `reference/firefox/gfx/wr/webrender/src/render_backend.rs`

### D9 plne multi-process renderer
**Aktualne:** thread-stub s mpsc channel, ProcessManager skeleton
**Chybi:**
- `std::process::Command::spawn` skutecny child proces
- IPC: Windows named pipes / Linux Unix domain sockets / macOS XPC
- Serialization (bincode/serde) pro cmd/event payloads
- **Shared GPU memory**: Windows D3D11 shared handles / Linux dma-buf / macOS IOSurface
- Crash recovery (browser zustane kdyz renderer crashne)
- **Sandboxing**: AppContainer (Win) / seccomp-bpf (Linux) / sandbox-exec (mac)
- Site Isolation per-origin policy
- CORB (Cross-Origin Read Blocking)
**Prace:** 3-4 tydny
**Reference:** `reference/chromium/content/browser/renderer_host/render_process_host_impl.cc`,
              `reference/firefox/dom/ipc/ContentParent.cpp`

### Range.getBoundingClientRect real impl
**Aktualne:** stub vraci rect s zeros
**Chybi:** layout_rects globally accessible z native closures (lookup pres
captured interpreter ref). Real union start/end container.
**Prace:** 1 den (refactor native closure capture)

### getClientRects per-line
**Aktualne:** single-rect approx
**Chybi:** inline line box tracking ve flush_inline output, expose per-line
rects pres LayoutBox API.
**Prace:** 2-3 dni (inline layout refactor)

### Cycle GC plne integrace
**Aktualne:** module `interpreter::gc` foundation
**Chybi:**
- Periodicke spousteni v event loop (kazdy N framu)
- Roots collection (global env + active scopes + JS stack)
- Real free (Rc lze `clear()` props k rozbiti cycle, ne force-drop)
- Weak ref refactor pro back-refs (parent pointer, event target = Weak)
**Prace:** 3-5 dni
**Reference:** Bacon-Rajan 2001 paper, V8 Oilpan

### mix-blend-mode + backdrop-filter GPU impl
**Aktualne:** BlendBegin/BlendEnd + BackdropFilterBegin/End markers emitted
**Chybi:**
- Subtree render do offscreen RT pri BlendBegin
- Composite pres shader-side blend formula (Multiply/Screen/Overlay/Darken/...)
- Backdrop snapshot scenes za elementem + filter apply + composit pod
**Prace:** 1-2 tydny GPU shader work
**Reference:** Chromium `core/paint/effect_paint_property_node.cc`,
              SVG Compositing spec

## Browser feature gaps

### HTTP/2 + HTTP/3 (QUIC)
**Aktualne:** ureq HTTP/1.1 sync
**Chybi:**
- Replace ureq pres isahc (sync, curl-backed, HTTP/2) NEBO bridge tokio + hyper
- HTTP/3 = quinn integration
**Prace:** 1 tyden (isahc replace), 2 tydny (tokio bridge)
**Doporuceni:** isahc misto reqwest - zachova sync interpreter model

### WebAssembly
**Aktualne:** ne
**Chybi:**
- wasmtime / wasmer integration (Rust WASM runtime)
- WebAssembly JS API: instantiate, Module, Memory, Table
- Linear memory share s JS interpreter
**Prace:** 2-3 tydny (full WAAPI)
**Reference:** wasmtime crate, bytecodealliance

### Service Workers
**Aktualne:** ne
**Chybi:**
- Per-origin SW registry s persistent storage
- Parallel JS interpreter v SW kontextu
- Fetch event interception (SW prerusi network request)
- Cache API
- Background sync, push notifications (separate)
**Prace:** 4-6 tydnu
**Reference:** Chromium `content/browser/service_worker/`

### CSS Subgrid (uz partial)
**Aktualne:** layout/mod.rs Display::Subgrid + grid.rs:1696 recursive layout
**Chybi:** named line propagation, gap inheritance edge cases
**Prace:** 1-2 dni doladeni
**Test:** WPT css/css-grid-2/ subgrid tests

### View Transitions API
**Aktualne:** ne
**Chybi:**
- `document.startViewTransition` builtin
- DOM snapshot capture pred / po update
- Cross-fade default + custom CSS @view-transition

**Prace:** 1-2 tydny
**Reference:** Chromium `core/view_transition/`

### CSS Anchor positioning runtime
**Aktualne:** parsed ano, runtime resolve partial
**Chybi:** `anchor-scroll`, `anchor-default`, position-area complete impl
**Prace:** 1 tyden
**Reference:** Chromium `core/layout/anchor_position_visibility_observer.cc`

### TouchEvent / PointerEvent
**Aktualne:** ne
**Chybi:**
- winit touch events forwarding
- Pointer event unified model (mouse + touch + pen)
- Multi-touch tracking
**Prace:** 1 tyden
**Reference:** Pointer Events L3 spec

## Networking / Security

### CSP (Content Security Policy)
**Prace:** 1 tyden parse + enforce script-src/style-src/connect-src
**Reference:** CSP3 spec

### HSTS / SRI / Mixed content
**Prace:** 3-5 dni
**Reference:** HSTS RFC 6797, SRI W3C spec

### CORS proper
**Aktualne:** ureq propusti vse
**Chybi:** preflight OPTIONS, origin check, credentials mode
**Prace:** 1 tyden
**Reference:** Fetch spec §3

### Cookie store SameSite
**Aktualne:** limited
**Chybi:** SameSite Strict/Lax/None enforce, secure flag, partition by site
**Prace:** 1 tyden

## DevTools quality

### Source maps + breakpoints
**Aktualne:** parse_source_map V3 + VLQ decode existuje
**Chybi:** runtime breakpoint pause, step/over/out, scope inspection
**Prace:** 2-3 tydny (bytecode VM debugger hooks)

### Performance profiler
**Aktualne:** title bar timings
**Chybi:** flame graph capture, sampling profiler, GC trace, network waterfall
**Prace:** 2 tydny

### Memory inspector
**Prace:** 1 tyden (snapshot Rc counts, heap walk)

## Test infra

### WPT (Web Platform Tests) harness
**Prace:** 2-3 tydny
- testharness.js implementation
- WPT runner z official repo
- Reference results comparison
- Subset import: dom/, css/css-flexbox/, css/css-grid/
**Reference:** https://github.com/web-platform-tests/wpt

### Reftests (visual diff)
**Prace:** 1 tyden
- Reference HTML render → PNG
- Pixel diff with tolerance
- Headless run automation

### Fuzz testing
**Prace:** 3-5 dni
- libfuzzer integration
- Targets: HTML parser, CSS parser, JS lexer, JS parser

## A11y

### Screen reader integration
**Aktualne:** ne (ARIA parsed)
**Chybi:**
- AT-SPI (Linux) / NSAccessibility (mac) / UIA (Windows) bridge
- Accessibility tree mirror DOM
- Role/state mapping per ARIA
**Prace:** 1-2 tydny per platform = 3-6 tydnu total
**Reference:** Chromium `content/browser/accessibility/`

## JS engine

### JIT compiler
**Aktualne:** tree-walker (bytecode VM existuje ale not authoritative)
**Chybi:**
- Activate bytecode VM jako default
- Type inference + monomorphic inline caches
- Tier-up JIT (Cranelift integration)
**Prace:** 2-3 mesice (mega)
**Reference:** V8 Ignition, SpiderMonkey Baseline

### Modules (ESM) plne
**Aktualne:** partial
**Chybi:** dynamic import(), import.meta, top-level await proper
**Prace:** 1 tyden

## Layout

### Floats (CSS Floats L1)
**Aktualne:** limited
**Prace:** 1 tyden BFC + float interaction edge cases

### Multi-column gap/break
**Aktualne:** partial
**Prace:** 3-5 dni break-before/after/inside, column-fill

### Tables complete
**Prace:** 2 tydny (rowspan/colspan edge, border-collapse, caption-side)

## Paint

### Sub-pixel anti-aliasing (LCD)
**Aktualne:** monochrome / grayscale fallback
**Chybi:** dual-source blend (vyzaduje wgpu feature DUAL_SOURCE_BLENDING)
**Prace:** 3-5 dni

### Color emoji rendering full
**Aktualne:** detected (COLR/CPAL/CBDT/SBIX/SVG)
**Chybi:** real paint per format (COLRv1 layered, CBDT bitmap, SVG)
**Prace:** 1-2 tydny

### HDR / wide-gamut color
**Aktualne:** sRGB only
**Chybi:** color-managed pipeline (P3, Rec2020)
**Prace:** 1 tyden

## Memory

### Image atlas dynamic
**Aktualne:** 4096×4096 hardcoded shelf-pack
**Chybi:** multi-page atlas, eviction LRU
**Prace:** 3-5 dni

## Total roadmap odhad

- **D4 plne production**: 3-4 tydny (compositor thread, multi-process, sandbox, tile GPU)
- **Spec compliance fixes**: 3-4 tydny (rAF, observers wire, blend GPU, backdrop GPU, subgrid done, anchor)
- **Network/security**: 2-3 tydny (HTTP/2, CSP, HSTS, CORS, cookies)
- **WebAssembly**: 2-3 tydny
- **Service Workers**: 4-6 tydnu
- **DevTools**: 4-6 tydnu (source maps debugger, profiler, memory)
- **Testing**: 3-4 tydny (WPT, reftests, fuzz)
- **A11y**: 3-6 tydnu (per OS)
- **JS engine optimization**: 8-12 tydnu (JIT)
- **Paint quality**: 2-3 tydny (LCD AA, emoji, HDR)
- **Layout edge cases**: 2-3 tydny (floats, multicol, tables)

**Total**: 36-56 tydnu = **9-14 mesicu plne tym** pro production browser kvalitu.

## Reference

- `reference/chromium/` - 106 MB sparse (cc/, blink scroll/input/page)
- `reference/firefox/` - 108 MB sparse (gfx/wr/, gfx/layers, gfx/2d, layout/painting)
- `reference/INSPIRATION.md` - cross-mapping nas kod → upstream files
