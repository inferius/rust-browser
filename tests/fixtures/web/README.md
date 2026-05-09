# Web layout fixtures

Reference layouty z Chrome/Firefox pro pixel-perfect compliance testing
oproti reálnym browserum.

## Workflow

1. **Otevri stranku** v Chrome (file:// nebo http://). Doporuc 1024x768
   viewport (devtools "Toggle device toolbar" -> Responsive).
2. **DevTools Console** (F12). Vyber tab "Console".
3. **Paste skript** `export_layout.js`. Stiskni Enter.
4. **JSON v clipboardu** - alert v console. Save jako
   `tests/fixtures/web/<name>.json`.
5. **Run test**:
   ```bash
   cargo test web_fixture_<name> -- --ignored --nocapture
   ```
6. **Verbose mismatchu**: `FIXTURE_VERBOSE=1 cargo test web_fixture_<name> -- --ignored --nocapture`

## JSON format

```json
{
  "url": "file:///.../engine-test.html",
  "viewport": { "width": 1024, "height": 768, "dpr": 1 },
  "html_source": "<full HTML>",
  "css_inline": "<all <style> tagy>",
  "tree": {
    "tag": "html", "id": "", "classes": [],
    "rect": { "x": 0, "y": 0, "w": 1024, "h": 4000 },
    "computed": { "color": "rgb(...)", ... },
    "children": [ ... ]
  }
}
```

## Pridani noveho fixture

V `src/browser/tests/web_fixtures.rs` pridej novy test:

```rust
#[test]
#[ignore]
fn web_fixture_NAME() {
    run_fixture("tests/fixtures/web/NAME.json", 5.0, 0.0);
}
```

`tolerance` = max px diff per axis (default 5px).
`min_pass` = required pass-rate fraction (0.0 = report only, 0.5 = 50% must match).

## Cilove pass-rate progress

| Fixture | Stav | Pass-rate | Cil |
|---------|------|-----------|-----|
| engine-test.json | TBD | - | 80% |
| simple-flex.json | TBD | - | 95% |
| simple-grid.json | TBD | - | 95% |

## Limitations

- Fonts: Chrome system font vs nase Inter atlas - text widths se lisi
  -> rect.height na text node muze diff > 5px. Zatim flow-based skip text nodes.
- Subpixel positioning: Chrome ma fractional pixels, my round na f32.
- Computed style values: `rgb(0, 0, 0)` vs `#000000` - serializace difference.
- Dynamic JS: po onLoad muze layout shift. Skript exporting po DOMContentLoaded
  by mel byt OK; pri SPA s lazy load capture pozdeji.
