# Prechodovy plan - nove vlakno

Cti **driv nez zacnes**. Plus `CLAUDE.md`, `README.md`, `TODO_CSS.md`.

## Stav

- Build: **OK**, 0 errors.
- Tests: **805 passed, 0 failed, 3 ignored**.
- Posledni commit: `eea6aa9 FontFace API + document.fonts`.
- Tree: ciste.
- Branch master, ~210 commitu pred origin/master.

## Test runner

```bash
powershell -ExecutionPolicy Bypass -File run_tests.ps1   # Win
./run_tests.sh                                            # Linux/Mac
```

## Kompletni stav: rendering / runtime / JS API

**Realne implementovane render/runtime** (ne jen parser):
- WGSL shader 9 modu: solid (0), text (1), linear gradient (2), shadow (3),
  image (4), inset shadow (5), radial grad (6), conic grad (7), blurred (8)
- Filter drop-shadow, filter blur (mode 8 smoothstep edge)
- 2D rotation (cos/sin matrix kolem centroid)
- 3D rotate aproximace (axis Z = 2D, X/Y = scale-based)
- Translate/Scale single + chain
- Counter API runtime (counter() v ::before/::after content)
- ::before/::after pseudo (s content/attr/counter)
- ::first-letter + ::first-line text split
- list-style-type marker (disc/circle/square/decimal/roman/alpha)
- text-decoration solid/double/dotted/dashed/wavy
- outline render (mimo border)
- Position: sticky (clamp dle parent)
- Anchor positioning (anchor-name -> rect, position-anchor lookup)
- Scroll-driven animations (animation-timeline: scroll())
- pointer-events: none hit-test skip
- Per-text font lookup (GlyphAtlas (family, char, size))
- Glyph + Image atlas rendering
- @font-face FS load + Font registry
- transition events dispatch (transitionend)
- animation events (animationstart/-end/-iteration)
- Form submit real POST (ureq)
- Multiple backgrounds tiling
- Multi-layer cascade (Layers / Pseudo / Containers)

**Parser-only** (nelze plne render bez wgpu RT pipeline):
- Filter blur 2-pass gaussian (RT setup hotov, pipeline+shader chybi)
- 3D perspective shader (matrix uniform per-vertex)
- Filter na cely subtree (RT capture)
- Polygon clip-path (stencil/SDF)
- Hue-rotate / saturate / contrast filtry (CPU u single-element OK,
  cely subtree TODO)
- WebGL real render (jen stub)

**JS API:**
- DOM kompletni (element/document/append/prepend/before/after/replaceWith/
  remove/insertAdjacentHTML/cloneNode/contains/getBoundingClientRect/...)
- Element.classList (add/remove/toggle/contains)
- Element.dataset
- Element.matches/closest
- HTMLAnchorElement url parts
- HTMLLabelElement.control + htmlFor
- HTMLOptionElement.text/label/defaultSelected
- HTMLSelectElement.options/selectedIndex/selectedOptions
- HTMLTableElement.rows + tr.cells
- HTMLDialogElement (show/showModal/close)
- HTMLDetailsElement.open
- HTMLMediaElement (play/pause/load/currentTime/...)
- HTMLInputElement (validity/select/setSelectionRange/...)
- HTMLTemplateElement.content
- HTMLElement.style (setProperty/getPropertyValue/removeProperty)
- form-controls.form / .labels / .form_data + submit() real POST
- innerHTML/outerHTML getter+setter
- namespaceURI/localName/prefix
- previousElement/Sibling, nextElement/Sibling
- childElementCount, firstElementChild, lastElementChild
- isConnected, ownerDocument

- Canvas 2D (getContext + fillRect/strokeRect/clearRect/fillText +
  beginPath/moveTo/lineTo/arc/closePath/stroke/fill)
- WebGL stub (constants + 40+ no-op methods)
- ResizeObserver/IntersectionObserver/MutationObserver/PerformanceObserver
- requestAnimationFrame/cancelAnimationFrame/queueMicrotask
- customElements (define/get/whenDefined/upgrade)
- new CSSStyleSheet()
- new URL() + new URLSearchParams()
- new Headers()
- new FormData() + new Blob()
- localStorage/sessionStorage
- navigator (userAgent/language/platform/clipboard/geolocation/...)
- TextEncoder/TextDecoder
- crypto (randomUUID/getRandomValues/subtle stubs)
- performance (now/timeOrigin/mark/measure)
- AbortController + AbortSignal
- history (pushState/replaceState/back/forward/state/length)
- WebSocket / EventSource / BroadcastChannel stuby
- IndexedDB (open/deleteDatabase/databases stubs)
- new FontFace(family, src) + document.fonts
- document.startViewTransition + 16+ document props

**CSS:**
- Selectors L4 vc :is/:where/:not/:has/~/nth-*/of-type/empty
- Form pseudo-classes (:required/:optional/:disabled/:enabled/:checked/
  :read-only/:read-write/:placeholder-shown/:valid/:invalid/:default)
- Color L4 (oklch/oklab/lab/lch/hsl/hwb/color-mix/modern syntax)
- Values L4 (min/max/clamp/env + math fci L4)
- Logical Properties L1
- Animations L1+L2 (fill-mode/play-state/cubic-bezier/steps/iteration)
- Nesting L1 (& selector)
- Container Queries L1 (cq* units)
- Cascade Layers @layer
- Box-shadow inset
- Radial+conic gradients
- Transitions L1 (parser + state diff + interpolace)
- Filter Effects parser + drop-shadow + CPU color matrix + blur shader
- Pseudo-Elements ::before/::after (s content + attr + counter)
- Backgrounds L3 (multi-layer + position/size/repeat/clip/origin)
- @font-face (FS runtime)
- SVG basic shapes (rect/circle/ellipse/line/text)
- Canvas tag default
- clip-path parser + CPU render (inset/circle/ellipse)
- text-shadow
- @media L4 (prefers-*/hover/pointer/range syntax)
- Math fci L4
- text-transform/aspect-ratio/text-decoration L4
- @scope/@supports/@starting-style/@page/@property/@import/@namespace/
  @counter-style/@font-feature-values/@document/@view-transition
- Anchor Positioning L1 (parser + runtime)
- Scroll-driven anims (parser + runtime)
- View Transitions parser
- Subgrid L2 (Display enum)
- Outline shorthand
- 100+ dalsi properties (font-stretch/-variant/-feature/-variation/
  ruby-position/quotes/mask-image/shape-outside/direction/writing-mode/
  content-visibility/contain-intrinsic-size/will-change/isolation/
  mix-blend-mode/pointer-events/user-select/caret-color/resize/
  touch-action/hyphens/tab-size/word-break/overflow-wrap/text-wrap/
  text-align-last/transform-style/perspective/backface-visibility/
  page-break-*/break-*/orphans/widows/counter-set/print-color-adjust/
  forced-color-adjust/math-style/math-depth/speak/speak-as/bookmark-*/
  string-set/object-fit/object-position/background-blend-mode/
  image-rendering/table-layout/border-collapse/border-spacing/
  caption-side/empty-cells/vertical-align/list-style-image/...)
- Display L3: Contents/ListItem/Table*/InlineFlex/InlineGrid/Subgrid/Ruby

## TODO zbyle

### Velke (RT/shader heavy)
1. Filter blur 2-pass gauss s offscreen RT (RT je vytvoren, jen pipeline+
   shader pass orchestration chybi)
2. Filter na cely subtree (RT capture)
3. 3D perspective shader (matrix uniform per-vertex)
4. Polygon clip-path (shader stencil)
5. WebGL real render
6. Hue-rotate/saturate/contrast filtry na cely subtree

### Mensi runtime
- direction: rtl runtime (text flow, text-align default)
- text-emphasis render
- ruby layout
- Subgrid real layout (taffy support)
- shape-outside real layout (text wrap)
- mask-image render
- Real custom elements upgrade callback

### TypeScript kompilator
**User pozadoval**: po kompletu prokonzultujeme.

## Pracovni flow

- Po fici: build + test (run_tests.ps1) + commit
- Commit cesky, ASCII
- Komunikace cesky CAVEMAN MODE
