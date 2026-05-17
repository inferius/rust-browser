# RustWebEngine - Architektonicky Audit (2026-05-17)

Retrospektiva po nekolika session perf optimalizaci kde se ukazalo, ze
opravujeme bez planu a duplicitne. Dokument mapuje **aktualni stav**,
**bolava mista (duplikace + antipatterns)** a **cilovy stav arch**.

## Mapa: Co kde zije

### Workspace crates

```
crates/
  engine/                  - lib rwe_engine + bin rwe-engine
    src/browser/           - HTML/CSS/JS rendering
      cascade.rs           - CSS rules -> StyleMap per element
      layout/              - taffy flex/grid + custom inline
      paint.rs             - LayoutBox -> DisplayCommand stream
      render/              - wgpu pipeline, atlas, present
      compositor/          - L1 layer detection (FOUNDATION, no real use yet)
      devtools_panel.rs    - LEGACY native devtools paint (4400 LOC)
      editor.rs            - EditorState - shared text input model
      selection.rs         - SelectionRegistry - shared text selection model
    src/embed/
      webview.rs           - WebView - vlastni interpreter + cascade/layout/paint
      engine.rs            - Engine - Arc shared device/queue/atlas
      devtools_target.rs   - CDP adapter (DOM/CSS/Runtime/... handlers)
    src/interpreter/       - JS interpreter
    src/devtools/          - LEGACY DevToolsState model (uses native paint)

  devtools-proto/          - CDP wire types (DevtoolsRequest/Response/Event)
  devtools-frontend/       - HTML/CSS/JS devtools app (single index.html
                              z mockupu + cdp.js)
  shell/                   - rwe-shell - 3-WV host
    src/app.rs             - ShellApp s 3 WebView (chrome/page/devtools) +
                              CDP channel + pump_cdp + compositor present
```

### Per-WebView state

WebView struct (`crates/engine/src/embed/webview.rs`) ma:
- `interpreter: Option<Interpreter>` - own JS engine
- `document: Option<Document>` - DOM tree
- `stylesheets: Vec<Stylesheet>` - parsed CSS
- `last_layout_root: Option<LayoutBox>` - cached layout
- `cascade_cache_key + cascade_cache_value` - StyleMap cache
- `layout_cache_key` - layout cache pres LAYOUT_RELEVANT_PROPS fingerprint
- `last_paint_fingerprint` - paint cache pres full style hash
- `last_layer_tree: Option<LayerNode>` - L1 layer extraction (foundation)
- `layer_textures: HashMap<id, LayerTextureSlot>` - L2 placeholder (unused)
- `editors: HashMap<node_id, EditorState>` - text input state per element
- `hovered_node_local + focused_node_local` - per-WV hover/focus
- `target_texture + target_view` - offscreen RT
- `scroll_x/y + scroll_target_x/y` - smooth scroll
- ... prof_*_ms, last_render_dom_version, dirty bit, etc.

### Shell state

ShellApp ma 3 WebView + CDP channel + render compositor (present_layered).

## Duplikace + Antipatterns

### A) Hover/Focus state - DVAKRAT

```rust
// crates/engine/src/browser/cascade.rs
thread_local! { static HOVERED_NODE: RefCell<Option<usize>> = ... }
pub fn set_hovered_node(id: Option<usize>) { ... }
pub fn get_hovered_node() -> Option<usize> { ... }

// crates/engine/src/embed/webview.rs
pub(crate) hovered_node_local: Option<usize>,
```

**Pre fixu:** thread_local sdileny mezi vsemi WV v threadu. Pohyb mysi v
jedne WV invalidoval cascade cache vsech ostatnich.

**Po fixu:** per-WV state hovered_node_local. PRED cascade walk se thread_local
set z per-WV. Selectory pri matching `:hover` ctou thread_local.

**Problem:** thread_local STALE existuje. Implementace stale ohledne thread_local
api. Per-WV jen wrapper.

**Spravne:** drop thread_local, cascade api bere `hovered: Option<&NodeData>`
parameter primo. Selectory matching ctou parametr.

### B) DevTools - DVE plne implementace

**1) Engine native paint** (`crates/engine/src/browser/devtools_panel.rs`):
- 4414 LOC
- Pure Rust paint code, emit DisplayCommand stream
- Aktualne pouzity v engine browser mode (`cargo run -- browser`)
- Native input handling (devtools_hit_test)
- Pouzival DevToolsState struct + per-element data

**2) Shell CEF model** (`crates/devtools-frontend/static/index.html`):
- 2168 LOC HTML/CSS/JS
- WebView renderuje jako normalni stranka
- CDP bridge pres `__rwe_cdp_send_native` JS native
- Cilove arch (jako Chrome/Edge)

**Problem:** dva systemy. Native ma full feature, shell partial. User pouziva
shell, native = dead. Ale stale buildujeme + drzime + linkujeme.

**Spravne:** Drop native. Engine browser mode prepnout na bez devtools nebo
spawn devtools WV stejne jako shell.

### C) Cascade walk - O(N×M) misto O(N×log M)

```rust
// crates/engine/src/browser/cascade.rs:2080+
for node in walk(root):
    for sheet in stylesheets:
        for rule in sheet.rules:
            for sel in rule.selectors:
                if key.might_match(node) { ... }  // quick reject
                if matches_selector(node, sel) { ... }
```

`might_match` O(1) quick reject ale stale **N × M iterations**. Pro 106 nodes
× 200 rules = 21,000 might_match calls per cascade walk.

**Spravne:** rule bucketing pre-build:
```rust
struct RuleIndex {
    by_tag: HashMap<String, Vec<RuleId>>,
    by_class: HashMap<String, Vec<RuleId>>,
    by_id: HashMap<String, Vec<RuleId>>,
    universal: Vec<RuleId>,
}
```

Per node: union rules_by(tag) + rules_by(class) foreach class + rules_by(id) +
universal. Lookup O(matching) typicky 5-15 rules.

### D) Cascade re-runs per frame, even if nothing changed

`render_via` ma dirty bit ALE: pri mouse hover `dirty=true` -> full cascade walk.
**Bez hover invalidation set** = re-cascade per pixel pohyb.

**Spravne (Chrome model):**
1. Per element cascade flag `affected_by_hover: bool` (built during initial cascade)
2. Pri mouse_move hover change: pokud NIKDO z (prev_hovered, new_hovered)
   neni affected -> skip cascade re-run. paint cache uz drzi result.
3. Pokud aspon jeden affected: partial re-cascade jen affected subtree.

### E) Layout walks tree even when style change is paint-only

Pre fixu: layout cache pres `Rc::as_ptr` = miss pri kazde hover (novy Rc).
Po fixu: layout cache pres `layout_fingerprint` = hit pri color/bg change.

**OK ted.** Layout cache funguje.

### F) Paint + GPU walks per frame even if display list same

Pre fixu: paint emit display list every frame + wgpu submit every frame.
Po fixu: `last_paint_fingerprint` skip pipeline pri stejnem style content.

**OK ted.** Paint cache funguje.

ALE: pokud `:hover` rule existuje na element pres ktery prejdes, cascade
prepise color -> paint_fingerprint zmeni -> full paint+gpu. **Spravne:**
diff-based display list (jen ovlivnene Rect/Text cmds repaint) + scissor rect.

### G) shell::redraw vola render_via na vsech 3 WV per frame

```rust
fn redraw(&mut self) {
    chrome.render_via(...)
    page.render_via(...)
    devtools.render_via(...)  // i kdyz nezmenene
}
```

Per WV uvnitr render_via je `if !dirty && !needs_tick return cached`. Tj.
nedrahy ale stale 3 fn calls + state setup.

**OK.** Per-WV dirty skip funguje. Zadny issue.

## Component reuse - co je sdilene

### Sdilene (good)

- `Arc<Engine>` - device + queue + atlas + font registry. Vsechny WV sdileji.
- `editor::EditorState` - text input/textarea model. Per element ale stejna struktura.
- `selection::SelectionRegistry` - text selection. Per WV ale stejna struktura.
- `paint::DisplayCommand` - Rect/Text/Image/Gradient/... primitives. Vsechny WV emit stejne.
- `render::Renderer` - shared wgpu pipeline. WV vola `draw_segments_into_view_clipped`.

### Per-WV (good - nepotrebne sdilet)

- DOM tree
- Interpreter (JS engine)
- StyleMap cache
- LayoutBox cache
- Scroll position
- Hover/focus state
- Mouse position

### Dvojita reprezentace (bad)

- DevTools: native paint + frontend HTML (drop native)
- Hover/focus: thread_local + per-WV (drop thread_local)

## Cilovy stav arch

### Phase 1: Cleanup (TADY zacneme)

1. **Fix DOM tree visible v shell devtools** - root cause `setInterval` polling.
2. **Drop `paint_devtools_panel.rs`** + ostatni native devtools cestu. Engine
   browser mode buďto:
   - prepnout na shell-like spawn devtools WV
   - nebo dropnout engine browser mode uplne (pouzivat shell)
3. **Drop `devtools/` model dir** - DevToolsState + per-panel models pouzity
   jen native paint.

### Phase 2: Engine reform

1. **Cascade api bere hovered/focused param** - drop thread_local.
2. **Rule bucketing** (P1) - HashMap<tag/class/id, Vec<RuleId>>.
3. **Hover invalidation set** (P2) - per element `affected_by_hover` flag.

### Phase 3: Compositor (L1-L5)

1. **L1** - layer detection (HOTOV foundation)
2. **L2** - per-layer wgpu::Texture allocator
3. **L3** - compositor pass (blit layers do final target)
4. **L4** - composite-only animations (transform/opacity = jen GPU uniform)
5. **L5** - dirty rect tracking

### Phase 4: Component polish

- Scrollbar - single shared paint helper, per-WV state OK
- Text edit - editor::EditorState + paint hooks (HOTOV)
- DevTools panel infrastructure (rebuilt as JS frontend uplne)

## Co NEZAHRNUJEM v Phase 1-2 (parking lot)

- WebKit/Blink-style style sharing (Rc ComputedStyle across siblings)
- Off-thread cascade (worker thread)
- Async raster (paint na background thread)
- GPU-side text rasterization

Tyhle jsou daleky cil. Pri current effort ne realisticke.

## Mereni success

Po Phase 1-2:
- DOM tree v shell devtools fully funkcni
- Hover na devtools tabs: < 5ms/frame (60 FPS) v debug
- Hover na page: < 5ms/frame v debug
- Engine LOC -4400+ (paint_devtools_panel drop)

Po Phase 3:
- Animations transform/opacity: 0 CPU work, GPU only
- Hover na page s aktivnymi animations: 120 FPS

## Klicova lesson

**Premyslej pred fix.** Posledni session = 15 commitu reaktivne. Po teto
retrospektive mam pevny plan. Nez psat kod = check planu.

Pri novem bugu:
1. Reprodukovat
2. Kde je v arch
3. Existuje uz reseni v jine vrstve? (cache, helper, ...)
4. Pokud cista oprava = fix. Pokud nova ficha = audit. Pokud arch problem = plan.
