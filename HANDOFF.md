# Prechodovy plan - nove vlakno

Toto cti **driv nez zacnes**. Plus `CLAUDE.md` v rootu.

## Stav

- Build: **OK** (`cargo build` proslo, 1 nepodstatny warning v `debug_view/devtools.rs:108` - `suspicious_double_ref_op` na sort_by_key).
- Posledni commit: `90cb0b6 Console + network log capture do DevTools + projekt CLAUDE.md`.
- Working tree: ciste.
- Branch master, 60 commitu pred origin/master (nepushovano - **nepushovat bez vyzvy uzivatele**).

## Co bylo zrovna hotovo

Console.log + fetch network capture do DevTools panelu:
- `Interpreter` ma `console_log: Rc<RefCell<Vec<(String, String)>>>` a `network_log: Rc<RefCell<Vec<(String, u16)>>>`.
- Builtins setup_builtins bere obe Rc jako parametry, console.log/warn/error/info/debug pisi do log, fetch pise (url, status) do network_log.
- main.rs `devtools` rezim: vytvori interpreter, nastavi document, spusti vsechny `<script>` taky, pak borrow().clone() logu a preda do `generate_devtools_html`.
- Funkcni overeni: `cargo run -- devtools` -> devtools.html ma Console panel s live entries z test.html `<script>` bloku.

## TODO (priorita shora dolu)

1. **CSS animation runtime application** (next)
   - @keyframes uz parsuju (`browser/css_parser.rs`) + interpoluji (existujici `animation_tick`).
   - Co zbyva: kdyz element ma `animation: name 2s linear infinite`, behem render frame:
     a) Vypocti progress = (now - start_time) % duration / duration
     b) Najdi `@keyframes name`, interpoluj mezi from/to dle progress
     c) Aplikuj na ComputedStyle pred paint
   - Time source: `App.start_time` v `browser/render.rs` uz existuje.
   - Trigger redraw kazdy frame pokud aspon jeden element animuje (`window.request_redraw()`).

2. **GPU image rendering** - image cache (RGBA bytes) uz se nacita v paint.rs, ale render.rs nevykresluje. Bud per-image bind group (jednoduche, slow), nebo RGBA atlas (rychle, slozite). **Zeptat se uzivatele A/B.**

3. **Radial + conic gradient** - linear funguje (gradient mode v shaderu). Pridat radial (`radial-gradient(circle at center, ...)`) a conic - dalsi shader mode + paint emit.

4. **Canvas API** - `<canvas>` element + getContext('2d') vrati objekt s metodami: fillRect, fillText, beginPath, moveTo, lineTo, stroke, fill, arc. Vlastni display list per-canvas, pak texture do GPU.

5. **@font-face** - parser `@font-face { font-family: X; src: url(...) }`, fetch font binary, pridat do fontdue Font registry.

6. **SVG support** - basic shapes (rect, circle, path) -> display list.

7. **Box-shadow inset, clip-path, WebGL, form submit** - lower priority.

8. **Drobnost**: fixnout warning v `debug_view/devtools.rs:108` pri nejblizsim doteku - `entries.sort_by_key(|(k, _)| k.clone())` na `&&String` je suspicious double ref. Zmenit na `|(k, _)| (*k).clone()` nebo restrukturovat.

## Pracovni flow (uzivatel ocekava)

- Po kazde fici: **build + test + commit**.
- Commit message cesky, ASCII, strucny popis "co + proc".
- Pred psanim kodu pri nejasnosti se ptat (A/B/C varianty).
- Komunikace cesky, CAVEMAN MODE aktivni (terse), kod normalne.

## Klicove soubory pro orientaci

- `src/main.rs` - CLI rezimy, dobry vstupni bod.
- `src/interpreter/mod.rs` - Interpreter struct, JsValue, run().
- `src/interpreter/builtins.rs` - vsechny globalni builtins (velky soubor).
- `src/browser/render.rs` - winit + wgpu, App struct, frame loop.
- `src/browser/paint.rs` - DisplayList emission z LayoutBox.
- `src/browser/layout.rs` - box model + taffy + inline.
- `src/browser/cascade.rs` - selector matching + specificity.
- `src/browser/css_parser.rs` - CSS -> StyleSheet.
- `static/test.html` + `static/test.css` - hlavni testovaci stranka.

## Co necist hned (velke soubory)

- `src/interpreter/builtins.rs` (>2000 lines) - cti az kdyz potrebujes konkretni builtin.
- `src/debug_view/devtools.rs` (>500 lines) - cti az kdyz upravujes DevTools panel.
- `src/interpreter/mod.rs` - velky, cti po sekcich.

## Dalsi krok pri pokracovani

Uzivatel pravdepodobne rekne "pokracuj". Default volba: **CSS animation runtime application** (bod 1). Pokud nejsi jisty kterou cestu (image rendering ma A/B), zeptat se.
