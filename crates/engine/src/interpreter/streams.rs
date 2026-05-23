//! Streams API foundation - ReadableStream, WritableStream, TransformStream.
//!
//! Spec: https://streams.spec.whatwg.org/
//!
//! Pres queue + reader/writer interface. Backpressure pres high/low water mark.

use std::collections::VecDeque;
use std::rc::Rc;
use std::cell::RefCell;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StreamState {
    Readable,
    Closed,
    Errored,
}

/// ReadableStream foundation - queue of chunks + reader state.
#[derive(Debug)]
pub struct ReadableStream {
    pub state: StreamState,
    pub queue: VecDeque<Vec<u8>>,
    pub locked: bool,
    pub high_water_mark: usize,
}

impl ReadableStream {
    pub fn new(high_water_mark: usize) -> Self {
        Self {
            state: StreamState::Readable,
            queue: VecDeque::new(),
            locked: false,
            high_water_mark,
        }
    }

    /// Enqueue chunk z source. Vraci backpressure flag - true = source by mel pause.
    pub fn enqueue(&mut self, chunk: Vec<u8>) -> bool {
        if self.state != StreamState::Readable { return false; }
        self.queue.push_back(chunk);
        self.queue.len() >= self.high_water_mark
    }

    pub fn close(&mut self) {
        self.state = StreamState::Closed;
    }

    pub fn error(&mut self) {
        self.state = StreamState::Errored;
        self.queue.clear();
    }

    /// Read 1 chunk z queue. None = drained nebo not readable.
    pub fn read(&mut self) -> Option<Vec<u8>> {
        if self.state == StreamState::Errored { return None; }
        self.queue.pop_front()
    }

    pub fn desired_size(&self) -> i64 {
        self.high_water_mark as i64 - self.queue.len() as i64
    }
}

/// WritableStream foundation.
#[derive(Debug)]
pub struct WritableStream {
    pub state: StreamState,
    pub buffer: Vec<u8>,
    pub locked: bool,
}

impl WritableStream {
    pub fn new() -> Self {
        Self { state: StreamState::Readable, buffer: Vec::new(), locked: false }
    }

    pub fn write(&mut self, chunk: &[u8]) -> bool {
        if self.state != StreamState::Readable { return false; }
        self.buffer.extend_from_slice(chunk);
        true
    }

    pub fn close(&mut self) {
        self.state = StreamState::Closed;
    }
}

impl Default for WritableStream {
    fn default() -> Self { Self::new() }
}

/// TransformStream = ReadableStream + WritableStream + transformer fn.
pub struct TransformStream {
    pub readable: Rc<RefCell<ReadableStream>>,
    pub writable: Rc<RefCell<WritableStream>>,
    pub transformer: Box<dyn FnMut(&[u8]) -> Vec<u8>>,
}

impl TransformStream {
    pub fn new<F: FnMut(&[u8]) -> Vec<u8> + 'static>(transformer: F) -> Self {
        Self {
            readable: Rc::new(RefCell::new(ReadableStream::new(16))),
            writable: Rc::new(RefCell::new(WritableStream::new())),
            transformer: Box::new(transformer),
        }
    }

    /// Pipe chunk through transformer.
    pub fn process(&mut self, chunk: &[u8]) {
        let transformed = (self.transformer)(chunk);
        self.readable.borrow_mut().enqueue(transformed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readable_enqueue_read() {
        let mut s = ReadableStream::new(8);
        s.enqueue(vec![1, 2, 3]);
        assert_eq!(s.read(), Some(vec![1, 2, 3]));
        assert_eq!(s.read(), None);
    }

    #[test]
    fn backpressure_signal() {
        let mut s = ReadableStream::new(2);
        assert!(!s.enqueue(vec![1]));
        assert!(s.enqueue(vec![2])); // hit watermark = backpressure
    }

    #[test]
    fn close_blocks_enqueue() {
        let mut s = ReadableStream::new(8);
        s.close();
        assert!(!s.enqueue(vec![1]));
    }

    #[test]
    fn desired_size() {
        let mut s = ReadableStream::new(10);
        s.enqueue(vec![1]);
        s.enqueue(vec![2]);
        assert_eq!(s.desired_size(), 8);
    }

    #[test]
    fn writable_buffer_accumulates() {
        let mut w = WritableStream::new();
        w.write(b"hello");
        w.write(b" world");
        assert_eq!(w.buffer, b"hello world");
    }

    #[test]
    fn transform_pipeline() {
        let mut t = TransformStream::new(|chunk| {
            chunk.iter().map(|b| b.to_ascii_uppercase()).collect()
        });
        t.process(b"abc");
        let out = t.readable.borrow_mut().read().unwrap();
        assert_eq!(out, b"ABC");
    }
}
