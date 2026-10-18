//! Receiving side of the link (port of the original padbridge-netpad loop):
//! a peer's pads become virtual pads here, its MIDI goes to the local port,
//! its keyboard/mouse events to a virtual keyboard/mouse. Games' rumble goes
//! back to the peer.
//!
//! Runs on its own thread with poll(), so force-feedback uploads from games
//! (which block the game until answered) get an answer within microseconds.

use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::os::unix::io::AsRawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use padbridge_core::key::NetKey;
use padbridge_core::protocol::{self, Kind, Packet, PACKET_LEN};

use super::next_seq;
use crate::engine::Msg;
use crate::platform::{self, EventSink, VirtualKeyMouse, VirtualPad};

/// No packet for this long: release everything so nothing keeps "running".
const SILENCE_TIMEOUT: Duration = Duration::from_millis(500);
const MAX_PADS: u8 = 4;
/// MIDI and rumble go back only to a peer heard from this recently.
const PEER_TIMEOUT: Duration = Duration::from_secs(5);
/// Upper bound for one loop iteration: silence checks and rumble refresh.
const TICK: Duration = Duration::from_millis(20);
/// While a pad rumbles, repeat its level this often (the peer stops after 500 ms of silence).
const RUMBLE_REFRESH: Duration = Duration::from_millis(200);
/// Replay protection: per sender address the sequence must increase, and
/// nothing older than this behind the newest packet from anyone is accepted,
/// so a captured packet cannot be replayed later from another port.
const REPLAY_WINDOW_MICROS: u64 = 60_000_000;

#[derive(Debug, Clone, Default)]
pub struct ReceiverStatus {
    pub listening: Option<u16>,
    pub error: Option<String>,
    /// The peer, if heard from within `PEER_TIMEOUT`.
    pub peer: Option<SocketAddr>,
    /// Virtual pads: (index, device description, receiving right now).
    pub pads: Vec<(u8, String, bool)>,
    pub keymouse: bool,
    pub midi_packets: u64,
}

/// Lets the engine send through the receiver's socket to the current peer.
struct Outbox {
    socket: UdpSocket,
    key: NetKey,
    peer: Mutex<Option<(SocketAddr, Instant)>>,
}

impl Outbox {
    fn send(&self, kind: Kind, pad: u8, payload: [u8; 16], to: SocketAddr, copies: usize) {
        let data = protocol::encode(&self.key.0, &Packet { kind, pad, seq: next_seq(), payload });
        for _ in 0..copies {
            let _ = self.socket.send_to(&data, to);
        }
    }

    fn live_peer(&self) -> Option<SocketAddr> {
        let (peer, seen) = (*self.peer.lock().unwrap())?;
        (seen.elapsed() <= PEER_TIMEOUT).then_some(peer)
    }
}

pub struct Receiver {
    stop: Arc<AtomicBool>,
    outbox: Arc<Outbox>,
    pub status: Arc<Mutex<ReceiverStatus>>,
    thread: Option<JoinHandle<()>>,
}

impl Receiver {
    pub fn start(port: u16, key: NetKey, events: EventSink) -> std::io::Result<Receiver> {
        let socket = UdpSocket::bind(("0.0.0.0", port))?;
        socket.set_nonblocking(true)?;
        let outbox = Arc::new(Outbox { socket: socket.try_clone()?, key: key.clone(), peer: Mutex::new(None) });
        let status = Arc::new(Mutex::new(ReceiverStatus { listening: Some(port), ..Default::default() }));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (outbox, status, stop) = (outbox.clone(), status.clone(), stop.clone());
            std::thread::Builder::new()
                .name("net-receiver".into())
                .spawn(move || Loop::new(socket, key, outbox, status, events).run(&stop))?
        };
        Ok(Receiver { stop, outbox, status, thread: Some(thread) })
    }

    /// MIDI from the local port (e.g. a DAW's clock) back to the peer, twice.
    pub fn send_midi_in(&self, message: &[u8]) {
        if let Some(peer) = self.outbox.live_peer() {
            self.outbox.send(Kind::MidiIn, 0, protocol::midi_payload(&[message.to_vec()]), peer, 2);
        }
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Slot {
    pad: Box<dyn VirtualPad>,
    name: String,
    last_packet: Instant,
    idle: bool,
    rumble_sent: (u8, u8),
    rumble_sent_at: Instant,
}

struct Loop {
    socket: UdpSocket,
    key: NetKey,
    outbox: Arc<Outbox>,
    status: Arc<Mutex<ReceiverStatus>>,
    events: EventSink,
    slots: HashMap<u8, Slot>,
    keymouse: Option<Box<dyn VirtualKeyMouse>>,
    keymouse_last: Option<Instant>,
    last_seq: HashMap<SocketAddr, u64>,
    newest_seq: u64,
    peer: Option<SocketAddr>,
    midi_packets: u64,
    /// MIDI devices of the peer by slot, and their names being assembled.
    devices: HashMap<u8, String>,
    device_names: HashMap<u8, protocol::NameAssembler>,
}

impl Loop {
    fn new(
        socket: UdpSocket,
        key: NetKey,
        outbox: Arc<Outbox>,
        status: Arc<Mutex<ReceiverStatus>>,
        events: EventSink,
    ) -> Self {
        Loop {
            socket,
            key,
            outbox,
            status,
            events,
            slots: HashMap::new(),
            keymouse: None,
            keymouse_last: None,
            last_seq: HashMap::new(),
            newest_seq: 0,
            peer: None,
            midi_packets: 0,
            devices: HashMap::new(),
            device_names: HashMap::new(),
        }
    }

    fn log(&self, line: String) {
        let _ = self.events.send(Msg::Log(line));
    }

    fn run(mut self, stop: &AtomicBool) {
        let mut buf = [0u8; 512];
        let mut last_status = Instant::now();
        while !stop.load(Ordering::Relaxed) {
            // Sleep until a datagram arrives, a game sends force feedback, or the tick passes.
            let mut fds: Vec<libc::pollfd> = std::iter::once(self.socket.as_raw_fd())
                .chain(self.slots.values().filter_map(|s| s.pad.poll_fd()))
                .map(|fd| libc::pollfd { fd, events: libc::POLLIN, revents: 0 })
                .collect();
            unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, TICK.as_millis() as i32) };

            while let Ok((len, from)) = self.socket.recv_from(&mut buf) {
                if len != PACKET_LEN {
                    continue;
                }
                let Some(packet) = protocol::decode(&self.key.0, &buf[..len]) else { continue };
                let device = matches!(packet.kind, Kind::DeviceMidi | Kind::DeviceName | Kind::DeviceRaw);
                if !self.fresh(from, packet.seq) || (!device && packet.pad >= MAX_PADS) {
                    continue;
                }
                if self.peer != Some(from) {
                    self.log(format!("Netzwerk: Gegenstelle {from}"));
                    self.peer = Some(from);
                }
                *self.outbox.peer.lock().unwrap() = Some((from, Instant::now()));
                self.handle(from, packet);
            }
            self.housekeeping();
            if last_status.elapsed() > Duration::from_millis(200) {
                last_status = Instant::now();
                self.publish();
            }
        }
        // Leave nothing pressed behind.
        for slot in self.slots.values_mut() {
            let _ = slot.pad.neutral();
        }
        if let Some(km) = self.keymouse.as_mut() {
            let _ = km.release_all();
        }
    }

    fn fresh(&mut self, from: SocketAddr, seq: u64) -> bool {
        let last = self.last_seq.entry(from).or_insert(0);
        if seq <= *last || seq + REPLAY_WINDOW_MICROS < self.newest_seq {
            return false;
        }
        *last = seq;
        self.newest_seq = self.newest_seq.max(seq);
        true
    }

    fn handle(&mut self, from: SocketAddr, packet: Packet) {
        match packet.kind {
            Kind::State => {
                if !self.slots.contains_key(&packet.pad) {
                    match platform::create_virtual_pad() {
                        Ok(mut pad) => {
                            let name = pad.describe();
                            self.log(format!("Netzwerk: virtuelles Pad {} angelegt ({name})", packet.pad + 1));
                            let now = Instant::now();
                            self.slots.insert(
                                packet.pad,
                                Slot {
                                    pad,
                                    name,
                                    last_packet: now,
                                    idle: false,
                                    rumble_sent: (0, 0),
                                    rumble_sent_at: now,
                                },
                            );
                        }
                        Err(e) => {
                            self.set_error(format!("Virtuelles Pad nicht möglich: {e}"));
                            return;
                        }
                    }
                }
                let slot = self.slots.get_mut(&packet.pad).unwrap();
                slot.last_packet = Instant::now();
                slot.idle = false;
                if let Err(e) = slot.pad.apply(protocol::wire_state(&packet.payload)) {
                    let msg = format!("Pad {}: Schreiben fehlgeschlagen: {e}", packet.pad + 1);
                    self.set_error(msg);
                }
            }
            Kind::Ping => self.outbox.send(Kind::Pong, packet.pad, packet.payload, from, 1),
            Kind::Input => {
                self.keymouse_last = Some(Instant::now());
                if self.keymouse.is_none() {
                    match platform::create_virtual_keymouse() {
                        Ok(km) => {
                            self.log("Netzwerk: virtuelle Tastatur/Maus angelegt".into());
                            self.keymouse = Some(km);
                        }
                        Err(e) => {
                            self.set_error(format!("Virtuelle Tastatur/Maus nicht möglich: {e}"));
                            return;
                        }
                    }
                }
                if let Some(km) = self.keymouse.as_mut() {
                    let _ = km.apply(&protocol::input_events(&packet.payload));
                }
            }
            Kind::Midi => {
                self.midi_packets += 1;
                for message in protocol::midi_messages(&packet.payload) {
                    let _ = self.events.send(Msg::NetMidiOut(message));
                }
            }
            Kind::DeviceName => {
                let assembler = self.device_names.entry(packet.pad).or_default();
                if let Some(name) = assembler.add(&packet.payload) {
                    if self.devices.get(&packet.pad) != Some(&name) {
                        self.log(format!("Netzwerk: MIDI-Gerät „{name}“ der Gegenstelle"));
                        self.devices.insert(packet.pad, name.clone());
                    }
                    let _ = self.events.send(Msg::NetDeviceSeen(name));
                }
            }
            Kind::DeviceMidi => {
                // Without a name yet there is no port to play it to; the name follows within a second.
                if let Some(name) = self.devices.get(&packet.pad) {
                    self.midi_packets += 1;
                    for message in protocol::midi_messages(&packet.payload) {
                        let _ = self.events.send(Msg::NetDeviceMidi(name.clone(), message));
                    }
                }
            }
            Kind::DeviceRaw => {
                if let Some(name) = self.devices.get(&packet.pad) {
                    for message in protocol::midi_messages(&packet.payload) {
                        let _ = self.events.send(Msg::NetDeviceRaw(name.clone(), message));
                    }
                }
            }
            Kind::Bye => {
                if let Some(slot) = self.slots.get_mut(&packet.pad) {
                    slot.idle = true;
                    let _ = slot.pad.neutral();
                }
            }
            _ => {}
        }
    }

    fn housekeeping(&mut self) {
        if let (Some(km), Some(last)) = (self.keymouse.as_mut(), self.keymouse_last) {
            if km.holding() && last.elapsed() > SILENCE_TIMEOUT {
                let _ = km.release_all();
                let _ =
                    self.events.send(Msg::Log("Netzwerk: Tastatur/Maus-Sender still, alle Tasten losgelassen".into()));
            }
        }
        let peer = self.outbox.live_peer();
        let mut released = Vec::new();
        for (&index, slot) in self.slots.iter_mut() {
            if !slot.idle && slot.last_packet.elapsed() > SILENCE_TIMEOUT {
                slot.idle = true;
                let _ = slot.pad.neutral();
                released.push(index);
            }
            slot.pad.process_force_feedback();
            let level = slot.pad.rumble();
            let changed = level != slot.rumble_sent;
            if changed || (level != (0, 0) && slot.rumble_sent_at.elapsed() >= RUMBLE_REFRESH) {
                if let Some(peer) = peer {
                    // "Off" twice: losing it would leave the motors running until the peer's timeout.
                    let copies = if level == (0, 0) { 2 } else { 1 };
                    self.outbox.send(Kind::Rumble, index, protocol::rumble_payload(level.0, level.1), peer, copies);
                }
                slot.rumble_sent = level;
                slot.rumble_sent_at = Instant::now();
            }
        }
        for index in released {
            self.log(format!("Netzwerk: Pad {} still, alles losgelassen", index + 1));
        }
    }

    /// Shows the error in the status and logs it, once while it stays the same
    /// (a peer's pad packets arrive 60 times a second).
    fn set_error(&self, error: String) {
        let mut status = self.status.lock().unwrap();
        if status.error.as_ref() != Some(&error) {
            self.log(error.clone());
            status.error = Some(error);
        }
    }

    fn publish(&self) {
        let mut s = self.status.lock().unwrap();
        s.peer = self.outbox.live_peer();
        let mut pads: Vec<_> = self.slots.iter().map(|(&i, slot)| (i, slot.name.clone(), !slot.idle)).collect();
        pads.sort();
        s.pads = pads;
        s.keymouse = self.keymouse.is_some();
        s.midi_packets = self.midi_packets;
    }
}
