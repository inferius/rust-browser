# RustWebEngine - HANDOFF pro dalsi vlakno

Cti **driv nez zacnes**. Plus `CLAUDE.md`, `README.md`, `TODO_CSS.md`.

## Session N+4: DevTools Firefox-style + browser shell foundation

**Build clean, 2416 tests pass.**

### Hotovo (plan 1 - DevTools Firefox-style)

- **Phase 1** Bug: highlight overlay zmizi pri panel_open=false (toggle F12)
- **Phase 2** Tab overflow ▼ popup menu pri uzkem okne (Firefox-style)
- **Phase 3** Three-column Inspector layout (tree | styles | side panel)
- **Phase 4** Side panel framework: SidePanelTab enum (Layout/Computed/
  Changes/Compatibility/Fonts/Animations) + collapsible sections
  (SectionId enum + collapsed_sections HashSet)
- **Phase 5** Page-side flex/grid overlays:
  paint_inspector_overlays - dashed border container, per-item solid border,
  gap stripes, free space hatch. State.overlays Vec<OverlayDescriptor>.
- **Phase 6** Firefox dark = vychozi tema (Default ThemeSelection -> Firefox+Auto)
- **Phase 14** var() token chips v styles pane (paint_decl_line)
- **Phase 15** Color swatch inline u rgb/hex/hsl/named (parse_css_color)
- **Phase 17** Source label per matched rule (right-aligned)
- **Phase 18** Specificity badge (a,b,c) za selektorem

### Hotovo (plan 2 - browser shell)

- **Phase 1 lightweight**: shell_mode flag + chrome bar paint
  (tab strip + nav bar + URL bar). CLI: `cargo run -- shell [path]`.

### Co zbyva

**Plan 1 deferred (deep UX, lower priority):**
- Phase 7-13: Layout sub-tab full (flex item diagram s basis/final/grow/shrink),
  Computed migrace, Animations timeline scrubber, Fonts glyph preview,
  Changes diff tracker
- Phase 16: Color picker popup (HSV trojuhelnik + RGB/HEX inputs)
- Phase 19: Kompatibilita tab data (browser support matrix per prop)
- Overlay toggle UI hit-test (state.overlays manipulace pres console pro ted)

**Plan 2 deferred (true browser shell):**
- True multi-tab: Vec<Tab> per separate Document/Interpreter/scroll/history
- Page area pod chrome bar (layout viewport_h - chrome_h)
- Tab bar interactions (klik, close, +new)
- Nav bar buttons (back/forward/reload) clickable
- URL bar focus + edit (uz mame Ctrl+L overlay - integrace)
- Keyboard shortcuts (Ctrl+T/W/Tab/1-9)
- History persistence (~/.rwe/history.json)
- Bookmarks bar
- Session restore

**Tests added (firefox_devtools_tests.rs - 14 tests):**
- default_theme_fallback_je_firefox
- side_panel_tabs_visible_default_je_5
- side_panel_tab_kompatibilita_skryta_default
- devtools_state_default_initialized
- overlay_descriptor_basic
- collapsed_sections_toggle
- parse_css_color_hex_3/6/8 + rgb/rgba/named/invalid
- compute_tab_layout_overflow

## Session N+3: text edit unifikace + bugfixes (latest)

Bugfixy:
- **WebGL z-order**: WebGL canvas pass behi mezi page CSS a overlay (devtools/
  scrollbar/addr/find). Predtim WebGL clear color prekryl devtools.
  `draw_full_frame(page_cmds, overlay_cmds, ...)`, split point v App::render
  pred paint_element_highlight.
- **Hit-test units**: vsechny mouse/wheel/scrollbar handlery prevedeny na
  logical px. `panel_h_logical()`, `viewport_w/h_logical()`,
  `point_in_devtools()` helpery. Predtim mix logical/physical pri zoom/HiDPI
  -> wheel zachytaval devtools i kdyz kurzor nad strankou.
- **Styles pane scrollbar + clip**: `StylesState::estimate_total_h()` +
  scrollbar render + clamp scroll_y na max_scroll. `in_view()` guard skipne
  text mimo body rect (top + bottom). Driv infinite scroll a content bleed
  do tab area.
- **Tree row bleed**: skip rows s y < body_y nebo y + ROW_H > body_y + body_h.
- **Main page scrollbar drag**: V/H thumb LMB hit-test + drag prevod mouse
  pos -> scroll_target_y/x.

Text edit unifikace (phase 1-7 z planu):
- **TextBuffer trait** v `src/devtools/model/text_buffer.rs`. Primitivy
  text/cursor/anchor/replace_range, default impls insert/backspace/move/
  select_all/cut/...
- ConsoleInput, **SimpleStringBuffer**, **DomInputBuffer** vsechny TextBuffer.
  DomInputBuffer adapter pres Rc<NodeData> + value attr cache + commit_back
  pri Drop. NodeData rozsireny o `input_cursor: Cell<usize>` + `input_anchor:
  Cell<Option<usize>>`.
- **Centralni dispatch_text_key + dispatch_text_click** v
  `src/browser/render/text_input.rs`. Vsech 6 mist (console, inline edit,
  form input, addr bar, find, elements search) ted volaji jeden dispatch +
  per-handler outcome routing (Submit/Cancel/Tab/Newline/Handled).
- **Cursor icon stack** - jedna funkce `compute_cursor_icon()` s prioritou
  devtools panel -> page scrollbar -> page element classify. ColResize u
  splitteru, RowResize u resize gripu, Text uvnitr edit/console/search.
- **InteractiveElement classify** v `src/browser/interactive.rs`.
  `InteractiveKind` enum (Link/Button/Checkbox/Radio/TextInput/Select/Option/
  Label/Summary/None) + `cursor_icon()`, `is_focusable()`, `accepts_text()`.
  Foundation pro budouci click handler dispatch unify.
- **Page selection** - per-text-box highlight namisto single big rect.
  Walk layout, kazdy text run intersect rect emit highlight per box. Full
  text-run model (char-byte selection, copy preserves text only) je TODO
  phase 6 future.

Material Symbols Outlined font pro icons (chevron_right E5CC, expand_more
E5CF, close E5CD, light_mode E518, dark_mode E51C, center_focus_strong
E3B4). Predtim CamingoMono renderoval velka kolecka.

### Option D - SelectionRegistry hotove

`Document.selection: RefCell<SelectionRegistry>` ted drzi:
- `input_states: HashMap<NodeId, InputState>` - per-element cursor +
  anchor pres DomInputBuffer.commit_back / Drop. NodeData clean (16B
  saving per node).
- `active_input` foundation pro JS Selection API.
- `page_selection: PageSelection { anchor, current, dragging, cached_text }` -
  App.selection_* mirrored po kazdem write (mouse handlers + Ctrl+A).
  cached_text snapshotuje compute_selection_text z layout pro JS API.
- W3C bridge: window.getSelection() + document.getSelection() toString()
  cte z registry. Driv stub vracel prazdny string.

### Vse hotovo

- App.selection_* fields smazany, registry je primary state.
  page_sel_anchor/current/dragging/begin/update_current/end_drag/clear/
  set_full helpers na App. compute_selection_text walk extract via
  fontdue advance.
- Click handler migrace na InteractiveKind: classify(node) jednou,
  per-kind match (Button -> form submit, Select -> dropdown, Link ->
  navigate, Checkbox/Radio -> toggle checked + radio name uncheck siblings).
- Char-level selection highlight: per text box, find first/last char
  ktere mid-x spada do selection range, snap na char boundaries.
  Ctrl+C delegate na compute_selection_text - extract jen selectovany
  range, ne whole boxes.

### Co zbyva (next session)

- **DomInputBuffer click-to-position v page form input**: TextInput
  element klik momentalne jen focusne. Pri-button klik mapovat
  mouse_x na byte cursor pres measure_text_width per char z page font
  (ne CamingoMono - to je devtools only).
- **Multi-line text within single LayoutBox**: char-level extract
  predpokladu jednoradkovy box. Wrap detect (\n v textu) by dovolil
  multi-line slicing.
- **Selection start/end per node, ne global rect**: aktualne anchor +
  current jsou (f32, f32) v page space. Browser pouziva (Node, offset).
  Reorder DOM/CSS-induced layout shift by neposunul existujici selection.
  Pro to potreba (run_idx_global, byte_idx) reprezentace + run_idx
  resolved kazdy frame z layout walk order.

## Stav projektu (po session N+2: devtools rework phase 1-10)

**Build:** clean, 0 warnings.
**Tests:** 2402 pass / 0 failed / 3 ignored (41 novych devtools tests).

11 devtools commitu (88cb8b8 -> latest), ~5500 LOC noveho kodu, 41 novych testu.
**wgpu:** 29 (latest stable).
**naga:** 29.
**winit:** 0.30.

## Session N+2 highlights (devtools rework)

Vytvoren novy `src/devtools/` modul - sjednoceny model + state pro inline +
static frontends. Static HTML export zustava ale je deprecated (F11 stale
funguje pro snapshot, ale aktivni vyvoj jen na inline panelu).

### Phase 1 - fundament (commit 88cb8b8)

`src/devtools/`:
- `theme.rs` - ThemeMode (Light/Dark/Auto) + ThemeFlavor (Chrome/Firefox)
  + OS dark mode detection (Windows/macOS/Linux) cached pres OnceLock
  + 4 palety (chrome_dark/chrome_light/firefox_dark/firefox_light)
  + 50+ semantickych barev (bg/border/text/syn_*/log_*/net_*/overlay_*)
- `mod.rs` - DevToolsState (theme, tab, panel_h/open, focus, frame_counter,
  ElementsState, ConsoleState, NetworkState, SourcesState, PerformanceState,
  StylesState, ContextMenuState)
- `model/elements.rs` - ElementRow + RowKind + build_rows respektuje collapsed
  HashSet. **KEY FIX:** text nodes ted v stromu (driv skipped).
- `model/console.rs` - LogEntry + LogLevel + ConsoleInput (cursor + selection
  + history + clipboard support) + AutocompleteState. 12 unit testu.
- `model/network.rs` - NetworkEntry + NetworkResourceType + NetworkFilter
- `model/sources.rs` - SourceFile + SourcesState + Breakpoint
  + parse_source_map (V3 format) + decode_mappings + decode_vlq_seq
  (full base64-VLQ decoder per spec). 12 unit testu.
- `model/performance.rs` - FrameSample (total_ms, layout/paint/gpu) +
  240-frame ring buffer + counters
- `model/styles.rs` - MatchedRule + RuleSource (UserAgent/Inline/StyleBlock/
  External) + StylesState
- `context_menu.rs` - MenuItem + MenuAction (15+ variants per Tab)
  + builders elements_row_menu / console_text_menu / network_row_menu / sources_line_menu
- `search.rs` - tag / class / id / CSS selector / XPath subset (//tag, [@a],
  //tag[N], /tag/tag) s detect_mode auto-routing. 11 unit testu.
- `focus.rs` - FocusTarget enum (Page / DevToolsConsole / DevToolsElementsSearch
  / DevToolsSourcesEditor / AddressBar / FindOverlay / ContextMenu)

### Phase 2 - frontend rewrite (commit 50cb5fa)

`browser/devtools_panel.rs` 569 LOC -> ~1080 LOC kompletni rewrite jako
frontend nad DevToolsState + Palette. Vsechny hardcoded barvy nahrazeny.

7 tabu: Elements / Console / Network / Sources / Performance / Application / Settings

KLICOVE FIXES:
- Text nodes ted v Elements tree
- Scrollbar (vertikal) emit pri overflow
- Collapsible tree s caret '>' / 'v' indicators
- Open + close tag radky pri expanded element
- Self-closing detect pro void elements (br/img/input/...)
- Selection persists pres F12 close (overlay highlight rendered VZDY)
- Element highlight overlay (Chrome-like): margin (oranzova) + border (zluta)
  + padding (zelena) + content (modra) layered rectangles + label box s tag
  selector + dimensions
- Hover row highlight
- Search bar UI s placeholder text "Find by tag / .class / #id / [attr] / //xpath"

### Phase 3 - integration wire-in (commit a978cb2)

CONSOLE:
- Mirror interpreter.console_log -> devtools.console.log per render frame
- Console eval s `$0` binding (selected DOM node jako JsValue::DomNode)
- Proper text input - cursor / selection / history / clipboard
  (Left/Right s Shift, Home/End, Up/Down history, Ctrl+A/C/X/V, Esc)
- Focus model - input dostane chary jen pri DevToolsConsole focus

ELEMENTS:
- Ctrl+F otevre devtools elements search (kdyz panel + Tab Elements)
- Auto-expand ancestors pri jump-to-search-match
- Computed styles wire-in - cascade output -> devtools.styles.computed
- Computed values panel zobrazuje vsechny resolved CSS properties
- Box info sekce (rect, padding, margin, border-width)

SOURCES:
- Inline + external `<script>` tagy registrovany jako SourceFile pri parse
- URL = src attribute nebo "<inline #N>"
- Auto-select first source pri prepnuti na Sources tab
- Klik na file row -> selected_id, content + line numbers + breakpoint gutter

THEME:
- Ctrl+Shift+T cycle Auto -> Light -> Dark
- Theme dot v toolbar tez cycle
- Settings tab - klik na Auto/Light/Dark a Chrome/Firefox volby

### Phase 5 - context menu dispatch + perf instrumentation (this commit)

CONTEXT MENU:
- RMB v devtools panel otevre per-tab menu
- Klik na item -> dispatch_menu_action s konkretnim ucinkem:
  * CopySelector / XPath / OuterHtml / InnerHtml -> clipboard
  * ScrollIntoView - posune page do view selected element
  * ExpandAll / CollapseAll - subtree z node_id
  * ClearConsole - clear log + interp.console_log
  * Copy / Cut / Paste / SelectAll - console input clipboard ops
  * CopyUrl / CopyAsCurl - network row -> clipboard
  * AddBreakpoint / RemoveAllBreakpoints - sources

PERFORMANCE:
- FrameSample push do PerformanceState ring buffer per render frame
  (frame_index, total_ms, display_list_size)
- Performance tab graf 240-frame s 16.7ms threshold cara

## Phase 6 - Interaktivni DOM/CSS edit (commit 8c7275e)

- EditState + EditTarget (AttributeValue / AttributeName / TextNode / InlineStyleProperty)
- Double-click detection (400ms okno + < 5px) zacina editaci
- attribute_at_x helper najde attr name v rowu pri x souradnici
- Edit input render = inline ConsoleInput buffer (cursor + selection)
- Keyboard route: pri elements.edit.is_some() vsechny keys do edit.buffer
  (Backspace/Delete/Arrow + Shift, Ctrl+A/C/X/V, Enter/Tab commit, Esc cancel)
- Commit:
  * AttributeValue: node.attributes.borrow_mut().insert
  * TextNode: vytvori novy Rc<NodeData>, swap v parent.children
  * InlineStyleProperty: parse + replace + serialize "style" attr
- Invalidate cached_style_map + cached_layout_root + rebuild_tree

## Phase 3 - Console autocomplete (commit c4e77e4)

- suggest(text, cursor, globals) vraci AutocompleteHit list
- Member access detect: `obj.x` -> properties z hardcoded table
  (console/Math/JSON/Object/Array/Number/String/Date/Promise/Symbol/document/
   window/navigator/localStorage/sessionStorage)
- Plain ident: globals z Environment::names() + JS keywords
- UI: Tab triggers, Up/Down navigate, Enter/Tab accept, Esc close
- 5 unit testu

## Phase 7 + persist + deprecate (this commit)

APPLICATION TAB:
- localStorage / sessionStorage list zobrazeni (key + value)
- Cte z interpreter.global pres "__storage_data__" prop

SOURCES TAB:
- Debugger toolbar (Continue / Step Over / Step Into / Step Out buttons)
- Status indicator (Paused at line N nebo Running)
- (Buttony cosmetic - real breakpoint pause vyzaduje AST span retrofit
  + interpreter pause/resume mechanism, viz TODO nize)

THEME PERSIST:
- save_persisted() ukladaa do %APPDATA%/rwe/devtools.json (Win) nebo
  ~/.config/rwe/devtools.json (unix)
- Format: `{ "mode": "auto|light|dark", "flavor": "chrome|firefox" }`
- Default::default() automaticky load_persisted() nebo Auto+Chrome fallback
- Wire: ThemeToggle / ThemeChoice / FlavorChoice -> save_persisted po zmene

DEPRECATE STATIC HTML:
- Doc-comment `DEPRECATED` v src/debug_view/devtools.rs
- F11 log "[F11 DEPRECATED] ... prefer F12 inline panel"
- Static export zachovan pro snapshot use case ale neziskava nove featury

## Phase 4 real - Breakpoints (commit 9836999)

AST RETROFIT:
- Stmt::WithLine { line: u32, inner: Box<Stmt> } wrapper
- Parser parse_stmt zachyti line z self.cur().line PRED inner parse
- AST consumers peel WithLine pri match (eval_call, bytecode compile_program,
  bytecode Stmt::Class super() detect, bytecode compile_stmt)

INTERPRETER:
- exec_stmt handler pro Stmt::WithLine: update current_line, check breakpoint,
  log "Breakpoint hit at line N", debugger.pause_at(line), pak dispatch inner
- DebuggerState struct (breakpoints, paused_at, hit_count) v Rc<RefCell>

UI WIRE:
- Per render frame: sync devtools.sources.breakpoints -> interp.debugger
- Mirror interp.debugger.paused_at -> devtools.sources.current_pause_location
- Sources tab Continue/Step buttony hit-test -> debugger.resume()

LIMITACE:
- Logical pause (no actual blocking) - JS pokracuje, jen log + UI indikator
- Real blocking pause vyzaduje async runtime (vsechen state pres channels)
- Step Over/Into/Out vsechna jednoda jako Continue

## Phase 8 - Source maps + multiline + network detail + add-attr (this commit)

SOURCE MAP FETCH:
- SourcesState::load_source_map(file_id, base_url, fetcher) - resolve relative
  + data: URI shortcut + parse_source_map (V3 format pres lite JSON parser
  + base64-VLQ decoder)
- SourcesState::map_position(file_id, gen_line, gen_col) -> (orig_file, orig_line, orig_col)
- Wire v render: po add_file scriptu hned try fetch source map pres
  fetch_text_url

CONSOLE MULTILINE:
- Shift+Enter v console input -> insert("\n") (multiline edit)
- Plain Enter stale submit (eval + log)

NETWORK DETAIL POPUP:
- Klik na network row -> network.selected + network.detail_open = true
- Pri detail_open: pravy panel zabira 40% sirky tabu, zobrazi Status, URL,
  Method, Response (preview placeholder)

ADD ATTRIBUTE:
- Context menu "Add attribute" akce -> EditTarget::AttributeName edit start
- Po commit prida novy attr s prazdnou hodnotou na node

## Phase 9 - hromada doplneni (commit e9931ae)

- Edit CSS property dvojklik v Computed pane (EditStyleValue dispatch)
- JsValue::pretty_print + multi-line console render (Object/Array indent)
- Local variables panel pri pause (capture_locals pres parent_chain walk)
- StepKind enum (Into/Over/Out) + step_should_pause + render dispatch
- Network filter tabs (All/Doc/CSS/JS/Img/Font/XHR) + apply na entries list
- Cookies sekce v Application (parse document.cookie)
- IndexedDB stores list v Application (cte indexedDB.__databases__)
- Source map "Show Original" toggle (pres sourcesContent[0])

## Phase 10 - network filter apply (commit pripraven)

- Filter aplikace na NetworkEntry list pri rendrovani (predtim jen state)

## Phase 11 - "early abort" pause + Continue rerun (commit pripraven)

ARCHITEKTONICKY KOMPROMIS:
- Tree-walking interpreter beti UI thread (interp.run() volame z UI flow).
- Real blocking pause = UI zamrzne, user neda klikat Continue. Nepouzitelne.
- Async refactor (Rc->Arc + worker thread + mpsc channels) = velky rework
  pres ~30 souboru, deferred.

PRAGMATICKE RESENI:
- Signal::Paused(line) novy variant v interpreter::Signal enum
- exec_stmt pri breakpoint hit -> return Ok(Some(Signal::Paused(line)))
- Propaguje pres exec_stmts + loops nahoru (existujici "Some(s) => return"
  wildcard arm)
- run() ho zachyti, log "[debugger] script paused at line N (early abort)",
  vraci Undefined gracefully

CONTINUE FLOW:
- DebuggerState::resume() premisti paused_at -> skip_once_line
- UI Continue button volaa rerun_paused_scripts(): vytvori novy Interpreter
  s zachovanim console_log + document + breakpoints + skip_once_line, pak
  znovu spusti vsechny <script> tagy
- exec_stmt pri stejne pause line s skip_once_line == Some(line) preskoci
  pause + konzumuje skip_once -> dalsi hit ZNOVU pause

LIMITS:
- Side effects PRED prvnim BP hit (DOM mutace, fetch calls) se opakovaly
  pri Continue rerun. Idempotentni JS funguje OK, mutating ne.
- Step Over/Into/Out vyzaduji bezne pause + pri Continue user clicks step
  na presnou line - aktualne all step kinds funguji jako "next stmt"
  pause (which is == Step Into).
- Local vars panel ukazuje snapshot pri pause (z capture_locals).

NEXT-LEVEL VYLEPSENI (TODO):
- [ ] Async refactor pres Arc<Mutex> + worker thread = real freeze pause
- [ ] Idempotent rerun protection (snapshot DOM pred run + revert pri rerun)
- [ ] Conditional breakpoints (eval expr na pause check)
- [ ] Logpoint (log expr namisto pause)

## Phase 13 - HYBRID debug mode (real freeze pause AKTIVNI)

ARCHITEKTONICKE RESENI:
- **Bezne browsing**: 0 overhead. Sync exec na UI thread, current Rc<RefCell> path.
- **Debug session** (F12 open + breakpoints set): spawn worker thread s
  vlastni Interpreter. Worker eval skripty, posila events pres mpsc channel.
  UI thread pollu events per render frame, pri pause UI je responsive.

KEY INSIGHT: Interpreter !Send nevadi pokud je VYTVAREN UVNITR worker thread
closure. Vse Rc/RefCell zustane single-thread (na worker). Sdileny mezi UI a
worker je jen `Arc<Mutex<DebuggerState>>` + Condvar + mpsc channels (vsechny Send).

NOVY MODUL: src/devtools/debug_runner.rs

`DebugRunner` struct:
- event_rx: Receiver<WorkerEvent> (Log, Network, Pause, Done, Error, Started)
- cmd_tx: Sender<UiCommand> (Continue, StepOver/Into/Out, ToggleBreakpoint, Quit)
- debugger: Arc<Mutex<DebuggerState>> sdileny s worker
- continue_signal: Arc<(Mutex<bool>, Condvar)> pro block_for_continue notify
- handle: JoinHandle pro graceful join
- is_paused, last_pause_line - cached UI state z events

`DebugRunner::spawn(html, base_url, breakpoints)`:
- Vytvori channels + Arc<Mutex<DebuggerState>> + Condvar
- Spawn worker thread s 64MB stack
- Worker closure: Interpreter::new() UVNITR (Send-clean), set doc, attach
  shared debugger, run scripts, emit events pres tx, exit

`DebugRunner::notify_continue()` - po klik Continue button v UI
`DebugRunner::drain_events()` - per frame poll, vraci nove events
`DebugRunner::is_finished()` - worker thread skoncil
`DebugRunner::join()` - blocking wait na exit

WORKER MAIN:
- Parse HTML uvnitr workera (rcdom Rc na worker safe)
- Cyklicky pres scripts: process pending UI commands (BP toggle), parse +
  interp.run(prog), flush console_log + network_log diff -> tx
- Po vsem skriptech send Done event

INTEGRATION v src/browser/render/mod.rs:
- Renderer +debug_runner: Option<DebugRunner>
- activate_debug_mode() - spawn worker s aktualnim HTML + breakpoints
- deactivate_debug_mode() - notify continue (wake any pause) + join
- poll_debug_runner() - per render frame, drain events:
  * Log -> devtools.console.log push
  * Network -> devtools.network.entries push
  * Pause -> devtools.sources.debugger_paused + locals mirror z shared dbg
  * Done -> sources.debugger_paused = false, log "Script done", auto-deactivate
- F12 toggle: pri otevreni s breakpoints aktivuje, pri zavreni deaktivuje
- Klik na BP gutter (prvni BP): auto-aktivace pokud panel open

TRIGGERY DEBUG MODE:
1. F12 (otevri panel) + breakpoints uz set -> auto-spawn
2. Klik na line gutter (prvni BP) + panel open -> auto-spawn
3. F12 (zavri panel) -> auto-deactivate + join worker

UI INDIKACE:
- Console log "[debug-mode] Worker thread spustil eval JS - real freeze pause aktivni"
- Pri pause: Sources tab pause indicator + line highlight + locals panel
- Po Done: "[debug-mode] Script done"

VYKONOSTNI PROFIL:
- Pri devtools closed nebo zadne breakpoints: 0 overhead, sync exec.
- Pri debug mode aktivni: serialization cost per event (mikrosekundy).
  Pri tisicich events za frame mozne perceptible. Pro typicke debug session
  s few BP hits = negligible.
- DOM mutations Z workera nejsou sdileny do UI - UI ukazuje cached layout
  z page load. Po script done worker exit (DOM zmeny lost). Acceptable
  trade-off pro debug mode.

LIMITS:
- Worker DOM != UI DOM (separate page parse). Page interactivity behem
  debug session omezene.
- Console.log z workera mirror pres event channel (instead of Rc<RefCell>
  shared).
- Step Over/Into/Out implementace ceka na Step kind dispatch pres cmd_tx
  (foundation hotova).

Build clean, 2402 testu pass.

## Phase 12 - Async pause infrastructure (foundation)

PRIDANE FOUNDATION pro real freeze pause (aktivuje se po Arc rework):

interpreter/mod.rs:
- type SharedDebugger = Arc<Mutex<DebuggerState>>
- type ContinueSignal = Arc<(Mutex<bool>, Condvar)>
- Interpreter +shared_debugger: Option<SharedDebugger> +continue_signal: Option<ContinueSignal>
- Interpreter::attach_shared_debugger(dbg, signal) API
- Interpreter::block_for_continue() - Condvar wait until UI notify
- exec_stmt branching:
  * continue_signal None -> early abort + Signal::Paused (current sync path)
  * continue_signal Some -> blocking_for_continue() v worker thread (foundation
    pripravena, AKTIVNI ZATIM NENI - Renderer interp stale UI thread)

browser/render/mod.rs:
- Renderer +shared_debugger: SharedDebugger +continue_signal: ContinueSignal
- notify_continue() - UI helper pri Continue button (notify Condvar)

PROC NENI SPAWN WORKER ZAPOJEN:
Interpreter struct ma 30+ Rc<RefCell<...>> fields (Environment, JsObject, NodeData,
Document, console_log, ...). std::thread::spawn(move || ... interp ...) failuje
na Send check kvuli Rc/RefCell uvnitr - i s `unsafe impl Send for SendInterp`
wrapper, closure auto-trait check projde dovnitr a Rust odmitne.

PRO SKUTECNE ZAPOJENI WORKER THREAD - POKUS + SELHANI (2026-05-09):

Provedl jsem plne sed nahrazeni Rc->Arc, RefCell->Mutex, .borrow()->.lock().unwrap()
pres ~2400 lokalit napric 41 souboru. Build prosel (po fix std::rc::Arc artefakt
kde sed vytvoril nesmyslna std::rc::Arc -> std::sync::Arc).

PROBLEM: testy DEADLOCKUJI. Rc<RefCell> umoznuje multiple .borrow() a chained
.borrow_mut() na same thread (runtime check). std::sync::Mutex NENI re-entrant -
same thread .lock() podruhe = permanent deadlock. Aktualni interpreter pouziva
hluboce nested chains kde pri eval volame na same data zase (Environment::define
volana z capture_locals z exec_stmt s already-locked debugger, etc.).

REVERTED.

PRO PLNOU FUNCTIONAL REAL PAUSE potreba:
1. parking_lot dep + replace `std::sync::Mutex` -> `parking_lot::ReentrantMutex`
2. Tato lock() vraci primo guard - replace `.lock().unwrap()` -> `.lock()`
   napric ~2400 mistech
3. Pak Rc -> Arc, RefCell -> ReentrantMutex sed pass znovu (uz idempotent)
4. Build + test (re-entrancy fix predbehne deadlocky)
5. Spawn worker thread (UnsafeSendWrapper + per-fn Send check fix) - mozne
   problemy s closure auto-trait check pres Rc-internal fields

ALTERNATIVNE:
- tokio::sync::Mutex + async runtime + .await skrz vsechny fn signatures
- Continuation-passing eval (ulozit state, opustit, resume z save) - velky rewrite

ALTERNATIVNI CESTY pro real pause:
A) Continuation-passing eval (rewrite ~30 fn signatures pro re-entry from save)
B) Bytecode VM jako primary cesta + opcode-level pause (uz existuje partial)
C) winit::EventLoop::pump_app_events vlozit do exec_stmt pause spinu (vyzaduje
   cross-trait sharing of EventLoop, !Send/!Sync)

KAZDY z techto vyzaduje samostatny multi-hour session. Foundation v phase 12
je pripravena pro variantu (A) Arc rework + spawn worker.

AKTUALNI PRIMARY PATH ZUSTAVA: early abort + rerun (Phase 11). Pro idempotent
JS skripty (read-only, no DOM mutation pred BP) funguje plne korektne. Pro
mutating skripty: rerun opakuje side effects.

## Zbyly devtools TODO

Network response body capture:
- [ ] ureq sync response.into_string() vyzaduje volat pred next request,
  pridat capture path v fetch builtin + ulozit do NetworkEntry.body_preview

CSS edit doplnky:
- [ ] Toggle property checkbox (! za property name, klik = !important)
- [ ] CSS rule add/delete (Styles section header buttons)

Source maps stack trace:
- [ ] Pri error/console log: parse trace lines + remap pres map_position

Static HTML export:
- [ ] Eventually delete src/debug_view/devtools.rs (DEPRECATED ponechan)

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
| Ctrl+F | Find on page (NEBO Elements search pri devtools open) |
| Ctrl+Shift+T | Cycle theme (Auto -> Light -> Dark) |
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
