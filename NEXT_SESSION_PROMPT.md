# Hand-off prompt pro novou Claude Code session

Copy-paste cely tento soubor jako prvni zpravu novemu Claude vlaknu.

---

## Context

Working v worktree `J:\Claude\Worktrees\RustWebEngine\serene-bassi-0a7b83` (branch `inferius-dev/serene-bassi-0a7b83`). Projekt = RustWebEngine, Rust browser engine od nuly.

**Prectit pred startem:**
1. `CLAUDE.md` (root) - projekt instrukce
2. `HANDOFF.md` - Session N+22 detail status (Edge/CEF shell-as-crate refactor + WebView orchestration + polarity invert)
3. `git log --oneline -50` - posledních ~50 commitu

## Co je hotove (N+22 session, ~60 commitu)

### Architektura
- Cargo workspace `crates/engine` + `crates/shell`
- Edge/CEF model: shell crate = nezavisly host. Engine = embeddable WebView
- `crates/engine/src/embed/` - Engine + WebView + InputEvent/EventResponse + loader

### WebView feature parity s Chrome WebContents
- Cascade + CSS transitions (detect+apply+transitionend) + @keyframes anim tick (start/end/iter events)
- Layout + sticky positioning + paint anim
- Display list cull + scroll shift + scrollbar overlay
- Canvas2D + WebGL canvas frame
- Atlas warm + text runs extract + selection paint
- Caret blink pro focused input
- `<select>` popup overlay
- async_jobs drain + interpreter event queues (WS/fetch/rAF)
- overlay_painter callback hook

### WebView input dispatch
- Mouse: down/up/move/leave/wheel + click-vs-drag (5px threshold)
- mousedown/mouseup/click event dispatch do JS
- `<a href>` -> NavigationRequest
- Keys: keydown/keyup do focused element
- TextInput: caret-based insert + Backspace + Delete + Arrow + Home/End
- Enter on input -> form submit event + NavigationRequest (preventDefault honored)
- :hover state update + MouseLeave clear
- focus management + blur na non-focusable
- Scroll: smooth target (lerp v render_via)
- Scrollbar thumb drag (V + H)
- Text selection drag (anchor/current/extract pres painted_text_runs)
- Cursor icon (Pointer/Text/Default)
- Select all + clear selection (Esc)

### Shell crate (full browser host)
- Vlastni Window/Surface/Renderer/Engine
- All winit events wired: Resized/CursorMoved/MouseInput/MouseWheel/KeyboardInput/DroppedFile/ModifiersChanged
- Keyboard shortcuts: Ctrl+C/A/+/=/-/0/R/Wheel/L, Alt+Left/Right, F5, Esc, PageUp/Down, Arrows, Home/End, Space
- Clipboard copy (arboard)
- Navigation pres response.navigation (Get -> load_url, Post -> load_url_post)
- History stack + back/forward + reload
- Drag-drop file load
- Window title sync z webview.title()
- Continual redraw pri active animations/transitions/smooth scroll/caret blink

### Engine cleanup
- ~3000 LOC smazany: shell_chrome.rs (242), 16 dead `if false` bloku (720), TabManager + tabs.rs (747), App fields shell-only (10 polozek), READING_MODE_CSS, ChromeHit/hit_chrome, 1266 LOC App.render rewrite s thin pres webview

### Polarity invert (7/7 effective)
- App.title/zoom/scroll_target_x/y/scroll_x/y/html/css/base_url/current_path - vsechno smazane, delegate webview pres helpers
- App.interpreter zustal field pro legacy compat ALE novy path uses webview.interpreter()
- `sync_webview(html, css, base, path)` brát args (no self.* cache)
- App::resumed pres `initial: Option<(String, String, Option<String>, Option<PathBuf>)>` take()'d
- Reload sites passuji data primo

### App.render dual-mode
- **Default**: `App::render_via_webview()` thin path (~150 LOC):
  1. poll_debug_runner + sync_devtools_from_interp
  2. smooth_scroll_tick
  3. webview.set_zoom + render_via (vse rendering)
  4. Overlay pass nad webview RT (start_clear=false):
     - paint_element_highlight_offset
     - paint_inspector_overlays
     - paint_devtools_panel
     - FPS counter
  5. present_external_to_swap_chain
- **Legacy fallback**: `RWE_RENDER_LEGACY=1` env var -> original 1266 LOC inline App.render pipeline (debugging regression compare)

## Co dale (priority order)

### Krok 1: Validate render_via_webview parity s legacy

Spustit:
```
cargo run -- browser static/test.html
```
Default cesta = render_via_webview. Overit vizualne:
- Page renderuje normalne (text, kolize, anim, transitions)
- Devtools F12 panel otevre (Elements/Console/Network)
- Inspector overlay funguje (Ctrl+Shift+C inspect mode -> click element -> modry box)
- Scrollbar drag funguje
- FPS counter (Ctrl+Shift+F)
- Devtools Sources breakpoints / pause

Compare s legacy:
```
$env:RWE_RENDER_LEGACY="1"; cargo run -- browser static/test.html
```

Pokud parity OK -> krok 2. Pokud regression -> debug + fix.

### Krok 2: Smaze legacy App.render telo

V `crates/engine/src/browser/render/mod.rs` linka ~4382 zacatek `fn render(&mut self) {`. Dual-mode pres env var check:
```rust
fn render(&mut self) {
    if std::env::var("RWE_RENDER_LEGACY").is_err() {
        self.render_via_webview();
        return;
    }
    use super::{...}; // legacy continues
    ...
}
```

Smaze cely legacy telo + env var check. Final:
```rust
fn render(&mut self) {
    self.render_via_webview();
}
```

Po smaze App.render legacy = velke smaze App fields ktere uses jen legacy:
- `cached_stylesheets`, `cached_stylesheets_hash`
- `cached_style_map`, `cached_cascade_hash`
- `cached_pseudo_map`, `cached_matched_key`
- `cached_layout_root`, `layout_root`
- `animation_origin`, `animation_pause_start`, `start_time`
- `paused_animation_nodes`, `paused_node_styles`
- `animations_scrubber_drag`
- `prev_style_map`, `active_animations`, `active_transitions`, `animation_iterations`
- `painted_text_runs`
- `display_list_buffer`
- `animations_affect_layout`, `css_uses_*` flags
- `layout_affecting_animations`, `width_height_only_animations`, `position_only_animations`
- `interpreter` (now redundant, webview primary)
- `html, css, base_url, current_path` partial smaze - drz `initial` tuple v App::resumed

Risk: hodne App-side state co webview duplikuje. Po smaze App = thin host ~300 LOC.

### Krok 3: Devtools rework D1-D6

Po App.render legacy smaze + App fields cleanup, App = ~300 LOC. Zacit devtools rework dle Chrome model:

**D1: Protocol crate** (`crates/devtools-proto/`)
- DevtoolsRequest enum: DOM/CSS/Runtime/Debugger/Network/Console/Performance domains
- DevtoolsEvent enum (broadcast)
- serde JSON serialization
- mpsc channel transport

**D2: Target adapter** (`crates/engine/src/embed/devtools_target.rs`)
- `DevtoolsTarget` struct holds Arc<RefCell<WebView>>
- handle_request per-domain dispatch:
  - DOM.getDocument -> walk webview.document
  - CSS.getMatchedStylesForNode -> webview.stylesheets cascade lookup
  - Runtime.evaluate -> webview.interpreter.run
  - Debugger.setBreakpoint -> webview.interpreter.debugger
  - Network -> webview.interpreter.network_log
  - Console -> webview.interpreter.console_log

**D3: Devtools front-end** (`crates/devtools-frontend/`)
- HTML/CSS/JS pages: index.html (tab strip) + elements.html + console.html + sources.html + network.html + performance.html
- JS pouziva `window.cdp.send(method, params).then(...)` API
- Render DOM pres recursive divs (treeview)

**D4: Shell 2-WebView host**
- ShellApp drzi `page: WebView`, `devtools: Option<WebView>`
- F12 toggle creates devtools WebView + load devtools-frontend index.html
- Window split: top 70% page, bottom 30% devtools (resizable)
- Devtools WebView's interpreter ma JS binding `window.cdp.send(method, params)` -> DevtoolsTarget

**D5: Inspector overlay**
- Page WebView overlay_painter callback emit blue highlight box pres devtools state
- F12 inspect mode: page click -> emit Inspect event -> devtools WebView elements panel updatuje

**D6: JS bindings**
- `interpreter::Interpreter::register_global_fn` + Rust closure handler pro `window.cdp.send`

## CLI cheat sheet

```powershell
# Engine bin (default = naked viewport + webview render_via_webview)
cargo run                          # JS demo
cargo run -- browser src.html      # browser thin path
$env:RWE_RENDER_LEGACY="1"; cargo run -- browser   # legacy fallback (debug)
cargo run -- debug src.js out.html # debug viewer
cargo run -- dump src.html         # layout dump

# Shell bin (full browser host)
cargo run -p rwe-shell             # WebView pipeline (no chrome bar)
cargo run -p rwe-shell -- static/test.html

# Build / test
cargo build --workspace            # 0 warnings
cargo test --workspace             # 2697 pass
```

## Stats

- render/mod.rs: 9700 -> ~7800 LOC
- shell_chrome.rs: -242 LOC
- tabs.rs: -747 LOC
- Celkem engine shrink: ~-3000 LOC
- WebView grow: ~+1500 LOC (full Chrome WebContents parita)
- Shell crate: ~450 LOC (winit App + handlers)
- 2697 testy pass

## Branch + commits

```
git log --oneline -10
```

Aktualne: `10588e3 refactor(engine): smaze take_interpreter calls`
Branch: `inferius-dev/serene-bassi-0a7b83`

---

## Tvuj task ted

1. **Validate render_via_webview parity** (rucni test `cargo run -- browser`). Pokud regression - identifikuj co chybi v render_via_webview vs legacy + doplnit.
2. Pokud OK -> **smaze legacy App.render** (~1100 LOC) + env var check.
3. **Smaze unused App fields** ze duplikuji webview.
4. Po App = ~300 LOC -> zacit **devtools rework D1** (protocol crate skeleton).

User chce postupne work + commit per krok + test pass kazdym.

Konvence projektu: cesky komentar, ASCII only v kodu, caveman mode v komunikaci, kazdou nejistotu zeptat pred psanim kodu.
