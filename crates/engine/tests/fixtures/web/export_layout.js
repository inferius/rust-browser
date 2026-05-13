/**
 * Web layout exporter pro RustWebEngine fixtures.
 *
 * Vyexportuje DOM tree + per-element layout (x/y/w/h) + computed styles
 * z aktivni stranky. Vystup = JSON string co se vlozi do clipboard.
 *
 * USAGE:
 *   1. Otevri stranku v Chrome (file:// nebo http://).
 *   2. Otevri DevTools Console (F12).
 *   3. Paste cely tento skript + Enter.
 *   4. Skript copy JSON do clipboardu (alert info).
 *   5. Save jako tests/fixtures/web/<name>.json.
 *   6. Spusti `cargo test web_fixture_<name> -- --nocapture` v Rust.
 *
 * Output JSON format:
 * {
 *   "url": "...",
 *   "viewport": { "width": N, "height": N, "dpr": N },
 *   "html_source": "<full HTML>",
 *   "css_inline": "<all <style> tagy concat>",
 *   "tree": {
 *     "tag": "html", "id": "", "classes": [], "text": "",
 *     "rect": { "x": 0, "y": 0, "w": 1024, "h": 4000 },
 *     "computed": { "color": "rgb(...)", "background-color": "...", ... },
 *     "children": [ ... ]
 *   }
 * }
 */
(function () {
    'use strict';

    // Properties ktere chceme zaznamenat (vsechny relevantni pro layout/visual).
    const TRACKED_PROPS = [
        'display', 'position', 'float', 'clear',
        'top', 'left', 'right', 'bottom',
        'width', 'height', 'min-width', 'min-height', 'max-width', 'max-height',
        'margin-top', 'margin-right', 'margin-bottom', 'margin-left',
        'padding-top', 'padding-right', 'padding-bottom', 'padding-left',
        'border-top-width', 'border-right-width', 'border-bottom-width', 'border-left-width',
        'border-top-style', 'border-top-color',
        'box-sizing',
        'flex-direction', 'flex-wrap', 'flex-grow', 'flex-shrink', 'flex-basis',
        'align-items', 'align-self', 'align-content', 'justify-content',
        'gap', 'row-gap', 'column-gap',
        'grid-template-columns', 'grid-template-rows', 'grid-template-areas',
        'grid-column', 'grid-row', 'grid-area', 'grid-auto-flow',
        'column-count', 'column-rule-width',
        'background-color', 'color', 'opacity',
        'font-family', 'font-size', 'font-weight', 'line-height',
        'text-align', 'text-decoration', 'text-transform',
        'overflow-x', 'overflow-y',
        'z-index', 'transform', 'transform-origin',
        'border-radius',
        'visibility', 'pointer-events',
        'object-fit', 'object-position',
        'writing-mode', 'direction',
    ];

    function nodeRect(node) {
        if (!(node instanceof Element)) {
            return { x: 0, y: 0, w: 0, h: 0 };
        }
        const r = node.getBoundingClientRect();
        return {
            x: Math.round(r.left + window.scrollX),
            y: Math.round(r.top + window.scrollY),
            w: Math.round(r.width),
            h: Math.round(r.height),
        };
    }

    function computedFor(node) {
        if (!(node instanceof Element)) return {};
        const cs = window.getComputedStyle(node);
        const out = {};
        for (const p of TRACKED_PROPS) {
            const v = cs.getPropertyValue(p);
            if (v && v.length > 0) out[p] = v;
        }
        return out;
    }

    function captureNode(node, depth) {
        if (depth > 50) return null; // safety
        if (node.nodeType === Node.TEXT_NODE) {
            const t = (node.textContent || '').trim();
            if (!t) return null;
            return { tag: '#text', text: t, rect: { x: 0, y: 0, w: 0, h: 0 } };
        }
        if (node.nodeType !== Node.ELEMENT_NODE) return null;
        // Skip script/style obsah (mame v css_inline / source).
        const tag = node.tagName.toLowerCase();
        if (tag === 'script' || tag === 'style' || tag === 'link' || tag === 'meta') return null;
        const obj = {
            tag,
            id: node.id || '',
            classes: Array.from(node.classList || []),
            rect: nodeRect(node),
            computed: computedFor(node),
            children: [],
        };
        const childNodes = node.childNodes;
        for (let i = 0; i < childNodes.length; i++) {
            const c = captureNode(childNodes[i], depth + 1);
            if (c) obj.children.push(c);
        }
        return obj;
    }

    function captureCssInline() {
        const styles = document.querySelectorAll('style');
        let out = '';
        styles.forEach((s) => { out += s.textContent + '\n'; });
        return out;
    }

    const root = document.documentElement;
    const data = {
        url: location.href,
        viewport: {
            width: window.innerWidth,
            height: window.innerHeight,
            dpr: window.devicePixelRatio || 1,
        },
        ua: navigator.userAgent,
        timestamp: Date.now(),
        html_source: '<!DOCTYPE html>\n' + document.documentElement.outerHTML,
        css_inline: captureCssInline(),
        tree: captureNode(root, 0),
    };
    const json = JSON.stringify(data, null, 2);
    // Copy to clipboard.
    if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(json).then(() => {
            console.log('[export] JSON v clipboardu (' + json.length + ' znaku). Save jako tests/fixtures/web/<name>.json');
        }, (err) => {
            console.error('[export] clipboard fail, output v console.', err);
            console.log(json);
        });
    } else {
        console.log(json);
        console.log('[export] clipboard not supported - copy z console output rucne');
    }
    // Take vrat objekt (pro inspekci v Console).
    return data;
})();
