// Spust v Chrome console (F12 -> Console) na cilove strance.
// Vystup ve stejnem formatu jako nas Ctrl+Shift+D - lze diffnout.
//
// Pouziti:
//   1. otevri stranku v Chrome
//   2. F12 -> Console
//   3. Paste cely tento soubor + Enter
//   4. Pravy-klik na output v consoli -> "Save as..." -> chrome-dump.txt
//   5. Posli mi soubor
//
// Format kazdy element:
//   <tag id="..." class="..."> rect=(x,y wxh) display=... ew=Some(N)|None mw="..."
//
// Body height nedosadi `<html>` na 0 - Chrome merit cely document content.

(function dump() {
    function fmtRect(r) {
        return `(${Math.round(r.x)},${Math.round(r.y)} ${Math.round(r.width)}x${Math.round(r.height)})`;
    }
    function fmtDisplay(s) {
        // Map browser computed display strings na nas format.
        const map = {
            'block': 'Block', 'inline': 'Inline', 'flex': 'Flex',
            'grid': 'Grid', 'inline-flex': 'InlineFlex', 'inline-grid': 'InlineGrid',
            'inline-block': 'InlineBlock', 'none': 'None',
            'list-item': 'ListItem', 'table': 'Table', 'table-row': 'TableRow',
            'table-cell': 'TableCell', 'table-caption': 'TableCaption',
            'table-header-group': 'TableHeader',
        };
        return map[s] || s;
    }
    const lines = [];
    function walk(el, depth) {
        if (el.nodeType !== 1) return;  // jen Element nodes
        const r = el.getBoundingClientRect();
        const cs = getComputedStyle(el);
        const tag = el.tagName.toLowerCase();
        const id = el.id || '';
        const cls = el.className && typeof el.className === 'string' ? el.className : '';
        const display = fmtDisplay(cs.display);
        // explicit_width - z computed width pokud neni 'auto'
        const cw = cs.width;
        let ew = 'None';
        if (cw && cw !== 'auto' && cw.endsWith('px')) {
            const n = parseFloat(cw);
            if (!isNaN(n)) ew = `Some(${n})`;
        }
        const mw = cs.maxWidth || '';
        const indent = '  '.repeat(depth);
        lines.push(`${indent}<${tag} id="${id}" class="${cls}"> rect=${fmtRect(r)} display=${display} ew=${ew} mw="${mw}"`);
        for (let i = 0; i < el.children.length; i++) walk(el.children[i], depth + 1);
    }
    walk(document.documentElement, 0);
    const out = lines.join('\n');
    console.log(out);
    console.log(`\n[chrome-dump] ${lines.length} elements`);
    // Pokud chce user copy/paste, vraci string.
    return out;
})();
