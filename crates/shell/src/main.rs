// rwe-shell bin. Phase 4c step 3 = shell crate ma vlastni run_window ktery
// VLASTNI Window + Renderer + WebView a kompozituje pres engine API.
//
// Dva rezimy:
// - `cargo run -p rwe-shell` (no args) -> shell::run_window se static/test.html
// - `cargo run -p rwe-shell -- legacy` -> deleguje na engine::run_cli (puvodni
//    browser rezim s chrome bar v engine App - dokud nepresunute chrome paint
//    do shell crate v Phase 5).
//
// Plnohodnotny shell (s chrome bar + tabs + addr + find + bookmarks) bezi
// nyni JEN pres legacy delegaci. Phase 5 sem presune chrome paint, pak
// shell::run_window prevezme primary cestou.

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

    // Legacy delegate na engine pro chrome bar + tabs (Phase 5 sem presune).
    if args.iter().any(|a| a == "legacy" || a == "--legacy") {
        let mut filtered: Vec<String> = args.iter()
            .filter(|a| a.as_str() != "legacy" && a.as_str() != "--legacy")
            .cloned()
            .collect();
        if filtered.len() == 1 || (filtered.len() >= 2 && !matches!(
            filtered[1].as_str(),
            "debug" | "devtools" | "browser" | "window" | "shell" | "dump"
        )) {
            filtered.insert(1, "browser".to_string());
        }
        rwe_engine::run_cli(filtered);
        return;
    }

    // Phase 4c step 3 path - shell crate vlastni run_window pres WebView API.
    // Bez chrome bar (Phase 5 doda).
    let target = args.iter()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .cloned()
        .unwrap_or_else(|| "static/test.html".to_string());

    // Pouzij stejny loader jako engine - http/file dispatch + CSS aggregace.
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
