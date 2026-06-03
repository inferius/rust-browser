//! WebTransport API - bidirectional client-server pres QUIC/HTTP3.
//!
//! Spec: https://w3c.github.io/webtransport/
//! Foundation: connection state + datagram/stream queues. Real impl pres quinn crate.

use std::collections::VecDeque;
use std::rc::Rc;
use std::cell::RefCell;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransportState {
    Connecting,
    Connected,
    Closed,
    Failed,
}

pub struct WebTransport {
    pub url: String,
    pub state: TransportState,
    pub datagram_in: VecDeque<Vec<u8>>,
    pub datagram_out: VecDeque<Vec<u8>>,
    pub streams: Vec<Rc<RefCell<TransportStream>>>,
    pub max_datagram_size: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StreamDirection {
    Unidirectional,
    Bidirectional,
}

#[derive(Debug)]
pub struct TransportStream {
    pub id: u64,
    pub direction: StreamDirection,
    pub readable: VecDeque<Vec<u8>>,
    pub writable: VecDeque<Vec<u8>>,
    pub closed: bool,
}

impl WebTransport {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.into(),
            state: TransportState::Connecting,
            datagram_in: VecDeque::new(),
            datagram_out: VecDeque::new(),
            streams: Vec::new(),
            max_datagram_size: 1200, // typical MTU
        }
    }

    pub fn connect(&mut self) { self.state = TransportState::Connected; }
    pub fn close(&mut self) { self.state = TransportState::Closed; }

    pub fn send_datagram(&mut self, data: Vec<u8>) -> bool {
        if self.state != TransportState::Connected { return false; }
        if data.len() > self.max_datagram_size { return false; }
        self.datagram_out.push_back(data);
        true
    }

    pub fn receive_datagram(&mut self) -> Option<Vec<u8>> {
        self.datagram_in.pop_front()
    }

    pub fn create_stream(&mut self, direction: StreamDirection) -> Rc<RefCell<TransportStream>> {
        let id = self.streams.len() as u64 + 1;
        let s = Rc::new(RefCell::new(TransportStream {
            id, direction,
            readable: VecDeque::new(),
            writable: VecDeque::new(),
            closed: false,
        }));
        self.streams.push(Rc::clone(&s));
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_state() {
        let mut t = WebTransport::new("https://example.com");
        assert_eq!(t.state, TransportState::Connecting);
        t.connect();
        assert_eq!(t.state, TransportState::Connected);
    }

    #[test]
    fn send_datagram_after_connect() {
        let mut t = WebTransport::new("https://x.com");
        assert!(!t.send_datagram(vec![1, 2, 3])); // not connected
        t.connect();
        assert!(t.send_datagram(vec![1, 2, 3]));
        assert_eq!(t.datagram_out.len(), 1);
    }

    #[test]
    fn datagram_size_limit() {
        let mut t = WebTransport::new("https://x.com");
        t.connect();
        t.max_datagram_size = 100;
        assert!(!t.send_datagram(vec![0u8; 200]));
    }

    #[test]
    fn create_streams() {
        let mut t = WebTransport::new("https://x.com");
        t.connect();
        let s1 = t.create_stream(StreamDirection::Bidirectional);
        let s2 = t.create_stream(StreamDirection::Unidirectional);
        assert_ne!(s1.borrow().id, s2.borrow().id);
        assert_eq!(t.streams.len(), 2);
    }
}
