//! Chrome DevTools Protocol (CDP) - Debugger domain stub.
//!
//! Spec: https://chromedevtools.github.io/devtools-protocol/
//! Commands: Debugger.setBreakpoint, Debugger.pause, Debugger.resume, ...
//! Events: Debugger.paused, Debugger.resumed, Debugger.scriptParsed.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Breakpoint {
    pub id: u64,
    pub script_id: u64,
    pub line: u32,
    pub column: Option<u32>,
    pub condition: Option<String>,
    pub log_message: Option<String>,
    pub hit_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PauseReason {
    Other,
    Breakpoint,
    Step,
    Exception,
    DebuggerStatement,
    XHR,
    DOM,
    EventListener,
}

#[derive(Debug, Clone)]
pub struct DebuggerPaused {
    pub reason: PauseReason,
    pub call_stack: Vec<CallFrame>,
    pub hit_breakpoints: Vec<u64>,
}

#[derive(Debug, Clone)]
pub struct CallFrame {
    pub function_name: String,
    pub script_id: u64,
    pub url: String,
    pub line: u32,
    pub column: u32,
    pub scope_chain_ids: Vec<u64>,
}

#[derive(Default)]
pub struct DebuggerState {
    pub breakpoints: HashMap<u64, Breakpoint>,
    pub next_bp_id: u64,
    pub paused: Option<DebuggerPaused>,
    pub step_mode: Option<StepMode>,
    pub skip_pauses: bool,
    pub pause_on_exceptions: PauseOnException,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StepMode {
    StepInto,
    StepOver,
    StepOut,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PauseOnException {
    None,
    Uncaught,
    All,
}

impl Default for PauseOnException {
    fn default() -> Self { PauseOnException::None }
}

impl DebuggerState {
    pub fn new() -> Self { Self::default() }

    pub fn set_breakpoint(&mut self, script_id: u64, line: u32, column: Option<u32>, condition: Option<String>) -> u64 {
        self.next_bp_id += 1;
        let id = self.next_bp_id;
        self.breakpoints.insert(id, Breakpoint {
            id, script_id, line, column, condition,
            log_message: None, hit_count: 0,
        });
        id
    }

    pub fn remove_breakpoint(&mut self, id: u64) -> bool {
        self.breakpoints.remove(&id).is_some()
    }

    pub fn breakpoints_at(&self, script_id: u64, line: u32) -> Vec<&Breakpoint> {
        self.breakpoints.values().filter(|bp| bp.script_id == script_id && bp.line == line).collect()
    }

    pub fn pause(&mut self, reason: PauseReason, call_stack: Vec<CallFrame>, hit_ids: Vec<u64>) {
        // Increment hit_count for each hit BP
        for id in &hit_ids {
            if let Some(bp) = self.breakpoints.get_mut(id) { bp.hit_count += 1; }
        }
        self.paused = Some(DebuggerPaused {
            reason,
            call_stack,
            hit_breakpoints: hit_ids,
        });
    }

    pub fn resume(&mut self) {
        self.paused = None;
        self.step_mode = None;
    }

    pub fn step(&mut self, mode: StepMode) {
        self.paused = None;
        self.step_mode = Some(mode);
    }

    pub fn is_paused(&self) -> bool { self.paused.is_some() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> CallFrame {
        CallFrame {
            function_name: "main".into(),
            script_id: 1, url: "x.js".into(),
            line: 5, column: 10,
            scope_chain_ids: vec![1, 2],
        }
    }

    #[test]
    fn set_remove_bp() {
        let mut d = DebuggerState::new();
        let id = d.set_breakpoint(1, 10, None, None);
        assert!(d.remove_breakpoint(id));
        assert!(!d.remove_breakpoint(id));
    }

    #[test]
    fn bp_lookup_by_position() {
        let mut d = DebuggerState::new();
        d.set_breakpoint(1, 10, None, None);
        d.set_breakpoint(1, 10, None, Some("x > 0".into()));
        d.set_breakpoint(1, 20, None, None);
        assert_eq!(d.breakpoints_at(1, 10).len(), 2);
        assert_eq!(d.breakpoints_at(1, 20).len(), 1);
    }

    #[test]
    fn pause_resume() {
        let mut d = DebuggerState::new();
        let id = d.set_breakpoint(1, 5, None, None);
        d.pause(PauseReason::Breakpoint, vec![frame()], vec![id]);
        assert!(d.is_paused());
        assert_eq!(d.breakpoints.get(&id).unwrap().hit_count, 1);
        d.resume();
        assert!(!d.is_paused());
    }

    #[test]
    fn step_clears_paused() {
        let mut d = DebuggerState::new();
        d.pause(PauseReason::DebuggerStatement, vec![frame()], vec![]);
        d.step(StepMode::StepOver);
        assert!(!d.is_paused());
        assert_eq!(d.step_mode, Some(StepMode::StepOver));
    }

    #[test]
    fn pause_on_exception_setting() {
        let mut d = DebuggerState::new();
        d.pause_on_exceptions = PauseOnException::Uncaught;
        assert_eq!(d.pause_on_exceptions, PauseOnException::Uncaught);
    }
}
