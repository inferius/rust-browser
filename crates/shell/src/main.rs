// rwe-shell bin. Shell crate ma vlastni run_window ktery vlastni Window +
// Renderer + WebView a kompozituje pres engine API (`render_via` ->
// `present_external_to_swap_chain`).
//
// Chrome bar (tabs/addr/find/bookmarks) zatim chybi - Phase 99 sem presune
// z engine App. Pro plnohodnotny chrome experience pouzij prozatim:
//
//     cargo run -p rwe-engine -- browser [src.html]
//
// rwe-shell jede JEN novy WebView pipeline (Edge/CEF model).

use std::path::PathBuf;

/// Extract <style>...</style> bloky z HTML + spoj do jednoho CSS textu.
fn extract_inline_styles(html: &str) -> String {
    let lc = html.to_ascii_lowercase();
    let mut out = String::new();
    let mut cursor = 0;
    while cursor < lc.len() {
        let open = match lc[cursor..].find("<style") {
            Some(p) => cursor + p, None => break,
        };
        let after_tag = match lc[open..].find('>') {
            Some(p) => open + p + 1, None => break,
        };
        let close = match lc[after_tag..].find("</style>") {
            Some(p) => after_tag + p, None => break,
        };
        out.push_str(&html[after_tag..close]);
        out.push('\n');
        cursor = close + "</style>".len();
    }
    out
}

fn main() {
    let handle = std::thread::Builder::new()
        .name("rwe-main".into())
        .stack_size(256 * 1024 * 1024)
        .spawn(real_main)
        .expect("nelze spawnout main worker thread");
    // Panic v rwe-main threadu PROPAGOVAT jako nenulovy exit code. Drive
    // `let _ = join()` panic spolknul -> proces skoncil 0 = "tiche umreni"
    // nerozlisitelne od normalniho zavreni (spatne diagnostikovatelne).
    // Default panic hook uz hlasku vypsal na stderr pri unwindu.
    if handle.join().is_err() {
        std::process::exit(101);
    }
}

fn real_main() {
    let args: Vec<String> = std::env::args().collect();

    let target = args.iter()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .cloned()
        .unwrap_or_else(|| "static/test.html".to_string());

    // --devtools flag = run devtools-mockup s mock data z `target`. Misto
    // F12 split (page + dev WV), devtools UI je page sama. Mock CDP wire
    // returns precomputed data (DOM tree, matched styles, computed).
    let devtools_mode = args.iter().any(|a| a == "--devtools");
    if devtools_mode {
        let mock = match rwe_engine::embed::devtools_test::generate_mock_data(&target) {
            Some(m) => m,
            None => { eprintln!("[shell] devtools-test: nelze nacist {target}"); return; }
        };
        // Build standalone HTML.
        let template = rwe_devtools_frontend::INDEX_HTML;
        let mock_script = format!(
            "<script id=\"mock-cdp\">window.__MOCK_CDP__ = {};\n{}\n</script>",
            mock.mock_json,
            mock.override_js,
        );
        let html = template.replace(
            "<script id=\"cdp-js\"></script>",
            &mock_script,
        );
        // Extract <style> bloky z generated HTML do css string.
        let css = extract_inline_styles(&html);
        if let Err(e) = rwe_shell::run_window(html, css, Some(mock.base_url), None) {
            eprintln!("[shell] error: {e}");
        }
        return;
    }

    // Stejny loader jako engine - http/file dispatch + CSS aggregace.
    let loaded = match rwe_engine::embed::loader::load_page(&target) {
        Some(l) => l,
        None => { eprintln!("[shell] nelze nacist {target}"); return; }
    };

    if let Err(e) = rwe_shell::run_window(
        loaded.html,
        loaded.css,
        loaded.base_url,
        loaded.local_path.as_ref().map(PathBuf::from),
    ) {
        eprintln!("[shell] error: {e}");
    }
}
