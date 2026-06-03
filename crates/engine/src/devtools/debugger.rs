//! JS debugger - breakpoint registry + step/continue + scope inspection.
//!
//! Inspired by Chrome DevTools Protocol Debugger domain
//! (`reference/chromium/third_party/blink/renderer/core/inspector/inspector_debugger_agent.cc`).

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DebuggerState {
    /// Running normally.
    Running,
    /// Paused at breakpoint - waiting for resume/step.
    Paused,
    /// Step over - run until next statement v same fn.
    SteppingOver,
    /// Step in - run until next statement (descend into fn calls).
    SteppingIn,
    /// Step out - run until return z current fn.
    SteppingOut,
}

#[derive(Debug, Clone)]
pub struct Breakpoint {
    pub id: u32,
    pub source_url: String,
    pub line: u32,
    pub column: u32,
    pub condition: Option<String>, // pause jen kdyz JS expr true
    pub enabled: bool,
    pub hit_count: u32,
}

#[derive(Debug, Clone)]
pub struct StackFrame {
    pub function_name: String,
    pub source_url: String,
    pub line: u32,
    pub column: u32,
    /// Local variable scope - var name -> JsValue display string.
    pub locals: HashMap<String, String>,
}

#[derive(Default)]
pub struct Debugger {
    pub state: DebuggerState,
    pub breakpoints: Vec<Breakpoint>,
    pub next_bp_id: u32,
    pub call_stack: Vec<StackFrame>,
    /// Pri pause: current source url + line pro highlight v devtools.
    pub pause_location: Option<(String, u32, u32)>,
}

impl Default for DebuggerState {
    fn default() -> Self { DebuggerState::Running }
}

impl Debugger {
    pub fn new() -> Self {
        Self {
            state: DebuggerState::Running,
            breakpoints: Vec::new(),
            next_bp_id: 1,
            call_stack: Vec::new(),
            pause_location: None,
        }
    }

    pub fn set_breakpoint(&mut self, url: &str, line: u32, column: u32) -> u32 {
        let id = self.next_bp_id;
        self.next_bp_id += 1;
        self.breakpoints.push(Breakpoint {
            id, source_url: url.into(), line, column,
            condition: None, enabled: true, hit_count: 0,
        });
        id
    }

    pub fn remove_breakpoint(&mut self, id: u32) {
        self.breakpoints.retain(|b| b.id != id);
    }

    pub fn enable_breakpoint(&mut self, id: u32, enabled: bool) {
        for b in &mut self.breakpoints {
            if b.id == id { b.enabled = enabled; }
        }
    }

    /// Check zdali instruction at (url, line) ma breakpoint. Volat z VM hook.
    pub fn check_breakpoint(&mut self, url: &str, line: u32) -> bool {
        for b in &mut self.breakpoints {
            if !b.enabled { continue; }
            if b.source_url == url && b.line == line {
                b.hit_count += 1;
                return true;
            }
        }
        false
    }

    pub fn pause(&mut self, url: &str, line: u32, column: u32) {
        self.state = DebuggerState::Paused;
        self.pause_location = Some((url.into(), line, column));
    }

    pub fn resume(&mut self) {
        self.state = DebuggerState::Running;
        self.pause_location = None;
    }

    pub fn step_over(&mut self) { self.state = DebuggerState::SteppingOver; }
    pub fn step_in(&mut self) { self.state = DebuggerState::SteppingIn; }
    pub fn step_out(&mut self) { self.state = DebuggerState::SteppingOut; }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_remove_breakpoint() {
        let mut d = Debugger::new();
        let id = d.set_breakpoint("app.js", 42, 0);
        assert_eq!(d.breakpoints.len(), 1);
        d.remove_breakpoint(id);
        assert_eq!(d.breakpoints.len(), 0);
    }

    #[test]
    fn check_breakpoint_increments_hit_count() {
        let mut d = Debugger::new();
        d.set_breakpoint("app.js", 10, 0);
        assert!(d.check_breakpoint("app.js", 10));
        assert!(d.check_breakpoint("app.js", 10));
        assert_eq!(d.breakpoints[0].hit_count, 2);
    }

    #[test]
    fn disabled_breakpoint_skipped() {
        let mut d = Debugger::new();
        let id = d.set_breakpoint("app.js", 10, 0);
        d.enable_breakpoint(id, false);
        assert!(!d.check_breakpoint("app.js", 10));
    }

    #[test]
    fn pause_resume_state() {
        let mut d = Debugger::new();
        d.pause("app.js", 5, 0);
        assert_eq!(d.state, DebuggerState::Paused);
        assert!(d.pause_location.is_some());
        d.resume();
        assert_eq!(d.state, DebuggerState::Running);
        assert!(d.pause_location.is_none());
    }

    #[test]
    fn stepping_states() {
        let mut d = Debugger::new();
        d.step_over();
        assert_eq!(d.state, DebuggerState::SteppingOver);
        d.step_in();
        assert_eq!(d.state, DebuggerState::SteppingIn);
        d.step_out();
        assert_eq!(d.state, DebuggerState::SteppingOut);
    }
}
