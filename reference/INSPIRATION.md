# Chromium reference

Sparse shallow clone (~106 MB) v `chromium/`. Klicove cesty:

```
chromium/cc/input/                                  - compositor input + scroll
chromium/cc/animation/                              - timeline + curves
chromium/cc/trees/                                  - layer tree + proxy main
chromium/third_party/blink/renderer/core/scroll/    - blink scroll core
chromium/third_party/blink/renderer/core/page/scrolling/  - main-thread scroll
chromium/third_party/blink/renderer/core/input/     - input handlers
```

## Source-of-truth per topic

### Scrollable area abstrakce
- `blink/renderer/core/scroll/scrollable_area.h` - base class kazdy scroll container
- `blink/renderer/core/scroll/scrollable_area.cc`
- nase: `crates/engine/src/browser/scroll.rs` trait Scrollable + Mut

### Scrollbar controller (drag/track-click/hover)
- `cc/input/scrollbar_controller.cc` + `.h`
- `cc/input/scrollbar_animation_controller.cc` - fade-in/out
- nase: WebView::handle_input MouseDown inner scrollbar branch

### Smooth scroll easing
- `blink/renderer/core/scroll/scroll_animator.cc`
- `cc/animation/scroll_offset_animation_curve.cc`
- nase: lerp 25 %% per frame (jednoduche, jerky)

### Scroll chaining / overscroll
- `cc/input/overscroll_behavior.h`
- `blink/renderer/core/scroll/scrollable_area.cc` UserScrollChainable
- nase: nemame, scroll stuck na vnitrnim elementu

### Scroll snap CSS
- `cc/input/scroll_snap_data.cc/.h`
- nase: nemame

### Input handling main vs compositor thread
- `cc/input/input_handler.cc/.h` - compositor thread
- `cc/input/main_thread_scrolling_reason.h` - kdy musi main thread
- nase: single-thread vsechno na main

### Layer tree (compositor)
- `cc/trees/layer_tree_host.cc`
- `cc/trees/layer_tree_host_impl.cc` - compositor side
- `cc/trees/proxy_main.cc` - BeginMainFrame triggers
- nase: `compositor::extract_layer_tree` jen layer tree extraction

### Hit test (overflow + scroll aware)
- `blink/renderer/core/page/scrolling/...`
- `cc/input/hit_test_opaqueness.cc`
- nase: LayoutBox::hit_test pres scroll_offset_x/y

## Plan integrace

1. **Smooth scroll easing** - prepsat lerp na cubic-bezier dle scroll_offset_animation_curve.cc invariantu (duration / distance ramp). Comment ref na source.

2. **Scroll chaining** - dite dosahne max + dy zbyva -> propagovat dx/dy na parent ancestor pres find_scroll_target. Reference: scrollable_area.cc::UserScroll loop.

3. **BeginMainFrame triggers audit** - cc/trees/proxy_main.cc ma kompletni list. Doplnit needs_continuous_render() pokud chybi.

4. **Compositor thread** - dlouhodoby. Vyzaduje split: main = layout/paint, compositor = animations/scroll/input forwarding, IPC via mpsc channels.

5. **Multi-proces** - jeste dlouhodobejsi. Renderer per tab + IPC.

## Firefox / WebRender reference

Sparse shallow clone `firefox/` (~108 MB). Klicove cesty:

```
firefox/gfx/wr/                       - WebRender (Rust GPU compositor!)
firefox/gfx/wr/webrender/src/         - WebRender core
firefox/gfx/layers/                   - Older Layers system (pre-WebRender)
firefox/gfx/2d/                       - 2D graphics primitives
firefox/layout/painting/              - Frame painter
```

### WebRender source-of-truth (Rust)

**Compositor + tile caching:**
- `gfx/wr/webrender/src/picture.rs` - PictureLayer = caching unit (= LayerNode)
- `gfx/wr/webrender/src/tile_cache.rs` - Tile-based texture caching
- `gfx/wr/webrender/src/composite.rs` - Compositor pass
- `gfx/wr/webrender/src/picture_textures.rs` - Per-picture GPU texture pool
- `gfx/wr/webrender/src/render_target.rs` - RT management

**Spatial tree (scroll + transform):**
- `gfx/wr/webrender/src/spatial_tree.rs` - hierarchical transform/scroll
- `gfx/wr/webrender/src/spatial_node.rs` - per-node scroll/transform

**Damage:**
- `gfx/wr/webrender/src/picture.rs::Picture::update_dirty_rect`

### WebRender vs Chromium compositor

| Aspekt | Chromium cc | WebRender |
|--------|-------------|-----------|
| Lang | C++ | Rust |
| Layers | LayerImpl tree | Picture tree |
| Cache | Texture per layer | Picture + tile cache |
| Damage | DamageTracker | Dirty rect per tile |

WebRender = idiomaticky Rust = lepsi reference. Chromium = vetsi feature coverage.

**Strategy:** WebRender pro core (tile cache, picture, spatial tree). Chromium pro features (capture phase, scroll-snap, observer dispatch).

## License

Chromium BSD-3-Clause. Inspirace algoritmem OK. Doslovne kopirovani vyzaduje:
- Zachovat copyright header
- Pridat na LICENSE
Pro nas idiomaticky Rust prepis = derived work, jen prevzeti vzoru, ne kodu.

V kazdem souboru kde Chromium-inspired: comment "// Inspired by Chromium <path>:<line> @ <commit>".
