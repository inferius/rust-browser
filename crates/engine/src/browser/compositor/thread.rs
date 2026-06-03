//! Compositor thread - dedicated worker pro composite-only operations.
//!
//! Architektura:
//! - Main thread: layout, paint, layer texture updates
//! - Compositor thread: composite pass (sample layer textures + blend),
//!   scroll uniform updates, transform/opacity anim tick
//!
//! Inspired by Chromium `cc::ProxyMain` + `cc::ProxyImpl` + Firefox
//! `WebRenderBackendChild` model.
//!
//! ## Komunikacni protokol
//!
//! Main -> Compositor (CompositorCmd):
//! - `UpdateLayerSnapshot { layers }` - novy layer tree po layout/paint
//! - `ScrollUpdate { layer_id, x, y }` - scroll position change
//! - `TransformUpdate { layer_id, matrix }` - transform anim tick
//! - `OpacityUpdate { layer_id, opacity }` - opacity anim tick
//! - `Resize { width, height }` - swap chain reallocate
//! - `Shutdown` - clean exit
//!
//! Compositor -> Main (CompositorEvent):
//! - `InputForward { event }` - input event nezpracovany v compositor
//! - `ScrollRequest { layer_id, dx, dy }` - request scroll target update
//! - `FrameDone { frame_id, ms }` - composite pass dokonceny
//!
//! ## Limitations
//!
//! Aktualne foundation - real wgpu multi-thread bezeci compositor pass je
//! risky na Windows (D3D12 swap chain thread affinity). Plne impl vyzaduje:
//! - Arc<Device> + Arc<Queue> sdilene mezi threads (Send + Sync OK pres wgpu 0.20+)
//! - Swap chain ownership presunut do compositor thread (BREAKING change ve
//!   shell architecture)
//! - Synchronizace pres barriers / fences pro layer texture writes z main
//! - Input event forwarding pres compositor pro low-latency scroll
//!
//! Pro session = channel pair + worker thread stub. Real GPU work zustane
//! v main thread, foundation pro budouci split.

use std::sync::mpsc;

/// Commands sent from main to compositor thread.
#[derive(Debug)]
pub enum CompositorCmd {
    UpdateLayerSnapshot { /* layer_tree snapshot - serializable subset */ },
    ScrollUpdate { layer_id: usize, x: f32, y: f32 },
    TransformUpdate { layer_id: usize, matrix: [f32; 16] },
    OpacityUpdate { layer_id: usize, opacity: f32 },
    Resize { width: u32, height: u32 },
    Shutdown,
}

/// Events from compositor back to main.
#[derive(Debug)]
pub enum CompositorEvent {
    /// Input event ktery compositor nemoze handlovat (hit-test miss, JS dispatch)
    /// -> forward main pro DOM dispatch.
    InputForward { event_kind: String },
    /// Request main to update scroll target (compositor decided scroll path).
    ScrollRequest { layer_id: usize, dx: f32, dy: f32 },
    /// Composite pass dokonceny - per-frame timing.
    FrameDone { frame_id: u64, compose_ms: f32 },
}

/// Compositor thread handle. Drzi mpsc senders + worker JoinHandle.
pub struct CompositorThreadHandle {
    pub cmd_tx: mpsc::Sender<CompositorCmd>,
    pub event_rx: mpsc::Receiver<CompositorEvent>,
    pub worker: Option<std::thread::JoinHandle<()>>,
}

impl CompositorThreadHandle {
    /// Spawn compositor worker. Real impl by predala wgpu::Device + Queue
    /// + swap chain handle. Foundation = empty loop reading cmds.
    pub fn spawn() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<CompositorCmd>();
        let (event_tx, event_rx) = mpsc::channel::<CompositorEvent>();
        let worker = std::thread::Builder::new()
            .name("rwe-compositor".into())
            .stack_size(4 * 1024 * 1024)
            .spawn(move || {
                compositor_main_loop(cmd_rx, event_tx);
            })
            .expect("compositor thread spawn failed");
        CompositorThreadHandle {
            cmd_tx,
            event_rx,
            worker: Some(worker),
        }
    }

    /// Send shutdown signal + join.
    pub fn shutdown(&mut self) {
        let _ = self.cmd_tx.send(CompositorCmd::Shutdown);
        if let Some(h) = self.worker.take() {
            let _ = h.join();
        }
    }
}

impl Drop for CompositorThreadHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Compositor thread main loop. Read cmds + event-back na main.
/// Foundation impl - no GPU work. Real impl by:
/// 1. Init compositor wgpu state (Arc<Device>, swap chain ownership pres barrier)
/// 2. Per CompositorCmd dispatch composite pass / uniform update / etc
/// 3. Per frame send FrameDone event
fn compositor_main_loop(
    cmd_rx: mpsc::Receiver<CompositorCmd>,
    event_tx: mpsc::Sender<CompositorEvent>,
) {
    let mut frame_id: u64 = 0;
    loop {
        match cmd_rx.recv() {
            Ok(CompositorCmd::Shutdown) | Err(_) => break,
            Ok(CompositorCmd::UpdateLayerSnapshot { .. }) => {
                // Real: kopirovat layer snapshot + invalidate composite cache.
                frame_id = frame_id.wrapping_add(1);
                let _ = event_tx.send(CompositorEvent::FrameDone {
                    frame_id, compose_ms: 0.0,
                });
            }
            Ok(CompositorCmd::ScrollUpdate { .. }) => {
                // Real: update scroll uniform per layer, re-composite.
            }
            Ok(CompositorCmd::TransformUpdate { .. }) => {
                // Real: update transform matrix uniform per layer, re-composite.
            }
            Ok(CompositorCmd::OpacityUpdate { .. }) => {
                // Real: update opacity per layer alpha, re-composite.
            }
            Ok(CompositorCmd::Resize { .. }) => {
                // Real: reallocate swap chain + main RT.
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compositor_thread_spawn_and_shutdown() {
        let mut h = CompositorThreadHandle::spawn();
        h.shutdown();
    }

    #[test]
    fn compositor_thread_processes_snapshot_cmd() {
        let mut h = CompositorThreadHandle::spawn();
        h.cmd_tx.send(CompositorCmd::UpdateLayerSnapshot {}).unwrap();
        // Wait for FrameDone event back.
        let event = h.event_rx.recv_timeout(std::time::Duration::from_secs(2));
        assert!(event.is_ok(), "compositor should respond to snapshot");
        match event.unwrap() {
            CompositorEvent::FrameDone { frame_id, .. } => {
                assert!(frame_id >= 1);
            }
            _ => panic!("expected FrameDone"),
        }
        h.shutdown();
    }
}
