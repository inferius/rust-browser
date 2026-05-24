//! Multi-process renderer architecture - foundation.
//!
//! Chrome/Firefox model:
//! - Browser (parent) process: UI shell, tab management, navigation
//! - Renderer (child) process: per tab/origin - HTML/CSS/JS engine + DOM/layout/paint
//! - GPU process: shared wgpu device + composite (= main thread cross all renderers)
//! - Network process: HTTP/IPC pro fetch (later)
//!
//! ## Security benefits
//! - Renderer sandboxed: crash v JS/page = jen ten tab umre, browser zustane
//! - Cross-origin isolation: separate process per origin (Site Isolation)
//! - GPU isolation: pripadne corrupt shader nelozí browser
//!
//! ## IPC kanaly
//!
//! Browser <-> Renderer:
//! - `RendererCmd::Navigate { url }` - browser instructs renderer load URL
//! - `RendererCmd::Resize { w, h }` - tab resize
//! - `RendererCmd::Input { event }` - forward user input
//! - `RendererCmd::Shutdown`
//! - `RendererEvent::FrameReady { texture_handle }` - GPU process pull texture
//! - `RendererEvent::NavigationRequest { url }` - JS window.location change
//! - `RendererEvent::ConsoleLog { level, msg }`
//! - `RendererEvent::TitleChanged { title }`
//!
//! ## Implementation strategy
//!
//! Phase 1 (this foundation): Define IPC types, channel struct, stub process
//! spawn. Aktualne stale single-process - renderer = WebView v hostujici thread.
//!
//! Phase 2 (next session+): `std::process::Command` spawn child + named pipes
//! / domain sockets pro IPC. Shared GPU memory pres dma-buf (Linux) /
//! D3D shared handles (Windows).
//!
//! Phase 3: Sandboxing pres seccomp (Linux) / AppContainer (Windows).
//!
//! Inspired by Chromium `content/browser/renderer_host/render_process_host_impl.cc`
//! + Firefox `dom/ipc/ContentParent.cpp`.

use std::sync::mpsc;

/// Commands sent from browser process to renderer process.
#[derive(Debug, Clone)]
pub enum RendererCmd {
    Navigate { url: String },
    Resize { width: u32, height: u32 },
    /// Generic input event - serialized form. Real impl by pres bincode/serde.
    Input { event_kind: String, payload: Vec<u8> },
    /// JS execution request (devtools console).
    EvalScript { code: String, request_id: u64 },
    Shutdown,
}

/// Events from renderer back to browser.
#[derive(Debug, Clone)]
pub enum RendererEvent {
    /// Frame texture ready - real IPC by predala texture handle (Windows
    /// shared handle / Linux dma-buf fd). Foundation = stub.
    FrameReady { width: u32, height: u32 },
    /// JS requested navigation (window.location = ..., link click).
    NavigationRequest { url: String, target: String },
    /// Title change z JS (document.title).
    TitleChanged { title: String },
    /// Console log entry pro browser devtools forward.
    ConsoleLog { level: String, message: String },
    /// EvalScript response.
    EvalResult { request_id: u64, value: String },
    /// Renderer process zemrelo / unexpected exit.
    Crashed { reason: String },
}

/// Renderer process handle - browser side. Drzi IPC senders + process handle.
/// V foundation impl renderer = local thread, ne separate process. Real impl
/// pres std::process::Command s named pipes.
pub struct RendererProcessHandle {
    pub renderer_id: u64,
    pub cmd_tx: mpsc::Sender<RendererCmd>,
    pub event_rx: mpsc::Receiver<RendererEvent>,
    /// Process handle - None v foundation (in-process), Some v real impl.
    pub process: Option<std::process::Child>,
    /// Worker thread (foundation only - real impl by tot pres process).
    pub worker: Option<std::thread::JoinHandle<()>>,
}

impl RendererProcessHandle {
    /// Spawn renderer process. Foundation = local thread misto child process.
    pub fn spawn(renderer_id: u64) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<RendererCmd>();
        let (event_tx, event_rx) = mpsc::channel::<RendererEvent>();
        let worker = std::thread::Builder::new()
            .name(format!("rwe-renderer-{}", renderer_id))
            .stack_size(16 * 1024 * 1024)
            .spawn(move || {
                renderer_main_loop(renderer_id, cmd_rx, event_tx);
            })
            .expect("renderer thread spawn failed");
        RendererProcessHandle {
            renderer_id,
            cmd_tx,
            event_rx,
            process: None,
            worker: Some(worker),
        }
    }

    pub fn navigate(&self, url: String) {
        let _ = self.cmd_tx.send(RendererCmd::Navigate { url });
    }

    pub fn resize(&self, width: u32, height: u32) {
        let _ = self.cmd_tx.send(RendererCmd::Resize { width, height });
    }

    pub fn shutdown(&mut self) {
        let _ = self.cmd_tx.send(RendererCmd::Shutdown);
        if let Some(h) = self.worker.take() {
            let _ = h.join();
        }
        if let Some(mut p) = self.process.take() {
            let _ = p.kill();
            let _ = p.wait();
        }
    }
}

impl Drop for RendererProcessHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Renderer "process" main loop - foundation v threadu. Real impl by spustila
/// engine WebView a slouzila IPC requests.
fn renderer_main_loop(
    renderer_id: u64,
    cmd_rx: mpsc::Receiver<RendererCmd>,
    event_tx: mpsc::Sender<RendererEvent>,
) {
    let _ = renderer_id;
    loop {
        match cmd_rx.recv() {
            Ok(RendererCmd::Shutdown) | Err(_) => break,
            Ok(RendererCmd::Navigate { url }) => {
                // Real: WebView.load(url). Then send FrameReady.
                let _ = event_tx.send(RendererEvent::TitleChanged {
                    title: format!("{} - foundation", url),
                });
            }
            Ok(RendererCmd::Resize { width, height }) => {
                // Real: WebView.resize(w, h).
                let _ = event_tx.send(RendererEvent::FrameReady { width, height });
            }
            Ok(RendererCmd::Input { .. }) => {
                // Real: WebView.handle_input(event).
            }
            Ok(RendererCmd::EvalScript { code, request_id }) => {
                // Real: WebView.eval(code). Foundation = echo back.
                let _ = event_tx.send(RendererEvent::EvalResult {
                    request_id,
                    value: format!("foundation eval: {}", code.chars().take(40).collect::<String>()),
                });
            }
        }
    }
}

/// Browser-side process manager. Drzi mapu renderer_id -> handle. Per tab
/// = renderer instance (mozne sdileni per origin pres Site Isolation policy).
pub struct ProcessManager {
    pub renderers: std::collections::HashMap<u64, RendererProcessHandle>,
    pub next_id: u64,
}

impl Default for ProcessManager {
    fn default() -> Self { Self::new() }
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            renderers: std::collections::HashMap::new(),
            next_id: 1,
        }
    }

    /// Spawn novy renderer pro tab. Vraci renderer_id.
    pub fn spawn_renderer(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let h = RendererProcessHandle::spawn(id);
        self.renderers.insert(id, h);
        id
    }

    pub fn kill_renderer(&mut self, id: u64) {
        if let Some(mut h) = self.renderers.remove(&id) {
            h.shutdown();
        }
    }

    pub fn navigate(&self, id: u64, url: String) {
        if let Some(h) = self.renderers.get(&id) {
            h.navigate(url);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_manager_spawn_renderer() {
        let mut mgr = ProcessManager::new();
        let id1 = mgr.spawn_renderer();
        let id2 = mgr.spawn_renderer();
        assert_ne!(id1, id2);
        assert_eq!(mgr.renderers.len(), 2);
        mgr.kill_renderer(id1);
        assert_eq!(mgr.renderers.len(), 1);
    }

    #[test]
    fn renderer_processes_navigate_cmd() {
        let mut mgr = ProcessManager::new();
        let id = mgr.spawn_renderer();
        mgr.navigate(id, "http://example.com".into());
        let h = mgr.renderers.get(&id).unwrap();
        let event = h.event_rx.recv_timeout(std::time::Duration::from_secs(2));
        assert!(event.is_ok());
        match event.unwrap() {
            RendererEvent::TitleChanged { title } => {
                assert!(title.contains("example.com"));
            }
            _ => panic!("expected TitleChanged"),
        }
    }

    #[test]
    fn renderer_processes_eval_cmd() {
        let mut mgr = ProcessManager::new();
        let id = mgr.spawn_renderer();
        let h = mgr.renderers.get(&id).unwrap();
        h.cmd_tx.send(RendererCmd::EvalScript {
            code: "1 + 2".into(),
            request_id: 42,
        }).unwrap();
        let event = h.event_rx.recv_timeout(std::time::Duration::from_secs(2));
        assert!(event.is_ok());
        match event.unwrap() {
            RendererEvent::EvalResult { request_id, .. } => {
                assert_eq!(request_id, 42);
            }
            _ => panic!("expected EvalResult"),
        }
    }
}
