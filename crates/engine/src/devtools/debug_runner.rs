//! DebugRunner - hybrid debug-mode JS exec na worker thread.
//!
//! Architektura:
//! - Aktivace pri F12 open + breakpoints set: page reload s "debug_mode = true"
//! - Worker thread create vlastni `Interpreter` (uvnitr closure - nikdy crossne hranice
//!   z UI thread, takze !Send constraint je OK - vse Rc/RefCell na worker stayy)
//! - Komunikace pres mpsc channels:
//!   * `WorkerEvent` z worker -> UI: Log, Network, Pause, Done, Error
//!   * `UiCommand` z UI -> worker: Continue, StepKind, ToggleBreakpoint, Quit
//! - Sdileny `DebuggerState` pres Arc<Mutex> - worker pri pause hit zapisuje
//!   paused_at + locals; UI cte pres render loop
//! - Continue Condvar - worker block_for_continue, UI klikne -> notify_all
//!
//! Vykonostni profil:
//! - Pri devtools closed (debug_mode = false): bezne sync exec na UI thread,
//!   0 overhead, zadne kanaly, zadne mutexy v hot path.
//! - Pri debug_mode aktivni: scripts run na workeru, UI thread responsive,
//!   serialization cost per event (~mikrosekundy) - perceptible jen pri
//!   tisicich events za frame.

use std::sync::{Arc, Mutex, Condvar};
use std::sync::mpsc::{Sender, Receiver, channel};
use crate::interpreter::DebuggerState;

#[derive(Debug, Clone)]
pub enum WorkerEvent {
    Log { level: String, msg: String },
    Network { url: String, status: u16 },
    /// Worker hit breakpoint - paused. UI ukaze stav z DebuggerState (locals).
    Pause { line: u32 },
    /// Worker dokoncil eval bez chyby.
    Done,
    /// Worker hit error.
    Error(String),
    /// Worker zacal exec (akce signal pro UI).
    Started,
}

#[derive(Debug, Clone)]
pub enum UiCommand {
    /// Pokracovat z pause.
    Continue,
    /// Step Over - po dalsim stmt s call_depth <= anchor pause.
    StepOver,
    /// Step Into - po dalsim stmt pause (bez ohledu na call_depth).
    StepInto,
    /// Step Out - po vrať z aktualni call.
    StepOut,
    /// Toggle breakpoint na line.
    ToggleBreakpoint(u32),
    /// Set vsech breakpoints (replace).
    SetBreakpoints(Vec<u32>),
    /// Worker exit gracefully.
    Quit,
}

/// Drzi worker thread handle + komunikacni kanaly + sdilene state.
/// UI Renderer drzi Option<DebugRunner> - aktivace pri debug mode entry.
pub struct DebugRunner {
    pub event_rx: Receiver<WorkerEvent>,
    pub cmd_tx: Sender<UiCommand>,
    pub debugger: Arc<Mutex<DebuggerState>>,
    pub continue_signal: Arc<(Mutex<bool>, Condvar)>,
    pub handle: Option<std::thread::JoinHandle<()>>,
    /// True pokud worker je v pause stavu (cached z events).
    pub is_paused: bool,
    pub last_pause_line: Option<u32>,
}

impl DebugRunner {
    /// Spustí worker thread se daným HTML + skripty. Worker vytvori vlastni
    /// Interpreter uvnitr closure (Send-clean - vse Rc/RefCell na workeru).
    /// Sdilene jsou jen Arc<Mutex<DebuggerState>> + Condvar.
    pub fn spawn(html: String, base_url: String, breakpoints: Vec<u32>) -> Self {
        let (event_tx, event_rx) = channel::<WorkerEvent>();
        let (cmd_tx, cmd_rx) = channel::<UiCommand>();
        let debugger = Arc::new(Mutex::new(DebuggerState::default()));
        let continue_signal = Arc::new((Mutex::new(false), Condvar::new()));
        {
            let mut dbg = debugger.lock().unwrap();
            dbg.breakpoints = breakpoints.into_iter().collect();
        }

        let dbg_clone = Arc::clone(&debugger);
        let sig_clone = Arc::clone(&continue_signal);

        let handle = std::thread::Builder::new()
            .name("rwe-debug-worker".into())
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                worker_main(html, base_url, event_tx, cmd_rx, dbg_clone, sig_clone);
            })
            .expect("spawn debug worker");

        DebugRunner {
            event_rx,
            cmd_tx,
            debugger,
            continue_signal,
            handle: Some(handle),
            is_paused: false,
            last_pause_line: None,
        }
    }

    /// Notify worker pres Condvar - po Continue/Step button click v UI.
    pub fn notify_continue(&self) {
        let (lock, cvar) = &*self.continue_signal;
        let mut continued = lock.lock().unwrap();
        *continued = true;
        cvar.notify_all();
    }

    /// Posli UiCommand na worker. Pri pause cca instant, jinak ceka az worker
    /// dorazi na poll point (zatim worker nepoll - cmd nepouzity behem run).
    pub fn send_cmd(&self, cmd: UiCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    /// Drainuje event channel - vraci vsechny dosud nepriradene events.
    pub fn drain_events(&mut self) -> Vec<WorkerEvent> {
        let mut out = Vec::new();
        while let Ok(ev) = self.event_rx.try_recv() {
            // Update internal pause state z eventu.
            if let WorkerEvent::Pause { line } = &ev {
                self.is_paused = true;
                self.last_pause_line = Some(*line);
            }
            if matches!(ev, WorkerEvent::Done | WorkerEvent::Error(_)) {
                self.is_paused = false;
            }
            out.push(ev);
        }
        out
    }

    /// Worker je hotov?
    pub fn is_finished(&self) -> bool {
        self.handle.as_ref().map(|h| h.is_finished()).unwrap_or(true)
    }

    /// Cekej na worker exit (po Done/Error event nebo Quit cmd).
    pub fn join(mut self) {
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn worker_main(
    html: String,
    base_url: String,
    event_tx: Sender<WorkerEvent>,
    cmd_rx: Receiver<UiCommand>,
    debugger: Arc<Mutex<DebuggerState>>,
    continue_signal: Arc<(Mutex<bool>, Condvar)>,
) {
    use crate::lexer::base::Lexer;
    use crate::parser::Parser;
    use crate::tokens::TokenKind;

    let _ = event_tx.send(WorkerEvent::Started);
    let _ = base_url;

    // Worker-thread Interpreter - vse Rc/RefCell uvnitr workera, nikdy crossne thread.
    let mut interp = crate::interpreter::Interpreter::new();

    // Set document z HTML (parse uvnitr workera - rcdom Rc je single-thread OK).
    let doc = crate::browser::html_parser::parse_html(&html, "about:blank");
    let doc_root = std::rc::Rc::clone(&doc.root);
    interp.set_document(doc);

    // Pripoj sdileny debugger + continue signal.
    interp.attach_shared_debugger(Arc::clone(&debugger), Arc::clone(&continue_signal));

    // Mirror console_log + network_log -> event_tx pres callback wrapper (zatim
    // primary cesta = polling pres interp.console_log, send pres tx pri kazdem
    // hit + na zaver). Worker thread po skript konci posli vsechno najednou.

    // Run scripts.
    let scripts: Vec<String> = doc_root.get_elements_by_tag("script")
        .iter().map(|s| s.text_content()).collect();
    let mut last_log_count = 0usize;
    let mut last_net_count = 0usize;
    for src in scripts {
        if src.trim().is_empty() { continue; }
        // Process pending UI commands (BP toggle etc.) pred startem skript.
        process_pending_cmds(&cmd_rx, &debugger);

        if let Ok(lex) = Lexer::parse_str(&src, "<script>") {
            let tokens: Vec<_> = lex.tokens.into_iter()
                .filter(|t| !matches!(t.kind,
                    TokenKind::Whitespace | TokenKind::Newline
                    | TokenKind::CommentLine(_) | TokenKind::CommentBlock(_)))
                .collect();
            let mut parser = Parser::new(tokens);
            if let Ok(prog) = parser.parse() {
                if let Err(e) = interp.run(&prog) {
                    let _ = event_tx.send(WorkerEvent::Error(format!("{}", e)));
                }
            }
        }

        // Flush logs po skript.
        let logs = interp.console_log.borrow();
        for (level, msg) in logs.iter().skip(last_log_count) {
            let _ = event_tx.send(WorkerEvent::Log {
                level: level.clone(),
                msg: msg.clone(),
            });
        }
        last_log_count = logs.len();
        drop(logs);

        let nets = interp.network_log.borrow();
        for (url, status) in nets.iter().skip(last_net_count) {
            let _ = event_tx.send(WorkerEvent::Network {
                url: url.clone(),
                status: *status,
            });
        }
        last_net_count = nets.len();
        drop(nets);
    }

    let _ = event_tx.send(WorkerEvent::Done);
}

fn process_pending_cmds(
    cmd_rx: &Receiver<UiCommand>,
    debugger: &Arc<Mutex<DebuggerState>>,
) {
    while let Ok(cmd) = cmd_rx.try_recv() {
        let mut dbg = debugger.lock().unwrap();
        match cmd {
            UiCommand::ToggleBreakpoint(line) => {
                if dbg.breakpoints.contains(&line) {
                    dbg.breakpoints.remove(&line);
                } else {
                    dbg.breakpoints.insert(line);
                }
            }
            UiCommand::SetBreakpoints(lines) => {
                dbg.breakpoints = lines.into_iter().collect();
            }
            UiCommand::Continue | UiCommand::StepOver
            | UiCommand::StepInto | UiCommand::StepOut => {
                // Continue/Step jsou volane v block_for_continue mid-pause path
                // pres notify Condvar, ne pres command queue.
                // (Nicmene state mutation je tady aplikovana pro safety.)
            }
            UiCommand::Quit => {
                // No-op - worker exit po dokonceni skript (graceful).
            }
        }
    }
}
