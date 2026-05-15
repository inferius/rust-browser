# Hand-off prompt pro novou Claude Code session

Copy-paste cely tento soubor jako prvni zpravu novemu Claude vlaknu.

---

## Context

Working v worktree `J:\Claude\Worktrees\RustWebEngine\serene-bassi-0a7b83` (branch `inferius-dev/serene-bassi-0a7b83`). Projekt = RustWebEngine, Rust browser engine od nuly.

**Prectit pred startem:**
1. `CLAUDE.md` (root) - projekt instrukce
2. `HANDOFF.md` - Session N+22 + N+23 detail status
3. `git log --oneline -50` - posledních ~50 commitu

## Co je hotove (N+23 session, ~35+ commitu, 2844 testu)

### Architektura
- Cargo workspace: `crates/engine` + `crates/shell` + `crates/devtools-proto` + `crates/devtools-frontend`
- Edge/CEF model: shell = nezavisly host, engine = embeddable WebView
- `crates/engine/src/embed/` - Engine + WebView + InputEvent/EventResponse + DevtoolsTarget + loader

### DevTools rework (D1-D6 + D4b/c/d + D5)
- **D1** Protocol crate `crates/devtools-proto/` - DevtoolsRequest/Response/Event + Method enum (8 tests)
- **D2** Target adapter `embed/devtools_target.rs` - per-domain dispatcher, &mut WebView per call (12 tests)
- **D3** Frontend crate `crates/devtools-frontend/` - INDEX_HTML + 5 panels + theme.css + cdp.js (3 tests)
- **D4a** F12 toggle - lazy devtools WebView create + INDEX HTML inject + CDP channel arm
- **D4b** Real split layout - present_split_external_to_swap_chain (page top 60%, devtools bottom 40%)
- **D4c** Input routing po y koord - mouse hit-test dle pane + y offset adjust pres dispatch_input
- **D4d** Splitter drag - Ns resize cursor + drag MouseMove updatuje split_ratio
- **D5** Inspector overlay - Ctrl+Shift+C + pick_node_at + overlay_painter outline + click emit DOM.inspectNodeRequested + elements.html listener
- **D6a/b** CDP channel - CdpChannel { req_queue, resp_queue } Rc<RefCell<VecDeque>> + native fns + pump_cdp per redraw

### CDP target handlers (real, no stubs)
- DOM: getDocument / querySelector / querySelectorAll / getAttributes / setAttributeValue / removeAttribute
- CSS: getMatchedStyles / getComputedStyle / setPropertyText
- Runtime: evaluate (lexer + parser + interp.eval pres Stmt::Expr, unwrap WithLine)
- Debugger: setBreakpoint / removeBreakpoint / resume + 4 step stubs
- Network: getResponseBody stub (body cache TBD)
- Performance: getMetrics real (Documents/Nodes/LayoutObjects/JSEventListeners)

### DOM API Tier 1-5 (32 polozek, 95+ tests)

**Tier 1**: element.style cached+persistence, getBoundingClientRect, getComputedStyle, offset/client/scroll dims, matches/closest/contains, Event constructors, window.addEventListener

**Tier 2**: insertBefore + DocumentFragment, replaceChild, insertAdjacentElement, cloneNode, removeEventListener real (function identity), document.activeElement, createDocumentFragment

**Tier 3**: scrollIntoView, window.scrollTo/By/scrollX/Y + pageXOffset/Yoffset, focus/blur real s dispatch

**Tier 4**: DOMRect.toJSON, DOMTokenList full (length/item/replace/value/iterator), Array.from(iterable), MutationObserver real

**Tier 5**: document.styleSheets + CSSStyleSheet API, attachShadow + ShadowRoot real, scrollingElement, CSSStyleDeclaration full (cssText/length/item), document.fonts.forEach, window.getSelection stub

### Wire-up (interpreter <-> webview)
- `webview.layout_rects: Rc<RefCell<HashMap<usize, (x,y,w,h)>>>` - populated po render_via
- `webview.cascade_props: Rc<RefCell<HashMap<usize, HashMap<String, String>>>>` - populated z style_map
- `webview.stylesheets_data` - flat format pro CSSOM
- `interp.scroll_pos: Rc<RefCell<(f32, f32)>>` - bidirectional sync s webview.scroll_x/y
- `interp.layout_lookup` + `cascade_lookup` + `stylesheets_lookup` callbacks

### Window events dispatch
- load + DOMContentLoaded pri load_html po run_scripts
- resize pri webview.resize (skutecne size change)
- scroll pri set_scroll

### Shell features (full browser host)
- F12 toggle DevTools + split layout + splitter drag
- Ctrl+Shift+C inspector mode
- Ctrl+L address bar (stdout feedback)
- Ctrl+F find on page (stdout feedback)
- Alt+Left/Right back/forward + F5/Ctrl+R reload
- Ctrl+C/A/+/-/0/Wheel zoom + clipboard + select all
- PageUp/Down/Arrow/Home/End/Space scroll keys
- Esc clear selection

## Aktualne TBD

### Tier 6+ (advanced - optional)
- **Network.getResponseBody body cache** - vyzaduje fetch native refactor v builtins.rs (add response_bodies HashMap Rc sdileny pres setup_builtins)
- **Debugger.step{Over,Into,Out,Pause}** - vyzaduje pause infrastructure refactor (per-statement check v exec_stmt). Aktualne resume + breakpoint flag jen.
- **Visual overlay address bar + find** - render UI Pred page area (top 32px bar). Shell paint overlay nad webview RT pred present.
- **Find highlight matches** - walk webview.text_runs find substring + paint yellow rects pres overlay_painter.

### Tier 7+ (longer-term)
- **WebSocket cross-page** + ServiceWorker
- **IntersectionObserver real** (per-frame intersection check)
- **ResizeObserver real**
- **History API** real (pushState/replaceState/popstate event)
- **Storage events** (cross-tab broadcast)
- **WebGL2 + WebGPU JS API**

## CLI cheat sheet

```powershell
# Engine bin (CLI rezimy)
cargo run                          # JS demo
cargo run -- browser src.html      # browser thin path
cargo run -- debug src.js out.html # debug viewer
cargo run -- dump src.html         # layout dump

# Shell bin (plnohodnotny browser host)
cargo run -p rwe-shell             # WebView pipeline
cargo run -p rwe-shell -- static/test.html

# Build / test
cargo build --workspace            # 0 warnings
cargo test --workspace             # 2844 testu pass
```

## Konvence

- **Cesky** v komunikaci + komentarich. Diakritika OK.
- **ASCII** v kodu (-> ne ->, em-dash ne -).
- **CAVEMAN mode**: terse czech v komunikaci, kod normalne.
- **Po kazde feature**: build + test + commit. Commit msg kratky cesky.
- **Pri nejistote zeptat se** drive nez psat kod.

## Branch + commits

```
git log --oneline -10
```

Aktualne (top of branch): `518c47a docs(HANDOFF): DOM API Tier 5 completion`
Branch: `inferius-dev/serene-bassi-0a7b83`

---

## Tvuj task ted

User chce dale rozsirovat. Doporuceno postupne:
1. **Network.getResponseBody body cache** (1-2h) - fetch refactor
2. **Visual overlay address bar** (2-3h) - render UI nad webview
3. **Find highlight matches** (1-2h) - text_runs walk
4. **Debugger.step* infrastructure** (4-6h) - per-statement pause

User chce **postupne work + commit per krok + test pass kazdym**. Caveman cesky.
