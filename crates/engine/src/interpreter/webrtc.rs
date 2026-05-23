//! WebRTC stub - RTCPeerConnection, RTCDataChannel.
//!
//! Real WebRTC = ICE/STUN/TURN, SDP, DTLS, SRTP. Vyzaduje libwebrtc nebo
//! webrtc-rs crate integration. Foundation: API surface stubs.

use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RtcConnectionState {
    New,
    Connecting,
    Connected,
    Disconnected,
    Failed,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RtcSignalingState {
    Stable,
    HaveLocalOffer,
    HaveRemoteOffer,
    HaveLocalPranswer,
    HaveRemotePranswer,
    Closed,
}

#[derive(Debug)]
pub struct RtcDataChannel {
    pub label: String,
    pub ready_state: RtcChannelState,
    pub buffered_amount: usize,
    pub buffer: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RtcChannelState {
    Connecting,
    Open,
    Closing,
    Closed,
}

impl RtcDataChannel {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.into(),
            ready_state: RtcChannelState::Connecting,
            buffered_amount: 0,
            buffer: Vec::new(),
        }
    }

    pub fn send(&mut self, data: &[u8]) -> bool {
        if self.ready_state != RtcChannelState::Open { return false; }
        self.buffered_amount += data.len();
        self.buffer.push(data.to_vec());
        true
    }

    pub fn open(&mut self) { self.ready_state = RtcChannelState::Open; }
    pub fn close(&mut self) { self.ready_state = RtcChannelState::Closed; }
}

pub struct RtcPeerConnection {
    pub connection_state: RtcConnectionState,
    pub signaling_state: RtcSignalingState,
    pub data_channels: HashMap<String, Rc<RefCell<RtcDataChannel>>>,
    pub local_description: Option<String>,
    pub remote_description: Option<String>,
}

impl Default for RtcPeerConnection {
    fn default() -> Self { Self::new() }
}

impl RtcPeerConnection {
    pub fn new() -> Self {
        Self {
            connection_state: RtcConnectionState::New,
            signaling_state: RtcSignalingState::Stable,
            data_channels: HashMap::new(),
            local_description: None,
            remote_description: None,
        }
    }

    pub fn create_data_channel(&mut self, label: &str) -> Rc<RefCell<RtcDataChannel>> {
        let ch = Rc::new(RefCell::new(RtcDataChannel::new(label)));
        self.data_channels.insert(label.into(), Rc::clone(&ch));
        ch
    }

    /// Stub create offer - real impl returns SDP string.
    pub fn create_offer(&mut self) -> String {
        self.signaling_state = RtcSignalingState::HaveLocalOffer;
        format!("v=0\r\no=- {} 1 IN IP4 0.0.0.0\r\nstub offer", offer_id())
    }

    pub fn set_local_description(&mut self, sdp: &str) {
        self.local_description = Some(sdp.into());
    }

    pub fn set_remote_description(&mut self, sdp: &str) {
        self.remote_description = Some(sdp.into());
        if self.signaling_state == RtcSignalingState::HaveLocalOffer {
            self.signaling_state = RtcSignalingState::Stable;
            self.connection_state = RtcConnectionState::Connected;
        }
    }

    pub fn close(&mut self) {
        self.connection_state = RtcConnectionState::Closed;
        self.signaling_state = RtcSignalingState::Closed;
        for ch in self.data_channels.values() {
            ch.borrow_mut().close();
        }
    }
}

fn offer_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_channel_lifecycle() {
        let mut ch = RtcDataChannel::new("test");
        assert_eq!(ch.ready_state, RtcChannelState::Connecting);
        assert!(!ch.send(b"data")); // not open yet
        ch.open();
        assert!(ch.send(b"hello"));
        assert_eq!(ch.buffered_amount, 5);
    }

    #[test]
    fn peer_connection_offer_answer() {
        let mut pc = RtcPeerConnection::new();
        let offer = pc.create_offer();
        assert!(offer.contains("v=0"));
        assert_eq!(pc.signaling_state, RtcSignalingState::HaveLocalOffer);
        pc.set_local_description(&offer);
        pc.set_remote_description("v=0\r\nanswer");
        assert_eq!(pc.signaling_state, RtcSignalingState::Stable);
        assert_eq!(pc.connection_state, RtcConnectionState::Connected);
    }

    #[test]
    fn data_channel_registered() {
        let mut pc = RtcPeerConnection::new();
        let _ch = pc.create_data_channel("chat");
        assert!(pc.data_channels.contains_key("chat"));
    }

    #[test]
    fn close_propagates() {
        let mut pc = RtcPeerConnection::new();
        let ch = pc.create_data_channel("c");
        ch.borrow_mut().open();
        pc.close();
        assert_eq!(pc.connection_state, RtcConnectionState::Closed);
        assert_eq!(ch.borrow().ready_state, RtcChannelState::Closed);
    }
}
