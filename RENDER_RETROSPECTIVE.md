# Rendering (paint + GPU) - retrospektiva

Analyza naseho paint+render pipelinu vs Mozilla WebRender (Servo/FF) + Chromium cc.
Identifikace known issues + design upgrades.

## Aktualni pipeline

```
LayoutBox tree (4991 LOC layout)
   |
   v
[paint::build_display_list_culled(root, scroll_y, viewport_h)]
   |   2325 LOC paint.rs
   |   Walks LayoutBox tree, emit per-element commands:
   |   - Rect (bg color s radius)
   |   - Border (outline)
   |   - Text (content + color + font + decorations)
   |   - Gradient (linear/radial/conic, multi-stop)
   |   - Shadow (outer + inset)
   |   - Image / ImageFit (src + cover/contain/fill)
   |   - BlurredRect (mode 8 smoothstep)
   |   - FilterBegin/End markers (subtree off-RT)
   |   - BackdropFilterBegin/End (snapshot za elementem)
   |   - SkewedQuad (rotateY/X 3D persp)
   |   - PolygonAA (clip-path polygon)
   |   ...
   |   ~14 DisplayCommand variants, 123 ::variant references
   |
   v
[browser::compositor::extract_layer_tree(layout_root)]
   |   Detect layer boundaries:
   |   - position:fixed/sticky
   |   - opacity < 1
   |   - transform != none
   |   - filter != none
   |   - mix-blend-mode != normal
   |   - clip-path != none
   |   - z-index != auto na positioned
   |   - isolation:isolate
   |
   v
[build_layered_display_list]  Per-layer commands list
   |   layer_paint_cache: HashMap<layer_id, Vec<DisplayCommand>>
   |   damage_rect on each layer
   |
   v
[D4 GPU pipeline (default-on)]
   |   For each damaged layer:
   |     - render_into_layer(layer_view, cmds)
   |     - DisplayCommands -> Vertex buffer -> GPU drawcall
   |   Composite pass:
   |     - compose_view_to_view(target_view, layer_view, pos, opacity, blend)
   |   Overlay paint po composite (scrollbar, devtools).
   |
   v
[Renderer.render_into_layer / compose_view_to_view]
   |   8233 LOC render/mod.rs
   |   wgpu pipelines: RECT, BLUR, TRANSFORM, COMPOSE, LCD
   |   Atlas allocator (glyph atlas 4096x4096, image atlas)
   |   Vertex buffer build_vertices()
   |
   v
[Swap chain present]
   present_external_to_swap_chain / present_layered_external
   final framebuffer
```

---

## Porovnani s WebRender (Mozilla)

### WebRender klicove abstrakce
**Cesty:** `firefox/gfx/wr/webrender/src/`

- `display_list.rs` - DisplayItem enum (Rect/Line/Text/Image/Border/Shadow/...)
- `picture.rs` - **Picture** = caching unit. Strom Picture (= ekvivalent nasi LayerNode)
- `tile_cache.rs` - **TileCache** = picture rasterized do 512x512 tiles, per-tile damage tracking
- `picture_textures.rs` - GPU texture pool (recycle)
- `composite.rs` - Native compositor abstrakce (macOS CoreAnimation, Windows DComp, Linux GLX)
- `render_target.rs` - RenderTarget = offscreen wgpu texture per Picture
- `frame_builder.rs` - DisplayList -> Picture tree -> Tile tasks
- `batch.rs` - Draw call batching per primitive type (Rect / Text / Image / ...)
- `gpu_cache.rs` - GPU-side storage for static data (gradient stops, etc.)
- `clip.rs` - Clip stack as ClipNode tree
- `spatial_tree.rs` - Transform/scroll hierarchy

### Nase ekvivalenty

| WebRender | Nas | Gap |
|-----------|-----|-----|
| DisplayItem (40+ variants) | DisplayCommand (~14 variants) | OK pro mainstream content |
| Picture tree | `compositor::LayerNode` | OK base, missing recursion deep |
| TileCache (512x512 tiles, per-tile damage) | `compositor::Tile` foundation + damage_rect on layer | **Tiles existuji ale not used pro real raster** |
| picture_textures (RT pool) | `layer_textures` per-layer single RT | **No tile-based texture pool** |
| frame_builder (display list -> picture tree) | `build_layered_display_list` | OK |
| Native compositor (CoreAnimation/DComp) | None - all sw composite via wgpu | macOS/Win get OS compositor wins; my get every frame full GPU |
| Batching per primitive type | Single batched vertex buffer | Mixed types in one buffer - inefficient |
| GPU cache (gradient stops in texture) | Inline vertex attribs | More CPU->GPU traffic per frame |
| Clip stack jako tree | DisplayItemClip flat per-cmd | More state per-command |
| Spatial tree | scroll_x/y + bx.transform per box | No shared hierarchy = recompute per descendant |

### Hlavni gapy z renderingu

#### 1. **Tile-based rasterization NEEXISTUJE pro real raster**
- Foundation `compositor::Tile` struct + damage tracking je placeholder.
- Real per-layer texture = WHOLE LAYER. Pri velkych pages scroll = re-raster cele vrstvy mesto jen visible tiles.
- WebRender rasterizes 512x512 tiles na demand, cache per tile, only repaint dirty tiles.

#### 2. **Native compositor BYPASS**
- macOS Safari/FF pouzivaji CoreAnimation - OS-level compositor zdarma
- Windows Edge pouziva DirectComposition
- My vsechno renderujeme sami pres wgpu kazdy frame
- Result: vyssi GPU load, vyssi power draw

#### 3. **Vertex buffer batching slabsi**
- Vsechny commands -> jeden Vertex array s mixed primitive types
- Per-vertex `mode` field switche shader behavior (rect / text / image / gradient)
- WebRender separate batch arrays per primitive type - faster GPU pipeline

#### 4. **GPU cache neexistuje**
- Gradient stops + filter color matrices passnute kazdy frame pres vertex/uniform
- Pro static gradienty + filtery = redundant upload kazdy frame

#### 5. **Damage rect na layer level, ne tile level**
- Pri changed pixel v layer = repaint cely layer (vsechny stovky tiles)
- Tile-level damage by stacilo repaint jen affected 512x512 region

---

## Porovnani s Chromium cc (compositor)

### cc/ klicove abstrakce
**Cesty:** `chromium/cc/`

- `cc/trees/layer_tree_host_impl.cc` - compositor thread main
- `cc/tiles/tile_manager.cc` - tile prioritization + raster scheduling
- `cc/tiles/picture_layer_tiling.cc` - per-zoom-level tile grid
- `cc/raster/gpu_raster_buffer_provider.cc` - GPU raster (skia bridge)
- `cc/output/direct_renderer.cc` - GL/Vulkan/Skia direct render
- `cc/quads/draw_quad.cc` - DrawQuad = single GPU primitive (TextureDrawQuad / SolidColorDrawQuad / ...)
- `cc/animation/animation_host.cc` - compositor-driven animations (transform/opacity)
- `cc/input/input_handler.cc` - scroll thread (= compositor thread)

### Nase gapy vs cc/

| Aspekt | Chromium cc | Nas | Gap |
|--------|-------------|-----|-----|
| Compositor thread | Separate thread | Main thread vse | Block paint = block input |
| Tile prioritization | Visible > soon > eventually | None - whole layer | Allocates more memory pre-load |
| Skip GPU raster (mainstream tiles) | Pre-raster + cache | Per-frame raster all | Higher GPU load |
| Animations on compositor | transform/opacity = no main repaint | All animations re-cascade main | Stuttery animations pod main-thread load |
| Scroll thread | Input -> compositor thread fast path | Input -> main thread | Slow scroll under heavy JS |
| Quad-based draw items | TextureDrawQuad, SolidColorDrawQuad, ... | DisplayCommand enum | Similar shape |

### Klicovy gap: compositor je SINGLE-THREAD (main)
- Real browser: main = layout/paint, compositor = composite/scroll/animation, NEnenezavisle
- My: vse sequentially pres render_via na main thread
- **Vysledek:** pri pomale JS = laggy scroll (input cekkat na JS done)

---

## Known issues v render pipelinu (uzivatel report)

### 1. Scroll jumps / backtrack
**Stav:** Fixed v N+25 (double-count v retarget_scroll + JS scrollTo anim clear).

### 2. Pixel-snap mismatch
LayoutBox rect je f32, paint emit f32 commands, wgpu rasterizes na fyzicky px grid.
Pri sub-pixel positions (e.g. center-aligned content) glyph edges blurry.

**Fix:** v `paint::build_display_list` snap rect.x/y na nearest physical px (`(x * dpr).round() / dpr`).
**Currently:** atlas glyph keys snapnute, ale ne dest rect positions. Mid-pixel render.

### 3. Glyph atlas re-raster pri zoom
Pri zoom in Ctrl++ = font size physical changes = atlas key changes = re-raster all glyphs.
Pomale na velkych pages.

**Fix:** progressive atlas - drz multiple zoom levels v atlasu, lookup nearest. 
**Status:** existing kod uz dela `(font_size * zoom).round()` key. OK.

### 4. Image atlas re-resample pri zoom
Image stored at natural size, resample pri zoom change.
Atlas size limit 4096x4096 - velke imagy downscale.

**Issue:** pri rychly zoom in/out = continuous re-resample lag.
**Fix:** keep source bytes + lazy resample only when stable for N frames.
**Status:** existing kod ma source_bytes cache. OK partial.

### 5. Box-shadow heavy CPU
Existing shadow rendering pres SDF rect (mode 5) na CPU vertex generation.
Pri 100+ shadowed elementu = laggy paint.

**Fix:** GPU compute pass pro shadow blur per box. Or pre-baked alpha map per radius.

### 6. clip-path polygon edge AA
Existing fan triangulation s outward-normal feather (1 phys px) = decent ale edge cases.
**Issue:** convex polygons OK, concave can produce overlapping triangles = z-fighting.

### 7. Mix-blend-mode + backdrop-filter NA GPU CHYBI
Foundation math hotov v `render::blend`, shader_id() per mode.
**Issue:** wgpu pipeline pro 17 blend modes nevytvoreny. Currently fall back to "normal" blend.
**Fix:** generate WGSL with per-mode blend fn, pipeline cache.

### 8. Subpixel AA (LCD) inkonsistentni
Foundation `subpixel_aa.rs` ma math. Real impl uses dual-source blend wgpu config.
**Issue:** wgpu dual-source blend support varies per backend. Currently grayscale fallback always.

### 9. Filter chain compose
Multiple filters (blur + brightness + contrast) need offscreen RT chain.
Existing `FilterBegin/End` + offscreen RT works for single filter.
**Issue:** Multiple filters = multiple RTs allocated per frame = memory pressure.

### 10. Compositor thread split BUDOUCE
Real browsers split main + compositor. We don't.
**Issue:** Scroll laggy when JS busy. Transform/opacity animation hitches under main load.
**Fix:** dlouhodoby refactor.

### 11. Text shaping per frame
Inline layout re-shapes text every layout pass (changed scroll = no, changed style = yes).
**Issue:** Heavy CPU for long-text pages.
**Fix:** Shape cache keyed by (text, font_family, size, weight, italic, letter_spacing).
**Status:** existing ma `shape_text` ale ne cache. ADD cache.

### 12. Render dirty region appling
Existing damage tracking je per-layer. Pri scroll = damage cely viewport.
**Issue:** Wasteful repaint on scroll.
**Fix:** Per-tile damage (= WebRender pattern). Scroll = damage only newly-revealed strip.

---

## Top 5 priorit pro rendering

### 1. Tile-based rasterization (high ROI, high effort)
Replace `layer_textures` (whole-layer textures) s tile grid (512x512 per layer).
Damage tracking per tile.
Pri scroll = only repaint revealed tiles.
Memory tradeoff: more textures but smaller, cache-friendly.

**Source-of-truth:** WebRender `gfx/wr/webrender/src/tile_cache.rs`.

### 2. Pixel-snap dest rects (low effort, real visual fix)
V paint emit rounded coords:
```rust
fn snap_to_device_px(x: f32, dpr: f32) -> f32 { (x * dpr).round() / dpr }
```
Apply na vsechny Rect/Border/Text rect.x/y.

**Win:** Crisp 1px borders, no sub-pixel blur na image edges.

### 3. Text shape cache (medium effort, big win)
Add LRU cache `(text_hash, font_family, size, weight, italic) -> ShapedText`.
Existing `shape_text` ma vertex layout ready, just memoize.

**Win:** 80% reduction text-heavy page layout time.

### 4. Compositor-driven animations (high effort, high ROI)
Transform/opacity changes:
- Skip main thread re-cascade
- Just update layer transform/opacity uniform
- GPU composite re-runs with new value

**Source-of-truth:** Chromium `cc/animation/animation_host.cc`.

### 5. Blend mode + backdrop-filter GPU pipelines (medium effort)
Add wgpu render pipeline variants per blend mode (Multiply/Screen/Overlay/...).
For backdrop-filter: snapshot scene -> filter (existing pipeline) -> composite under content.

**Win:** correct visual fidelity for sites with rich design (blogs, dashboards).

---

## Sekundarni issues

### Native compositor on macOS/Win
- macOS: CoreAnimation via wgpu's MetalLayer integration
- Win: DirectComposition via wgpu DXGI swap chain
- Linux: no native (compositor in user space already)
**Effort:** medium per OS, separate scope.

### GPU cache (gradient stops + color matrices)
Pre-bake static data into textures, sample per-vertex. Save uniform upload bandwidth.
**Effort:** medium. Win small (~5%).

### Better batching
Group commands by primitive type for separate draw calls.
**Effort:** low-medium. Win small (~10%).

### Filter chain offscreen RT recycling
Pool offscreen RTs, reuse across filter operations.
**Effort:** low. Win: memory pressure under filter-heavy pages.

### Polygon AA improvement
Use signed-distance field approach for clip-path AA instead of fan triangulation.
**Effort:** medium. Win: clean edges for arbitrary polygons.

---

## Architektura: paint vs render decoupling

**Aktualne:** paint emits Vec<DisplayCommand>. Render walks list, builds vertices, draws.
**Issue:** No intermediate "frame plan" - paint output je flat list ne hierarchy.

**WebRender pattern:** DisplayList -> SceneBuilder -> FrameBuilder -> RenderTaskGraph -> Render.
Each step adds optimization (batching, occlusion culling, task dependencies).

**Improvement:** Add `SceneBuilder` stage between paint and render. Build RenderTaskGraph (= DAG of GPU passes). Allow:
- Offscreen RT recycling
- Pass ordering (depth-first vs breadth-first)
- Occlusion culling
- Skip invisible passes

---

## Reference

- WebRender architecture: https://hacks.mozilla.org/2017/10/the-whole-web-at-maximum-fps-how-webrender-gets-rid-of-jank/
- WebRender source: `firefox/gfx/wr/` (lokalni shallow clone)
- WebRender deep dive: https://github.com/servo/webrender/wiki
- Chromium compositor explainer: https://docs.google.com/document/d/14Z0lmJv7sCb47XGYpkU_TKaQXgQYzZbAY7QnQOlAyao/
- Skia paint primer: https://skia.org/docs/user/api/
- DisplayLink + VSync: https://developer.apple.com/documentation/quartzcore/cadisplaylink

---

## Implementace TODO

Po teto retrospektive logicke navazne tasky (po convince checking benchmarks):

- [x] Pixel-snap dest rects (~50 LOC, immediate visual fix) - **next session**
- [ ] Text shape cache LRU (~150 LOC, big perf)
- [ ] Mix-blend-mode WGSL pipelines (~400 LOC, real spec compliance)
- [ ] Tile-based rasterization (~2000 LOC refactor, big perf)
- [ ] Compositor-driven animations (~1500 LOC refactor, big UX)
- [ ] SceneBuilder + RenderTaskGraph stage (~1000 LOC, architecture)
- [ ] Native compositor on macOS (~500 LOC per OS, big power saving)

Total estimate: 4-6 sessions pro top 5.
