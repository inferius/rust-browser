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

fn main() {
    let handle = std::thread::Builder::new()
        .name("rwe-main".into())
        .stack_size(256 * 1024 * 1024)
        .spawn(real_main)
        .expect("nelze spawnout main worker thread");
    let _ = handle.join();
}

fn real_main() {
    let args: Vec<String> = std::env::args().collect();

    let target = args.iter()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .cloned()
        .unwrap_or_else(|| "static/test.html".to_string());

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
