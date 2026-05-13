// rwe-shell bin. V Phase 1 je to wrapper okolo rwe_engine::run_cli
// - shell logika zatim zije v engine crate (`browser::render::run_window_with_shell`).
//
// Phase 3-5 sem presunou tab strip, address bar, bookmarks bar, history navigaci
// + kompozici (engine page RT + shell chrome RT -> swap chain).
//
// Pro ted: rwe-shell == rwe-engine + force `browser` rezim pokud uzivatel
// neda jiny prikaz. (rwe-engine bin defaultne na CLI demo / interpreter dispatch.)
fn main() {
    let handle = std::thread::Builder::new()
        .name("rwe-main".into())
        .stack_size(256 * 1024 * 1024)
        .spawn(|| {
            let mut args: Vec<String> = std::env::args().collect();
            // Default action pro shell bin = browser mode (s chrome).
            // Pokud uzivatel preda `debug` / `devtools` / `dump`, prepustime tomu.
            if args.len() == 1 || (args.len() >= 2 && !matches!(
                args[1].as_str(),
                "debug" | "devtools" | "browser" | "window" | "shell" | "dump"
            )) {
                args.insert(1, "browser".to_string());
            }
            rwe_engine::run_cli(args)
        })
        .expect("nelze spawnout main worker thread");
    let _ = handle.join();
}
