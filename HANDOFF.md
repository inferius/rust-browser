# RustWebEngine - HANDOFF pro dalsi vlakno

Cti **driv nez zacnes**. Plus `CLAUDE.md`, `README.md`, `TODO_CSS.md`, `debug_utils.md`.

## Session N+41: VYKON (scroll fast-path + off-screen layout-anim) + canvas

User: "vykon extreme tragicky, kresleny canvas jde pres top bar", pak
follow-up: "canvas pozadi ma byt barevne, sine wave nefunguje, canvas
mizi pri scrollu". 6 commitu, 4201 testu pass. Profilovano + screenshoty.

### 5. CANVAS width=0 -> sine wave/draw barva/particles (follow-up)
- Scripty bezi v load_html PRED prvnim layoutem -> getBoundingClientRect=0
  -> resizeCanvas() nastavil canvas.width=0 -> sine wave smycka (for x=0
  to width) 0x = prazdno, hsl(offsetX/0)=seda draw, particles na x=0.
- Fix: po PRVNIM layoutu dispatch window 'load'+'resize' (first_layout_done
  flag) -> resizeCanvas prepocita s validnim rectem. Sine wave 3 barevne
  vlny, draw cary barevne.

### 6. CANVAS mizi pri scrollu (scroll fast-path regrese) (follow-up)
- Scroll fast-path re-emitoval jen scrollbar overlay, ne canvas ops ->
  canvas behem compose-only framu zmizel (blikani). Fast-path ted
  re-emituje canvas ops (clip + scroll shift).

### Nereprodukovano (follow-up)
- mix-blend "pruh dole pri scrollu": na aktualnim buildu mix-blend boxy
  ciste (full paint i fast-path frame, FPS 220). User screenshot byl
  pred dnesnimi zmenami; scroll fast-path / canvas overlay zmenil
  compose poradi. Sledovat jestli se vrati.
- Hover sekce 1: user "70 FPS, moc velky propad, ale lepsi". Na mem HW
  ~100 FPS (10ms). Paint 6.4ms dominuje (damage-driven) - dalsi optim
  by chtela tile prefetch / non-visible anim layer skip.

### Puvodni 4 fixy:

### 1. SCROLL FAST PATH (10ms -> 1ms compose-only frame)
- Scroll byl tragicky: KAZDY frame full pipeline (10ms, dipy 22ms). Layer
  textury jsou v LAYOUT coords - pri scrollu se nemeni, staci posun v compose.
- Novy gate (po compositor anim gate ve render_via): !dirty + scroll se meni
  + |scroll - last_full_paint| < 450px (CULL buffer) + zadna VIDITELNA
  paint-only animace (has_visible_noncompositor_anim) + zadne transitions/
  sel/focus. Akce: re-compose last_layer_tree s novym offsetem, sticky
  re-pozice (sticky_layers vec), overlay re-emit, compositor anims tick.
- Klic: engine-test ma 19 CSS @keyframes anims i bez JS -> is_empty() nikdy.
  Fast-path bezi kdyz zadna VIDITELNA paint-only anim (off-screen culled).
- RWE_SCROLL_FASTPATH_OFF opt-out. Smooth scroll = 20 compose frames @ 1ms.

### 2. OFF-SCREEN layout-anim neinvaliduje layout_fp (12ms -> 2ms)
- "hover sekce 1 zpomaluje 240->40". Pricina: OFF-SCREEN typewriter width
  anim (sekce 12) hashovala animovanou width do layout_fp CELEHO stromu
  kazdy frame -> layout cache MISS -> 12-15ms full re-layout kazdy frame.
- apply_animations_tracked_vis(visible): layout-prop hash JEN pro nody ve
  viewportu (+400px). Off-screen anim hodnota se stale aplikuje (po scrollu
  k ni v cull -> hash -> MISS -> re-layout s aktualni hodnotou).
- Hover box: 22ms -> 10ms (45 -> 90+ FPS), layout 12-15 -> 2ms.

### 3. CANVAS clip pres topbar (docx v6)
- Canvas2D ops jsou overlay (nad vsim vc. sticky headeru) -> kresleni
  pretekalo pres topbar. paint_canvas_ops clipuje na canvas box ∩
  {y >= pod-sticky-header} (push_clipped: Rect intersect, ostatni bbox).

### 4. CANVAS cara z rohu = destructuring ASSIGNMENT
- "kresli cary z leveho rohu do kurzoru". JS `[lastX,lastY]=[e.offsetX,
  e.offsetY]` (destructuring assignment do EXISTUJICICH lvalues) spadl v
  assign_to do catch-all Error (tise spolknuto) -> lastX/lastY zustaly 0.
- assign_to: Expr::Array + Expr::Object arms (rekurzivni). Obecny JS fix.

### Perf stav po session
- Idle (top): ~10ms/90 FPS po startup warm (initial frame 357ms = atlas warm).
- Scroll: full paint frame (10ms) na wheel event, mezi nimi 1ms compose.
- Hover sekce 1: 10ms (bylo 22). Paint 6.4ms dominuje (19 anim damage).

### Docx v6 ZBYVA (dalsi session)
- Particles canvas: 3200 JS iteraci/frame (spojnice O(n^2)) = interpreter
  zatez; trail efekt nefunguje (op-replay model, chce bitmapu).
- Selection vybira i menu pri tazeni pres stranku (sjednotit per-block).
- mix-blend pruh dole; writing-mode glyf rotace; column-count sloupec navic;
  marquee jede mimo box (overflow clip na transform anim); typewriter border.
- Sekce 13 position texty rozhozene; sekce 9 circle maly/ellipse.
- Forms: select option styling+sirka, progress styl, label->focus, range
  accent-color zluty thumb, input text selection.
- DnD ghost element + dragover styl + kurzor; :has() label klik.
- Scrollbar sipky (custom styling); JS Events mouse move sekce + desetinna.
- Paint 6.4ms baseline (damage-driven, 19 anim) - dalsi optimalizace = tile
  prefetch / nez-viditelne anim layery neraster.

## Session N+40: docx v6 - SCROLLBAR vyresen, text-shadow, focus ring, IO offsety

Docx v6 (44 polozek; rada formularovych se NEREPRODUKUJE na aktualnim
buildu - psani/validace/number filter/range drag/textarea Enter vse
funguje, uzivatel mel patrne starsi build). 4 commity, 4201 testu pass.

### 1. SCROLLBAR konecne vyresen (3. opakovani stiznosti)
- paint_box mel LEGACY "Stage 1" staticky indikator (thumb VZDY na 0)
  kresleny do page/layer contentu - zapekal se do tile textur a
  PREKRYVAL zivy overlay thumb. ODSTRANEN.
- emit_inner_scrollbars ignoroval scroll parametry (content coords v
  viewport-relative overlay pasmu = mimo viewport). Ted bx.rect-scroll.
- Diagnoza trvala dlouho: dva systemy pres sebe; bisect pres magenta
  debug barvu + pixel mereni (GetPixel). Thumb overeno 364->400 po wheel.

### 2. text-shadow (sekce 12 "nefunguje vubec")
- flush_inline nededil text_shadow na text node deti (text se kresli z
  TEXT boxu, shadow byl na elementu). + glow tuning: 2 rings nizsi alpha.

### 3. Focus ring (sekce 6 "pulka modreho borderu")
- Compositor fp (layer+tile) nehashoval outline/box_shadow a tile
  intersect nepocital s presahem MIMO box rect -> focus ring se kreslil
  jen v nahodou-dirty tiles. Fix: hash + extent expanze.
- :focus-visible: jen keyboard focus (Chrome heuristika) - po mysim
  kliku ring NENI. Tab navigace TODO (set_focus_from_keyboard(true)).

### 4. IntersectionObserver (fire jen jednou)
- collect_rects nerespektoval inner scroll offsety predku -> targety
  v io-row (H-scroll container) mely konstantni rect = zadny crossing.

### Overeno bez reprodukce (pixel mereni)
- Tabulka hover: hranice IDENTICKE pred/po hoveru (1px presnost).
- Formulare: psani okamzite, :valid/:invalid, number filter, range
  drag + hodnota, textarea Enter - vse OK na cerstvem buildu.

### Docx v6 ZBYVA (dalsi session)
- Vykon: scroll FPS dipy, hover sekce 1 (240->40), particles canvas,
  resize 14 FPS (@container), IO/resize demo perf.
- Canvas kresleni: cary z leveho rohu (lastX/lastY se nedrzi mezi tahy).
- Selection: vybira i menu pri tazeni pres stranku (sjednotit per-block).
- Topbar backdrop-filter + pruhlednost (#10), blur(4px) zesilit,
  drop-shadow doladit, mix-blend pruh dole.
- writing-mode: glyfy nerotovane; column-count: sloupec navic vs Chrome;
  typewriter border pozice; marquee jede MIMO box (overflow clip na
  transform animaci nefunguje).
- Sekce 13 position/z-index: texty rozhazene; sekce 9 circle maly,
  ellipse jinak; sekce 18 aspect-ratio vyska.
- Forms detaily: select option styling + sirka, progress styl, label
  click->focus, range accent-color (zluty thumb), input text selection.
- DnD: ghost element, dragover styl, kurzor; :has() label klik +
  odsazeni; scrollbar sipky (custom styling podpora); JS Events mouse
  move sekce + desetinna mista; "text mensi/divny vs Chrome" (font
  rendering kvalita).

## Session N+39: docx v5 sweep - menu klik, flex, SVG/SMIL, counter, float, marquee, columns

Direktiva: "udelej vsechen dokument, pracuj na tom a zbytecne se neprerusuj".
7 commitu, 4201 testu pass. Vsechno overovano pmclick/cap3 screenshoty.

### 1. Sticky hit-test/paint sync (38aa683) - MENU KLIK ZASE FUNGUJE
- Clean save last_layout_root PRED apply_paint_animations (driv se pres
  layout-HIT take-cyklus akumuloval sticky/top baseline, +470px po par framech).
- anim_baseline init NEodecita offset_top/left pro Sticky - layout sticky
  top NEaplikoval (dela az apply_sticky pri scrollu). Odecteni delalo
  our_delta_y=+48 -> VSECHNY deti sidebaru kreslene +48px pod hit-test
  stromem = klik trefoval o polozku niz. Sticky tez vyrazen z is_oof.
- Overeno: klik 07 Gradienty -> scroll na gradienty + active highlight.

### 2. FLEX fixy (daa42eb) - MutationObserver demo
- layout_flex/grid inner_x/y NEpricitaji bx.margin (parent margin uz
  aplikoval do rect) - flex container s marginem mel deti 2x posunute.
- est_h z intrinsic_text_h je content vyska -> + padding+border
  (button est 14 -> 28, container height spravne, items nepretekaji).
- Inline span sirka: text_w bez font_size flooru + content-sum fallback
  (nested span "0" 6.7px misto 11.2) + 0.5px epsilon wrap podminky.
  Pocitadlo "Pocet mutaci: N" inline a live.

### 3. MARQUEE translateX(%) (b0f9dc3)
- resolve_translate_pct(ops,w,h): % proti VLASTNI velikosti boxu pred
  compute_transform_matrix (matrix path % zahazoval) - webview compose
  i paint 3D cesta.
- CompositorAnimStore: lerp_transform TranslateMixed arm (driv fallback
  to.clone() = stalo); apply_to_layer_tree syncuje i layer.transforms
  (PLURAL - compose cte chain, ne singular).

### 4. COLUMN-COUNT fragmentace textu (d02e761)
- layout_block_multicol: pure-text children -> word-wrap na col_w,
  radky do sloupcu (balance pres div_ceil). IDEMPOTENTNI: fragmenty
  z minuleho behu se join-nou a wrapnou znovu (layout bezi vickrat
  s ruznou sirkou - pre-pass/final).

### 5. SVG: case tagy + SMIL + foreignObject (868aa02)
- svg_canonical_tag: html5ever lowercasuje, SVG case-sensitive (resvg
  "lineargradient" nezna -> MASK/gradient demo prazdne).
- SMIL aproximace v serialize_svg: <animate>/<animateTransform> ->
  override atributu aktualni interpolovanou hodnotou (from/to, values
  seznam, repeatCount=indefinite; lerp_numbers_positional pro path d).
  animateTransform se SKLADA s base transform (gear stred). POZN:
  animace bezi jen kdyz neco generuje framy (na test strance FPS
  counter) - invalidation scheduler pro ciste staticke stranky TODO.
- foreignObject: layout rect z attrs + block layout deti; paint je
  kresli normalni cestou pres SVG bitmapu (resvg FO preskakuje).

### 6. COUNTER + list-style (885f4e5)
- parse_content_value: content je SEKVENCE komponent (string counter()
  attr() open-quote...) - tokenizer + concat (driv jedina komponenta
  na cely string -> doslovny vypis syntaxe).
- counter-reset default 0 per spec (increment 1).
- list-style shorthand expanze (none/type/position/image).

### 7. FLOAT obtekani (08bbf40) - shape-outside demo
- flush_inline: floats param + line_lx/line_rx per-line bounds.
  Vsechna wrap mista pres ne. Multi-line text rect.x = line start
  prvni radky (render kresli \n radky od rect.x). Aproximace: radky
  pod floatem zustanou odsazene - plna per-line x = text fragmentace TODO.

### Dalsi drobnosti
- MouseUp :active clear nastavuje dirty hned (button release animace
  driv cekala na mouse-leave).
- RWE_LAYOUT_DUMP=force: dump i bez JS (dom_version gate skip) +
  hloubka 12.
- Tabulka hover jump: OVERENO OK (vyreseno drivejsimi hover fixy).
- Sekce 11 scrollbary OK; sekce 18 aspect-ratio/contain/env OK.

### Docx v5 ZBYVA (vetsi render projekty)
- filter: blur() slabsi nez Chrome + lehce oriznuty nahore; drop-shadow.
- backdrop-filter top bar (#10).
- writing-mode: glyfy se NErotuji (sloupce ok, plna compliance chce
  rotovany glyph run v renderu).
- Float: per-line x pro radky pod floatem (text fragmentace).
- Scroll perf next: tile prefetch, compose-only fast path.
- "Pocet mutaci:" mezera za dvojteckou mizi po content-only patchi
  (trailing space pred nested spanem - kosmeticke).
- Chrome-srovnani jednotlivych sekci po tehle davce (novy docx?).

## Session N+38: hover/typing perf, value pseudos, kompositor GATE

User feedback po N+37: hover 240->30 FPS, typing porad lagoval, border
:valid/:invalid se neukazoval/mizel, pismena v number, menu active
"spatne", + vytka ze docx v5 nebyl zpracovan cely (sekce viz TODO nize).
4 commity, 4201 testu pass.

### 1. HOVER perf (28ms -> 7.3ms move frame) - 2c00c04
- cascade miss UZ NEforcuje re-layout (layout_fp rozhoduje sam).
- rebake_changed_paint_props (render/mod.rs): pri layout cache HIT po
  cascade zmene diff prev vs new style_map per node a prepis paint props
  VC. RESETU (hover-off -> bg None, transform clear...). KLIC: apply_
  paint_animations resetovat neumi (prepisuje jen pritomne props) ->
  stale hover bg zustavala navzdy ("oznacene" nav linky).
- POZOR ARCH: hover menici ::before/::after pseudo styly rebake nevidi
  (pseudo boxy bez node) - pri probleme force full layout pres dirty.

### 2. TYPING (25ms+ -> 4.5-6ms per pismeno) - 2c00c04
- value mutace = bump_dom_version_content_only + WebView::
  update_control_text (primy zapis do cached layout stromu). ZADNY
  re-layout/re-cascade per pismeno.
- :valid/:invalid/:placeholder-shown: (a) cascade cache klic hashuje
  validity STAVY inputu (hash_validity ve webview render_via);
  (b) matched_decls/walk cache focus_bit nese value-state bity
  (empty=8, invalid=16, form marker=4) - bez toho per-node cache
  vracela matched rules z doby pred psanim.
- number filtr: 2. brana is_valid_number_prefix po editu (revert pri
  fail) - "ee12e3e" -> "12e3" Chrome parity.

### 3. MENU active "spatna sekce" - VERDIKT: Chrome parity (800205a)
Ground-truth v realnem Chrome (localhost http server + JS sonda):
klik na "15 Formulare" -> Chrome oznaci "19 Modern CSS"; scroll 1470 ->
"09 Clip-path". Identicke chovani jako my = vlastnost naivniho
scroll-spy JS na strance (posledni isIntersecting entry vyhrava),
NE nas bug. RWE_IO_DBG / [IO-CALL] / [IO-FIRE] instrumentace zustava.

### 4. CompositorAnimStore GATE (35bc011) - 156 -> 238 FPS na anim sekci
- compose_layer_tree_into: compose smycka extrahovana do fn (sdilena).
- FAST PATH na zacatku render_via: anim-only frame (podminky: !dirty,
  no transitions/scroll anim/resize/selection) = tick store + apply na
  last_layer_tree + re-compose z textur + cached overlay
  (last_overlay_cmds). 0.84-0.99ms per frame.
- Registrace: started @keyframes 2-stop 0/100% transform/opacity-only
  single-op anim -> CompositorAnim; ended -> remove. Ne-friendly
  zustavaji main pipeline (multi-stop, barvy, cubic-bezier, delay).
- RWE_GATE_DBG, RWE_COMPOSITOR_GATE_OFF. Znama omezeni: animation-
  iteration eventy se behem gate nedispatchuji (animationend ano -
  gate propada do full framu pri dobehu); caret blink behem gate
  zamrzly (cached overlay) - fokus+anim soucasne je vzacne.

### NOVA TRAJEKTORIE: GOOGLE.COM COMPAT (user: "posunout to")
Smoke test `cargo run -- browser https://www.google.com` renderuje
prekvapive blizko (logo, search box, header linky). Opraveno (a6e6e72):
flex:<basis> shorthand (search pole 200->504px), input submit label +
intrinsic sirka (tlacitka maji text). Chrome ground-truth screenshot
porizen pres Chrome MCP (prihlaseny ucet - pro fair srovnani pouzit
anonymni). ZBYVAJICI Google blockery (priorita pro dalsi session):
- "Co se vam honi hlavou?" JS-vlozeny element s obrim fontem spatne
  positioned (v Chrome maly/skryty) - zjistit pres runtime DOM dump
  ktery selektor/styl nematchujeme (JS-inserted <style>? CSSOM?).
- SVG ikony rozsypane (hamburger/avatar oblast vpravo nahore, lupa
  v search boxu - cerna cara + blob).
- Footer se nerenderuje (Cesko + linky radky).
- Cookies/consent + prihlasovaci tlacitko (anonymni HTML varianta).
- Dump rezim nespousti skripty - pro Google debugging pridat
  `dump --run-js` nebo runtime layout dump hotkey.

### Session N+38b: testovaci stranka "od podlahy" - SYSTEMOVY overflow fix
- **Layout overflow-clip placeholder bug (78eddb5)**: block child bez
  vysky dostaval placeholder 20px; layout_block s overflow clips() ho
  vzal jako parent-preset a na konci NATVRDO vratil (content spocitany
  66px -> box 20px, obsah oriznuty). Fix: overflow-clip boxy placeholder
  nedostavaji (0 = auto). OPRAVILO: .resize-demo (Observers) + obecne
  vsechny overflow:hidden/auto boxy bez explicit vysky. POZOR: zmenil
  se layout cele test stranky (menu pozice ve scriptech posunute).
- ResizeObserver demo KOMPLET funkcni: styl + obsah ("sirka: 951 px")
  + resize grip drag (951->548 px) + observer updaty.
- JS Events sekce OVERENA FUNKCNI (mouse zone x/y updaty, keyboard
  zone focus + keydown/keyup do logu) - docx stav byl zastaraly.
  offsetX/Y round na int (Chrome parity).
- Google compat (a6e6e72): flex:<basis> shorthand (drive cely ignorovan
  -> search pole 200px) + input submit/button label (control_text) +
  intrinsic sirka tlacitek.

DALSI KROKY (overene stopy):
- WHEEL nad overflow containerem scrolluje STRANKU (sekce 11
  "overscroll-behavior: contain" box): find_scroll_target vraci None.
  Debug: RWE_SCROLL_DBG=1 vypise [SCROLL] target. Zkontrolovat
  needs_scrollbar_y (inner_content_h vs rect po overflow fixu) +
  collect_path zasah boxu.
- MutationObserver demo: "Pocet mutaci:" cislo (#mutation-count span
  "0") se nezobrazuje; tlacitka bg pretahuje nad label radek.
- IO demo boxy: vsech 6 zlutych (vsechny visible) - container bez
  overflow-x => mozna spravne; srovnat s Chrome (io-row sirka).
- JS Events sekce: keyboard area neklikatelna/no focus border/keys;
  mousemove "jen nekdy".
- Observers: IO demo boxy mozna obracene (overit proti Chrome jako
  menu); resize element styl + nejde resizovat + chybi obsah;
  MutationObserver layout pod tlacitky (margin/cislo na radku).
- Sekce 11: VNITRNI scrollbar (overflow container) thumb se nehybe +
  preteceni textu.
- Buttony: release animace az po odjeti mysi (:active clear na MouseUp
  bez ohledu na pozici?), cursor rucicka vs text-beam sjednotit, teal
  focus border po kliku (focus-visible vs focus).
- Gradienty: radial circle tvar, animated gradient se neanimuje,
  repeating-linear/radial tiling.
- filter: blur cut nahore + slabsi nez Chrome; drop-shadow styl.
- mix-blend-mode: cela sekce jina (vsechny barvy stejne) - mozna
  blend compose path nefunguje per-mode.
- clip-path: circle prvni 2 male, ellipse tvar.
- Typografie: text-shadow nefunguje, writing-mode otoceni glyfu,
  column-count chovani, typewriter border pozice, marquee nejede.
- Inline SVG sekce 13/14 (circle 75% temer neviditelny, prazdne boxy).
- Obskurni CSS horni cast; tabulka hover jump (px); top bar backdrop
  blur; devtools responsivita; plynuly resize.

## Session N+37: docx v5 - selection painting, sidebar vh bug, FORMS refaktor

User mandat: docx v5 (a) Chrome-kontrastni selection, (b) sidebar border
az dolu, (c) PORADNY refaktor formularu, (d) odpoved na architekturu text
renderingu. 4 commity, 4201 testu pass.

### 1. Chrome-style selection painting (573a3fb)
Highlight plnou alphou (#3297FD / ::selection bg) + recolor JEN vybranych
znaku: recolor Text klony jdou zvlast passem (draw_selection_text_pass)
s thread-local SELECTION_GLYPH_CLIP - build_vertices kresli jen glyfy
s centrem uvnitr sel rectu. Auto-kontrast dle luma bg (tmavy->bily text).
Nevybrane znaky zustavaji z puvodni vrstvy. Overeno pixel-presne.

### 2. Sidebar border / VH BUG (008a986) - SYSTEMOVY
`height: calc(100vh - 48px)` v runtime = 720 (768 fallback!) misto 852.
ROOT CAUSE: stranka s @container jde pres cascade_with_container_sizes,
ktera NEsetovala MATH_VIEWPORT thread-local ani NEvolala
resolve_viewport_units na deklaracich -> VSECHNY vh/vw na strance
resolvovaly proti 1024x768 fallbacku. Fix = stejny prolog jako
cascade_with_viewport. Diagnostika: RWE_SB_DBG=<id> vypise po layoutu
rect boxu + vp_arg + math_vp (takhle se naslo math_vp=0x0).

### 3. FORMS refaktor (5cfe44e) - control_text replaced model
Root cause vetsiny form bugu: obsah controlu sel do bx.text -> layout
sizoval box podle TEXTU:
- input se po 1. znaku SCVRKNUL na vysku radky bez paddingu (text branch
  flush_inline misto children/replaced branch; placeholder byl virtualni
  text CHILD = jina cesta nez s value = nestabilni vyska),
- select 12px vysoky s textem POD boxem, range ignoroval width:100%
  (UA default explicit_width 129 vyhraval nad width_pct), progress 120x0
  (flex measure reset rect.height non-explicit childu).

Refaktor (Chrome model: form control = REPLACED element):
- `LayoutBox.control_text` - obsah (value/selected/textarea) ODDELENY od
  text. Layout ho NEVIDI. Kresli jen paint (padding + v-center; pri empty
  placeholder z attru placeholder_color barvou). Form control NEMA
  obsahove children (skip v build loop).
- Synteticke dims po CSS apply: h = line_h + pad + border (border-box),
  min-height clamp, zafixovano i do explicit_height (flex/grid measure
  reset by jinak spadl na pad+border floor = 14px prouzek).
- CSS width/height `%` NULUJE UA/attr explicit default.
- DAMAGE: layer structural_fp + tile fp hashuji control_text + checked +
  input value (jinak psani/klik NEdamaguje -> stary obraz; presne tohle
  bylo "psani se zpozdenim" a "checkbox dead"). D4 overlay items (select
  popup...) potrebuji warm_atlas_for - drive glyfy popup options chybely.
- Interakce: type=number filtr znaku; AltGr (Ctrl+Alt = @ na CZ) neni
  ctrl-shortcut + input-local Ctrl+A/C/V/X jde do dispatch_text_key
  (paste do inputu driv MRTVY); Enter v textarea = newline; value mutace
  bumpa STYLE verzi kdyz CSS ma :valid/:invalid/:placeholder-shown
  (jinak cascade cache prezila a invalid border se neukazal); caret
  pozice z Document.selection registry (ne legacy input_caret mapy).

Harness pozn.: typekeys2.ps1 (SendKeys + AttachThreadInput foreground
force; SetForegroundWindow sam o sobe FLAKY - kdyz selze, klavesy jdou
do konzole a "nic se nedeje" = falesny zaver). "~" v SendKeys = Enter,
muj escape ho posilal doslovne - pro Enter pouzit {ENTER}.

### Zbyva (priority dle docx v5)
- **CompositorAnimStore gate** (perf next level, user approved):
  compositor_anims.tick + apply_to_layer_tree UZ BEZI v render_via;
  zbyva GATE = pri compositor-only zmenach skip layout+paint a jen
  re-compose z last_layer_tree (plan v N+36b TODO nize).
- Top bar "pruhlednost" pri scrollu = chybejici backdrop-filter blur
  (header rgba bg .7 bez bluru prosvita ostre). Task #10.
- Canvas sekce, tabulka hover jump, @container/:has, SVG sekce,
  writing-mode glyphy, repeating gradienty, text-shadow - viz N+36 TODO.
- Select popup: hover highlight + option text barvy pro dark theme
  stranky (kosmetika).
- Textarea multi-line caret (x_at_char pres celou value vc. \n - caret
  na 2.+ radku spatne x; render OK, jen caret pozice).
- 13 unused-import warnings v interpreter/browser modulech NEpochazi
  z teto session (web_locks/popover/jxl_decode/test262... - cizi
  rozpracovana prace, nesahano).

## Session N+36: chyby-rbro v4 "od podlahy" - systemove fixy jadra

User mandat: docx v4 (10.6. po N+35 fixech) + HANDOFF deferred, "vezmi to
konecne od podlahy". Triage realnou interakci odhalila, ze cast docx
stiznosti uz neplatila, ale nasla SYSTEMOVE root causes za "klikani nikde
nefunguje". 7 commitu, 4201 testu pass.

### KRITICKA lekce - testovaci harness
Stare scripty (inject/typeseq/drag/wheel.ps1, SendInput ABSOLUTE) maji na
multi-monitor + DPI setupu NEKONZISTENTNI offset (kliky dopadaly o desitky
px vedle -> falesne zavery). NOVY `%TEMP%\pmclick.ps1` (PostMessage
WM_MOUSE* primo do okna, client px presne; mode: click/move/drag/wheel) +
`%TEMP%\cap3.ps1` (PrintWindow window-size bitmap + presny client crop;
cap2 mel bitmapu client-size = orezany pravy/dolni okraj + (8,31) border
offset NEzapocteny -> VSECHNY pixel pozice z cap2 byly posunute!).
Capture pozice -> client coords: ODECIST (8,31) presne via cap3.

### Opraveno (commity v poradi)

1. **Sticky/fixed hit-test** (05b84a7, JADRO "klikani nikde nefunguje"):
   hit_test nezna NoScrollShift -> sticky sidebar/fixed top bar pri
   scroll > 0 KOMPLETNE neklikatelne (klik/hover/kurzor sly content
   coords = proletely na obsah pod). `hit_test_scrolled(x,y,sx,sy)`:
   fixed/sticky subtree testovat s viewport coords; CSS painting order
   (positioned deti PRED normal-flow pri hit-testu); r-tree Sticky do
   has_special + special_at_point viewport query; vsechny call sites
   predavaji scroll. + Anchor `<a href="#id">` navigace (drive fragment
   ignorovan = sidebar nav mrtva). + webview sel_begin guard pri
   DragSession/user-select:none ("zasekne se select" pri DnD - webview ma
   VLASTNI selection pipeline, N+35 guardoval jen App vrstvu). DnD po
   fixech funguje (dragstartALPHA + dropALPHA v event logu).

2. **Scrollbar gutter + overflow text** (37e0716): canvas-bg Rect na
   index 0 padal do D4 layer casti (zahozen + posouval d4_overlay_start
   o 1) -> bily pruh. Fix: paint::canvas_background() -> D4 clear color
   (renderer.page_clear_color, UNORM 1:1 bez gamma); monolithic insert(0)
   po scroll shiftu. + Text bbox v overflow-clip dropu pocital sirku pres
   CELY content vc. \n -> zalomeny text "stred mimo clip" -> DROPNUT =
   prazdne overflow boxy (sekce 11 ov-scroll, @container, RO box). Fix:
   bbox = nejdelsi radek x pocet radku. Main scrollbar thumb se hybal
   spravne uz pred fixem (docx stiznost ze stareho buildu).

3. **Gradienty** (9276d4c): shift_command_x/y neposouval Radial/Conic
   STRED (jen rect) -> v D4 layer-local rasteru stred ve world coords ->
   shader sampluje transparent cast = radial/conic/repeating-radial
   CERNE na strankach s vice layery (v izolaci fungovaly!). + parse
   config-vs-stop: "#e8ff6a 90deg" zahozen jako config (parse_color
   selhal) -> is_gradient_config_part = jen keywordy. + interp edge
   clamp (pred prvnim stopem barva PRVNIHO stopu, ne fallthrough na
   posledni) + radial outer fill annulus za poslednim stopem. + dedup
   double-emit (bg_gradient + backgrounds layer kreslily totez 2x).

4. **Border-radius per-corner + elliptical** (f8f39ac): LayoutBox.
   border_radius_corners [(val,is_pct);4] (tl,tr,br,bl; slash elliptical
   prumerem; longhandy; CSS overlap constraint scale-down).
   DisplayCommand::RectRounded -> mode 11 SDF (radii v color2 slotu,
   sdf_rounded_box_4 vybira radius dle kvadrantu). Bg + bg-layer +
   rounded border ring (outer+inner inset). half=D-tvar, blob, asym=leaf.

5. **Layer text LCD** (501e6c6): LCD subpixel = DRUHY draw pass mode-9
   vertexu dual-source pipeline. Do TRANSPARENT layer textury zapsal
   barevna per-channel data -> premul-OVER compose je neslozil =
   "rozsypany barevny text" v text layerech (mix-blend .blend-text,
   z-index spany). Fix: LCD pass skip pri VIEWPORT_OVERRIDE (layer/tile
   raster) - composited layery grayscale AA (jak Chrome). + layer
   root_rect px snap (floor origin + ceil size) v extract_layer_tree.
   Diagnostika: debug_dump_texture (RWE_DUMP_LAYERS=<dir>) GPU->PNG;
   RWE_LAYERCACHE_DBG (per-layer cmds+texty); RWE_INPUT_DBG=chain
   (hit-chain dump).

6. **Typografie** (9dcf92a): MULTI-ANIMATION (from_styles_multi - vsechny
   subspecy `animation: a 3s, b 1s`; drive jen prvni -> typewriter
   neblikal). text_overline INHERIT v inline smycce (underline/strike se
   dedily, overline NE -> double overline mrtvy). Vertical text: PASS 0
   lame na SLOUPCE dle inner_h + flex da vertical itemu available cross
   height (drive 1 nekonecny sloupec pretekal pres sekce). white-space:
   nowrap ZAPOJEN (parsoval se, nikde nepouzival) do vsech 3 wrap branchi
   flush_inline + inherit -> marquee na 1 radku.

### Triage zavery (co FUNGUJE na aktualnim buildu, docx je stale)
Keyboard events (focus border + klavesy + log), mousemove (offsetX OK;
offsetY SPATNE ~5.5 misto ~40 - viz TODO), DnD (po sel guard fixu),
checkbox/radio/range/typing, table render, calc/clamp, :has() styling,
color-mix, mix-blend barvy, filtry, clip-path tvary, IO fire (loguje pri
scrollu), anchor nav (po fixu), formulare render.

### Dodatecne overeno/opraveno (po HANDOFF zapisu)
- **classList.toggle(token, force)** (8919a4c): force arg ignorovan ->
  IO `toggle('visible', isIntersecting)` prepinal pri kazdem fire =
  "IO funguje obracene". Po fixu vsech 6 .io-box zlute visible ✓.
- **MutationObserver**: mutateDemo appendChild + styling FUNGUJE
  (node #1/#2 v targetu s barvami). Zbyva: vnoreny span
  (#mutation-count "0" uvnitr span textu) se nelayoutuje v inline
  flow rodice -> cislo nevidet; buttony lehce prekryvaji box.
- **DnD plne overeno**: dragstartALPHA + dropALPHA v event logu i na
  plne strance (drop zone "Dropnuto" mizi po 2s - capture to nestihl,
  vypadalo flaky, NENI).
- **offsetY v mousemove NENI bug** - offset je vuci nejhlubsimu
  targetu (text/p dite), Chrome stejne.

## Session N+36b: PERF "zalostny vykon" - idle 240 FPS, resize 3.4x

Mereno RWE_PROF + [PAINT-SUB]/[RVW]/[RV-PRE]/[RSZ] instrumentace (gated).
Vysledky: IDLE 95 -> 240 FPS (vsync cap, 698 -> 23 full renderu/10s);
SCROLL pres celou stranku 127-240 FPS (drive dipy 42-68); viditelne
animace ~100-150 FPS (PROF total 5.3ms); RESIZE drag 230 -> 67ms/step.

Komity ae2de01 (idle/anim) + 744eed7 (resize):
1. Content-only DOM mutace (SVG points RAF anim) per-node tracking ->
   off-screen zmeny NEdirti render (Chrome damage model).
2. CSS animace mimo viewport netickaji (any_active_animation_visible).
3. layout_fp z CASCADE BASE ^ hash animovanych layout hodnot (fp cache
   HIT misto O(N) rehash; fill:forwards konstanta = HIT).
4. layer_local_cache PERSISTENT (drive new HashMap kazdy frame = full
   re-paint vseho).
5. Resize: pending_resize koalescing (engine bin, mirror shell) +
   resize_layout_override debounce (rapid <400ms -> layout/cascade na
   puvodni velikosti = cache HIT + 0 raster; settle 350ms -> reflow).

ZBYVA (dalsi perf level - HANDOFF TOP sekce "compositor-only fast
path"): anim frame ~5.3ms breakdown: layout-blok 1.4 (clone 0.46 +
paint_anims 0.57 + extract 0.65) + paint 2.5 (warm+raster colorCycle
layeru 2.3 - submit overhead per raster) + gpu 1.1 (compose 41 layeru,
per-layer buffer/bind alloc). Pro 240 pri viditelnych anim: zapojit
CompositorAnimStore + batch raster submitu + bind group cache.
POZN: full layout MISS (textContent 2x/s FPS counter) = ~12ms full
layout_tree - subtree cache hit rate stoji za audit.

### TODO dalsi session (zbytek docx v4)
- **Canvas sekce NEKRESLI** (hlavni zbyvajici JS bug): klik "Sine wave"
  button zaregistrovan (highlight ✓) ale canvas zustal prazdny. Zadny
  JS error. Pravdepodobne canvas mode switch / ops pipeline / RAF
  kresleni - potreba deep-dive (canvas_ops drain -> paint_canvas_ops).
- **Marquee animace nejede** (translateX 100%->-100% na inline-block;
  zalamovani uz OK). Typewriter width-anim OK + blika.
- **column-count single text block** fragmentace (1 radek pres sirku,
  pretejka doprava; multi-child funguje).
- **MutationObserver UI**: buttony prekryvaji box pod nimi, "Pocet
  mutaci: 0" na spatnem radku. **IO boxy** se nehighlightuji pri
  intersection (fire ok, class change styling se neprojevi?).
  **ResizeObserver box** prazdny obsah (dynamicky JS textContent?).
- **Canvas sekce**: Kreslit/Particles/Sine wave buttony + kresleni
  (netestovano po fixech harness).
- **Tabulka hover jump** overit pmclick harness (N+35 tvrdi fixed).
- **@container resize drag** (sekce 19; box po text-fixu uz neni prazdny
  - overit obsah + drag).
- **backdrop-filter top bar** (deferred N+35 #1), **resize okna perf**
  (73ms/frame drag), **filter blur top-cut**, **AA zubate rotovane**
  (supersampling), **text-shadow glow** vyraznejsi, **repeating gradient
  tiling** (px stops = 1 cyklus), **inline SVG** boxy 2/5/6 prazdne
  (stroke-dashoffset/foreignObject), **shape-outside** circle tvar +
  text overlap, **CSS counter()** v ::before content literal.
- Stare harness scripty NEpouzivat (DPI offset) - pmclick/cap3 only.

## TODO (dedikovany tah): PERF 120 -> 240 FPS = compositor-only fast path

**Stav:** 35 -> 120 FPS hotovo (commit "PERF: paint-animovane elementy na vlastni
layer" - colorCycle apod. bg/border animace co NEjsou transform/opacity ted
dostavaji vlastni layer pres FORCE_LAYER_NODES, jinak damagovaly root -> full
root re-paint 17.7ms). 120 FPS je vsync cap na 120Hz; na 240Hz pada na 120
protoze frame work ~5-7ms > 4.2ms (240Hz interval).

**Problem:** kazdy frame transform/opacity animace bezi CELA pipeline i kdyz se
meni jen jak se layer slozi (ne obsah). Layout faze ~3ms = layout_fingerprint
prepocet (~0.7ms, O(boxes) kazdy frame), layout_root.clone (0.46ms),
apply_paint_animations (0.57ms walk vsech boxu), extract_layer_tree rebuild
(0.65ms), mark_damage. + paint ~0.7ms + gpu ~1ms. (Profilovano RWE_PROF +
docasne timery; repro: tall page 500 divu + spin = layout 2.9ms, paint 0.6.)

**Korenova pricina:** compositor-driven anim path je NEZAPOJENY.
`CompositorAnimStore` (crates/engine/src/browser/compositor/anim.rs: tick +
apply_to_layer_tree) existuje, ALE `register_transform_anim`/`register_opacity_anim`
(webview.rs ~615) maji ZADNE callery. Takze CSS animace jdou plnou cestou
(apply_paint_animations -> style_map -> box.transform -> extract -> layer)
kazdy frame.

**Fix (fast path) - jak Chrome/FF/WebRender (SCENE vs FRAME, reuse scene):**
1. Detekovat compositor-only frame: gating `needs_tick && is_compositor_only_frame()
   && !self.dirty && layout_key == self.layout_cache_key && last_layer_tree.is_some()
   && target_view.is_some()`. (is_compositor_only_frame uz existuje webview.rs ~3611.)
2. Skip cela layout+paint faze (webview.rs render_via ~2693-3360).
3. Reuse `self.last_layer_tree` (= scene). cascade faze (keyframe tick, ~2560, bezi
   PRED paint_cache_hit ~2676) UZ zapsala interpolovany transform do style_map ->
   read ho a update layer.transforms/opacity na last_layer_tree (lightweight walk
   layeru, ne vsech boxu).
4. Re-compose: compose sekce webview.rs ~3362-3510 UZ pouziva self.last_layer_tree
   + compose_view_to_view_transform_into_encoder. Zavolat ji (extract do metody
   NEBO fast-path branch s early return).

**Komplikace (proc to chce dedikovany fokus + testovani):**
- (a) OVERLAY: scrollbar/devtools/canvas se kresli v paint fazi (display_list po
  d4_overlay_start) co fast path skipuje -> zmizely by. Nutno cachovat overlay
  cmds z posledniho full frame + reuse, NEBO gate fast path na "zadny overlay".
- (b) presne gating (nesmi se spustit pri DOM/scroll/hover/layout-animaci jako width).
- (c) reconcile s apply_paint_animations (aby se transform neaplikoval 2x).

**Reference (stazene v `F:\Projects\browser-refs`, shallow):**
- `gecko-dev/gfx/wr/webrender/src/` - WebRender (frame_builder.rs, composite.rs,
  picture.rs) = primo model meho compositoru. SCENE vs FRAME split.
- `chromium/cc/trees/` - property_tree (TransformTree/EffectTree), damage_tracker,
  draw_property_utils = jak cc kresli frame z trees BEZ main-thread relayoutu.
- `servo/components/layout` + `gecko-dev/layout/generic` (nsFlexContainerFrame,
  nsGridContainerFrame) = flex/grid intrinsic reference.

**Verifikace:** RWE_PROF (frame time klesne ~5ms -> ~1ms) + capture (animace bezi
plynule + scrollbar/overlay NEzmizel). Revert pri regresi.

**Doporuceni:** nechat na dedikovany tah (ne spech). 120 FPS je plynule, neni
urgence; fast path je velka hot-path zmena s klesajicim perceptualnim prinosem.
Ale ma hodnotu i mimo FPS (spravna architektura, nizsi spotreba).

## Session N+35: chyby-rbro v3 systematicky - 13 fixu interakce + rendering

User mandat: "systematicky projdi cely dokument a opravuj to konecne vsechno"
(reference Chromium/FF algoritmy). **Verifikacni lekce (kriticka):** overovat
REALNOU INTERAKCI (SendInput hw injection + gated traces + deterministicke
Rust testy), NE jen rendering screenshoty. 13 commitu, vse takto overeno.
4194 testu pass, engine-test 85 FPS bez regrese.

### Verifikacni harness (PowerShell scripty v %TEMP%, reusable)

- **cap2.ps1** `-titleMatch X -outName Y.png -cw -ch [-sx -sy]` = PrintWindow
  capture + crop. **POZOR titleMatch: pouzij "Rust" NE "Engine"** - user ma Chrome
  s engine-test otevreny ("Engine Test Suite" koliduje). rwe titulek je "Rust Web
  Engine - <path>" pri loadu, "RustWebEngine - N FPS" po renderu (FPS overlay) ->
  pro engine-test (titulek = page <title> "Engine Test Suite") matchni "FPS".
  Okno byva minimalizovane -> script dela ShowWindow(SW_RESTORE)+SetForeground.
- **inject.ps1** `-clientX -clientY -vkHex` = klik (mouse_event ABSOLUTE 0x8001/2/4)
  + 1 klavesa. Klik AKTIVUJE okno (SetForegroundWindow z bg PS nestaci pro WM_*).
- **typeseq.ps1** `-clickX -clickY -text` = klik + napis string (VkKeyScan).
- **wheel.ps1** `-clientX -clientY -ticks -dir` = klik + wheel. **POZOR**
  `[uint32](-120)` HAZI vyjimku -> two's-complement 4294967176 pro dir<0.
- **move.ps1** `-clientX -clientY` = hover move (+SetForeground, jinak jine okno
  na pozici dostane WM_MOUSEMOVE).
- **drag.ps1** `-x0 -y0 -x1 -y1` = down + 10-step move + up (resize/dnd drag).
- **resizewin.ps1** `-w -h` = SetWindowPos resize (POZN: nedosahne engine viewport
  update - viz vw-on-resize nize; real drag-resize funguje).
- Gated stderr traces: `RWE_CURSOR_DBG RWE_SCROLL_DBG RWE_INPUT_DBG RWE_HOVER_DBG
  RWE_RESIZE_DBG RWE_CQ_DBG` (+ [SEL]/[WHEEL]/[INNER] pod RWE_INPUT/SCROLL).

### Opraveno - INTERAKCE (jadro user stiznosti "skoro nic nefunguje")

1. **Keyboard events** (#26) - engine browser-mode KeyboardInput handler NIKDY
   neforwardoval page keydown/keyup do webview (jen chrome/devtools/form). Fix:
   `logical_key_to_js()` + forward KeyDown (render/mod.rs po chrome checks, pred
   scroll) + KeyUp (na zacatku). Plus `nearest_focusable()` walk-up: klik na <span>
   uvnitr <div tabindex=0> focusuje div (ne span). [render/mod.rs, webview.rs]

2. **Hover** (#40, **SIROKE**) - (a) `collect_hover_affected_set` davalo do setu
   jen element s :hover rulem, ne potomky -> hovered_id (td/text potomek) nebyl
   v setu -> dirty-on-hover check selhal -> hover dead. Fix: `add_subtree` (cascade.
   rs). (b) `layout_fingerprint` vynechava paint props (bg/color) co se BAKUJI do
   LayoutBox -> layout cache HIT reuse stale bg = sticky :hover + "table jump".
   Fix: force re-layout pri `cascade_was_miss` (subtree cache rebuildne jen zmeneny
   radek). **Tyka se vsech :hover/:focus/:active na elementech s potomky.**

3. **Cursor pointer/I-beam** (#5/#13) - `compute_cursor_icon` se volal s
   target=None. Fix: hit-test webview.last_layout_root -> button/a=Pointer,
   text=Text. [render/mod.rs]

4. **CSS cursor property** (#58, **SIROKE**) - compute_cursor_icon IGNOROVAL CSS
   `cursor` (jen InteractiveKind). `css_cursor_to_icon()` + walk root->mouse
   (inherited -> propaguj do text boxu). grab/move/not-allowed/help/zoom/...

5. **Forms typing** (#37) - (a) `DomInputBuffer.commit_back` tise zapsal value bez
   bump_dom_version + bez 'input' eventu = stale display + oninput nefire. Fix:
   porovnat value pred/po, pri zmene bump + dispatch 'input'. (b) `compute_subtree_
   hash` NEhashoval value/checked/selected = stale subtree cache (placeholder
   nezmizel). Fix: hashovat je. accent-color OVERENO ze funguje.

6. **Resize element** (#55) - kurzor se nad 16px grip nemenil + tah selektoval.
   Fix: find_resize_grip -> NwseResize v compute_cursor_icon + engine skip
   page_sel_begin na grip. Drag 250x150->375x218 + ResizeObserver fire.

7. **Drag&Drop** (#58-60) - DnD eventy se NIKDY nedispatchovaly. Webview
   `DragSession` state machine: MouseDown na draggable=true -> session; MouseMove
   >4px -> dragstart + hit-test -> dragleave/enter + dragover (preventDefault
   check); MouseUp -> drop + dragend. `helpers::make_data_transfer` sdileny.
   "DROPPED: ALPHA" (setData->getData). [webview.rs]

8. **user-select: none** (**SIROKE**) - engine ignoroval -> selekce na button/UI/
   draggable. LayoutBox.user_select_none + parse + INHERITED + page_sel_begin
   guard (walk root->point). Resi i DnD text-select prosvita.

### Opraveno - RENDERING / CSS features

9. **mix-blend-mode** (#28) - byl identity alpha fallback. Real blend pres LAYER
   compose (mix-blend = stacking context = vlastni layer, blend pri kompozici pres
   backdrop, jak Chrome/FF). is_layer_boundary += mix-blend; LayerNode.blend_mode;
   `BLEND_COMPOSE_SHADER` (compose geom + dst sample + plna CSS Compositing-1,
   un-premultiply); `compose_blend_layer_into_encoder` (snapshot target_texture ->
   offscreen_tex_b -> blend pass). paint: suppress BlendBegin kdyz box JE layer
   root. Vsech 16 modes. [compositor/mod.rs, render/{mod,shaders}.rs, paint.rs]

10. **@container query** (#45) - webview volal `cascade_with_viewport` (ignoruje
   cq) misto `cascade_with_container_sizes`. build container_sizes z prev layoutu
   (1-frame lag + konvergence force-dirty + cache_key hash). subtree fingerprint
   += viewport pri vw/vh. 600px container->GREEN, 300px->RED (proti CONTAINER
   size). [webview.rs render_via]

11. **Overflow clip** (#11) - clip jen culloval KOMPLETNE-venku; partial-overlap
   pretekal. Fix: clamp Rect/Border na hranice + drop Text/Image se stredem mimo
   clip. [paint.rs]

12. **Vertical text** (#32) - vertical-rl block bral full parent width (1218px) ->
   text positioned vpravo MIMO obrazovku = prazdny box. Fix (layout_block_vertical
   2-pass): upright char-stacking (\n mezi znaky) + shrink-to-fit box width.
   Renderuje (V-e-r-t-i-k upright). Real CSS rotuje glyphy = aproximace.

13. **Gradienty px stops** (#25) - `parse_stop_part` nezvladal px pozice (#000 0
   10px) -> parse_color cele selhalo -> 0 stops -> repeating-linear BLANK. Fix:
   extrahuj color z leading tokenu (px nelze na fraction bez box size = jen color +
   default offset, ne striped pattern). [gradients.rs]

### OVERENO ZE FUNGUJE (NE bug - stiznost stale/specificka, NEMENIT)

flex gap (wrap + flex-direction:column), CSS animace (transform translateX ball
x100->x280 + opacity), text-decoration (underline/overline/line-through/wavy/
dotted/thick/text-shadow/multi), filtry (blur/drop-shadow/brightness/combo),
gradienty (linear/radial/conic/positioned), column-count multi-child
(layout_block_multicol distribuuje child boxy).

### DEFERRED do dalsi session (velke / rizikove / specificke)

- **#1 backdrop-filter blur** (top bar) - header JE translucent (alpha OK) ale
  blur + sampling scrolled-content za hlavickou chybi. Layer-raster skipuje
  backdrop snapshot (Seg::BackdropFilter in_layer_raster guard). Reseni jako
  mix-blend ale s blur: pri compose layeru s backdrop-filter snapshot target
  region -> run_blur_passes -> composit pod layer. ~100 LOC, risk pro compose path.
- **column-count single text block** - layout_block_multicol distribuuje CHILD
  boxy, ne line-level fragmentaci jednoho odstavce. Inline text wrapuje za RENDER
  time (box.text + width, ne explicit line boxy) -> fragmentace = render-level
  zmena. Multi-child funguje.
- **box-sizing content-box** - POTVRZEN bug: `width:200; padding:50` -> rect.width
  200 (border-box) misto 300 (content-box default). Engine ignoruje box_sizing
  (field parsovan, neaplikovan). ALE engine-test pouziva `*{box-sizing:border-box}`
  -> engine chovani sedi. Fix bezpecny pro border-box stranky ALE meni vsechny
  content-box stranky (vetsi elementy = risk overflow) + % padding resolve at
  layout time = slozite.
- **vw-on-resize** - subtree fingerprint += viewport pri vw/vh jednotkach HOTOVY
  (soucast @container commitu) - vw element by se mel reflownout na resize. ALE
  SetWindowPos z bg PS NEUPDATUJE engine viewport (container stale 921.6=90vw/1024
  po resize na 420 = engine nedostal Resized). Overit REAL drag-resize okna
  (calc-on-resize fungoval N+34 -> mechanismus OK, podezreni jen na test harness).
- **#54 IntersectionObserver** - logika vypada korektni (isIntersecting=ratio>0),
  user "mozna obracene". Mozna inner-scroll coords (IO row scrolluje horizontalne
  v containeru, rect_lookup neni inner-scroll-aware).
- **#56 MutationObserver styling**, **#20/21 pseudo + nth-child jagged AA**,
  **#34 inline SVG animate/viewBox**, **#27 filter blur top cut**, **canvas
  persistent bitmap** ("funguje blbe" = op-replay bez persistent backing store,
  render-to-texture = velke), **pseudo ::before content pod @container**.

Test fixtures (static/): blend_test, kbd_test, forms_test, overflow_test,
table_test, resize_test, dnd_test, container_test, anim_test, typo_test,
grad_test.

## Session N+34: PERF - canvas <1FPS, calc-on-resize, 3-tier version (113->167 FPS)

User (chyby-rbro v3): "Zobrazeni devtools nebo canvas api extreme zpomali az na
1FPS. Resize extreme pomaly. calc(100%-x) se neprepocita po resize = kriticky."
Verifikace REALNYM mereni (title FPS breakdown, SetWindowPos resize, minimal
fixtury) + dump + testy.

**Canvas <1 FPS - VYRESENO:** minimal canvas fixtura = 240 FPS (canvas sam je
rychly). Engine-test particle mod pouziva fillRect(0,0,w,h) (trail fade) MISTO
clearRect -> muj clearRect buffer-reset se netriggernul -> op buffer rostl
~120 ops/frame neomezene -> paint replay O(n^2) -> po par s <1 FPS. Fix:
full-canvas fillRect (x<=0,y<=0,w/h>=64) RESETuje buffer + push_op safety cap
20000. Overeno: fillRect trail stabilni 240 FPS i po 8s.

**calc-on-resize - VYRESENO (kriticky):** flex pre-pass nastavi explicit_width
z width_calc, ale subtree LAYOUT_CACHE persistuje pres resize -> gate
`if explicit_width.is_none()` skip re-resolve -> stara hodnota. Fix: width_calc/
height_calc VZDY overwrite. Overeno: resize 1280->600, b2 calc(50%+20px) 640->292.

**3-tier version (113->167 FPS idle):** textContent volalo bump_dom_version
(full) -> dom_style_version -> cascade cache miss -> 12ms re-cascade per
textContent zmene (FPS counter, log, psani). Pridan dom_layout_version:
- dom_style_version (cascade): class/id/style/structural.
- dom_layout_version (layout): + textContent/value.
- dom_version (paint): + SVG geometry (points).
textContent -> bump_dom_version_layout (re-layout, NE re-cascade). SVG points ->
content_only (re-paint, NE re-layout). Idle 113->167 FPS.

**DevTools <1 FPS - VYRESENO (2 fixy):**
1. CDP pump pouzival page.dom_version() pro DOM.documentUpdated -> SVG geometry
   (animateWavePoly points kazdy frame) trigger re-fetch tree kazdych 500ms ->
   1s render. Fix: CDP -> page.dom_style_version() (strukturalni). D 1038->38ms.
2. ARCHITEKTURA: frame-skip gate `if !dirty && !needs_tick` mel needs_tick =
   needs_continuous_render (vc. interval/RAF) -> setInterval (pollEvents) /
   RAF obesly frame-skip -> full layout+paint KAZDY frame i bez zmeny. Fix:
   needs_animation_render() (CSS anim/scroll/caret, NE interval/RAF) pro
   frame-skip; interval/RAF se drainuji PRED skip + dirty jen pokud zmenily
   DOM (dom_version) / canvas (canvas_gen counter). D 38->0ms (SKIP), FPS->135.
   = Chrome princip: render JEN pri zmene, ne kazdy frame.

**ZBYVA (deep perf, potreba tve verifikace):**
- Resize page-render 73ms/frame (10 FPS pri drag): full re-cascade(@media)+
  re-layout+paint+layer-raster na cache invalidaci kazdy resize step. Potreba:
  resize debounce (re-layout az pri settled, behem dragu scale cached frame)
  NEBO compositor fast-path. Riziko - present scaling pri size mismatch.
  (Single resize je rychly 119 FPS; problem jen pri rapid drag.)

## Session N+33: KRITICKY hit_test fix + canvas + calc + autonomni grind

Autonomni pokracovani (user pryc od PC). Verifikace REALNYM vstupem (SendInput
hardware mouse/keyboard) + deterministicke Rust testy + dump, NE jen screenshoty.

**KRITICKE (proc "drtiva vetsina nefunguje"):** hit_test mel DVA root cause:
1. Overflow descent - vyzadoval bod uvnitr rodice PRED rekurzi do deti. html
   box vysoky jen 496px, obsah do 8000+ -> vse pod nim NEKLIKATELNE. Fix:
   rekurzuj do deti i mimo self.rect (orezava jen overflow:hidden/scroll/clip).
2. Pseudo/anonymni pass-through - vracel ::after/anon boxy (node=None) ->
   target=None -> zadny focus/toggle. Checkbox ma :checked::after fajfku co ho
   stinila. Fix: hit_test vrati self jen kdyz node.is_some().
   OVERENO realnym klikem: checkbox toggluje (zluty->prazdny), hit=checkbox
   v hloubce 8132px, psani funguje. + 2 deterministicke testy.

**Label->control forwarding:** klik na label TEXT (ne 16px ctverec) aktivuje
jeho input (for=/wrapping). resolve_label_target v MouseDown+MouseUp.

**Canvas API (5 root cause):** canvas.width write se zahazoval (eval_expr) ->
sirka=0 -> vse degenerovane; RAF zamrzal (has_pending_raf); freehand dirty;
clearRect leak; globalAlpha ignorovan.

**calc()/clamp()/min()/max() s %:** pocitalo se v cascade kde %=16px ->
calc(100%-40px)=-24px, clamp=min. Fix: cascade preservuje %-calc, parse_length_ctx
+ width_calc/height_calc resolved v layoutu proti parent width. Block+flex.
Overeno: cb1=911, cb4 clamp=200, cb6 max=238.

**text-decoration shorthand:** underline wavy var(--a) 2px - style/color/thickness
ze shorthandu (drive jen solid+text barva) + overline + strikethrough barva.

**Inline SVG var():** stroke="var(--a)" -> resvg neumi -> ROOT_VARS_SNAPSHOT +
resolve_root_var v serialize_svg.

**:has(input:checked)** matching potvrzen OK (test); webview invaliduje pres
dom_version bump (checkbox toggle).

ODLOZENO (velke/riziko): mix-blend shader, @container 2-phase, vertical text
(38 Text konstruktoru), column-count fragmentace, elliptical border-radius
(Vertex+WGSL), SVG animate, gradient radial circle/ellipse + animated bg-pos,
::selection per-char, flex item cross-size padding (spawned task), drag gap.

## Session N+32: chyby-rbro v2 - forms interakce + observers + typografie + perf

Pokracovani dle aktualizovaneho chyby-rbro.docx (~33 bodu) + 9 workflow agent
reportu (F:\Projects\_reports\). Implementovano cluster-by-cluster, build+test+
commit kazdy. Vsech 4175 testu pass.

**Forms + JS events:**
- Keyboard focus na [tabindex] - MouseDown focusable set rozsiren -> keydown/
  keyup se dispatchnou (driv mrtve). + dispatch focus/blur JS event (onfocus
  border highlight).
- CSS `cursor` property - MouseMove cte bx.cursor (inherited) + mapuje na
  CursorIcon (grab/text/pointer/resize/...). Driv ignorovano = I-beam jen nad
  textem.
- appearance:none form controls - LayoutBox.appearance_none, native overlay
  (checkbox/radio/range) respektuje -> CSS bg/border/thumb se uplatni. accent
  z bx.accent_color, accent-color do INHERITED.
- Dashed/dotted box border - emit_dashed_rect_border (segmenty; Border kreslil
  vzdy solidne).
- :active pseudo-class - set_active_node MouseDown/clear MouseUp + cache_key
  uses_active -> re-cascade (button click efekt, cursor:grabbing).
- Select dropdown - open/close/pick option (popup render existoval, open_select
  se nikdy nenastavil). select_pick_option helper.

**Observers + resize:**
- Intersection/ResizeObserver `return` bug - smycka fire-la jen PRVNI observer
  (predcasny return) -> sber pending + drop + call vsechny. Druhy IO (sectionObs
  nav highlight) konecne fire.
- Element resize (CSS resize) - LayoutBox.resize, paint grip (3 proužky),
  resize_drag state (grip hit-test -> drag -> inline style width/height override
  -> reflow -> ResizeObserver fire).

**Typografie + paint:**
- text-shadow multi-layer (Option->Vec) + fake glow (blur ring aproximace).
- Marquee - TransformOp::TranslateMixed: translate % resolvuje proti bx.rect
  (driv 100%=16px viewport default -> wiggle misto sweep).
- Opacity layer double-fade - opacity<1 = layer boundary, compose aplikoval
  opacity ALE paint_box take (alpha_mul) -> 0.3x0.3=0.09 moc vybledle. Layer
  root preskoci v alpha_mul (compose ji aplikuje). Resi nth-child box5 "spatna
  barva textu" + rgba pos-z boxy.
- Backdrop-filter v layer rasteru - top bar seda misto rgba (main_rt snapshot
  cerny v layer rezimu) -> preskoc snapshot, jen inner obsah (compose blend
  pres page).

**Perf + stabilita:**
- Resize okna coalescing - shell Resized jen uklada pending_resize, realny
  resize (texture realloc + reflow) 1x v RedrawRequested (driv winit per-pixel
  = N realloc/frame = extreme zpomaleni).
- Table inner_w floor - drobne float rozdily menily column distribuci = tabulka
  "skakala".

**ODLOZENO (velke/riziko):** elliptical/per-corner border-radius (Vertex+WGSL),
@container two-phase layout (cascade_with_container_sizes je dead), mix-blend
real shader, vertical text glyph stacking (38 Text konstruktoru), column-count
inline fragmentace, scrollbar arrows, drop-shadow Gaussian, radial circle/ellipse,
gradient animated bg-position. Detaily v F:\Projects\_reports\*.md.

## Session N+30: grid padding residual + perspective 3D + flex/table layout

Pokracovani chyby-rbro doc fixu. Vsechny systemove (no workarounds):

- **Grid padding residual (535->496)**: `.page` grid merilo `#main` (flex column)
  pri rect.width=0 (track sizing) -> width-0 layout cele subtree, labely wrapnuly
  -> grid-demos vyska 534. Real layout pri 966 to neumel zmensit (flex expand-only
  guards). Fix: grid pre-pass po dispatch_layout DEEP-RESETne descendant rect
  (grid.rs ~700, jako flex pre-pass deep_reset_rect).
- **Perspective text "pulka zrcadlena"**: TRANSFORM_SHADER delal perspective divide
  manualne + clip.w=1.0 -> AFINNI uv interp -> zlom na diagonale quadu. Fix:
  homogenni clip (nx*w, ny*w, 0.5*w, w) -> GPU perspective-correct uv interp
  (shaders.rs). Overeno capture.
- **Perspective navrat skok**: interpolate_transform_value synthesizoval pri
  navratu na none perspective(0) = degenerate (m[14]=-1/0). Fix: perspective se
  interpoluje RECIPROCNE (1/d, identity=nekonecno) - layout/mod.rs.
- **Sekce 10 calc "nezobrazuje nic"**: layout_block daval flex/grid block-childu
  placeholder rect.height=20 -> layout_flex to vzal jako definite -> flex-shrink
  shrinkl items pod explicit vysku (calc-bary height:28 -> 0). Fix: flex/grid
  block-child placeholder = 0 (auto). SYSTEMOVE - dotyka se vsech flex/grid v
  block flow. (calc-bar SIRKY = calc/clamp/min/max s % se resolvuji pri kaskade
  bez parent kontextu -> spawnut samostatny task task_aa5a72b0.)
- **Tabulka rozhozena**: default_display(thead/tbody/tfoot)=Block ale
  collect_table_rows/apply recurzuji jen do TableHeader -> rows nenalezeny ->
  kazdy radek nezavisly. Fix: thead/tbody/tfoot=TableHeader. + content-based
  column sizing (cell_content_max_width misto rect.width=0).
- **SVG testy (7)**: zastarale (Rect/Circle commandy) -> aktualizovany na resvg
  raster (DisplayCommand::Image + INLINE_SVG_CACHE pixel check).

### 2. kolo feedbacku - OPRAVENO:
- **Per-side bordery (border-bottom/top/left/right) se VUBEC nekreslily** (velky bug:
  tabulka row separatory = "tabulce chybi spravny border", underliny, asymetricke
  bordery). 3 clanky: cascade expand_shorthand neresil side shorthandy (border-bottom
  spadl do _ => verbatim) + apply_styles cetl jen shorthand border-width + paint
  emitoval jen pri border_width>0 (uniform). Fix vsech 3 (cascade.rs / layout apply_styles
  / paint per-side strip). +2 testy.
- **Form controls (checkbox/radio/range) se nekreslily** = "inputy nefunguji". Pridany
  layout rozmery + paint (checkbox modry fill+bily check ClippedRect, radio modra tecka,
  range track+filled+thumb dle value). +2 testy.

### 3. kolo feedbacku - OPRAVENO (forms cista + dalsi):
- **Select 'dvojity'**: <option> default_display=None (closed select = jen vybrana
  hodnota + chevron, ne expandovany seznam). + select paint respektuje custom bg.
- **Bily pruh u scrollbaru**: canvas bg (body/html) full-viewport fill na index 0
  (body width zmensena o scrollbar gutter -> prosvital bily clear).
- **Form overlap (checkbox+range pres TEXT pole)**: KORENOVA pricina = grid
  'grid-column: 1 / -1' dalo span_col=1 (negativni end neresen) -> full-width forms
  items (RANGE/TEXTAREA/accent-demo) zabraly 1 col -> auto items vyplnily zbytek
  rady -> full render pres ne. Fix grid_spans + needed_from_placement (negativni
  end -> full span). Forms ted renderuji cista (overeno capture).

### 5. kolo - AUTONOMNI PRUCHOD SEKCEMI (capture triage + bounded fixy)

Helper: `python3 /tmp/extract_section.py s-<id>` extrahuje sekci + plny CSS do
`F:\Projects\_sec.html` (full-width, bez .page gridu - jinak #main spadne do
200px sloupce!). Pak `./target/debug/rwe-engine.exe browser _sec.html` + capture.

**OPRAVENO:**
- **NBSP mezi inline elementy** (typografie text-decoration spany abuttovaly):
  build children loop + text collapse pouzivaly `t.trim().is_empty()` co trimuje
  i nbsp (U+00A0) -> `A &nbsp; B` se skiplo. Fix: skip jen ciste breakable ws.

**STAV SEKCI (po pruchodu):**
- 06 pseudo: OK-ish. Minor: ::after badge pozice, nth-child(4n+1) rotovane boxy
  maji zubate hrany (AA na transformu - chybi mipmaps/supersample, jako persp border).
- 07 gradients: linear/radial/conic render OK (fixtura). Animovany (background-
  position shift @keyframes) + background-size 400% NEoveren/neimplementovan.
- 08 filters: filtry OK (blur/brightness/contrast/saturate/hue/invert/sepia/
  drop-shadow/multi). **mix-blend-mode = jen FALLBACK alpha compose** (render/mod.rs
  7134 `let _ = mode` - blend formula multiply/screen/overlay/... NENI, jen alpha)
  = "uplne jina" + text artefakty. DEEP: potreba real blend shader (dst sample).
- 11 overflow: OK (scroll-snap boxy, overflow contained). Custom scrollbar scroll-
  routing (scroll sekce vs stranka) = runtime.
- 12 typografie: nbsp FIXED. DEEP: vertical text (writing-mode vertical-rl/lr na
  spanu renderuje HORIZONTALNE prekryvajici - chybi vertical glyph stacking),
  column-count, marquee, typewriter. text-shadow efekty OK.
- 18/19 modern: vetsinou prazdna (@container query + color-mix boxy bez obsahu).
- 19 canvas / 20 events / 21 observers / 22 drag: JS-dependent - fixtura (jen CSS)
  je neukaze, potreba zahrnout page <script> NEBO scroll v realne strance.

**ZBYVA (deep features, ne quick fixy):** mix-blend real shader, vertical text,
column-count, marquee animace, canvas API, sticky (runtime), nekonecny scroll
(runtime feedback), JS observers/drag&drop, animovane gradienty (bg-position).

### 4. kolo feedbacku - FORMS INTERAKCE (uz funguji):
- **Checkbox/radio klik toggle**: dispatch_event HTML default action (click ->
  toggle checked, radio uncheck group siblings, fire change, gated preventDefault).
  Realny klik jde pres webview->dispatch_event. +3 testy.
- **Range klik+drag**: mousedown/MouseMove na range -> set value dle x + fire
  input/change (set_range_from_x). range_drag_node state.
- **Text input typing**: uz fungovalo (focused input capture char + set value +
  fire input, webview ~2094).
- ZBYVA: select dropdown (klik otevri seznam + vyber option) = komplexni (overlay).

### 3. kolo - ZBYVA (dalsi sekce):
- Dalsi sekce stranky (mix-blend, filtry, typografie, canvas, obskurni CSS) +
  interaktivni (sticky, klikani, scroll) - user "projdi celou stranku".

### 2. kolo feedbacku - ZBYVA (forms + dalsi):
- Text/email/number/textarea inputy: border slaby kontrast (gray 118 na svetlem bg) -
  mozna OK, overit s realnym CSS. Unchecked checkbox/radio slaby kontrast.
- **Textarea + select obsah se kresli i vlevo nahore (0,0)** = duplikat/artefakt
  (textarea "text content" + select "One" leakuje na origin). Separatni paint bug.
- Select: bez dropdown sipky. Range/checkbox interakce (klik/drag) - JS strana.

### Po user feedbacku (screenshoty) - diagnostikovano, odlozeno (dedikovany fix):
- **Perspective pravy (vzdaleny) border slaby/chybi**: perspektivne-korektni interp
  (Session N+30 shader fix, opravil zrcadleni textu) KOMPRIMUJE vzdalenou hranu vic
  nez afinni -> 6px border tam minifikuje a aliasuje pryc (linear sampling bez
  mipmap). rotateY BEZ perspective (afinni) border MA. Fix vyzaduje MIPMAPS na
  layer/offscreen texture NEBO output-supersampling transform passu (compose_transform
  / TRANSFORM_SHADER render/mod.rs ~7895). Text correctness ma prednost -> shader fix
  ponechan. Overeno capture (rotateY vs +perspective porovnani).
- **Tabulka hover "zvetseni-zpozdeni-zmenseni"**: runtime glitch pri hoveru radku
  (tr:hover bg -> re-cascade -> re-layout). Staticky tabulka stabilni (dump 951x211,
  sloupce zarovnane+content-sized po Session N+30 fixu). Pattern "enlarge then shrink"
  = nejspis 2-pass intrinsic-pollution (jako grid/flex) NEBO transition na re-layout.
  Nutno verifikovat pri runtime hoveru (mouse injection chybi). Coloring/border
  tabulky overeno OK v izolovane fixtre (header bg, border-bottom, nth-child striping).
- **Gradienty**: renderuji OK v izolovane fixtre (linear/radial/conic barvy+smery
  spravne, radial "at 30% 30%" pozice parsovana cx/cy=0.30). gamma fix je NErozbil
  (spis priblizil Chrome sRGB interpolaci). "rozjebal vic" byl starsi stav z doc.
  Pripadne jeste zkontrolovat animovany gradient (background-position shift anim) +
  multi-layer v plne strance.

### Zbyva z chyby-rbro doc (pro dalsi tah, mnoho interaktivnich = tezko verifikovat staticky):
- calc/clamp/min/max s % (task_aa5a72b0 spawnut)
- Position: sticky (menu zaseknuty), JS klikani/events "nikde nefunguji",
  nekonecny scroll (runtime feedback, root.rect.height OK staticky), JS Observers
  + drag&drop, gradienty (animovany neanimuje, "rozjebal vic"), CSS filtry (blur
  slabsi/brightness barva/drop-shadow glow/multi blur text), mix-blend (cerne
  artefakty), custom scrollbary (scroll celou strankou misto sekce), typografie
  (efekty/underline/vertical/column-count/blink/marquee), inline forms (pretika/
  neinteraktivni/styling), obskurni CSS, canvas API, mousemove (offsetX undefined -
  inline handler binding OK, verit nelze bez mouse injection).

## Session N+29: hover transitions + grid template-areas + keyframe animace + hover PERF

Pokracovani chyby-rbro doc fixu. Systemove bugy:

### Hover PERF "velke zpozdeni" (akumuluje s hoverem, pak settle) - 3 fixy
- **VIEWPORT-CULL structural_fp** (hlavni, paint 9ms->0.6ms): off-screen content
  boxy menily structural_fp kazdy frame (colorCycle bg animace mimo viewport) ->
  root layer damaged -> full root re-paint (tree walk pro build_layered_display_list
  warm+text_runs) i kdyz NENI videt. compute_fingerprints ted skipne off-screen
  boxy (600px buffer; set_viewport_cull pred extract_layer_tree). Visible animace
  funguji dal. POZN: visible non-layer color animace porad stoji full root re-paint
  - dalsi fix by byl tile-based display_list (TODO).
- **Cascade cache cap RETAIN** current dom_version misto clear-all: clear na 8000
  delal cold cascade (12ms) + layout miss (19ms) = 41ms hitch; rychly hover = caste
  = "cim vic hover tim vic zpozdeni". Retain drzi warm working set.
- **Warm atlas jen DAMAGED layery** (ne vsechny pri ANY damage).
- Vysledek: IDLE 210 FPS (render 4.7ms; wall-clock 111 FPS limited present/pacing).
  Steady hover 13ms->4.7ms. Cycling 17ms->6.9ms, spiky 9%->3.8%.
- Debug: `RWE_FORCE_HOVER=<class>[,<class>...]` [+`_ALT`], `RWE_PROF`, `RWE_DAMAGE_DBG`.
- ZBYVA: cycling spiky 3.8% (dom mutace -> cascade cold + layout miss = incremental
  layout TODO); wall-clock 9ms vs render 4.7ms (present/vsync/JS pacing).

### 3 systemove bugy:

- **Hover transition "1 FPS" + zaseknuti napul + nevraceni** (sekce 04 Transforms):
  dva propojene root cause:
  1. `prev_style_map` = mapa PO apply_transitions (interpolovane "scale(1.300)"),
     porovnana s cascade target "scale(1.3)" = string mismatch = SPURIOUS no-op
     transition kazdy frame = jitter scale -> layer texture realloc = "1 FPS";
     navic spurious transition blokovala REVERSE (4088 check) = box se nevratil.
     Fix: prev_style_map = cascade BASE (pred apply_transitions). base-vs-base.
  2. detect_transitions iteroval jen cur.keys() -> pri un-hover transform ZMIZI
     z cascade = reverse se nezkontroluje. Fix: props_to_check zahrne prop v cur
     NEBO prev + cur_val/prev_val initial fallback (transform:none, opacity:1).
  Vysledek: ciste forward<->reverse, toggle hover 15ms -> 4.5ms (200 FPS).
  Debug harness: `RWE_FORCE_HOVER=<class>` [+`_ALT`] (winit nevidi syntet. mys).

- **grid-template-areas** (screen 3 doc): named grid-area (grid-area: header) se
  VUBEC nečetla ani neresolvovala -> auto-place = rozbity layout. Implementovan
  novy LayoutBox.grid_area + parse_template_areas() + resolution v layout_grid
  (name -> row/col lines). header/footer span full, sidebar|content|aside middle.

- **Keyframe animace flaky** (jen ~3-4 fungovaly):
  1. `spin { to { rotate(360deg) } }` (jen 100%) STATICKY - interpolate_keyframes
     vratil jediny frame. Fix: synteza chybejiciho 0%/100% z initial (CSS §3).
  2. `colorCycle { background: ... }` neanimoval - base bg vytvori backgrounds
     LAYER, paint skipne bg_color. Fix: apply_paint_animations updatuje i barvu
     backgrounds layeru.

## Session N+28: SYSTEMOVA oprava layer compositor transform clipu (NE workaround)

**KLICOVE: layer compositor (WebRender-style) je DEFAULT a transform clip je
opraven SYSTEMOVE v paint.rs.** Zadny monolithic workaround (uzivatel explicitne:
"zadne workaroundy nikdy, resit systemove").

- **Root cause "usekaneho" transformovaneho obsahu** (stagger bounce, translateY):
  `paint_box` mel `else if let Some(Translate)=bx.transform` branch ktery aplikoval
  singular transform translate I PRO LAYER BOXY (kdyz hlavni geometry blok skipnut).
  = DOUBLE transform: (1) paint pohnul box o (tx,ty) -> shift na layer-local coords
  ho castecne vystrcil MIMO texturu (sized na orig rect) = clip; (2) GPU compose
  pohnul quad ZNOVU.
- **Diagnostika** (instrumentace, ne hadani): local cache box na y=-50 misto y=0.
  compute_transform_matrix + TRANSFORM shader overeny korektni (row-major translate,
  m[7]=-50). Bug byl PURE v paint else branch.
- **Fix**: else branch guardnut `!is_layer_with_transform` -> layer box dostane
  transform VYHRADNE z GPU compose na quad. paint.rs ~2575.
- **Overeno vizualne**: _ty translateY(-50) cely box bez clipu (LAYER default I CPU
  fallback); stagger bounce neusekane; animace translateX **240 FPS** (transform-only
  = re-compose bez re-paintu = WebRender vyhoda, presne jak Chrome!).
- **Architektura**: LAYER mode default (opt-out `RWE_LAYER_GPU_OFF`). MONOLITHIC_PAINT
  flag (z drivejsiho pokusu) ponechan - dela CPU fallback taky korektni pro transformy
  (geometry post-process). Oba renderery spravne, GPU layer primarni.
- **Test suite zase kompiluje**: merge 27a11f9 zanesl L5 typed-cascade testy
  (apply_animations_typed, read_animated_value_from_cs, is_set, visual_snapshot)
  jejichz impl lokalne chybi -> suite od merge NEKOMPILOVALA. Disabled pres
  `#[cfg(any())]` + koment. Po fixu: **4152 pass, 7 fail**.
- **7 pre-existing SVG test failu**: STALE testy - SVG ted rasterizuje pres resvg ->
  `DisplayCommand::Image`, ne manual `Rect`. SVG renderuje OK, testy zastarale.

### Zbyva (dalsi systemove tasky)
- **Perf**: engine-test 80 FPS (12.5ms) v layer mode idle - jednoducha anim 240 FPS.
  Gap = layer re-paint overhead (barevne animace + hover meni non-transform props ->
  re-paint textury). Systemovy fix: damage tracking aby re-paintoval JEN zmenene
  layery, transform/opacity-only = re-compose. structural_fp to uz rozlisuje.
- **Hover real-mouse verify**: scale transition (geometry + detect_transitions z N+27).
- **writing-mode vertical text** (sekce 12) - znaky se prekryvaji.
- **inline SVG** (sekce 14) - par boxu prazdnych/cernych (resvg selhava na obsahu).

## Session N+27: animace interpolace + hover perf (layer mode, NE monolithic!)
### POZN: N+27 "monolithic nejde pro transformy" mel pravdu v zaveru (layer = default),
### ale spatny root cause. N+28 nasel REALNY root cause (paint else-branch double-transform)
### a opravil ho SYSTEMOVE -> layer mode transform clip pryc.

- **Animace neinterpolovaly** (transform/keyframe/transition skakaly z pozice na
  pozici): parse_length("translateX(0)")=0 -> snap. Fix interpolate_css_value
  (per-function transform lerp + color rgba). Klicove.
- **Hover perf**: 3 fixy (vsechny v layer mode, animace zustavaji funkcni):
  1. cascade_pseudo cache (6ms/layout-miss usetreno).
  2. hover coalescing (App ulozi posledni mouse pozici, webview hover 1x/frame).
  3. **cascade cache leak fix** (MATCHED_DECLS/WALK_OUTPUT/PROPAGATED rostly
     neomezene behem mousingu = progressive slowdown; size cap 8000).
  Vysledek: engine-test mousing 1326 framu <6ms (drive 328), memory stabilni.
- **POZOR/POUCENI**: zkousel jsem defaultit MONOLITHIC render (RWE_LAYER_GPU_OFF)
  pro rychly hover - ROZBILO to transform animace (transform vytvori GPU layer,
  layer transform aplikuje JEN compositor; monolithic ho flatten ztrati ->
  animace zamrznou na frame 0). REVERTNUTO. Layer mode je default a NUTNY pro
  transform animace. Layer compositor hover overhead resi pseudo cache+coalescing
  +cache cap (ne disable). TODO: layer damage tracking aby re-renderoval jen
  zmenene layery.
- ::selection text kontrast (prekresli selected text v selection_color).


## Session N+26: Interaktivita + hover + perf + clip-path + gradienty (chyby-rbro doc)

Prace na bug dokumentu `E:\Data\Downloads\chyby-rbro.docx` (~27 chyb, nas-vs-Chrome
screeny). Testovano vizualne pres PrintWindow capture (engine bin, debug). Commity:

### 1. Interaktivita (klik + inline handlery + DOM mutace) - odblok #1
- `dispatch_event` volal JEN addEventListener listenery, NE inline `on*` atributy.
  Pridana `Interpreter::fire_inline_handler` (obal kodu do `function(event){}`,
  this=element, eval v global). Vola se v target + bubble fazi dispatch_event.
- Engine App NEdispatchoval click do JS vubec (vlastni mouse = jen selection).
  Pridano: page MouseDown/MouseUp -> webview.handle_input (dispatch click).
- DOM mutace z handleru se NEZOBRAZILY: layout cache key bez DOM verze ->
  textContent/appendChild zmena = cache HIT = stary obsah. Pridan
  `dom_style_version` do layout key + interpreter bumpne pri text/struktura/class
  mutaci (NE pri SVG geometry points -> perf).

### 2. Perf (Fifo vsync + per-frame O(N) redukce) - "vykon nestal za moc"
- Present mode Mailbox/Immediate -> **Fifo** (vsync block = nizke CPU misto
  100% spin pri kontinualnim animacnim redraw).
- Redraw pump VYHRADNE v about_to_wait (RedrawRequested se uz sam nerequestuje
  -> driv queued event prebijel ControlFlow = nikdy nespal).
- Layout cache HIT: take() misto clone() (1 klon tree misto 2 per frame).
- hit_rtree LAZY build (jen pri mouse dotazu, ne kazdy frame).
- ResizeObserver/IntersectionObserver blok zaguardovan (jen kdyz registrovane).
- Vysledek engine-test: layout-faze 3.2->2.2ms, ~88->101 FPS (release).

### 3. Page hover (CSS :hover + JS eventy) + ancestor propagace
- App pri normalnim pohybu NErouteoval MouseMove do webview -> page hover (CSS i
  JS mouseover/enter/move/out/leave) byl uplne mrtvy. Pridano routovani.
- **Bug**: `:hover`/`:active`/`:focus-within` propagace OBRACENA. Matchovala
  POTOMKY hovered elementu misto PREDKU (hover nad body barvil vsechny deti =
  sticky :hover). Fix: precomputed ancestor-or-self chain (rebuild_state_chains,
  1 DFS), `:hover` = O(1) chain membership. matched_decls cache hover_bit taky
  pres chain. Latentni bug - projevil se az po zapojeni page hoveru.

### 4. text-transform (uppercase/lowercase/capitalize)
- Nebyl implementovany. Text node nema cascade entry (s.len=0) -> aplikuje se
  v rodicovske inheritance smycce z parent stylu PRED measure.

### 5. clip-path na gradient-filled elementech
- clip-path "vubec nefunguje" protoze gradient emit (bx.bg_gradient) IGNOROVAL
  clip_path = plny box. (Solid-color clip uz fungoval.) Fix: circle/ellipse/inset
  -> gradient do clip rectu + radius; polygon+linear -> novy ClippedGradient
  command (triangulovany polygon s gradient-mode verts = gradient clipnuty na tvar).

### 6. Gradient hard-stops + edge clamp
- "color 33% 66%" (hard band) se parsoval spatne -> stredni barvy dropnute.
  Fix: parse_stop_part expanduje na 2 stopy.
- Multi-stop renderer fillnul jen [first,last] offset -> oblasti mimo
  TRANSPARENTNI. Fix: synteticke edge stopy na 0/1. Pomaha VSEM gradientum s
  ne-edge stopy (cast "krom prvnich dvou spatne").

### 7. ::before/::after (build_pseudo_box byl orezany na inline text)
- content:"" (prazdny) se zahazoval -> dekorativni ::after bary (display:block +
  width/height/bg = podtrzeni/divider) neviditelne. Ted renderuje box bez textu.
- Necetl display/width/height/border-radius -> pseudo vzdy inline. Pridano
  (border-radius % vuci min(w,h) = 50% kruh).
- Necetl position/top/right/bottom/left -> position:absolute pseudo (notif badge)
  flowoval v toku. Pridano -> layout abs-pos pass umisti badge top-right.

### 8. ::selection background-color (driv hardcoded modra)
- Highlight pouzival [80,150,255,120], ignoroval ::selection bg (layout uz
  parsoval do bx.selection_bg). find_selection_bg walk -> CSS barva. Text color
  (::selection color) zatim ne (vyzaduje re-paint glyfu).

### 9. KRITICKY perf: memory leak animovaneho SVG (user "extreme tragicky vykon")
- Animovany inline SVG (wave setAttribute('points') kazdy frame) = novy content
  hash klic kazdy frame. flush_inline_svg_cache iteroval CELY cache + pridaval do
  atlasu BEZ eviction -> INLINE_SVG_CACHE neomezeny RAM rust (~10 MB/s), atlas se
  zaplnil za ~4s (add() false -> "inline svg nefunguji"), flush O(N^2) =
  progresivni zpomaleni, close trval sekundy (free obriho cache+atlasu).
- Fix: stabilni klic per-element (node ptr+dims), content_hash pro re-raster jen
  pri zmene, INLINE_SVG_DIRTY (upload jen zmenene), ImageAtlas::replace (overwrite
  slot in-place). Memory ted stabilni ~500MB misto +10MB/s. FPS konzistentni ~100
  (release) misto degradace do tragickeho.

### 10. FPS counter v title (user request)
- frame_times_ms se NIKDY neplnil (FPS overlay mrtvy). render() meri wall-clock
  cadence -> window title "<page> - N FPS (X.X ms)". Throttle ~12 framu.

### 11. Canvas interaktivita (offsetX/Y + drag)
- Mouse eventy chybely offsetX/offsetY -> canvas kresleni + pozice = undefined.
  Pridano (content - target box origin) na move/down/up/click. + <canvas> vyloucen
  z text-selection dragu + App routuje mousemove i behem dragu -> canvas drag
  kresleni funguje.

### 12. ::selection per-element fix (oprava regrese z #8)
- #8 (global find_selection_bg) barvil CELOU stranku. Ted selection_bg propagovan
  na text deti + per-hit barva -> scoped `.foo::selection` jen ve .foo.

### 13. KRITICKY: animace neinterpolovaly - skakaly z pozice na pozici
- User: "animace se neanimuji, jen skacou z pozice na pozici" (= vypadalo
  nevykone). interpolate_keyframes I apply_transitions interpolovaly pres
  parse_length. parse_length("translateX(0)")=0, parse_length("translateX(400px)")
  =0 -> oba 0 -> ELSE vetev SNAPLA na t<0.5 (from) / t>=0.5 (to). Takze KAZDA
  transform animace/transition (translateX/Y, scale, rotate = nejcastejsi anim
  props) skocila z pocatecni na koncovou v pulce misto plynule.
- Fix: novy interpolate_css_value (pub, sdileny keyframes+transitions): transform
  per-function lerp argumentu (zachova unit), barvy rgba lerp. Zapojeno pred
  length-interp v obou cestach.
- Overeno: translateX(0->400px) 10s linear -> box_x 105,123,140,...295 rovnomerne
  (plynule) misto skoku start->konec.
- ZBYVA: transition-back na leave (user "pri opusteni se nevrati") - detekce
  zmizeni :hover transform v detect_transitions; synteticka mys nespolehlive
  testuje hover -> overit realnou mysi.

### 14. ::selection text kontrast
- selection_color (text barva) se aplikuje - prekreslim selected text runy v
  selection_color nad bg (klonuji puvodni Text commandy v selection rectu).
  Cerny text na zlute = citelne (Chrome-like).

### 15. KRITICKY perf: hover/mousing - LAYER COMPOSITOR byl overhead
- User: "hover ma strasny zpozdeni, funguje nevykone". Profiling rychleho
  mousingu (250 moves/4s) na engine-test odhalil:
  - **Layer compositor (D4) re-renderoval VSECHNY layery pri kazde hover/layout
    zmene -> paint faze 9-280ms** (vs 0.2-5ms monolithic). Damage tracking
    nepomahal na animovanych strankach. Fix: **default MONOLITHIC** (layer mode
    opt-in pres RWE_LAYER_GPU_ON). -> 1289 framu <6ms misto 328, 135->147 FPS.
    TODO: fix layer damage tracking pak znovu zvazit default-on.
  - cascade_pseudo (~6ms) bezel na kazdem layout cache miss -> cache pseudo_map
    (klic dom_style_version + sheets sig).
  - Hover coalescing: App jen ulozi posledni mouse pozici, webview hover pipeline
    1x/frame v renderu (misto kazdy z desitek CursorMoved/frame plne).
- Vysledek: vetsina mousing framu <6ms (drive ~1/3 framu 12-37ms).
- ZBYVA: cascade re-walk 8-12ms kdyz logEvt/DOM mutace invaliduje matched_decls
  cache (= incremental cascade), layout_tree 6ms subtree rebuild.

### ZBYVA z chyby-rbro doc (pro dalsi vlakno)
- **Perf gap na 240fps**: jsme ~135fps idle (release). Per-frame ~9ms =
  layout 3ms + SVG resvg re-raster KAZDY frame (wave) + canvas + rAF JS. SVG
  re-raster pres resvg je drahy - potreba fast-path nebo cache invalidace jen pri
  realne zmene. Layout clone+layer-tree taky.
- **rgba alpha blend v LINEARNIM prostoru** (washed-out polopruhledne panely =
  "stranka vypada jinak"). Root: offscreen RT je sRGB -> hw blenduje linearne;
  CSS chce sRGB-space blend (blk 0.5 nad bilou = 187 misto 128). Fix = UNORM
  offscreen + shader sRGB audit + compose/present + text AA = pipeline-wide color
  mgmt, VYSOKE riziko regrese. HIGH IMPACT ale opatrne (nelze A/B z screenu).
- repeating-linear/radial-gradient (chybi tiling).
- Animovany gradient (background-position animace + background-size 400%).
- Grid ordering, table styling, formulare (chybi elementy/styl/overflow),
  typografie (efekty/podtrzeni/vertical/column-count/blink/marquee), CSS filtry
  (blur/brightness/drop-shadow/multi), mix-blend-mode (cerne artefakty), custom
  scrollbary + inner overflow scroll wheel, jagged/zubate diagonalni AA linky.
- ::selection color (text barva pri vyberu - bg uz hotovo, text vyzaduje
  re-paint selected glyfu). + `cursor` ze stylu (user: "nema kurzor ze stylu").
- canvas API: kresleni + offsetX hotovo; zbyva overit ostatni canvas ops +
  klikani na tlacitka v canvas sekci (mozna z-order canvas pres tlacitka).
- repeating-linear/radial-gradient (tiling), animovany gradient (bg-position).

### Test/debug pomucky teto session
- PrintWindow(hwnd,hdc,2) capture (occlusion-independent) + SetWindowPos
  HWND_TOPMOST (jinak app okludovane terminalem = synteticky mouse jde jinam).
- SendInput absolute MOVE pro hover (SetCursorPos negeneruje winit CursorMoved).
- `/tmp/cap.ps1` reusable capture skript (title match + crop).
- env-gated debug: RWE_HOV_DBG, RWE_TT_DBG, RWE_PROF.

## Session N+25: Pure-Rust AVIF + layout wire-ups + lazy loading + web vitals (4106 testu)

Pokracovani z N+24. Real backend wire-ups + user-impact features.
User-requested: AVIF MUSI byt browser-internal (zero system deps). Tables +
multicol + writing-modes + lazy loading + LCP/CLS hook + frame pacing.

### Pure-Rust AVIF dekoder (zero system deps)
Predchozi N+24 reseni s Cargo feature `avif` -> `image/avif-native` vyzadovalo
system libdav1d + NASM. User: "AVIF nemuze v systemu nic vyzadovat".

Fix:
- Drop Cargo feature gate.
- Add `zenavif = "0.1"` (pure-Rust AVIF codec) + `zenpixels-convert`.
- Backend `rav1d-safe` (pure-Rust port libdav1d od Memory Safety Initiative).
- Novy `browser::avif_decode` wrapper: `decode(bytes) -> (w, h, rgba8)`.
- Wire do `load_image_as`: AVIF detected -> zenavif::decode -> atlas.
- Compile cost: +~1m pres rav1d, **zero runtime deps**.
- Browser sam dekoduje, user nic neinstaluje.

### Layout wire-ups

**Tables auto-layout column widths:**
- `prelayout_table_columns(bx)` pre-pass na Display::Table v layout_dispatch.
- `tables::compute_column_widths_auto` -> per-column widths.
- Apply jako explicit_width na cells napric vsemi rows (shared per spec).

**Multicol balance_height wire:**
- Pres `multicol::balance_height` ceil(content/n) per column-fill:balance spec.

**Writing modes propagace:**
- `layout_block_vertical` inherituje writing_mode na deti.
- Sideways-rl pridan vedle vertical-rl pro RTL detection.

### Lazy loading wire (HTML loading=lazy)
- Pridano `loading_lazy: bool` field na LayoutBox.
- `apply_tag_html_attrs` parsuje `loading="lazy"` na <img> + <iframe>.
- Paint pass v `build_display_list_culled`: skip Image emit kdyz box mimo
  viewport + LAZY_MARGIN 1250px (Chrome default).
- 3 tests verifikuji.

### Web Vitals LCP + Frame pacing wire-up
- `WebVitalsCollector::collect_from_paint(commands, now_ms)` - scan display
  list, najde Image candidate, recorduj area > 100 (skip 1x1 trackery).
- WebView ma `web_vitals` + `frame_pacer` fields + public getters.
- `render_via` instrumented `begin_frame()` + `mark_presented()` na vsech
  exit paths (real GPU + 2 cache-hit fast paths).

### Scroll backtrack - dodatecne fix
- `set_scroll(x, y)` (programatic) clears active scroll_anim.
- JS scrollTo() sync path v render_via te clears anim.

### Pure-Rust JPEG XL + HEIF dekodery (dodatecne k AVIF)
- `jxl-oxide` 0.12 (pure-Rust JPEG XL) + `heic` 0.1 (pure-Rust HEIF/HEIC s H.265 SIMD).
- `browser::jxl_decode` + `browser::heif_decode` wrappers.
- Wire do `load_image_as` - real dekode misto tombstone.
- Vsechny tri (AVIF + JXL + HEIF) zero system deps, browser sam.

### Web Vitals CLS + INP wire-up
- `WebVitalsCollector::feed_layout_shift(prev_rects, curr_rects, vp, ...)` -
  detect movements > 3px, compute shift_score per W3C spec.
- `record_input_interaction(type, start_ms, processing_start, processing_end,
  presentation_ms)` - INP feed.
- Skip user-triggered shifts per spec.

### Tables fixed-layout algorithm wire
- `table-layout: fixed` -> use first-row widths only (per spec faster path)
- Pres `tables::compute_column_widths_fixed`.

### WPT runner assert_throws_js real
- `assert_throws_js(ctor, fn[, msg])` ted realne vola `fn()` pres interp_ptr.
  Pri thrown error = pass, jinak = fail (per spec).
- 2 nove tests verify pass-on-throw + fail-on-no-throw.

### Test growth
4097 (N+24) -> 4120 (N+25). +23 tests. 0 failures, 29 ignored.

### Pure-Rust image format coverage
PNG, JPEG, GIF, WebP, BMP, ICO, TIFF, TGA, EXR, QOI (via image crate) +
**AVIF** (zenavif/rav1d) + **JPEG XL** (jxl-oxide) + **HEIF/HEIC** (heic) +
SVG (resvg). Vsechny bez system deps - browser sam dekoduje.

### Stale TODO
- Real LCP timing presneji (now je always 0.0 placeholder)
- Tables fixed-layout algorithm
- Writing modes inline_layout vertical text flow
- HEIF/JXL pure-Rust decoders

## Session N+24: Scroll bug fix + image decoders + BFC + WPT runner (4097 testu)

Real wire-up vlna pro 4 prioritu z N+23 foundation modulu. User-reported scroll
regression + image AVIF support + spec-compliant BFC margins + real WPT runner.

### Scroll backtrack bug fix
**Symptom:** "po chvili skoci ve skrollu kus zpet" - scroll prejede target,
anim skonci, frame tick set scroll_y = scroll_target_y = jump back.

**Root cause:** Double-counting v `retarget_scroll`. Caller v `start_scroll_anim_y`
predava ABSOLUTNI new_target (`scroll_target_y + dy`), funkce ale jeste pridavala
`prev_target_remainder` -> accumulated_target > skutecny target -> anim presahla,
po dokonceni se frame sync vratil zpatky na `scroll_target_y`.

**Fix:** `retarget_scroll` pouziva `new_target` primo. Velocity continuity zustava.
Curve always resets od `current_value` s novym start_time (no t-progress backwards).

**Test coverage (Chrome/FF inspired):**
- `no_backtrack_after_anim_finish` - regression test (anim.target == scroll_target)
- `rapid_scroll_finishes_at_total_distance_no_overshoot`
- `chromium_update_target_keeps_progress_forward`
- `chromium_reverse_does_not_change_current_value`
- `chromium_duration_progress_positive`
- `firefox_velocity_carries_same_direction`

Reference: Chromium cc/animation/scroll_offset_animation_curve_unittest.cc.

### Image decoders real + AVIF (pure-Rust, bez system deps)
- `image` crate v0.25 pro PNG/JPEG/GIF/WebP/BMP/ICO/TIFF/TGA/EXR/QOI - jiz hotove.
- **AVIF pure-Rust:** `browser::avif_decode` modul pres crate `zenavif` 0.1
  (pouziva `rav1d` - pure-Rust port libdav1d od Memory Safety Initiative).
  Browser sam dekoduje, **user nic neinstaluje**. Bez system libdav1d, bez NASM.
- `browser::image_decoder::detect_format` foundation modul wired do `load_image_as`
  pro pre-empt classification.
- AVIF detected -> zenavif decode -> RGBA8 -> atlas (s resize pro velke).
- HEIF detected -> tombstone (vyzadovalo by libheif).
- JXL detected -> tombstone (image crate v0.25 nepodporuje).

AVIF default-on. Build cost: +~1m kompile zenavif/rav1d crates pure-Rust.

### BFC margin collapse spec-compliant
Existing layout_block mel `(m_t - prev_margin_bottom).max(0.0)` - spravne pro
all-positive (= max) ale chybne pro mixed signs (per CSS 2.1 8.3.1).

Wire `browser::layout::bfc::collapse_margins` - spec-compliant:
- Pos+Pos = max, Neg+Neg = min, Mixed = sum.

Pres `collapse_margins(prev_margin_bottom, m_t) - prev_margin_bottom` aby
existing flow (prev_m_b uz pricten k cursor_y) drzelo. Pri zaporne difference
cursor zpetne posuje (negative margin overlap per spec).

### WPT runner real exec
`testing::wpt::run_wpt_script(user_js)` spawne novy Interpreter, zaregistruje
testharness API jako native fns ktere primo execute callback + write do
shared `WptHarness`:

- `test(fn, name)` - call callback s mock `t`, catch JsError, record pass/fail
- `async_test(fn, name)` - immediate eval, stub `t.done()`
- `assert_equals/not_equals/true/false/array_equals/unreached/throws_js`
- `extract_inline_scripts(html)` - pull `<script>` blocks z HTML

Reference: Chromium third_party/blink/web_tests + WebKit Layout Tests harness.

**Use cases:**
- Spec compliance smoke tests (drop test file dovnitr, mereni pass/fail)
- Regression coverage (pridat assertion po novem feature)
- Self-test enginu (run subset WPT manualy, diff vs expectations)

10 unit tests verifikuji runner: simple pass, failing assert, multiple tests,
array assert, inline script extraction.

### Worker real thread spawn (verified existing)
Existing `interpreter::builtins::Worker` jiz spawne `std::thread::spawn` s mpsc
channels pro main<->worker komunikaci. Worker.postMessage/terminate wired v
eval_call.rs. `drain_workers` na main thread proces incoming messages s JSON
parse + onmessage callback dispatch.

Foundation modul `interpreter::worker_pool` je nezavisle abstraction ktery
NEsubstituuje existing path, ale poskytuje API surface pro SharedWorker /
WorkletGlobalScope features.

### Test growth
4086 (N+23) -> 4097 (N+24). +11 tests. 0 failures, 29 ignored.

### Co stale chybi (TODO post-N+24)
- **Compositor GPU tiles:** wire LayerTree + tiles do render pipeline (big refactor)
- **Tables auto-layout:** wire `tables::compute_column_widths_auto` (big refactor)
- **Multi-column wire:** `multicol.rs` balance algorithm not connected
- **Writing modes pipe do inline layout** (horizontal-tb only ted)
- **Lazy loading wire:** parse loading=lazy + viewport intersection check
- **Web Vitals (LCP/CLS):** collector hotov, chybi feed z paint loop
- **assert_throws_js real check** (vyzaduje closure invoke + check)

## Session N+23: Foundation modules vlna (4086 testu)

Pokracovani N+22 - implementace ~140 novych foundation modulu napric
browser/interpreter/testing podsystemy. Cilem bylo polozit API surface +
state machines pro vsechny chybejici web specifikace + browser-grade features
tak, aby pozdejsi GPU/syscall/codec real implementace mohla "doplnit" backend
bez prepisovani teto vrstvy.

### Modules pridany (selekce)

**Interpreter / Web APIs** (~50 modulu):
- web_animations, background_fetch, compression_streams, fenced_frames,
  web_share_target, view_transitions, navigation_api, storage_buckets,
  private_state_tokens, attribution_reporting, topics_api, shared_storage,
  federated_credential
- custom_elements, mutation_observer, resize_observer, intersection_observer,
  encoding, structured_clone, abort_signal, decorators, source_map,
  debugger_protocol, heap_profiler, async_runtime, promise_state, import_maps,
  regex_engine, bignum, proxy_handler, typed_arrays
- worker_pool, persistent_storage, file_blob, fetch_api, headers,
  eventsource_state, url_search_params, form_data, error_kinds
- v8_inspector, stack_trace, cpu_profiler

**Browser security/network** (~15 modulu):
- security/sri, mixed_content, referrer_policy, permissions_policy, coep_coop
- net/hpack, qpack, quic, dns, http_cache, multipart, cookie_jar

**Layout / Render** (~12 modulu):
- layout/bfc, tables, multicol, subgrid, positioning, anchor_positioning,
  writing_modes
- render/blend (16 blend modes), subpixel_aa, compositor (layer tree),
  tiles, frame_pacing, hit_test_tree

**CSS** (5 modulu):
- css/nesting, calc_resolver, conditional_rules, color_mix (OKLab),
  transitions (cubic-bezier + steps)

**SVG** (4 modulu):
- svg/path_parser, transform_parser, gradient, filter

**HTML5** (4 modulu):
- html5/entities, form_state (constraint validation), template_content,
  browsing_context

**Media** (7 modulu):
- media/mse, eme, container_sniff, webaudio_graph, vtt_parser, srt_parser,
  h264_parse, av1_parse

**Input** (3 moduly):
- input/pointer_events, keyboard_events, input_method_editor

**Locale / i18n** (5 modulu):
- locale/bcp47, number_format, date_format, plural_rules, collation

**Browser features** (~50 modulu):
- sandbox, image_decoder, accessibility_tree, url_parser, text_bidi,
  unicode_segmenter, font_fallback, opentype_features
- event_dispatch, shadow_dom, selector_engine
- viewport, hidpi, drag_drop, autoscroll, spellcheck, autofill
- favicon, manifest, password_manager, extensions, bookmarks, history_db,
  downloads, dialog_manager, private_browsing, session_state, tab_groups,
  reader_mode, translator, zoom_levels, site_settings, reload_strategy,
  proxy_resolver
- web_vitals, safe_browsing, speculation_rules, origin_trials,
  webdriver_protocol, contenteditable_model, spatial_nav
- lazy_loading, page_visibility, print_preview, snap_scroll, overscroll,
  input_devices, battery_status, network_info, focus_manager,
  clipboard_history, ad_blocker
- bf_cache, wheel_normalize, window_features, crash_reporter, pull_to_refresh,
  screen_orientation, display_link, telemetry, experiment_flags, quirks_mode,
  charset_detect, geolocation_provider, os_clipboard

**Testing** (1 modul):
- testing/test262 (frontmatter parser + run accumulator)

### Test growth
3172 testu (N+22 baseline) -> 4086 (N+23). +914 jednotkove testy, 0 fail,
29 ignored, 0 measured.

### Konvence pouzite pri vsech batch modulech
- Komentare cesky, ASCII only (per project + user CLAUDE.md).
- Kazdy modul s `#[cfg(test)] mod tests` blokem 3-8 testu.
- Spec referencni odkaz v doc-comment hlavicce.
- Failure paths tested (Err returns, edge cases).
- HashMap key enums vsechny `Eq + Hash` (chyceno + opraveno per E0599).
- Defaults rozumne (timeouts, quotas, refresh rates).

### Status quo zachovan
- Existujici engine bin, shell bin, devtools panel - vse beti.
- Render pipeline / interpretation / cascade nezmeneny.
- Embedded API contracty stejne.

### Co tyto moduly **nedelaji** (foundation only)
- Real OS syscall (sandbox policy install, OS clipboard IPC, file pickers)
- Real GPU shader pipeline (compositor tile raster zustava CPU only)
- Real codec decode (h264/av1 parse zustava header-only)
- Real network (HPACK/QPACK/QUIC parse zustava bez TLS+socket)
- Plne Test262 / WPT execution (jen harness + frontmatter parsing)

### Co je dalsi krok pro production-grade
1. Wire foundation modules do existujicich render/interpreter call sites.
2. Vyplnit real backends pro CPU-bound veci (image_decoder -> image crate,
   webaudio_graph -> cpal/rodio, regex_engine -> fancy-regex, ...).
3. Sandbox real install (sandbox.rs ma policy structs + permits check,
   doplnit per-OS impl: seccomp, AppContainer, sandbox-exec).
4. Compositor tile path real GPU raster (tiles.rs ma TileCache + LRU,
   chybi wgpu render-to-tile target + atlas binding).
5. Real HTTP/2 client (http_cache.rs + hpack.rs hotove; potreba TLS + socket
   pump - misto ureq doporucuji isahc nebo hyper).

## Session N+22: Engine shell strip + WebView polarity invert (step 1)

Pokracovani N+21 (workspace + embed API) - kompletni shell concerns
strip z engine + zacatek WebView authoritative polarity.

### Dosazeny stav

- shell_chrome.rs file (-242 LOC) + dead chrome paint blok v App::render (-363 LOC)
- 16 dead `if false { ... }` bloku (-720 LOC)
- TabManager + tabs.rs file (-747 LOC, 9 unit testy)
- 10 shell-only App fields (shell_chrome_h, addr_open, find_open, addr_input,
  find_query, find_match_idx, history, history_idx, bookmarks_bar_visible,
  bookmark_picker, reading_mode_on, shortcuts_overlay_open, tab_drag_*,
  shell_tab_*, status_hover_url)
- ChromeHit enum + hit_chrome fn
- READING_MODE_CSS const + ChromeBookmarkPickerState
- Multi-tab MenuAction::Tab*(idx) match arms (TabClose/CloseOthers/Duplicate/
  SetGroup/PinToggle/Reload)
- navigate_about fn -> no-op (about: pages = shell)
- find_apply / find_step / find_scroll_to_current / find_collect_matches /
  find_matches_in fns
- run_inline_scripts fn (App, duplicate s WebView::run_scripts)

**Net: render/mod.rs 9700 -> 8231 LOC (-1469). Plus -747 tabs.rs +
-242 shell_chrome.rs = ~-2400 LOC engine shrink (~25%).**

### WebView authoritative polarity (zacatek)

Drive App.html/css/interpreter byly PRIMARY, WebView mirror sync'nuty.
Po N+22 step 1+2 inverze:

- App::resumed: vola sync_webview_from_app, pak interpreter = webview.take_interpreter()
- reload_from_html (drag-drop): stejne
- form submit POST: stejne
- navigate_url http: stejne
- rerun_paused_scripts (debug resume): pres webview.run_scripts + take

WebView::load_html runs scripts (real). App.interpreter prevezme ownership.

### Shell crate plnohodnotny browser host (N+22 finale)

Po vsech orchestrace + input dispatch + shell wire features:

**Shell features hotove:**

Rendering pres webview.render_via:
- Full cascade + transitions + @keyframes animations + paint anim
- Layout + sticky positioning + paint
- Display list cull + scroll shift + scrollbar overlay
- Canvas2D + WebGL canvas frame
- Atlas warm + text runs extract
- async_jobs + interpreter event queues drain
- Selection highlight paint (modry overlay)
- Caret blink pro focused input
- `<select>` popup overlay

Input pres webview.handle_input:
- Mouse: down/up/move/leave/wheel
- Click-vs-drag distinguish (5px threshold)
- :hover state + focus / blur
- mousedown / mouseup / click event dispatch do JS
- `<a href>` -> NavigationRequest
- Text selection drag (anchor/current/extract)
- Scrollbar thumb drag (V + H)
- Keyboard: keydown/keyup do focused element
- TextInput: insert na caret pos + advance
- Backspace / Delete / Arrow / Home / End: caret + value edit
- Enter on input: form submit event + NavigationRequest
- Cursor icon (Pointer pri <a>/<button>, Text pri input/text node)

Shell-side handlers:
- WindowEvent::Resized -> webview.resize
- WindowEvent::CursorMoved -> MouseMove + cursor apply
- WindowEvent::MouseInput -> MouseDown/Up
- WindowEvent::MouseWheel -> Scroll (Ctrl+Wheel = zoom)
- WindowEvent::KeyboardInput -> KeyDown/Up + TextInput
- WindowEvent::DroppedFile -> webview.load_url
- WindowEvent::ModifiersChanged -> track Ctrl/Shift/Alt
- WindowEvent::RedrawRequested -> render + present + window.set_title

Shell-side keyboard shortcuts:
- **Ctrl+C** -> clipboard copy selection (arboard)
- **Ctrl+A** -> select all
- **Ctrl+Plus/=/Minus/0** -> zoom +/- /reset
- **Ctrl+R / F5** -> reload current page
- **Alt+Left / Alt+Right** -> history back/forward
- **PageDown/Up / Arrows / Home/End / Space** -> page scroll
- **Esc** -> clear selection
- **Ctrl+Wheel** -> zoom

Navigation:
- response.navigation Get -> webview.load_url + history push
- response.navigation Post -> webview.load_url_post (build_form_request + ureq POST)
- History stack pres back/forward + reload

Continual redraw kdy webview.has_active_animations() (@keyframes /
transitions / smooth scroll / caret blink) - shell request_redraw loop.

Co stale chybi (Phase 99 cleanup):
- Inspector overlay paint (App side, last_layout_root accessor ready)
- Devtools panel UI (user planuje rework do separate WebView app)
- App polarity invert (App.html/css/scroll/base_url duplicate s webview)
- preventDefault honoring v event dispatch
- Multi-tab v shell (Vec<WebView> + tab strip)
- Text selection rect pres painted_text_runs (currently flow-based)
- Devtools debug_runner JS pause/continue UI

---

### WebView full orchestration (Chrome WebContents parity)

User point: features ze App.render JIZ existovaly v engine browser
moduly (cascade::apply_animations, layout::apply_sticky, render::
apply_paint_animations, devtools_panel::paint_inspector_overlays, ...).
Stary WebView::render_via byl minimal first-draft (40 LOC = jen cascade
+ layout + paint + draw). Phase 4d migrace = volat existujici fns ze
WebView::render_via.

Po N+22 finale WebView::render_via orchestrace zahrnuje:

**Render pipeline:**
- Cascade s viewport
- CSS Transitions (detect/apply + transitionend event)
- @keyframes animations tick (apply_animations + scroll_animations +
  apply_paint_animations) + animationstart/end/iteration events
- Layout + sticky positioning
- Display list cull + scroll shift + scrollbar overlay
- Canvas2D ops paint (paint_canvas_ops)
- Atlas warm + text runs extract
- Draw segments
- WebGL canvas frame (run_webgl_frame)
- async_jobs.drain
- interpreter event queues (drain_websockets / drain_fetches /
  drain_raf_callbacks)

**Input handling:**
- MouseMove: hit-test layout -> set_hovered_node (:hover state)
- MouseDown (Left): hit-test + focus management + JS click event +
  `<a href>` navigation request emit
- MouseUp + MouseLeave: hover clear
- KeyDown: focused element keydown event + Backspace input edit +
  Enter form submit dispatch
- KeyUp: keyup event
- TextInput: append do focused input value attr + input event
- Scroll: smooth target (lerp v render_via)

**State exposures:**
- `text_runs()` + `hit_test_text(x, y)` - per-glyph selection foundation
- `last_layout_root()` - host overlay pass (inspector overlay)
- `has_active_animations()` - shell continual redraw signal

**Shell crate (ShellApp) ted uses:**
- MouseInput -> MouseDown/Up + navigation handling
- KeyboardInput -> KeyDown/Up + Character TextInput
- MouseWheel -> Scroll
- CursorMoved -> MouseMove
- Pri response.navigation: webview.load_url
- request_redraw loop dokud animations/transitions/smooth_scroll bezi

**Zustal v App.render (devtools-specific):**
- Inspector overlay paint (App.devtools state - separate render pass
  nad webview RT)
- Devtools panel paint (Elements/Console/Network/Sources panely)
- Devtools resize drag handler
- debug_runner poll (JS pause/continue)
- Paused animations frozen snapshot

User rekl: devtools dostane velky rework v dalsi session.

---

### Polarity invert finale - 6/7 fields hotove

Smazane App fields (6):
- `title` -> webview.title() delegate
- `zoom` -> webview.zoom() / set_zoom + cur_zoom capture
- `scroll_target_x/y` -> webview.scroll_target_x/y + cur_X capture
- `scroll_x/y` -> webview.scroll_x/y + cur_X capture
- `html`, `css`, `base_url`, `current_path` -> webview + initial tuple drz pres
  Option<(String, String, Option<String>, Option<PathBuf>)>, take()'d v
  App::resumed

Caller sites passuji data PRIMO do `sync_webview(html, css, base, path)`:
- App::resumed take initial
- reload_from_html (drag-drop): file:// url + path z drag
- form submit POST: response html + url
- navigate_url http: fetched html + css + url

Zbyle App field (1):
- `interpreter: Option<Interpreter>` - 59 ref, polarity invert vyzaduje
  **App.render kompletni rewrite** (1266 LOC).

### App.render rewrite plan (dalsi session)

Currently App.render = 1266 LOC inline pipeline:
- Section A: poll_debug_runner + devtools_wire + console mirror (4416-4767)
- Section B: cascade + style_map cache (4599-4767)
- Section C: drain WS/fetch/rAF + async_jobs + anim_apply (4769-4855) - **duplikat webview.render_via**
- Section D: layout_tree cache (4855-5026) - **duplikat webview**
- Section E: paint apply_sticky + apply_paint_animations + build_display_list (5026-5076) - **duplikat webview**
- Section F: post-paint shifts + canvas_ops (5076-5277) - **duplikat webview**
- Section G: overlays element_highlight + inspector + shell_chrome + fps + devtools_panel (5277-5404) - **APP SPECIFIC**
- Section H: atlas warm + text_runs + addr_overlay (5404-5588) - **duplikat webview**
- Section I: gpu draw + present (5588) - **duplikat webview**

**Plan:**
1. Extract App-specific (G) do `paint_devtools_overlays(&self, cmds, &layout)`.
2. Delete C-F + H-I (duplikat webview).
3. New App.render telo:
   ```
   self.poll_debug_runner(); self.sync_devtools_state();
   let renderer = self.renderer.as_mut()?;
   let webview = self.webview.as_mut()?;
   webview.set_zoom(self.zoom_local); // sync z App.zoom
   webview.render_via(renderer); // = sections B-F+H-I
   // Overlay pass nad webview RT (start_clear=false).
   let layout = webview.last_layout_root().cloned();
   if let (Some(l), Some(view)) = (layout, webview.target_view()) {
       let mut overlay_cmds = Vec::new();
       self.paint_devtools_overlays(&mut overlay_cmds, &l);
       renderer.draw_segments_into_view_clipped(view, &overlay_cmds, false, None);
   }
   if let Some(view) = webview.target_view() {
       renderer.present_external_to_swap_chain(view);
   }
   ```

Velikost odhad: 4-6 hodin
- Extract App-specific overlay paint (G) do helper - ~1 h
- Delete duplicit sections (C-F, H-I) - 30 min
- New render telo - 1 h
- Fix devtools state borrow conflicts - 1-2 h
- Fix tests + regression debug - 1-2 h

Po rewrite:
- App.render = ~50 LOC
- App.interpreter polarity invert trivialni (webview.interpreter() helper, NO borrow conflicts mimo overlay pass)
- App = thin host ~300 LOC total (Window + Renderer + devtools state + helpers)
- Engine bin = analogiou shell crate s devtools widget

Pripadne pres dalsi krok devtools rework D1-D6 (CDP protocol + frontend
HTML + 2-WebView shell).

---

### Polarity invert progress (po user pozadavku dokoncit pred devtools rework)

Smazane App fields (4):
- `App.title: String` -> `self.webview.as_ref().map(|w| w.title())`
- `App.zoom: f32` -> `self.zoom()` method + `self.set_zoom(z)`
- `App.scroll_target_x/y: f32` -> getters/setters
- `App.scroll_x/y: f32` -> getters/setters + `cur_scroll_y/x` capture
  na startu App.render (borrow conflict pres mut renderer borrow scope)

Zbyle App fields (Phase 99):
- `App.html: String`, `App.css: String` - primary v App, mirror v webview.raw_html/css.
  Smaze vyzaduje sync_webview_from_app refactor (take html/css args ze
  caller misto self.* fields).
- `App.base_url: Option<String>`, `App.current_path: Option<PathBuf>` -
  stejny problem: initial values z run_window args potreba pri prvnim
  sync. Drzene jako App fields aby sync mohla pouzit.
- `App.interpreter: Option<Interpreter>` - velky (59 ref). Po polarity
  invert webview.interpreter primary; App vola pres
  `self.webview.as_mut().and_then(|w| w.interpreter_mut())`. Borrow
  checker problemy pri scope kde webview + interpreter mut current.

Polarity invert dotaz: realne kompletne smaze vsechny App fields
vyzaduje App.render kompletni rewrite na shell-like pattern:
```
self.sync_webview_from_app(html, css, base_url, path);
let view = self.webview.as_mut().unwrap().render_via(renderer);
renderer.present_external_to_swap_chain(view);
```
+ devtools overlay pass pres `webview.last_layout_root()`.

To by smazlo 1260 LOC App.render. Phase 99 priority po devtools rework
(spise pred - clean App nez novy devtools).

---

### Phase 99 - polarity invert continuation

Zustava (NEresly N+22):
- App.html / css / base_url / current_path fields - duplicit s
  webview.html() / css() / base_url() / local_path(). Smazat App fields,
  refs nahradit pres helpers/getters.
- App.scroll_x / scroll_y / zoom fields - duplicit s webview.scroll()/zoom().
- App.layout_root cache - pojme presunout do webview internal cache.
- App.interpreter field - posledni primary. Smazat, pres webview.interpreter()
  / _mut() helpers. Risk: borrow checker (App mutace + webview borrow conflict).

Po polarity invert komplete: App degeneruje na "engine demo host wrapper"
(Window + Renderer + devtools panel + animations cache + JS debugger UI).
Mozno spojit s shell::ShellApp do unified host pattern.

### Pomocne metricy

Tests: 2697 pass (drive 2706 -9 ze smazanych tabs.rs internal testy).
Build: 0 warnings.
shell render: text + scrollbar + scroll OK.

---

## Session N+22 ORIGINAL: Engine shell strip (chrome bar mimo engine)

**2706 tests pass, 0 warnings.**

Pokracovaní Session N+21. Cilem: engine renderuje JEN naked viewport,
chrome bar (tabs/addr/find/bookmarks) zmizel z engine.

### Co se smazalo

1. **`lib.rs` shell dispatch** - args "shell" smazany; `browser` + `window`
   uz jsou aliasy bez `shell_mode` lokalniho flagu; `--no-shell` smazan;
   `run_window_with_shell` call odstranen
2. **`browser::render::run_window_with_shell` pub fn** smazana
3. **App field `shell_mode: bool`** smazany - vsech 25 references
   `self.shell_mode` -> `false` (dead branches)
4. **App init**: session restore (multi-tab) odstraneny - single tab
5. **`shell_chrome.rs` soubor smazan** (242 LOC chrome bar paint)
6. **Dead chrome paint blok v `App::render`** smazany (363 LOC):
   paint_shell_chrome_with_groups call, bookmark picker, reading mode
   badge, status bar URL hover, tab tooltip, F1 shortcuts overlay,
   zoom indicator, scroll-to-top button

### Co ZUSTALO (Phase 99 cleanup, ne kriticky)

- App fields stale tam (unused dead code):
  `tabs`, `addr_open`, `addr_input`, `find_open`, `find_query`,
  `find_match_idx`, `history`, `history_idx`, `bookmarks_bar_visible`,
  `shell_chrome_h`, `shell_tab_tooltip`, `shell_tab_hover_pending`,
  `tab_drag_idx`, `tab_drag_x_start`, `bookmark_picker`,
  `reading_mode_on`, `shortcuts_overlay_open`
- `tabs.rs` (747 LOC) - zustal jako page state holder (App init pouziva
  `tabs::Tab::new` pro single-tab page state). Shell-only metody
  (TabManager::switch_to, drag, ...) dead.
- Dead event handler bloky pod `if false`/`if self.shell_mode` (=false)
  vsude pres mod.rs - 9330 LOC stale. ~500 LOC dead.

### Validovany stav

```
cargo run -p rwe-engine -- browser       # naked viewport (ZADNY chrome bar)
cargo run -p rwe-engine -- browser src.html
cargo run -p rwe-shell                   # WebView pipeline naked
```

Pro plnohodnotny chrome (chrome bar + tabs + addr bar + bookmarks)
je NUTNE Phase 99 - shell crate dostane chrome paint code. Aktualne
NIKDE neni chrome dostupny.

### Commits

```
2ae6e33 chore(shell): vyhodit `legacy` arg delegation
a1408b7 refactor(engine): smazat shell dispatch z lib.rs + App.shell_mode
d5fd0d7 refactor(engine): smazat shell_chrome.rs (chrome paint je shell concern)
174500a refactor(engine): smazat 363 LOC dead chrome paint blok
73d8a94 refactor(engine): smazat 7 shell-only App fields
de245ad refactor(engine): smazat READING_MODE_CSS + reading_css cache shtub
```

### Phase 99 cleanup TODO (na ostraneni TabManager + zbylych shell concerns)

App.tabs field (TabManager) + tabs.rs (747 LOC) zustavaji. Multi-tab
keyboard shortcuts a chrome event handlers jsou v dead `if false` blocich
ALE pole stale alokovany kvuli ref sites uvnitr tehto bloku. Strip
vyzaduje:

1. Pridat App.current_tab: tabs::Tab field, init z initial_tab.
2. Pridat App::active_tab(&self) -> &Tab, active_tab_mut(&mut self) -> &mut Tab.
3. **JEDNOTLIVE** smazat vsech ~20 `if false { ... shell ... }` bloku v
   render/mod.rs (CharacterReceived handlers, mouse handlers, MenuAction
   match arms, hit_chrome calls). KAZDY block ma rozdilnou hloubku +
   nested match - automatic regex strip rozbije strukturu (zkouseno v
   N+22 - vlastni session strip neuspesny, revertovany).
4. Smazat App.tabs: TabManager field. Smazat init line.
5. Replace `self.tabs.active_tab()` / `_mut()` -> `self.active_tab()`
   / `_mut()` (POUZE po brackety strip jinak chyby misalign).
6. Smaze MenuAction::Tab*(idx) match arms.
7. Smaze hit_chrome fn + tabs.rs.
8. Engine.embed::loader pridat `extract_title` re-export z tabs.rs
   (pouzity i pro embed loadu). about: pages fns presunout do shell.

Dalsi shell-only fields ktere se po tomto kroku da smazat (po dead block
strip):
- `addr_input` (SimpleStringBuffer), `find_query`, `find_match_idx`
- `history`, `history_idx`
- `shell_tab_tooltip`, `shell_tab_hover_pending`
- `tab_drag_idx`, `tab_drag_x_start`
- `status_hover_url`
- `tabs::TabManager` + `tabs::Tab` (po current_tab refactor)

Strip neuspesny v N+22 protoze: PowerShell regex replace `self.tabs.X`
-> komentar rozbil multi-line `match` arm / `if let Some(t) = ...` 
struktury. Iterativni manual delete kazdeho dead bloku je nutny.

---

## Session N+21: Shell-as-crate refactor (Edge/CEF model)

**2706 testy pass, 0 warnings, 7 commitu na branche `inferius-dev/serene-bassi-0a7b83`.**

Cilem session: extrahovat shell (browser chrome) jako samostatnou crate `rwe-shell`, engine zustane pure embeddable renderer. Model = WebView2 / WKWebView / Servo WebView.

### Cargo workspace setup

Root `Cargo.toml` = `[workspace]` + members `crates/engine` + `crates/shell`.
default-members = engine (puvodni `cargo run` chovani zachovano).

```
crates/engine/  -> lib `rwe_engine` + bin `rwe-engine` (puvodni kod)
crates/shell/   -> lib `rwe_shell` + bin `rwe-shell` (novy host)
static/         -> root (test fixtures, accessible z obou bins z cwd=root)
```

`tests/` presunuto do `crates/engine/tests/` (fixtures used by taffy compliance + web fixtures testy).

### Embeddable API kontrakt (`embed` module)

Engine vystavuje:

- `embed::Engine` - sdilene `Arc<Device>`/`Arc<Queue>` + atlas placeholders + `EngineSettings`. `new(device, queue)` pro host integraci, `new_headless()` pro state-only testy.
- `embed::WebView` - per-tab page state: DOM, stylesheets, JS interpreter, layout cache, scroll, viewport, offscreen RT.
- `embed::InputEvent` / `EventResponse` / `KeyModifiers` / `MouseButton` / `CursorIcon` / `NavigationRequest`/`Method`/`Target`/`Result` - neutralni input/output typy (no winit dep ve WebView API).
- `embed::loader` - sdilene page resource fns (resolve_css_imports, extract_*, `load_page(url) -> LoadedPage`).

### WebView lifecycle

```rust
let device = Arc::new(renderer.device().clone());
let queue = Arc::new(renderer.queue().clone());
let engine = Arc::new(Engine::new(device, queue));
let mut webview = WebView::new(engine, 1280, 900);
webview.load_html(html, css, base_url);
// each frame:
webview.handle_input(InputEvent::Scroll { ... });
let view = webview.render_via(&mut renderer).unwrap();
renderer.present_external_to_swap_chain(view);
```

WebView pub fns:
- `new(engine, w, h)`
- `load_html(html, css, base_url) -> NavigationResult` - parse + run scripts
- `load_dom(html, css, base_url) -> NavigationResult` - parse BEZ scripts (mirror sync)
- `load_url(url) -> Option<NavigationResult>` - http/file dispatch via loader
- `handle_input(event) -> EventResponse` - scroll + resize implemented, click/key Phase 99
- `render() -> Option<&TextureView>` - clear-only (headless-friendly)
- `render_via(&mut Renderer) -> Option<&TextureView>` - real paint (cascade -> layout -> display list -> draw)
- `resize(w, h, scale_factor)` + `set_scroll` + `set_zoom`
- low-level: `document()`, `interpreter()`/`_mut()`, `take_interpreter()`/`set_interpreter()`, `stylesheets()`, `html()`/`css()` (raw source preserve), `local_path()`/`set_local_path()`, `target_view()`/`target_texture()`

### Renderer expose pub API (engine internals)

`browser::render::Renderer`:
- `pub struct` + `pub fn new(window)` + `pub fn resize_surface(w, h)`
- `pub fn device() / queue() / surface_size() / scale_factor_value()`
- `pub fn draw_segments_into_view_clipped(view, cmds, start_clear, scissor) -> bool`
- `pub fn present_external_to_swap_chain(src_view) -> bool` - acquire swap chain, compose fullscreen, present

### Shell crate runtime (Phase 4c+5 minimal)

`crates/shell/src/app.rs` - `ShellApp` s vlastnim winit `ApplicationHandler`:
- `resumed`: vytvori Window + Renderer + Engine(z renderer device/queue) + WebView + load_html
- `window_event::Resized` -> renderer.resize_surface + webview.resize
- `window_event::RedrawRequested` -> webview.render_via + renderer.present_external_to_swap_chain
- `window_event::CursorMoved` -> webview.handle_input(MouseMove)
- `window_event::MouseWheel` -> webview.handle_input(Scroll) + redraw

`crates/shell/src/lib.rs`:
- `pub fn run_window(html, css, base_url, local_path) -> Result<()>`

`crates/shell/src/main.rs`:
- default = shell::run_window pres embed API (no chrome)
- Bez chrome bar - pro chrome experience pouzij `cargo run -p rwe-engine -- browser`

### App.webview mirror (Phase 4a)

Engine `App` ma `webview: Option<WebView>` field. Sync v `resumed` + `reload_from_html` pres `load_dom` (no double-script-run). Mirror je read-only - pristup pres `App::webview() -> Option<&WebView>`. App.interpreter zustava primary; WebView je side-effect populated.

Phase 99 invertne: WebView authoritative, App reads delegated.

### CLI cheat sheet

```powershell
# Engine bin (puvodni rezimy, default cargo run)
cargo run                            # JS demo (CLI dispatcher)
cargo run -- debug src.js out.html   # debug viewer HTML
cargo run -- devtools src.html       # static devtools HTML
cargo run -- browser src.html        # browser s chrome (App primary)
cargo run -- browser --no-shell      # naked viewport (engine demo)
cargo run -- dump src.html           # layout/cascade dump

# Shell bin (Phase 4c+ runtime)
cargo run -p rwe-shell                       # WebView render path (no chrome)
cargo run -p rwe-shell -- static/test.html
# Plnohodnotny chrome (tabs/addr/find/bookmarks) zatim pres engine bin:
cargo run -p rwe-engine -- browser           # puvodni chrome bar
```

### Co Phase 99 udela

Plnohodnotny shell crate (parita s engine browser mode):

1. **Chrome paint v shell crate** - presunout `render/tabs.rs` + `render/shell_chrome.rs` + souvisejici App.chrome_state z engine do shell::ShellState. Shell composit shader = WebView texture + chrome paint nad to.
2. **Multi-tab v shell crate** - Vec<WebView> per ShellApp. Tab switching, session save/restore.
3. **Mouse click + keyboard dispatch do JS** - WebView::handle_input pro MouseDown/Up potrebuje hit-test pres layout tree + lookup DOM addEventListener registry + dispatch synthesized Event. Stejne pro KeyDown/Up.
4. **AddressBar/Find/Bookmarks** v shell crate - ShellState + paint + winit text input routing.
5. **WebView authoritative polarity invert** - App.html/css/interpreter mazat, App.webview primary. Currently mirror sync = redundant work pri kazdem reload.
6. **Engine multi-process izolace** - Phase 99 dle puvodniho planu. wgpu Device sharing zustane (Chrome model = separate renderer process + shared GPU process, ne separate device).
7. **App single-tab focus** - po shell extract App ztratit `shell_mode`, `tabs`, `addr_open`, `find_open`, `history`, `bookmarks_bar_visible`.

### Commits (Phase 1-5)

```
d1fd9a6 refactor: workspace skeleton - crates/engine + crates/shell (Phase 1)
131ffae chore: default-members = engine pro `cargo run` bez -p
2a0eb4d refactor(engine): embed API kontrakt - Engine + WebView stubs (Phase 2)
55910a7 feat(engine): WebView::load_html/load_url + loader helpers (Phase 3)
b200ff3 feat(engine): App.webview mirror field + sync (Phase 4a)
673db37 feat(engine): WebView offscreen RT + clear-only render (Phase 4b step 1)
8c0bbd9 feat(engine): WebView::render_via - real paint pipeline (Phase 4b step 2)
7a3a1e1 feat(shell): vlastni Window + Renderer + WebView runtime (Phase 4c)
a68356a feat(shell+engine): scroll + mouse move input dispatch (Phase 5 minimal)
```

---

## Session N+20: debug helpers + mileneckaseznamka.cz dalsi vlna fixes

**2497 tests pass, build clean.**

Autonomous session - vlakno fixovalo veci v neprítomnosti uzivatele.

### Debug helpers (`src/debug_bp.rs` + `debug_utils.md`)

Globalni breakpoint helper modul. Tri cesty zastavit proces na konkretnim elementu:

1. **Env var filter + IDE BP na sink fn**:
   - `BP_TAG=img BP_CLASS=photo-box cargo run -- browser url`
   - IDE BP na prazdny `breakpoint_layout()` / `breakpoint_paint()` /
     `breakpoint_cascade()` / `breakpoint_build()` v `src/debug_bp.rs`.
   - Sink fn se zavola jen pri match -> stop pres filter.
   - Wired call sites: `layout/mod.rs:build_box_inner` + `flush_inline` img branch,
     `paint.rs:paint_box`, `cascade.rs:cascade()` walk.

2. **Conditional BP na konkretni line**:
   - BP na zvolene line, condition: `crate::debug_bp::lb_is_id(bx, "photo-box")`
   - Predicates: `lb_is_id/class/tag/match`, `node_is_id/class/tag`, generic
     `should_break(tag, id, class)`.
   - Vsechny `#[inline(never)]` - optimizer je nezahodi.

3. **Active trap inline**:
   - `debug_bp::break_if("img", "photo-box", "")` - SIGTRAP/int3 kdy match.
   - `debug_bp::debug_break()` - raw trap bez podminky.

Macros: `bp_here!`, `bp_layout!`, `bp_paint!`, `bp_cascade!`, `bp_build!`.

Viz `debug_utils.md` pro plnou dokumentaci + RustRover workflow.

### Image sizing - photo-box height:100% fix

`mod.rs:3293` abs/fixed positioning handler resolvoval jen `explicit_height`,
ne `height_pct`. photo-box `position:absolute; height:100%` zustaval h=0, img
uvnitr cetl parent_h=0 a spadl na advance_h=24. Fix: pridana branch
`else if let Some(p) = child.height_pct { child.rect.height = cb_h * p; }`.

### Devtools console capture native errors

`[script error]` chyby chodily jen na stderr - DevTools panel byl prazdny.
Fix: pri `interp.run()` Err -> push do `interp.console_log` jako "error" level.
DevToolsState mirror loop sam pripoji do console panel. Take parse + lex
errors capture.

### XMLHttpRequest stub (sync + async)

Real `XMLHttpRequest` builtin v `setup_builtins`:
- `open(method, url, async?)`, `send(body?)`, `setRequestHeader`, `abort`,
  `getResponseHeader`, `getAllResponseHeaders`, `overrideMimeType`,
  `addEventListener` ("load"/"error"/"readystatechange"/"loadend").
- Sync ureq HTTP (jako fetch). Status, responseText, response, readyState.
- onload/onreadystatechange/onerror/onloadend - fire pres `pending_xhr_callbacks`
  drain v event loop.
- `Interpreter` field `pending_xhr_callbacks: Rc<RefCell<Vec<(JsValue, JsValue)>>>`.
- `drain_xhr_callbacks()` v `run()` po `drain_timers`.
- `ActiveXObject` alias (IE legacy).

### External `<script src=...>` fetch (real engine, ne stuby)

Predtim `run_inline_scripts` cetl JEN `s.text_content()` - pri externim
`<script src="https://code.jquery.com/jquery.js">` byl text empty, src ignored.
Nasledne stranky padaly s `ReferenceError 'jQuery is not defined'`.

Fix v `render/mod.rs::run_inline_scripts`:
- Pro kazdy `<script>` element: pokud ma `src=...` -> resolve_url(base, src) +
  `fetch_text_url(abs_url)`. Push do `interp.network_log` s 200/0 status.
- Pri fetch fail: console_log error "[script fetch failed]".
- Inline scripts (bez src) -> text_content jako predtim.
- Real jQuery / GTM / Tracy / analytics se ted natahaji ze stranky a evaluuji
  jako normalni JS - zadne fake stuby.

(Drive jsem pridal jQuery `$` no-op stub + Tracy + dataLayer + gtag + ga + fbq -
to bylo wrong approach. Vyhozeno - real engine = real script load.)

### `<br>` linebreak fix

flush_inline iterace ignorovala `<br>` (display:inline, no text, no children) -
padlo do replaced inline branch s rect 0. Fix: explicit handler na zacatku
loopu - emit force linebreak (`cursor_y += line_height; cursor_x = inner_x`).

### `ul`/`ol` UA padding-inline-start gating

Default `ul/ol { padding-left: 40px }` se aplikoval i pri `display: flex`.
Mileneckaseznamka.cz nav menu mel children pushed +40px doprava. Chrome dela
UA padding jen pri block/list-item display. Fix: gate UA padding za
`matches!(bx.display, Block | ListItem)`.

### Diakritika fallback

Times Roman ma ASCII subset - chybeji Czech znaky (ř, ě, č, í). Pri rasterize
fontdue vraci empty glyph + 0 advance -> text vypadal jako `P_ezdivka` +
overlap.

Two-stage fix:
1. **`atlas.rs`**: `font_for_char(family, ch)` - iteruje primary -> extra_fonts
   -> bold/italic variants -> default font, vraci ten s `lookup_glyph_index(ch) != 0`.
   Pouzite v `add()`.
2. **`measure_text_width_full`**: pri advance==0 + glyph_index==0 -> fallback
   chain (sans/default/bold/mono). Posledni resort `font_size * 0.5`.

### Text overlap (inline span s children)

cursor_x advance pouzival pre-pass `estimated_w` (sum text children widths).
layout_block uvnitr inline elementu mohl resize rect.width vetsi (nesting,
padding, text wrap). Bez re-read overlap dalsi sibling pres real width.
Fix: po `layout_block(&mut bx.children[idx])` re-read `bx.children[idx].rect.width`,
pouzij `max(estimated_w)`.

### NEzbyva (parking pro tve review)

1. Carousel ne-animuje ale CPU - perf hot path apply_paint_animations.
2. Web fonts @font-face fetch + register.
3. SVG zubaty - polygon AA pri small features.
4. Buttons Registrovat/Prihlasit styling - mozna cascade specificity / pseudo-class.
5. Profile fetch ne-funguje na strance - mozna XHR async callback fire ordering.

Vsechny tyto vyzaduji navrhove rozhodnuti nebo konkretni HTML/CSS sample
ke krokovani (= ted vime jak: `BP_CLASS=*` env + IDE BP).

## Session N+19: mileneckaseznamka.cz fixes + dual render arch

**2471 tests pass, build clean.**

Real-world web debugging (`https://www.mileneckaseznamka.cz`):

1. **Grid auto-row fallback nafukoval row tracky** (`c2b4fa4`)
   - `fallback_h = inner_h / rows` distribuoval container vysku rovnomerne
   - Nasledny intrinsic pass row_tracks updatoval JEN nahoru -> fallback nemohl
     SHRINKovat na intrinsic.
   - Real: right-container.h=6022, 3 in-flow items, 6022/3=**2007**.
     top-container intrinsic=120 (min-height:93px + content+padding) ale row
     zustal 2007 -> top-container.rect.height=2007 misto 93. Cely page rozjety.
   - Fix: fallback_h = 0. Auto rows shrinknou na intrinsic per spec.
   - Update grid_spec_tests::gs_layout_2x2/3x3 (drive prazdne child() rely on
     fallback - ted sized_child(0, 50)).

2. **position:absolute na display:inline** (`a961cc8`)
   - `#lost_pwd_button { position: absolute; right: 7px; top: 47px }`
   - display=inline -> padl do inline_buffer + flush_inline a contribuoval
     h=100 do parent content size. .login-section z 231 na 340.
   - Fix: layout_block - matches!(child.position, Absolute|Fixed) check PRED
     display dispatch. Treat as block, OOF, neposouva cursor_y.

3. **position:fixed CB = viewport** (`a961cc8`)
   - .development-mode-enabled-warning {position:fixed; top:0; left:10px}
   - Driv sedela pri parent's inner_box (= x=345) misto pri viewport edge.
   - Fix: flex/grid/block OOF check is_fixed - pak CB = MATH_VIEWPORT (0,0,vw,vh)
     misto cb_x/y/w/h parenta. MATH_VIEWPORT nastaven v cascade_with_viewport.

4. **Dual render pass** (`f480dfa` + `4d2e41f`)
   - **Phase 1** (`f480dfa`): shell_rt: wgpu::Texture pridana Renderer.
     Browser chrome (tabs, addr bar, scrollbars) jde do shell_rt separately,
     page content do main_rt. Compose: main_rt -> swap, pak shell_rt
     alpha-blend pres page. shell_split index v render() oznacuje hranici.
     draw_full_frame ma shell_cmds parametr. Shell_rt clearovan transparent
     (a=0) aby alpha-blend compose nezakryl page mimo shell area. Removed
     chrome_top page scissor (page muze full window, shell overlay sam).
   - **Phase 2** (`4d2e41f`): per-buffer state hash cache.
     prev_page_hash + prev_shell_hash polozky v Renderer. Per-frame hashing:
     - page_hash: scroll, zoom, viewport, cascade_hash, frame_counter,
       devtools state (panel_open, tab, selected, find_open, sel anchor)
     - shell_hash: active tab, addr_open, addr_input, bookmarks visible,
       tab list state, url len, tooltip
     Pri match s prev -> skip render do toho RT. draw_full_frame_cached
     nova varianta (page_skip, shell_skip). Backwards-compat
     draw_full_frame wrapper s skip=false.
     Idle frames = no render work, just compose cached -> swap.
     Shell hover bez page change = skip page render. Page scroll bez shell
     change = skip shell render.

**Co zbyva na mileneckaseznamka.cz** (visualni issues z screenshot, nepodarilo
se v teto session):

- **Diakritika boxes** (š/ž/ř/ě/ď). Font glyph rendering. Roboto variable font
  ma diakritiku ale asi atlas mapuje spatne. Investigate: atlas.rs + font
  loading. Maybe codepoint -> glyph_id mismatch pri variable font.
- **Yellow profile cards prazdne** = `.skeleton-card` (placeholder). Real data
  nacita JS pres `fetch(/api/...)` -> JSON -> insert do DOM. User rekl fetch
  je jednoduchy REST API. Pokud fetch nefunguje, run engine + check
  network/console log. Engine ma `fetch` impl pres ureq.
- **Photos in top-miniature** rozhazene texty. Carousel jsou img elementy
  ktere maji byt vedle sebe v animated divu. Investigate: img loading +
  lazy load (`data-src` pattern), animation transform.
- **Logo h 114 vs Chrome 92**. full-logo-img content h=70 ours vs 48 Chrome.
  Image natural size issue. Investigate: image decode dimensions for
  `/images/full-logo.png` (or whatever path).

Spustit `cargo run -- browser https://www.mileneckaseznamka.cz/` -> Ctrl+Shift+D
dump -> compare s chrome-dump-fixed.txt.

---

## Session N+18: layout pre-pass stale-state fixes + family-aware measure

**2470 tests pass, build clean.**

Web fixture engine-test.json (Chrome export, viewport 3045x2063):
- Strict 5px:  32 -> 36 (+4)
- Loose 20px:  57 -> 59 (+2)
- html h:    14137 -> 9424 (chrome 9274, off jen 150)
- transform-grid h: 752 -> 80 (chrome 80, exact)
- s-transforms section: 839 -> 167 (chrome 172, off jen 5)

Klicove fixy:
1. **resolve_math_func word-boundary** - `min(`/`max(` matchovaly uvnitr
   `minmax(120px, 1fr)` na offset 3 a vyrabely `min<num>` mezivysledek
   (`max(120,1fr)` eval rozpadl). CSS Grid auto-fill spadl na 1 sloupec.
2. **parse_track_tokens minmax handling** - sub_size pro Track::Minmax
   pouzije min_px (CSS Grid sect 7.2.2.1). Bez toho auto-fill s minmax
   = 1 sloupec misto N.
3. **layout_grid h shrink** - non-percent rows override rect.height na
   total_h. Predtim "grow only" zachoval pre-pass 8-row stack po realne
   1-row layoutu.
4. **layout_block h override** - bound > 0 + bez explicit_height/taffy
   preset/empty + tag.is_some() + ne html/body: rect.height = bound
   (override pre-pass stale value). Bez tohoto by test-body zustal na
   h=800 i kdyz transform-grid uvnitr ma h=80.
5. **measure_text_width_full** family-aware - rozeznava monospace/sans/serif
   rodiny + load realny font (Courier New, Segoe UI, Times). flush_inline
   + intrinsic_content_width prepnuty na _full.
6. **cascade::propagate_inherited** - top-down DOM walk po cascade pass.
   Inherited CSS props (font-family, color, line-height, ...) propagovany
   od parent na deti. font-size/font-weight EXCLUDED (UA defaults per tag).
7. **flush_inline space_w real glyph width** misto `font_size * 0.27`,
   slop 0.5 px na wrap condition (FP ulpu na presne hranici inner_w).
8. **build_box_inner inheritance** font_size + line_height + bold/italic
   + colors + family do text node deti.

Open: pass-rate plateau 8.5%-8.8% kvuli font width mismatch (Times vs
Chrome Inter) - kazdy span text mereny ~70 px misto chrome 118 px.
Loose tolerance 20 px nestaci na 50+ px width diff. Bez exact font
match (real Inter loaded) bude pass-rate omezen tim.

## Session N+17: Esc full handle + scroll-to-top + loading field

**2448 tests pass, build clean.**

Esc handler rozsireny:
- color picker -> settings -> class manager -> tab overflow -> addr bar
  -> find -> page selection clear (priority order)

Scroll-to-top button:
- Pri scroll_y > 200 floating button v pravem dolnim rohu (32x32 accent)
- Klik = scroll_target_y = 0 (smooth scroll)

Tab.loading: bool field (foundation pro busy spinner v tab chip).

## Session N+16: clear buttons + Ctrl+H/B + history filter API

**2448 tests pass, build clean.**

Console clear:
- Toolbar nahore s "✕ Vymazat (N)" button v Console tabu
- ConsoleClear hit handler -> log Vec clear

Network clear:
- "✕ Vymazat (N)" button vpravo v filter toolbaru Network tabu
- NetworkClear handler

Keyboard shortcuts:
- Ctrl+H = navigate about:history
- Ctrl+B = navigate about:bookmarks

History filter API:
- render_about_history_filtered(query) - filter URL/title.contains
- render_about_history() = filtered("") wrapper
- Foundation pro budouci search input v history page

## Session N+15: about:newtab dynamic + tab pin

**2448 tests pass, build clean (0 warnings).**

About:newtab dynamic:
- render_about_newtab() z history (top 8 sites) + bookmarks chips
- Stranky cards (about:config / history / bookmarks)
- Hint footer
- Tab::empty() pouziva render_about_newtab()

Tab pin:
- Tab.pinned: bool field
- Tab context menu prvni: "Pripnout"/"Odepnout"
- TabPinToggle action - togglane + sort pinned-first + preserve active
- Pinned chip 36px wide (vs 200px), 📌 emoji, no title/close
- Pinned not closable (TabClose disabled)

paint_shell_chrome_with_pins varianta s pinned bool list.

## Session N+14: shell polish + chrome interactions

**2448 tests pass, build clean (0 warnings).**

Devtools toggle button:
- F12 button vpravo v shell nav baru
- ChromeHit::DevtoolsToggle handler

Status bar URL preview:
- App.status_hover_url field, update_hover detect <a href>
- Shell mode render dole (sb_y = win_h - panel_h - 22)

Zoom indicator:
- Pri zoom != 1.0 vykresli accent badge "{:.0}%" v pravem hornim rohu

Bookmark star toggle:
- ★ icon na konci URL bar, yellow kdyz bookmarked
- Klik = add/remove bookmark
- ChromeHit::BookmarkStar handler

Find on page polish:
- Counter separated z labelu, red color pri zero matches
- ↑ ↓ nav arrows vpravo

Tab.document_root field foundation (per-tab Document caching).

## Session N+13: about pages + Esc close + flavor switcher

**2448 tests pass, build clean (0 warnings).**

About: pages:
- about:history - cely seznam navstivenych URL (max 500), per-row link
  + relative time (pred 5 min / pred 2 h / ...)
- about:bookmarks - list zalozek
- Wired do navigate_about() handler

Esc close priority popups:
- handle_escape_close_popups() - color picker > settings > class
  manager > tab overflow
- Pre-empts ostatni KeyboardInput handlers

Chrome height dynamic:
- Base 64 (tab + nav) + 24 (bookmarks bar) jen kdyz bms.len() > 0
- shift_page_for_chrome + paint_shell_chrome_with_favicons receive
  computed chrome_h

Settings popup flavor switcher:
- Pridana sekce Flavor (Chrome/Firefox) s active button highlight
- SelectFlavor action + DevtoolsHit::SettingsFlavor + persist

Tab close X visualni:
- 16x16 kruhova bg pod close button (hover hint)

Cleanup: 0 warnings.

## Session N+12: settings theme klik + tests + element label Inter

**2448 tests pass, build clean.**

Settings theme switcher klik handler:
- SettingsPopupAction::SelectTheme + DevtoolsHit::SettingsTheme
- Klik na tlacitko Auto/Svetly/Tmavy: state.theme.mode + save_persisted
- Theme zmena okamzite

Polish:
- Element highlight label ted Inter font (push_ui_text)
- Session save pri CloseRequested aktualizuje active tab state pred snapshot

Tests pridane (7 novych, 2448 pass total):
- hsv_to_rgb_red/green/blue/white/black
- tab_manager_close_does_not_remove_last
- change_kind_variants

## Session N+11: favicon + SV gradient + new tab cards + addr cursor

**2441 tests pass, build clean.**

Real favicon load:
- Tab::new() sync fetch_image_bytes pres derive_favicon_url
- Tab.favicon_bytes cache
- paint_shell_chrome_with_favicons render 16x16 Image v tab chip + posun text

Color picker SV gradient (real):
- 16x12 grid HSV cells (s = col/cols, v = 1 - row/rows)
- Aktivni hue propaguje do gradient barev
- SV marker white 6x6 + black 2x2 dot na (sat, 1-val) pozici

New tab page:
- Klikatelne <a href> cards (about:config / example.com / HN / GitHub)
- Hover bg highlight + hint footer s shortcuts

Address bar cursor blink (frame_counter mod 60).

Settings popup theme switcher (Auto/Svetly/Tmavy buttons).

## Session N+10: tab drag + addr autocomplete + state save polish

**2441 tests pass, build clean.**

Tab drag reorder:
- App.tab_drag_idx + tab_drag_x_start fields
- LMB on tab chip = init drag, CursorMoved = swap v Vec, Released = clear
- Active idx fix pri reorderingu (drag posune active jak treba)

Address bar autocomplete:
- Pri non-empty query match na bookmarks (★) + history (↻)
- Popup pod bar s title + URL preview
- Klik na suggestion -> navigate_url + zavri popup
- Inter font, dark theme

Tab state save consistency:
- Ctrl+T + ChromeHit::NewTab ted save current tab state (scroll/html/css/
  url) pred .open(empty)
- Driv ztracene zmeny pri new tab; konzistentni s TabClick

Favicon foundation:
- Tab.favicon_bytes: Option<Vec<u8>> (cache)
- TODO: async fetch + render v tab chip

## Session N+9: color picker write-back + RMB menus

**2441 tests pass, build clean.**

Color picker write-back full:
- swatch_zones nese property name (6-tuple)
- OpenColorPicker nese property + cilovy element
- write_back_color_picker zapise hex do inline style attr
- update_inline_style helper parsuje + slozi prop:value pairs
- ChangeEntry log (StyleEdit) viditelne v Changes sub-tabu
- Live preview na page (cache invalidate)

Tab + bookmark RMB context menu:
- ChromeHit::TabContextMenu/BookmarkContextMenu
- Items: Zavrit / Zavrit ostatni / Duplikovat / Obnovit / Otevrit / Smazat
- dispatch_menu_action handlery

Side panel splitter per-dock + favicon + about:config + add rule
(uvedeno v predchozim N+8 commitech).

## Session N+8: shell/devtools polish + RMB menus

**2441 tests pass, build clean.**

S6 Favicon parsing:
- Tab.favicon_url field + derive_favicon_url() pres <link rel=icon>
- resolve_favicon: absolute / //protocol / /path / relative
- TODO icon load + render v tab chip

S4 Settings page (about:config):
- render_about_config() native HTML s profile/dock/bookmarks/history
- navigate_about() handler pred fetch_text_url
- about: prefix check v navigate_url_no_history

Bookmarks bar interactivity:
- ChromeHit::BookmarkClick(url) hit-test
- LMB navigate, RMB context menu (Open/Delete)
- Ctrl+D = bookmark current page

Tab context menu (RMB v shell chrome):
- Zavrit / Zavrit ostatni / Duplikovat / Obnovit
- MenuAction extension + dispatch_menu_action handlery

Side panel splitter per-dock: drag mouse_x prevod do panel-local coords.

P-add Add new rule: + button v styles toolbar appendne "/* nova vlastnost */: ;"
do inline style attr selected node.

## Session N+7: B-fixes + S-features + P13/P19/tooltip

**2441 tests pass, build clean.**

B1-B6 quick fixes:
- B1 Right/Left dock hit-test x-offset (local_mx = mouse_x - panel_x)
- B2 Color picker SV box klik -> sat/val
- B3 hsv_to_rgb full HSV->RGB convert + cp.sat/val fields
- B4 Class manager checkbox toggle (add/remove class)
- B5 Var jump highlight (90 frame decay v tick_frame)
- B6 Resize cursor RowResize/ColResize per dock

S-features:
- S2 Session restore: load_session pri startu + save_session pri close
- S3 Bookmarks bar: 24px panel pod nav bar, per-bookmark chip,
  Ctrl+D = bookmark current page
- S5 New tab page (about:newtab): native HTML/CSS const v tabs.rs,
  centered grid s 4 informacnimi kartami

P-features:
- P13 Changes sub-tab: ChangeEntry log, ClassToggle hooks
- P19 Compatibility sub-tab: static caniuse-style data, green/yellow
  status dots
- P-tooltip: hover swatch -> hex tooltip, hover var chip -> jump hint

Tab switching: save state pred switch_to (drive ztracene changes).

## Session N+6: shell tab integration + class manager + @font-face

**2441 tests pass, build clean.**

Shell mode plne integrovan:
- App.tabs: tabs::TabManager s initial tab z launch args.
- paint_shell_chrome_full per-tab chip rendering, Inter font, active highlight.
- ChromeHit enum + hit_chrome dispatcher.
- Chrome interactions: TabClick (switch), TabClose, NewTab, Back/Forward,
  Reload, UrlBar -> open addr.
- Keyboard shortcuts: Ctrl+T new, Ctrl+W close, Ctrl+1..9 jump,
  Ctrl+Tab next, Ctrl+Shift+Tab prev.
- Page area shifted dolu o chrome_h pri shell_mode (page nezacina pod chrome).
- History persistence: navigate_url volaje history::append_entry ->
  ~/.rwe/profiles/<active>/history.json.

Session restore (src/devtools/session.rs):
- Session struct (tabs Vec<SessionTab>, active idx) + save/load.
- 2 unit testy.

Class manager popup:
- paint_class_manager modal (centred), list aktivnich CSS class
  s checkboxy + class names. Outside klik dismiss.

@font-face enumeration:
- StylesState.font_faces (family, src, weight, style) populate z stylesheets.
- Fonts sub-tab list per face s detail rows.

## Session N+5: A1-A4 + B1-B2 + C1-C3 (full sprint)

**2439 tests pass, build clean.**

A1 Color swatch click -> picker (RefCell swatch_zones cache + hit-test).
A2 :hov / .cls / + buttons (force pseudo cycle, class manager, add rule
   stub) + active highlight.
A3 var() chip click -> jump na :root rule (RefCell var_zones cache).
A4 Panel Left/Right dock content x-shift (local cmds buffer + flush
   s shift_cmd_x).

B1 Flex item diagram - basis (modry) + grow (zelena) bar + final size.
B2 Grid container info - grid-template columns/rows/areas + gap.

C1 TabManager (src/browser/render/tabs.rs) - Tab struct + TabManager
   (open/close/switch/next/prev) + 8 unit testu. Foundation pro multi-tab.
C2 History persistence (src/devtools/history.rs) - HistoryEntry +
   ~/.rwe/profiles/<active>/history.json + 4 unit testu.
   Bookmarks persistence (src/devtools/bookmarks.rs) + 3 unit testu.
C3 Animations timeline scrubber (track + playhead + tick markers).

Pred-tim phase (commited):
- Per-dock render geometry (Top/Bottom plne, Left/Right partial)
- Color picker popup (HSV slider, HEX/RGB labels)
- Inherited styles section ("Pododedeno z {tag}")
- Animations + Fonts sub-tabs full populace
- Computed sub-tab filter + per-prop color swatches
- Box model viz (Firefox nested rectangle)

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

---

## Session N+23: DevTools rework + DOM API Tier 1+2

### Devtools Edge/CEF model (D1-D6)

**D1 - Protocol crate** (`crates/devtools-proto/`, 8 testy)
- DevtoolsRequest / DevtoolsResponse / DevtoolsEvent / DevtoolsError typy
- Per-domain modules: dom / css / runtime / debugger / network / performance
- Method enum + error_codes

**D2 - Target adapter** (`crates/engine/src/embed/devtools_target.rs`, 12 testy)
- DevtoolsTarget: events buffer + breakpoint counter (stateless mimo to)
- `handle_request(&mut WebView, req) -> DevtoolsResponse` - per-domain dispatch
- DOM.getDocument/getAttributes/setAttributeValue/removeAttribute real
- DOM.querySelector/All real (cascade::matches_selector + DFS walk)
- CSS.getMatchedStyles real (walk stylesheets + match per node + serialize properties)
- Runtime.evaluate real (lexer + parser + interp.eval pres Stmt::Expr top-level)
- Debugger.setBreakpoint/resume real

**D3 - DevTools frontend** (`crates/devtools-frontend/`, 3 testy)
- Static HTML/CSS/JS resources jako &'static str pres include_str!
- INDEX_HTML (tab strip) + 5 panel HTMLs (Elements/Console/Sources/Network/Performance)
- THEME_CSS (dark Chrome-style) + CDP_JS (window.cdp.send/on/off + pollEvents)
- Panely v DOM od zacatku jako siblings, tab swap pres display style (setAttribute)

**D4 - Shell 2-WebView host** (shell/src/app.rs)
- D4a: F12 toggle, ShellApp.devtools: Option<WebView>, lazy init
- D4b: real split layout - present_split_external_to_swap_chain
  (top_view + bottom_view + ratio, viewport-based dual draw)
- D4c: input routing po y koord, point_in_devtools + devtools_y_offset
- D4d: splitter drag (point_on_splitter + drag MouseMove updatuje split_ratio)

**D5 - Inspector overlay** (shell + devtools-frontend)
- Ctrl+Shift+C toggle inspect_mode
- CursorMoved pick_node_at(layout_root, x, y) -> Option<usize ptr>
- overlay_painter callback paint 4 modre rect outline + polo-pruhledne pozadi
- LMB Press v inspect emit DOM.inspectNodeRequested CDP event + auto-open devtools
- elements.html listener selectne node v tree + scrollIntoView

**D6 - JS binding** (`__rwe_cdp_send_native` + `__rwe_cdp_poll_events`)
- D6a stub: native fns log + return ""
- D6b real: CdpChannel { req_queue, resp_queue } Rc<RefCell<VecDeque>>
- pump_cdp() v shell redraw - drain req_queue, dispatch via target,
  push response/events do resp_queue jako JSON
- cdp.js refactor: send() vrati pending Promise, response delivered pres pollEvents

### DOM API Tier 1 (8/8 hotove)

Vse v `crates/engine/src/interpreter/` + tests v `dom_tier1_tests.rs` (34 testu):

1. **element.style cached + setter persistence** - `Interpreter.style_cache:
   HashMap<usize, Weak<JsObject>>` per node, Object setter sync do node.style attr
2. **getBoundingClientRect** + getClientRects pres `Interpreter.layout_lookup:
   Option<Rc<dyn Fn(*const Node) -> Option<(f32,f32,f32,f32)>>>`
3. **window.getComputedStyle** pres `cascade_lookup` callback (HashMap props)
4. **offsetWidth/Height/Left/Top + clientW/H + scrollW/H** pres layout_lookup
5. **element.matches(selector) + closest(selector)** pres parse_selectors + cascade
6. **element.contains(other)** DFS subtree walk
7. **Event/CustomEvent/MouseEvent/KeyboardEvent constructors** v builtins
8. **window.addEventListener** real + `Interpreter::dispatch_window_event`

### Wire-up shell (webview.rs)

- `layout_rects: Rc<RefCell<HashMap<usize, (f32,f32,f32,f32)>>>` - node ptr -> rect
- `cascade_props: Rc<RefCell<HashMap<usize, HashMap<String, String>>>>` - node ptr -> styles
- Po render_via populate ze layout_root + style_map.
- Pri load_dom register interp.set_layout_lookup + set_cascade_lookup s Rc clones.
- Pri load_html dispatch DOMContentLoaded + load events.
- Pri resize() / set_scroll() dispatch resize / scroll events.

### DOM API Tier 2 (8/8 hotove, 21 testu)

- **insertBefore(newNode, refNode)** + DocumentFragment NodeKind variant
- **replaceChild(newNode, oldNode)** real
- **insertAdjacentElement(pos, el)** - beforebegin/afterbegin/beforeend/afterend
- **cloneNode(deep)** real recursive (kopiruje kind+attrs, listenery ne per spec)
- **removeEventListener real** s function identity (helper function_identity_eq
  v js_value_impl: User/Async/Generator pres (name, params.len(), Rc::ptr_eq(env)),
  Native pres Rc::ptr_eq)
- **document.activeElement** - Interpreter.focused_element: Rc<RefCell<Option<Rc<Node>>>>,
  focus() set, blur() clear (jen pri ptr_eq match), default fallback document.body
- **createDocumentFragment** real - new NodeKind::DocumentFragment, appendChild
  fragment-move semantics (presune children + clear fragment)
- **NodeKind::DocumentFragment** match arms doplneny ve 5 souborech (eval_member,
  debug_view/devtools, devtools/model/elements, embed/devtools_target, dom)

### Shell features (additional)

- D5 inspector overlay (Ctrl+Shift+C + hit-test + overlay_painter paint outline
  + click emit DOM.inspectNodeRequested + elements.html listener selectne node)
- Address bar Ctrl+L (stdout-only feedback, visual overlay TBD)
- D4d splitter drag (NS resize cursor + drag updatuje split_ratio)

### CDP target handlers (real, no stubs)

- DOM: getDocument / querySelector / querySelectorAll / getAttributes /
  setAttributeValue / removeAttribute
- CSS: getMatchedStyles / getComputedStyle / setPropertyText
- Runtime: evaluate (lexer + parser + interp.eval pres Stmt::Expr,
  unwrap WithLine)
- Debugger: setBreakpoint / removeBreakpoint / resume + 4 step stubs
- Network: getResponseBody stub (body cache TBD - vyzaduje fetch refactor)
- Performance: getMetrics real (Documents/Nodes/LayoutObjects/JSEventListeners)

### DOM API Tier 3 (3/3 hotove, 14 testu)

- **element.scrollIntoView(opts)** - pres layout_lookup posune scroll_pos.
  Default block=start, support center/end (heuristika 600/300 vh).
- **window.scrollTo/scrollBy/scroll + pageXOffset/pageYOffset/scrollX/scrollY** -
  scroll_pos Rc<RefCell<(f32,f32)>> field na Interpreter. JS modify pres
  scrollTo(x, y) nebo scrollTo({left, top}). Getter dynamic v eval_member.
- **element.focus()/blur() real** - dispatch focus/blur events pres
  dispatch_event. focus() pri prepnuti dispatchne blur na predchozim.

### DOM API Tier 4 (5/5 hotove, 26 testu)

- **DOMRect + toJSON()** - centralni helper make_dom_rect(x,y,w,h).
  Pouziva getBoundingClientRect + getClientRects.
- **DOMTokenList full** (classList): length, item(i), [0]/[1]/... indexed,
  replace(old,new), value getter/setter, Symbol.iterator (for-of + Array.from).
- **Array.from** rozsiren o Object iterable protocol + Array-like fallback.
- **MutationObserver** real dispatch z removeAttribute + setAttribute hooks.
- **IntersectionObserver/ResizeObserver** stub-level (API funguje, callback
  nikdy nefired - render-time check vyzaduje per-frame work).

### Wire-up scroll_pos (bidirectional sync)

WebView render_via dela:
1. Pre tick: check interp.scroll_pos. Pri zmene (JS scrollTo) apply do
   self.scroll_x/y + scroll_target.
2. Po smooth scroll tick: sync interp.scroll_pos = (scroll_x, scroll_y).
   JS pageXOffset/scrollX cte aktualni hodnotu.
3. set_scroll() take sync interp.scroll_pos.

### Shell features dokoncene

- Address bar Ctrl+L (stdout-only feedback)
- Find on page Ctrl+F (stdout-only, highlight TBD)

### Test counts (po N+23, Tier 1-4 done)

- 2804 engine, 8 devtools-proto, 3 devtools-frontend = 2815 testu
- 0 warnings, cargo build/test --workspace cisty
- 30+ commitov v session

### DOM API Tier 5 (8/8 hotove, 29 testu)

CSSOM + Shadow DOM + Selection/Range + scrollingElement.

- **Shadow DOM real** - attachShadow vraci ShadowRoot s DocumentFragment-based
  underlying DOM. Shadow_roots registry na Interpreter (host_ptr -> SR obj).
  ShadowRoot dispatch: appendChild/removeChild/querySelector/querySelectorAll/
  getElementById/contains real. Closed mode hide z host.shadowRoot. Double-
  attach throws NotSupportedError per spec.
- **document.scrollingElement** -> html_element (standard mode).
- **document.styleSheets real** s host wire-up:
  - Interpreter.stylesheets_lookup callback (Vec<sheet> kde sheet = Vec<rule>).
  - WebView.stylesheets_data Rc<RefCell> bridge - po load_html rebuild ze
    self.stylesheets do flat format.
  - StyleSheetList: length, item(i), indexed [0].
  - CSSStyleSheet: cssRules (CSSRuleList s length + item + indexed),
    insertRule/deleteRule stubs (vrati idx/undefined), replace/replaceSync
    Promise stubs (Constructable Stylesheets), href, disabled, type.
  - CSSRule: type=1 (STYLE_RULE), selectorText, cssText, style.
- **Selection API + Range API** existoval, pridan jen window.getSelection()
  mirror document.getSelection.
- **CSSStyleDeclaration full** - pridan length getter (__get_length__) +
  item(i) (vraci nazev i-te property). cssText getter/setter uz existoval.
- **document.fonts** - pridan forEach + addEventListener/removeEventListener
  stubs. status='loaded', size=0, ready=Promise.resolve, check, load OK.

### Test counts (po N+24, all 5 tiers done)

- 2833 engine, 8 devtools-proto, 3 devtools-frontend = 2844 testu
- 0 warnings, cargo build/test --workspace cisty
- 3 commit DOM Tier 5 (shadow + style + styleSheets)

### Network.getResponseBody body cache (N+24 final)

- Interpreter.response_bodies: Rc<RefCell<HashMap<String, String>>> field.
- drain_fetches po Ok outcome insertujeme body clone s URL klicem.
- CDP Network.getResponseBody lookup pres webview.interpreter().response_bodies
  klic = request_id (v nas modelu == URL).
- XHR body cache TBD.
