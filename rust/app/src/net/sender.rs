//! Sending side of the link (port of NetPadOutput.swift). The engine owns a
//! `Link` while anything goes to the peer (pad, MIDI or keyboard/mouse) and
//! drives its timers; a small thread receives the peer's answers.

use std::io;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use padbridge_core::key::NetKey;
use padbridge_core::keymouse::WireInput;
use padbridge_core::protocol::{self, Kind, Packet};
use padbridge_core::ControllerState;

use super::next_seq;
use crate::engine::Msg;
use crate::platform::EventSink;

/// The peer counts as answering if a pong arrived this recently.
const ANSWER_WINDOW: Duration = Duration::from_secs(3);

pub struct Link {
    socket: UdpSocket,
    key: NetKey,
    pub peer: SocketAddr,
    /// Identifies this link's messages; answers to an older link are ignored.
    pub generation: u64,
    alive: Arc<AtomicBool>,
    started: Instant,
    last_inbound_seq: u64,
    last_pong: Option<Instant>,
    latency_ms: Option<f64>,
}

impl Link {
    pub fn open(host: &str, port: u16, key: NetKey, generation: u64, events: EventSink) -> io::Result<Link> {
        let peer = (host.trim(), port)
            .to_socket_addrs()?
            .find(|a| a.is_ipv4())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Host nicht gefunden"))?;
        let socket = UdpSocket::bind(("0.0.0.0", 0))?;
        socket.connect(peer)?;
        let alive = Arc::new(AtomicBool::new(true));
        {
            let socket = socket.try_clone()?;
            let alive = alive.clone();
            let key = key.clone();
            socket.set_read_timeout(Some(Duration::from_millis(500)))?;
            std::thread::Builder::new().name("net-sender-rx".into()).spawn(move || {
                let mut buf = [0u8; 256];
                while alive.load(Ordering::Relaxed) {
                    // Errors here are timeouts or "connection refused" (ICMP): keep listening.
                    if let Ok(n) = socket.recv(&mut buf) {
                        if let Some(packet) = protocol::decode(&key.0, &buf[..n]) {
                            if events.send(Msg::Peer(generation, packet)).is_err() {
                                return;
                            }
                        }
                    }
                }
            })?;
        }
        Ok(Link {
            socket,
            key,
            peer,
            generation,
            alive,
            started: Instant::now(),
            last_inbound_seq: 0,
            last_pong: None,
            latency_ms: None,
        })
    }

    fn send(&self, kind: Kind, pad: u8, payload: [u8; 16], copies: usize) {
        let data = protocol::encode(&self.key.0, &Packet { kind, pad, seq: next_seq(), payload });
        for _ in 0..copies {
            // Errors (no route, peer not listening) show up as a missing pong.
            let _ = self.socket.send(&data);
        }
    }

    pub fn send_state(&self, pad: u8, state: &ControllerState) {
        self.send(Kind::State, pad, protocol::state_payload(state), 1);
    }

    pub fn send_bye(&self, pad: u8) {
        self.send(Kind::Bye, pad, [0; 16], 1);
    }

    pub fn send_ping(&self) {
        let now = self.started.elapsed().as_nanos() as u64;
        self.send(Kind::Ping, 0, protocol::timestamp_payload(now), 1);
    }

    /// Twice: the peer drops the duplicate, but one loss no longer loses a note-off.
    pub fn send_midi(&self, messages: &[Vec<u8>]) {
        for payload in protocol::midi_payloads(messages) {
            self.send(Kind::Midi, 0, payload, 2);
        }
    }

    /// MIDI of one device (slot) for its own port on the peer; twice, like MIDI.
    pub fn send_device_midi(&self, slot: u8, messages: &[Vec<u8>]) {
        for payload in protocol::midi_payloads(messages) {
            self.send(Kind::DeviceMidi, slot, payload, 2);
        }
    }

    /// What the device sent, for the peer's live view; once (a lost value heals with the next).
    pub fn send_device_raw(&self, slot: u8, messages: &[Vec<u8>]) {
        for payload in protocol::midi_payloads(messages) {
            self.send(Kind::DeviceRaw, slot, payload, 1);
        }
    }

    pub fn send_device_name(&self, slot: u8, name: &str) {
        for payload in protocol::name_payloads(name) {
            self.send(Kind::DeviceName, slot, payload, 1);
        }
    }

    /// Twice, like MIDI: a lost key-up would leave a key stuck. Empty = keepalive.
    pub fn send_input(&self, events: &[WireInput]) {
        for payload in protocol::input_payloads(events) {
            self.send(Kind::Input, 0, payload, 2);
        }
    }

    /// Filters replays and handles pongs; returns packets the engine must act on.
    pub fn inbound(&mut self, packet: Packet) -> Option<Packet> {
        if packet.seq <= self.last_inbound_seq {
            return None;
        }
        self.last_inbound_seq = packet.seq;
        if packet.kind == Kind::Pong {
            let sent = protocol::timestamp(&packet.payload);
            let now = self.started.elapsed().as_nanos() as u64;
            if now >= sent {
                self.latency_ms = Some((now - sent) as f64 / 1e6);
            }
            self.last_pong = Some(Instant::now());
            return None;
        }
        Some(packet)
    }

    pub fn answering(&self) -> bool {
        self.last_pong.is_some_and(|t| t.elapsed() < ANSWER_WINDOW)
    }

    pub fn latency_ms(&self) -> Option<f64> {
        self.answering().then_some(self.latency_ms).flatten()
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}
