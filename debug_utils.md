# Debug utils

Globalni helpery pro krokovani konkretnich elementu pri debug (`src/debug_bp.rs`).

## Princip

Vykreslovaci pipeline (layout / paint / cascade) prochazi tisice elementu per frame.
Stop-the-world breakpoint je zbytecny - chceme zastavit jen na `<img class="photo-box">`
nebo `<a id="logo">`. Tento modul resi tri cesty.

## Cesta A - Env var filter + IDE BP na sink fn

Nejjednodussi. Nastav env vars pred run, BP v IDE drz na prazdny `breakpoint_*()` fn.
Sink fn se vola jen kdyz match -> debugger stop jen na danem elementu.

### Env vars

| Var | Vyznam | Format |
|-----|--------|--------|
| `BP_TAG` | Match na tag name | `img` nebo `img,div` (OR) |
| `BP_ID` | Match na `id` atribut | `photo-box` nebo `foo,bar` (OR) |
| `BP_CLASS` | Match na class token | `card` (matche tridu v multi-token class) |

Vsechny tri kombinatelne (AND): `BP_TAG=img BP_CLASS=photo-box` = img s class photo-box.
Prazdny env = wildcard pro to kriterium. Vsechny prazdne = filter off, no-op overhead.

### IDE setup (RustRover)

1. Run/Debug Configurations -> Edit configurations -> Environment variables: `BP_ID=photo-box`
2. Otevri `src/debug_bp.rs`, BP na fn dle stage:
   - `breakpoint_build` - element-creation v `build_box_inner`
   - `breakpoint_layout` - img sizing v `flush_inline`
   - `breakpoint_paint` - paint walk v `paint_box`
   - `breakpoint_cascade` - cascade match v `cascade()`
   - `breakpoint_hit` - generic (`bp_here!` macro)
3. Debug -> stop pada jen na elementech matching filtru.

### CLI run

```bash
BP_TAG=img cargo run -- browser static/test.html
BP_ID=photo-box cargo run ...
BP_CLASS=card cargo run ...
BP_ID=foo,bar BP_CLASS=card cargo run ...   # multi-value OR
```

### Wired call sites (out-of-the-box)

| Soubor | Stage | Sink |
|--------|-------|------|
| `src/browser/layout/mod.rs:build_box_inner` | build_box | `breakpoint_build` |
| `src/browser/layout/mod.rs:flush_inline` img branch | layout (img sizing) | `breakpoint_layout` |
| `src/browser/paint.rs:paint_box` | paint walk | `breakpoint_paint` |
| `src/browser/cascade.rs:cascade()` walk | cascade match | `breakpoint_cascade` |

Pro pridani noveho call site:

```rust
crate::bp_here!(tag, id, class);   // generic
crate::bp_layout!(tag, id, class); // per-stage
```

Macros expanduji na fast-path no-op pokud filter prazdny.

## Cesta B - Conditional breakpoint na konkretni line

Pouzij kdyz chces stopnout uvnitr existujici fn na presne line bez env vars.

1. Klik gutter na zvolene line.
2. Right-click BP -> "More" / "Edit" -> Condition.
3. Vlozit predicate, napr.:

```rust
crate::debug_bp::lb_is_id(&bx_clone, "photo-box")
```

Promenna v expression musi byt v scope kde BP sedi. Predicates jsou `#[inline(never)]`
aby optimizer neztratil symbol.

### Predicates pro LayoutBox

```rust
debug_bp::lb_is_id(bx, "photo-box")           // bx.node.id == "photo-box"
debug_bp::lb_is_class(bx, "card")             // any class token == "card"
debug_bp::lb_is_tag(bx, "img")                // bx.tag == "img"
debug_bp::lb_match(bx, "img", "", "card")     // generic AND, empty str = ignore
```

### Predicates pro Node (build_box stage)

```rust
debug_bp::node_is_id(node, "photo-box")
debug_bp::node_is_class(node, "card")
debug_bp::node_is_tag(node, "img")
```

### Generic env-based

```rust
debug_bp::should_break("img", "photo-box", "")   // re-uses BP_* env vars
```

## Cesta C - Active trap (proces sam halti)

Bez ICE konfigurace BP. Vlozis call do kodu, proces vyhodi SIGTRAP, debugger ho zachyti.

```rust
debug_bp::debug_break();                            // raw int3
debug_bp::break_if("img", "photo-box", "");        // trap jen kdyz match env filter
```

Po debugu radek smaz. **Bez debuggeru attached SIGTRAP = abort/crash.**

Architektura:
- x86/x86_64 -> `int3` inline asm
- aarch64 -> `brk #0`
- ostatni -> `std::process::abort()`

## Sink funkce - kde sedi BP

`#[inline(never)]` empty fns v `src/debug_bp.rs`:

```rust
pub fn breakpoint_hit()       // generic
pub fn breakpoint_layout()    // layout stage
pub fn breakpoint_paint()     // paint stage
pub fn breakpoint_cascade()   // cascade stage
pub fn breakpoint_build()     // build_box stage
```

Telo prazdne (`black_box(())`). Optimizer je nesmi inlinout = release build je drzi
samostatne, BP funguje + i v profile-guided release.

## Macros

```rust
bp_here!(tag, id, class)     // -> breakpoint_hit()
bp_layout!(tag, id, class)   // -> breakpoint_layout()
bp_paint!(tag, id, class)    // -> breakpoint_paint()
bp_cascade!(tag, id, class)  // -> breakpoint_cascade()
bp_build!(tag, id, class)    // -> breakpoint_build()
```

Vsechny dely fast-path: `if bp_enabled() && bp_match(...) { breakpoint_*() }`.

## Filter API (raw)

```rust
debug_bp::bp_enabled() -> bool                              // any filter set?
debug_bp::bp_match(tag: &str, id: &str, class: &str) -> bool
```

`bp_enabled` cachuje parsed env vars pres `OnceLock` - parse 1x per proces, kazdy
call zdarma.

## Doporuceny workflow

1. Pri novem bug-investigation: spust s `BP_*` env + BP na sink fn -> stop na
   target elementu, projdi state v IDE inspector.
2. Pokud bug zavisi na konkretni line (napr. branch X vs Y), nahod conditional BP
   s `lb_is_*` predicate - presnejsi nez sink fn.
3. Pokud chces aktivne trap-nout bez IDE konfigurace, `break_if()` inline.
4. Kdyz user reportuje bug s konkretnim selectorem (`.photo-box`, `#logo`), vzdy
   nejdriv `BP_CLASS=photo-box` debug pred pokus-omyl edits v kodu.

## Co NE-resit pres tento modul

- Crash bugs - tam staci `dbg!` + panic backtrace.
- Performance bottlenecks - to je flamegraph, ne BP.
- Frame-level animace - `paint_animations` ma vlastni log path.

## Test

```bash
cargo test --bin RustWebEngine debug_bp
```

2 unit testy pro `bp_match` logiku.
