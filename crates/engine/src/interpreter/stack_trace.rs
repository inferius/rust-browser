//! V8-style stack trace capture + formatting.
//!
//! Used by Error.stack, Console API, and DevTools.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct StackFrame {
    pub function_name: String,
    pub script_id: u64,
    pub script_url: String,
    pub line: u32,
    pub column: u32,
    pub is_eval: bool,
    pub is_constructor: bool,
    pub is_async: bool,
}

impl StackFrame {
    pub fn format_v8(&self) -> String {
        let mut s = String::with_capacity(64);
        s.push_str("    at ");
        if self.is_async { s.push_str("async "); }
        if self.is_constructor { s.push_str("new "); }
        if !self.function_name.is_empty() {
            s.push_str(&self.function_name);
            s.push_str(" (");
        }
        if self.is_eval { s.push_str("eval at "); }
        s.push_str(&self.script_url);
        s.push(':');
        s.push_str(&self.line.to_string());
        s.push(':');
        s.push_str(&self.column.to_string());
        if !self.function_name.is_empty() { s.push(')'); }
        s
    }
}

#[derive(Debug, Clone)]
pub struct StackTrace {
    pub frames: Vec<StackFrame>,
    pub truncated: bool,
}

impl StackTrace {
    pub fn capture(frames: Vec<StackFrame>, limit: usize) -> Self {
        let truncated = frames.len() > limit;
        let frames = frames.into_iter().take(limit).collect();
        Self { frames, truncated }
    }

    pub fn format_v8(&self) -> String {
        let mut s = String::new();
        for f in &self.frames {
            s.push_str(&f.format_v8());
            s.push('\n');
        }
        if self.truncated { s.push_str("    ... (truncated)\n"); }
        s
    }
}

/// Async stack traces: keep parent context across promise boundaries.
#[derive(Debug, Clone, Default)]
pub struct AsyncStackChain {
    /// Each item is a "frame group" captured at an async boundary.
    pub chain: Vec<StackTrace>,
}

impl AsyncStackChain {
    pub fn new() -> Self { Self::default() }

    pub fn push_async_frame(&mut self, trace: StackTrace) {
        self.chain.push(trace);
        // Limit depth to avoid leaks.
        const MAX_DEPTH: usize = 32;
        if self.chain.len() > MAX_DEPTH {
            self.chain.remove(0);
        }
    }

    pub fn format(&self) -> String {
        self.chain.iter().rev().map(|t| t.format_v8()).collect::<Vec<_>>().join("    --- async ---\n")
    }
}

/// Per-script frame name resolution (used when emitting traces).
#[derive(Default)]
pub struct ScriptRegistry {
    pub urls: HashMap<u64, String>,
    pub next_script_id: u64,
}

impl ScriptRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn register(&mut self, url: &str) -> u64 {
        self.next_script_id += 1;
        let id = self.next_script_id;
        self.urls.insert(id, url.into());
        id
    }

    pub fn url(&self, id: u64) -> Option<&str> {
        self.urls.get(&id).map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(name: &str, line: u32, col: u32) -> StackFrame {
        StackFrame {
            function_name: name.into(),
            script_id: 1, script_url: "x.js".into(),
            line, column: col,
            is_eval: false, is_constructor: false, is_async: false,
        }
    }

    #[test]
    fn frame_format_basic() {
        let f = frame("foo", 5, 10);
        let s = f.format_v8();
        assert!(s.contains("foo"));
        assert!(s.contains("x.js:5:10"));
    }

    #[test]
    fn frame_format_async() {
        let mut f = frame("foo", 1, 1);
        f.is_async = true;
        assert!(f.format_v8().contains("async "));
    }

    #[test]
    fn trace_truncated_flag() {
        let frames: Vec<StackFrame> = (0..10).map(|i| frame(&format!("f{}", i), i, 0)).collect();
        let t = StackTrace::capture(frames, 5);
        assert_eq!(t.frames.len(), 5);
        assert!(t.truncated);
    }

    #[test]
    fn trace_format_contains_all() {
        let frames: Vec<StackFrame> = (0..3).map(|i| frame(&format!("f{}", i), i + 1, 0)).collect();
        let t = StackTrace::capture(frames, 10);
        let s = t.format_v8();
        assert!(s.contains("f0"));
        assert!(s.contains("f1"));
        assert!(s.contains("f2"));
    }

    #[test]
    fn async_chain_caps_depth() {
        let mut chain = AsyncStackChain::new();
        for _ in 0..50 {
            chain.push_async_frame(StackTrace::capture(vec![frame("x", 1, 1)], 1));
        }
        assert!(chain.chain.len() <= 32);
    }

    #[test]
    fn script_registry_round_trip() {
        let mut r = ScriptRegistry::new();
        let id = r.register("https://x.com/a.js");
        assert_eq!(r.url(id), Some("https://x.com/a.js"));
    }
}
