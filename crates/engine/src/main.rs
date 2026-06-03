// Thin bin shim - spawn main worker thread s velkym stackem (256 MB) a
// volat rwe_engine::run_cli. Implementace cely CLI dispatcheru zije v lib.rs
// (a postupne se bude refaktorovat).
//
// Windows main thread default = 1 MB; debug build (no inline) layout/paint
// recursion ma velke frames (30+ KB). Linker /STACK flag dava 64 MB ale
// dedicated thread je robustnejsi (vetsi rezerva pro winit + interpreter).
fn main() {
    let handle = std::thread::Builder::new()
        .name("rwe-main".into())
        .stack_size(256 * 1024 * 1024)
        .spawn(|| rwe_engine::run_cli(std::env::args().collect()))
        .expect("nelze spawnout main worker thread");
    let _ = handle.join();
}
