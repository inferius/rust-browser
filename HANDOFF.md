# RustWebEngine - HANDOFF pro dalsi vlakno

Cti **driv nez zacnes**. Plus `CLAUDE.md`, `README.md`, `TODO_CSS.md`.

## Stav projektu (po session N+1: cleanup + refactor split)

**Build:** clean, 0 warnings.
**Tests:** 2361 pass / 0 failed / 3 ignored.
**wgpu:** 29 (latest stable).
**naga:** 29.
**winit:** 0.30.

## Session N+1 highlights (refactor pass)

### Cleanup AI cruft + dead code (commit 20e7456 + 8fc4afc)

- Smazany mrtvy moduly: `evaluator.rs` (legacy 534 LOC nikde neimportovany), `lexer/identifier.rs` (1-line stub), `utils/string_utils.rs` (dead AdvancedStringMethods trait).
- Smazany mrtvy fns: `render::run_browser`, `run_window_with_html`, `build_form_get_url`, `old_build_form_get_url`; `html_parser::dump_tree`; `paint::cmds_offset_for_box`; `css_parser::declarations_to_map`; `interpreter::try_run_via_vm`; `utf8_cursor::from_string + reset_to`.
- Zacisteny historicke "driv X ted Y" / "zatim X" / "aktualne X" komentare patrici do commit msg.
- Globalni `#![allow(dead_code/unused_imports/unused_variables)]` zuzeno na `#![allow(dead_code)]` - test-expose API + future-pub variants ok, ale unused imports/vars ted aktivni warning.
- ~10 unused imports + 8 unused vars opraveno.

### Test extrakce (commit 20e7456)

- `parser/mod.rs` 2385 -> 1547 (parser/tests.rs 837 LOC)
- `lexer/base.rs` 597 -> 326 (lexer/tests/base.rs 272 LOC)
- `browser/woff.rs` 1068 -> 793 (tests/woff_tests.rs 275 LOC)
- `browser/emoji_fonts.rs` 371 -> 254 (tests/emoji_fonts_tests.rs 118 LOC)
- `browser/variable_fonts.rs` 164 -> 91 (tests/variable_fonts_tests.rs 73 LOC)

Pattern: `#[cfg(test)] #[path = "tests/X.rs"] mod tests;` v source souboru, test soubor pouziva `use super::*;` pro privates.

### Render split: render.rs 6555 -> render/ s 10 sub-moduly (commit d0496df)

```
src/browser/render/
  mod.rs (4310)         Vertex, build_vertices, Renderer, run_window_with_options, apply_paint_animations
  url.rs (150)          fetch_text_url + fetch_image_bytes + resolve_url + decode_base64
  forms.rs (115)        find_ancestor_form + build_form_request + post_form
  dirty.rs (41)         DirtyRegion (inkrementalni render)
  segments.rs (206)     Seg + partition_filter_segments + shift_command_x/y
  polygon.rs (149)      polygon math: signed_area + triangulate + clip + point_in_triangle
  atlas.rs (335)        GlyphAtlas + ImageAtlas + try_load_default_font
  shaders.rs (407)      WGSL shader strings: BLUR / TRANSFORM / COMPOSE / RECT
  primitives.rs (618)   push_rect / gradient / shadow / image / polygon vertex helpers
  canvas_paint.rs (228) paint_canvas_ops (Canvas2D ops -> DisplayCommand)
  webgl_paint.rs (86)   paint_webgl_canvases (WebGL queue drain stub)
```

### Layout split: layout.rs 5617 -> layout/ s 9 sub-moduly (commit 254390e)

```
src/browser/layout/
  mod.rs (3434)            LayoutBox struct, layout_tree, build_box, layout_block, flush_inline,
                            measure_text_width, build_pseudo_box, animations, sticky/anchor
  length.rs (148)          parse_length / parse_length_ctx (px/em/rem/vw/vh/%)
  shadows.rs (64)          parse_text_shadow + parse_box_shadow
  shape_fn.rs (86)         ShapeFunction enum + parse_shape_function
  transform.rs (154)       mat4 math + transform_op_matrix + compute_transform + needs_3d_pipeline
  transform_parse.rs (135) parse_transform_chain + parse_transform tokenize
  filter.rs (336)          FilterOp + parse_filter + apply_filter + color_matrix
  backgrounds.rs (344)     BgGradient/BgLayer/BgPosition/Size/Repeat/Box/Attachment + ClipPath + to_roman
  gradients.rs (197)       parse_any/radial/conic/linear_gradient
  color.rs (791)           parse_color CSS L4 superset (hex/named/rgb/hsl/hwb/oklab/lab/color()/...)
```

### Interpreter split: interpreter/mod.rs 5901 -> 1440 (-76%) + 6 sub-modulu (commit 0c1b481)

```
src/interpreter/
  mod.rs (1440)           Interpreter struct, JsValue, JsObject, JsMap/JsSet, JsFunc, Environment,
                           run(), drain_*, load_module, dispatch_event, iterator_helper_method
  eval_call.rs (2010)     eval_call - massive call dispatch
  eval_expr.rs (774)      eval (dispatcher) + eval_unary/binary/logical/assign + assign_to + destructure_bind
  eval_member.rs (649)    eval_member + get_prop (member access + prototype chain)
  exec_stmt.rs (390)      exec_stmts + exec_stmt
  class.rs (310)          make_class_func + construct_class + run_super_constructor + bind_params
  call_machinery.rs (373) call_function + call_new + construct_map/set/date/error/promise + call_generator
```

Pattern: kazdy sub-modul ma `impl Interpreter { ... }` block, metody `pub(super)` volane z mod.rs.

### Builtins partial split (commit 43b2ad5)

- `interpreter/builtins.rs` 5138 -> 4824 + `builtins_helpers.rs` 323 LOC.
- Extrahovany standalone helpery: run_worker_thread, make_message_port, build_search_params, make_object_store.
- `setup_builtins` (4800 LOC giant fn) zustava intact - splittovat ho do helper fns by vyzadovalo pohyb sdileneho state, riskantni.

### Soucet refactor

23107 -> 14008 LOC v hlavnich souborech, **9099 LOC rozdeleno do 26 sub-modulu**. Build clean, 2361 testu pass.

## Posledni hlavni opravy v session

### Critical bugy (vse opravene)

1. **Transform 3D rotateX/Y/Z useklou pulku** (f6309cd) - shader `nz=clamp(tz*inv_w,-1,1)` mimo wgpu NDC z range [0,1] = clipped. Fix `nz=0.5` constant.
2. **HiDPI scale_factor neignored** (a720a63) - vp uniform / mouse coords / scissor / uv_box pouzivaji `zoom * scale_factor` namisto jen zoom.
3. **SVG section content_h ignoroval shapes** (0e4de94) - explicit_height inline replaced ovlivnil line_height.
4. **Buttons section bottom padding mizel** (0e4de94) - element_h s padding ovlivnuje line_height.
5. **Text necitelny po LCD subpixel render** (817b41d) - revert na grayscale (proper LCD vyzaduje dual-source blend).
6. **Badge/highlight text not centered** (817b41d) - v_offset bez clampu, formula `(inner_h - 1.5*fs)/2`.
7. **Text node rect.width/height jen first word** (817b41d) - hit-test pres cely text + cursor I-beam.
8. **Polygon edge AA outward direction flipped** (ff598b5) - jagged hrany clipped polygons + SVG ellipse rotated.
9. **Per-element layout cache** (05d09bb) - subtree fingerprint, hover state change skip rebuild clean nodes.
10. **wgpu 0.20 -> 29 major upgrade** (0ebf297) napric 30+ API zmen.

### UX features pridane

- Zoom (Ctrl+/-/0)
- Find on page (Ctrl+F)
- Address bar (Ctrl+L)
- Print to PDF (Ctrl+P)
- Text selection + Ctrl+C/Ctrl+A clipboard
- Smooth scroll inertia
- Keyboard scroll (PageUp/Down, Home/End, Space)
- Devtools console Ctrl+V paste
- Cursor icon (Text/Pointer)
- Form input typing focused

## TODO - co zbyva

### High priority - real perf/UX gain

- [ ] **MSAA na offscreen RT** (vetsi pipeline rebuild). Polygon clip/SVG rotated stale jagged pri zoomu out (sub-pixel rasterization). MSAA 4x via offscreen RT s sample_count=4 + resolve_target. Vyzaduje:
  - msaa_offscreen_tex (sample_count: 4, COLOR_ATTACHMENT)
  - Pipeline_msaa s multisample.count=4
  - RenderPass color_attachment.resolve_target = single sample
  - draw_to_offscreen path pres MSAA RT pri shapes/polygon paths

- [ ] **getBoundingClientRect** vraci real layout dims namisto attrs (currently returns w=0/h=0 z attr lookups). Problem: interpreter nema pristup k LayoutBox - vyzaduje thread-through nebo thread-local snapshot.

- [ ] **LCD subpixel proper blend** (Chrome ClearType-style):
  - Vyzaduje wgpu DUAL_SOURCE_BLENDING feature
  - Per-channel alpha output @location(1)
  - Pipeline blend mode src_factor=One, dst_factor=OneMinusSrc1Color
  - Atlas storage uz hotovy (3x sirka swizzled RGB pres rasterize_subpixel)
  - Shader stale grayscale fallback - nutne prepsat blend pipeline

- [ ] **Houdini paint API** (CSS Paint API):
  - JS API: registerPaint, PaintWorkletGlobalScope
  - paint() callback z JS volane behem render
  - Heavy: 1000+ lines impl + interpreter integration

### Medium priority

- [ ] **Inline mid-line wrap reset to inner_x** - text wrap pri prelomeni resetuje na first-word x, ne container inner_x. Edge case.

- [ ] **DOM API:** getBoundingClientRect, offsetWidth/Height, scrollIntoView - currently stubs / wrong.

- [ ] **PDF export multi-page split** (printpdf 0.7) - currently emit cely layout na jeden long page. Add A4 page breaks.

- [ ] **MSAA pipeline OR alpha-to-coverage** pro polygon edge AA pri zoom < 1.

- [ ] **Per-glyph font hinting** - fontdue dela hinting? Investigate, maybe sharper text.

### Low priority / nice to have

- [ ] **Image atlas multi-page** - currently 4096^2 atlas. Pri velkych pages s mnoho fontu / glyph sizes muze overflow.

- [ ] **Sub-pixel text positioning** - integer pixel snap pri zoomu = cleaner ale ztrati sub-pixel detail. Investigate trade-off.

- [ ] **CSS containment** (`contain: layout`, `contain: paint`) pro better layout cache invalidation hints.

- [ ] **Scroll snap** CSS feature.

- [ ] **CSS @scope** - parser exists, runtime missing.

- [ ] **CSS @starting-style** - parser exists, runtime partial.

- [ ] **CSS-wide keywords** - `revert`, `revert-layer` cleanup.

### Specific known issues

- [ ] **JS error v engine-test.html:** `Runtime: CallMethod: callee not function` na `getBoundingClientRect()`. Bud chybna querySelector navratova hodnota nebo broken method dispatch.

- [ ] **engine-test.html nektere advanced CSS** (88 grid/flex usages, complex selectors :has/:where/:is, @container, sticky+backdrop-filter, conic-gradient, scroll-snap, animation-timeline) potrebuje audit.

- [ ] **WOFF2 specifikovane fonty validation** - 22 Google Fonts round-trip OK, ale realny rendering test stranky neuplny.

### Refactor / arch

- [ ] **build_box_inner full per-element cache** - aktualne fingerprint compute v rekurzivnim child build. Komplex prepis pro level-by-level cache by snizil rebuild cost vice.

- [ ] **layout_dispatch separated z build** - dnes layout_block POSITIONS po build. Pro per-element cache by stalo za to mit position-only update path (shift cached subtree o delta).

- [ ] **Texture atlas eviction** - kdyz atlas overflow, evict LRU glyfy. Aktualne `return` bez insert.

- [ ] **Bytecode VM:** existujici, jen opt-in pres console_eval_via_vm. Tree-walker authoritative. Plne switch + benchmarks (uz hotove) ukazuji 1.83-7.6x speedup.

- [ ] **builtins.rs setup_builtins split** - aktualne 4800 LOC giant fn. Splittovat do logickych sekci (setup_console, setup_math, setup_object, setup_storage, setup_observers, ...) by zlepsilo navigaci, ale vyzaduje opatrne presmerovavani sdileneho state (env, task_queue, console_log, ...). Zatim jen extrahovany standalone helpery do builtins_helpers.rs.

- [ ] **render/mod.rs Renderer + run_window_with_options split** - 4310 LOC po prvnim splitu. Renderer struct + ApplicationHandler tesne provazane se zoom/scroll/find/addr/PDF event handling. Mozne dalsi rozdeleni:
  - `render/window/event.rs` - mouse/keyboard event handlers
  - `render/window/find_overlay.rs` - Ctrl+F find UI
  - `render/window/address_bar.rs` - Ctrl+L address UI
  - `render/window/print_pdf.rs` - Ctrl+P PDF export

- [ ] **interpreter/builtins.rs split** - po extrakci helpers (323 LOC) zustava setup_builtins 4800 LOC. Viz vyse.

- [ ] **Devtools rework** (planovany v dalsi session) - bud sjednoceni static HTML export + inline panel, nebo new features (live edit, source maps, breakpoints, profiler). Cekame na rozhodnuti smerovani.

## Klavesove shortcuts

| Shortcut | Akce |
|----------|------|
| Ctrl+= / Ctrl++ | Zoom in (1.1x) |
| Ctrl+- | Zoom out |
| Ctrl+0 | Zoom reset 100% |
| Ctrl+F | Find on page |
| Ctrl+L | Address bar |
| Ctrl+A | Select all |
| Ctrl+C | Copy selection |
| Ctrl+P | Print to PDF |
| Ctrl+V | (devtools console) Paste |
| PageDown/Up | Page scroll |
| Arrow Down/Up | 60px scroll |
| Home / End | Top / bottom |
| Space | PageDown |
| Shift+Space | PageUp |
| Shift+Wheel | Horizontal scroll |
| F5 | Reload |
| F11 | Open static devtools.html |
| F12 | Toggle in-window devtools |
| Alt+Left/Right | Browser history |
| Esc | Close find/address overlay |
| Enter | Find next match / Submit address |
| Shift+Enter | Find prev match |

## Build / test

```bash
cargo build --release    # release profile
cargo test               # 2361 pass
cargo run --release -- browser static/test.html       # test stranka
cargo run --release -- browser static/engine-test.html  # advanced test
```

Test paths:
- `static/test.html` - 14+ sekci (typography, colors, box model, layout, lists, tables, forms, cards, buttons, animations, filters, gradients, transforms, SVG, polygon clip)
- `static/engine-test.html` - heavy modern CSS (grid, sticky, backdrop-filter, conic-gradient, scroll-snap, @container, :has, color-mix, animation-timeline) - moderate breakage on edge cases
- `static/transform_debug.html` - simple rotateY box pro debug

## Files s nejvetsi koncentraci kodu (po session N+1 refactoru)

- `src/interpreter/builtins.rs` (~4800 lines, setup_builtins giant fn - jeden velky setup pro vsechny global builtins)
- `src/browser/render/mod.rs` (~4310 lines, Renderer struct + run_window_with_options + winit ApplicationHandler + zoom/scroll/find/addr/PDF event handling)
- `src/browser/layout/mod.rs` (~3434 lines, LayoutBox + layout_tree + build_box + layout_block + flush_inline + cache + sticky/anchor)
- `src/interpreter/eval_call.rs` (~2010 lines, eval_call dispatch - extracted z mod.rs)
- `src/browser/paint.rs` (~1840 lines, display list build + transform/filter markers + SVG emit)
- `src/browser/cascade.rs` (~2150 lines, selectors + specificity + viewport queries + state hash)
- `src/parser/mod.rs` (~1547 lines)
- `src/browser/layout_engine/flex.rs` (~1615 lines)
- `src/interpreter/mod.rs` (~1440 lines, Interpreter struct + run + helpers - po splitu)
- `src/interpreter/bytecode.rs` (~2877 lines, JS VM)
- `src/interpreter/webgl.rs` (~1308 lines)
- `src/interpreter/helpers.rs` (~1332 lines)
- `src/browser/css_parser.rs` (~1209 lines)
- `src/browser/woff.rs` (~793 lines)

## Architektura cache

```
Render frame:
1. cargo run --release -- browser test.html
2. Parse CSS once -> stylesheets cache (css_hash key)
3. Cascade run -> style_map cache (cascade_hash key = css + zoom + viewport + hover/focus)
4. Layout build -> layout_root cache (= layout_cache_valid check)
   - Per-element cache: build_box_inner pri rekurzi child build vola cache_lookup_subtree
   - subtree fingerprint hash: node_ptr + tag + text + sorted style + child fingerprints
   - Cache hit -> clone prev subtree (skip style/struct rebuild)
5. Paint walk -> display_list (build_display_list_culled_into)
6. Build vertices (atlas lookup) + render
7. animations_affect_layout=false -> cache reuse, jen apply_paint_animations
```

## Klicove dependency hashe (k zachovani lock)

- wgpu 29.0.x + naga 29.x (lockstep)
- selectors 0.38 + cssparser 0.37 (latest, html5ever ekosystem)
- html5ever 0.27 stays (rcdom 0.5+ = +unofficial prerelease only)
- icu 1.5 stays (2.0 major API rewrite)
- ureq 2.12 stays (3.x API rewrite)
- printpdf 0.7 stays (0.8/0.9 = API rewrite, breaking)
- arboard 3.6 (clipboard)
- fontdue 0.9 (latest, glyph rasterizer)
- winit 0.30 (latest stable)

## Workflow konvence

- **Cesky** v komunikaci a komentarich. Diakritika OK.
- **Ciste ASCII** v kodu (-> ne ->, em-dash ne -, smart quotes ne ").
- **Komentar v Cargo.toml u kazde dependency** proc je tam.
- **Po kazde feature:** build + test + commit.
- **Commit message:** strucny, popis "co + proc". Cesky.
- **Pri nejistote zeptat se** drive nez psat kod.
- **NIKDY nedelat fake fixes** ("done" bez verifikace) - radeji zeptat user.
