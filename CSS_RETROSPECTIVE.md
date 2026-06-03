# CSS rendering pipeline - retrospektiva

Analyza naseho CSS pipelinu + porovnani s Mozilla Stylo (Servo/Firefox) + Chromium Blink.

## Nas pipeline (12,758 LOC napric 4 souborech)

```
HTML zdroj
   |
   v
[parse_html] html5ever -> DOM tree (Rc<RefCell<Node>>)
   |
   v
[css_parser] cssparser -> Vec<StyleSheet { rules }>
   |                          (1429 LOC)
   v
[cascade::cascade]  Pro kazdy DOM node:
   |   1. Match selector pres `selectors` crate
   |   2. Sort by specificity + source order
   |   3. Aplikuj declarations -> StyleMap[node_id] = HashMap<prop, value>
   |                          (4013 LOC)
   v
[layout::build_box]  StyleMap -> LayoutBox tree
   |   1. apply_tag_html_attrs (img src, input value, ...)
   |   2. resolve CSS lengths (px/em/rem/%/vw/vh)
   |   3. layout_dispatch dle Display:
   |      - Block -> layout_block (vertical stack + inline runs)
   |      - Flex -> layout_engine::flex
   |      - Grid -> layout_engine::grid
   |      - Table -> prelayout_table_columns + layout_block
   |   4. inline runs: flush_inline (word wrap, line boxes)
   |                          (4991 LOC)
   v
[paint::build_display_list]  LayoutBox -> Vec<DisplayCommand>
   |   1. Bg color/gradient
   |   2. Border + radius
   |   3. Box-shadow (outer + inset)
   |   4. Text glyphs (s atlas key)
   |   5. Image src + object-fit/position
   |   6. Filter, clip-path, transform
   |                          (2325 LOC)
   v
[render]  DisplayCommand -> wgpu vertex buffer -> GPU draw
       Per-layer texture pri RWE_LAYER_GPU (default on)
```

---

## Porovnani s Mozilla Stylo (Firefox)

### Stylo (Servo style system)
**Cesty:** `firefox/servo/components/style/` (bundled v FF mainline)

**Klicove abstrakce:**
- `ComputedValues` - immutable per-element computed style (Arc<>)
- `Stylist` - rule tree + selector matching engine
- `RuleNode` - hash-consed declaration blocks (max sharing)
- `Bloom filter` - pro descendant selector culling
- `RuleTree` - shareable per-element rule chain
- `ParallelTraversal` - Rayon-based parallel cascade

**Nase ekvivalenty:**
- ComputedValues -> `StyleMap[node_id]` HashMap (mutable, no sharing)
- Stylist -> `cascade::cascade(root, stylesheets)` (single-thread, linear)
- RuleNode sharing -> NEMAME (kazdy node ma vlastni HashMap copy)
- Bloom filter -> NEMAME (linear walk pres selectors)
- Parallel traversal -> NEMAME (sekvencni)

**Gap analyza:**

| Feature | Stylo | Nas | Ztraty |
|---------|-------|-----|--------|
| Rule sharing | Hash-cons, ~80% deduplication | None | Memory waste pri large pages |
| Bloom filter | 12-bit filter per element | None | O(N*M) selector match misto O(N*log M) |
| Parallel cascade | Rayon work-stealing | Single-thread | ~4x slower na multi-core |
| Restyle dampening | RestyleHint::for_animation_only | Always full restyle | Animacni paint stuttery |
| Style sharing cache | Lookup po LRU | None | Identical descendants restyled |

**Inspirace pro nas:**
1. **Rule sharing** - kdyz dva elementy maji stejny rule chain (e.g. div.box * N), share ComputedValues Arc<>. Velke pages s opakujicimi se elementy = velka uspora pameti.
2. **Bloom filter** - per-element 12-bit filter z ancestor class/id/tag. Selector matching skip descendant selectors kdyz bloom misses.
3. **Parallel traversal** - Rayon par_iter pres top-level children. Each subtree cascade independently.

---

## Porovnani s Chromium Blink

### Blink (style engine)
**Cesty:** `chromium/third_party/blink/renderer/core/css/`

**Klicove abstrakce:**
- `ComputedStyle` - per-element immutable computed style (RefCounted)
- `StyleResolver` - main entry pro cascade resolution
- `MatchedPropertiesCache` - dedup matching rules across elements
- `RuleSet` - bucketed rules po class/id/tag (= fast hash lookup misto linear)
- `StyleRecalcRoot` - smallest subtree co potrebuje recalc
- `StyleSharingCandidateSelector` - find sibling with shareable style

**Layout:**
- `LayoutObject` tree (parallel k DOM)
- `LayoutBox` (similar nas LayoutBox)
- `LayoutNG` (next-gen layout) - immutable layout tree, fragment-based
- `LineBox` / `RootInlineBox` - inline layout

**Paint:**
- `PaintLayer` - z-stack + transform + opacity boundary
- `PaintArtifact` - immutable display list
- `DisplayItemClient` - per-element paint cache key
- `PaintPropertyTree` - 4 trees: transform, clip, effect, scroll

**Nase ekvivalenty:**

| Blink koncept | Nas ekvivalent | Gap |
|---------------|---------------|-----|
| ComputedStyle (refcounted) | StyleMap HashMap (cloned) | No refcount sharing |
| StyleResolver | cascade::cascade fn | OK |
| MatchedPropertiesCache | None | Per-node rule match opakovan |
| RuleSet bucketing | None - linear iter | Slow selector match |
| StyleRecalcRoot | None - cascade always full | No partial restyle |
| LayoutObject | LayoutBox (similar) | OK structure |
| LayoutNG fragments | None - mutable LayoutBox | Re-layout mutates tree |
| PaintLayer | compositor::LayerNode | OK ekvivalent |
| PaintPropertyTree | None | Transform/clip ad-hoc per box |

**Inspirace:**

1. **RuleSet bucketing** - Pri parse stylesheet rozdel rules do bucketu:
   - by_id: HashMap<String, Vec<Rule>>
   - by_class: HashMap<String, Vec<Rule>>
   - by_tag: HashMap<String, Vec<Rule>>
   - universal_or_complex: Vec<Rule>
   Pri cascade: pro kazdy node lookup `by_id[node.id]` + per kazdou class `by_class[c]` + `by_tag[tag]`. Misto linear walku pres 1000 rules zkontroluje jen relevantni (10-20).

2. **MatchedPropertiesCache** - Dvourovou cache:
   - Pro kazdy node spocti hash matched_rules (set jen Rule pointers).
   - Lookup cache[hash] -> Arc<ComputedStyle>.
   - Sibling s identickym hash = same style.
   Pri 1000 list items s identickymi class atributem = 1 cascade misto 1000.

3. **PaintPropertyTree** - 4 paralelne stromy nad LayoutBox:
   - Transform tree: per-element transform matrix
   - Clip tree: per-element clip rect
   - Effect tree: opacity + blend + filter
   - Scroll tree: scroll containers
   Bez tohohle ad-hoc transform shift pri Position::Relative slozite + bugs.

4. **LayoutNG fragment-based** - Misto mutable LayoutBox tree, immutable Fragment tree. Re-layout produces NEW tree, prev tree zustava. Outcome: layout cache je trivial (compare fragment trees), animace na compositor.

---

## Nase silne stranky vs Stylo + Blink

1. **Idiomatic Rust** - Arc<>/Rc<>/RefCell hierarchy bez raw pointer Pain bodu Blink.
2. **Smaller codebase** - 12k LOC vs miliony LOC Blink CSS. Snadnejsi refactor.
3. **Single-language** - bez C++/Rust FFI boundary jako Servo->Stylo bridge.
4. **wgpu backend** - WebGPU/Vulkan/Metal cross-platform, nez Skia.

## Nase slabe stranky

1. **No parallel cascade** - StyleMap hashmap insert sekvencni. Big pages 50ms+ na cascade.
2. **No rule sharing** - kazdy node duplicit hashmap. 1000 list items = 1000 copies same style.
3. **No bucketed selector match** - linear walk pres vsechny rules. O(N*M).
4. **No partial restyle** - kazda DOM mutace = full re-cascade root.
5. **Mutable LayoutBox** - layout mutates tree. Animace nutne re-layout misto compositor-only.
6. **Inline runs ad-hoc** - flush_inline kreten samostatne pres word-wrap. Bez LineBox abstrakce.

---

## Doporuceni - top 5 priority pro CSS pipeline

### 1. RuleSet bucketing (high ROI, medium effort)
Reorganizuj `cascade::cascade` interni state:
```rust
struct RuleSet {
    by_id: HashMap<String, Vec<RuleRef>>,
    by_class: HashMap<String, Vec<RuleRef>>,
    by_tag: HashMap<String, Vec<RuleRef>>,
    universal: Vec<RuleRef>,
}
```
Pri stylesheet parse: vyhodnoceni selektoru s leading id/class/tag -> insert do prislusneho bucketu. Pri match: lookup bucketu, skip rules NEpasujici prefix.

Ocekavany speedup: 5-20x cascade na vetsich strankach (1000+ rules).

### 2. MatchedPropertiesCache (high ROI, low effort)
Pri cascade, pred resolution:
- Hash sorted set of `RuleRef` ktere match node.
- `cache[hash] -> Arc<ComputedStyle>` lookup.
- Hit = reuse Arc, miss = compute + insert.

Memory tradeoff: hashmap roste s unique combinations. Use LRU evict.

Ocekavany speedup: na pages s opakujicimi se patterny (table cells, list items) 10-100x cascade.

### 3. Parallel cascade pres Rayon (high ROI, medium effort)
Top-level children jsou nezavisle. Rayon `par_iter` pres root.children -> cascade kazdy subtree.

Catch: `style_map` shared mutable -> Mutex<HashMap> bottleneck. Misto: per-thread local HashMap, merge na konci.

Ocekavany speedup: ~num_cores x na multi-core (Linux 4-8x).

### 4. PaintPropertyTree (medium ROI, high effort)
Refactor: misto per-LayoutBox transform field, vystav stromy nad LayoutBox:
```rust
struct PaintPropertyTrees {
    transform: Vec<TransformNode>,
    clip: Vec<ClipNode>,
    effect: Vec<EffectNode>,
    scroll: Vec<ScrollNode>,
}
```
LayoutBox drzi indices do techto stromu. Render walks property trees + LayoutBox tree paralelne.

Win: animace transform/opacity = jen update jednoho TransformNode + composite. Bez re-layout. Bez re-paint.

### 5. Style sharing cache (medium ROI, low effort)
LRU cache: `(parent_hash, sibling_hash, attrs_hash) -> ComputedStyle Arc`.
Pri cascade: zkus reuse z cache. Pri miss compute + insert.

Diff od MatchedPropertiesCache: tady klic = strukturalni (parent + sibling + attrs), tam = matched rules.

Ocekavany speedup: 5-30% pages s repetitive markup.

---

## Sekundarni priority

### CSS Containment (`contain: layout/paint/strict`)
Element s `contain: layout` = layout subtree NE leaks rodici. Recursive layout fence.
Stylo + Blink to maji. Implementace = check `bx.contain` v dispatch + skip parent invalidation.

### Mutation observers wire do cascade
DOM mutace ted = re-cascade root. Real impl: track minimal subtree co need recalc.
StyleRecalcRoot v Blink, RestyleHint v Stylo.

### Style precomputed background-color hash
Pres pages s tisicovkami inline-styled elementy, parse `style="color:..."` per element je hot path.
Cache parsed declarations po style-attribute string.

### Lazy font shaping
Nas inline flush_inline reshape kazdy text run per relayout. Cache by hash(text, font_family, size, weight).

### CSS Custom Properties (`--var`) inheritance optim
Naivni: kazdy descendant resolved separately. Stylo: per-rule-tree node cache custom prop resolution.

### Animation timeline integration
Existing animation system tickne kazdy frame, recompute interpolated style. Lepsi: KeyframeEffect per element, animation_origin per timeline, sample on-paint.

---

## Architektura: layout tree vs render tree

**Blink:** DOM tree -> LayoutObject tree (1:1 mostly) -> PaintLayer tree (sparse, only stacking contexts).
**Stylo:** DOM tree -> ComputedStyle + Fragment tree (LayoutNG-style).
**Nas:** DOM tree -> LayoutBox tree (1:1) -> DisplayCommand list (flat).

**Vyhoda flat DisplayCommand:** GPU dispatch direct.
**Nevyhoda:** No incremental update. Cele list rebuild pri kazde zmene.

WebRender (gfx/wr/) ma Picture tree (= layer tree) + tile cache. Bez Picture = no compositor caching. **My MAME** picture-like layer cache pres `extract_layer_tree` + `layer_paint_cache` per-layer DisplayCommand store + `D4 GPU mode` per-layer texture.

---

## Performance benchmark targets

| Operation | Aktualne | Po RuleSet bucketing | Po MatchedPropertiesCache | Po Parallel |
|-----------|----------|----------------------|--------------------------|-------------|
| Cascade 100 elements | ~5ms | ~1ms | ~0.2ms | ~0.05ms |
| Cascade 1000 elements | ~50ms | ~10ms | ~2ms | ~0.5ms |
| Cascade 10000 elements | ~500ms | ~100ms | ~20ms | ~5ms |
| Layout 100 elements | ~3ms | - | - | ~3ms |
| Paint 100 elements | ~2ms | - | - | ~2ms |

Cascade je dominantni. Bucketing + cache = top priority.

---

## Reference

- Stylo paper: https://hsivonen.fi/stylo/ - ENG paper, design rationale.
- Blink style: https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/core/css/
- WebRender: https://github.com/servo/webrender + `firefox/gfx/wr/` lokalni.
- LayoutNG explainer: https://docs.google.com/document/d/1uxbDh4uONFQOiGuiumlJBLGgO4KDWB8ZEkp7Rd47fw4/
- Stylo arch: https://hacks.mozilla.org/2017/08/inside-a-super-fast-css-engine-quantum-css-aka-stylo/

## Implementace TODO

Po teto retrospektive logicke navazne tasky:
- [ ] RuleSet bucketing v cascade.rs (~500 LOC change)
- [ ] MatchedPropertiesCache LRU s hash(matched_rules) (~200 LOC)
- [ ] Rayon par_iter cascade s per-thread style_map merge (~300 LOC)
- [ ] PaintPropertyTree refactor (1500+ LOC)
- [ ] Style sharing cache LRU (~150 LOC)

Total estimate: 2-3 dalsi sessions pro top 3.
