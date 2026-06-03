//! Server-Sent Events (EventSource) - text/event-stream reader.
//!
//! Spec: https://html.spec.whatwg.org/multipage/server-sent-events.html
//! Parses field/value lines; emits MessageEvents with `id` + `event` + `data`.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EventSourceState {
    Connecting,
    Open,
    Closed,
}

#[derive(Debug, Clone, Default)]
pub struct EventSourceMessage {
    pub id: String,
    pub event_name: String,        // default "message"
    pub data: String,
    pub retry_ms: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct EventSourceParser {
    pub last_event_id: String,
    pub retry_ms: u64,             // default 3000 per spec
    pub leftover: String,          // unfinished line at chunk boundary
}

impl EventSourceParser {
    pub fn new() -> Self {
        Self { retry_ms: 3000, ..Self::default() }
    }

    /// Feed a chunk of bytes; returns dispatched messages.
    pub fn feed(&mut self, chunk: &str) -> Vec<EventSourceMessage> {
        let mut buf = std::mem::take(&mut self.leftover);
        buf.push_str(chunk);
        let mut out = Vec::new();
        let mut current = EventSourceMessage::default();
        let mut have_data = false;
        let mut lines = buf.split('\n').peekable();
        let mut consumed_up_to = 0usize;
        let mut pos = 0usize;
        while let Some(line) = lines.next() {
            let was_last = lines.peek().is_none();
            let line_len = line.len() + 1; // +1 for newline
            if was_last {
                // last partial line - keep as leftover
                self.leftover = line.trim_end_matches('\r').to_string();
                break;
            }
            consumed_up_to = pos + line_len;
            pos += line_len;
            let line = line.trim_end_matches('\r');
            if line.is_empty() {
                if have_data {
                    let mut m = std::mem::take(&mut current);
                    if m.event_name.is_empty() { m.event_name = "message".into(); }
                    if !m.data.is_empty() && m.data.ends_with('\n') {
                        m.data.pop();
                    }
                    out.push(m);
                    have_data = false;
                }
                continue;
            }
            if line.starts_with(':') { continue; } // comment
            let (field, value) = match line.find(':') {
                Some(i) => {
                    let v = &line[i + 1..];
                    let v = if v.starts_with(' ') { &v[1..] } else { v };
                    (&line[..i], v)
                }
                None => (line, ""),
            };
            match field {
                "data" => {
                    current.data.push_str(value);
                    current.data.push('\n');
                    have_data = true;
                }
                "event" => current.event_name = value.into(),
                "id" => {
                    if !value.contains('\0') {
                        current.id = value.into();
                        self.last_event_id = value.into();
                    }
                }
                "retry" => {
                    if let Ok(ms) = value.parse() {
                        self.retry_ms = ms;
                        current.retry_ms = Some(ms);
                    }
                }
                _ => {}
            }
        }
        let _ = consumed_up_to;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_message() {
        let mut p = EventSourceParser::new();
        let msgs = p.feed("data: hello\n\n");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].data, "hello");
        assert_eq!(msgs[0].event_name, "message");
    }

    #[test]
    fn custom_event_name() {
        let mut p = EventSourceParser::new();
        let msgs = p.feed("event: tick\ndata: payload\n\n");
        assert_eq!(msgs[0].event_name, "tick");
        assert_eq!(msgs[0].data, "payload");
    }

    #[test]
    fn id_tracked() {
        let mut p = EventSourceParser::new();
        p.feed("id: 42\ndata: x\n\n");
        assert_eq!(p.last_event_id, "42");
    }

    #[test]
    fn retry_updated() {
        let mut p = EventSourceParser::new();
        p.feed("retry: 5000\ndata: x\n\n");
        assert_eq!(p.retry_ms, 5000);
    }

    #[test]
    fn comments_skipped() {
        let mut p = EventSourceParser::new();
        let msgs = p.feed(": comment\ndata: x\n\n");
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn partial_chunk_holds_leftover() {
        let mut p = EventSourceParser::new();
        let msgs = p.feed("data: hello");
        assert!(msgs.is_empty());
        let msgs = p.feed("\n\n");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].data, "hello");
    }

    #[test]
    fn multi_data_lines_join() {
        let mut p = EventSourceParser::new();
        let msgs = p.feed("data: line1\ndata: line2\n\n");
        assert_eq!(msgs[0].data, "line1\nline2");
    }
}
