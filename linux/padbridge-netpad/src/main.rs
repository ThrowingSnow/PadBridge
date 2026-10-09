//! padbridge-netpad: UDP -> uinput bridge for PadBridge (see protocol.rs).

mod midi;
mod pad;
mod protocol;

use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use midi::MidiPort;
use pad::VirtualPad;
use protocol::{Packet, PACKET_LEN, TYPE_BYE, TYPE_MIDI, TYPE_MIDI_IN, TYPE_PING, TYPE_PONG, TYPE_STATE};

/// No packet for this long: release everything so nothing keeps "running".
const SILENCE_TIMEOUT: Duration = Duration::from_millis(500);
const MAX_PADS: u8 = 4;
/// MIDI from ALSA is only forwarded to a Mac heard from this recently.
const PEER_TIMEOUT: Duration = Duration::from_secs(5);

/// Everything both the UDP loop and the ALSA thread need to send to the Mac.
struct Outbox {
    socket: UdpSocket,
    key: Vec<u8>,
    /// Last sequence sent; see `next_seq`.
    seq: AtomicU64,
    peer: Mutex<Option<(SocketAddr, Instant)>>,
}

impl Outbox {
    /// Wall-clock microseconds, but always increasing (same rule as the Mac).
    fn next_seq(&self) -> u64 {
        let now = now_micros();
        let mut current = self.seq.load(Ordering::Relaxed);
        loop {
            let next = (current + 1).max(now);
            match self.seq.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => return next,
                Err(actual) => current = actual,
            }
        }
    }

    fn send(&self, kind: u8, pad: u8, payload: [u8; 16], to: SocketAddr) {
        let packet = Packet { kind, pad, seq: self.next_seq(), payload };
        let _ = self.socket.send_to(&protocol::encode(&self.key, &packet), to);
    }

    /// Sends a MIDI message from ALSA to the Mac (twice, see protocol.rs).
    fn send_midi(&self, message: Vec<u8>) {
        let Some((peer, seen)) = *self.peer.lock().unwrap() else { return };
        if seen.elapsed() > PEER_TIMEOUT {
            return;
        }
        let payload = protocol::midi_payload(&[message]);
        for _ in 0..2 {
            self.send(TYPE_MIDI_IN, 0, payload, peer);
        }
    }
}

struct Args {
    bind: String,
    port: u16,
    key_file: PathBuf,
}

fn parse_args() -> Result<Args, String> {
    let home = std::env::var("HOME").map_err(|_| "HOME not set")?;
    let mut args = Args {
        bind: "0.0.0.0".into(),
        port: 48100,
        key_file: PathBuf::from(home).join(".config/padbridge/netpad.key"),
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = || it.next().ok_or(format!("{flag} needs a value"));
        match flag.as_str() {
            "--bind" => args.bind = value()?,
            "--port" => args.port = value()?.parse().map_err(|e| format!("--port: {e}"))?,
            "--key" => args.key_file = PathBuf::from(value()?),
            "-h" | "--help" => {
                println!("usage: padbridge-netpad [--bind ADDR] [--port 48100] [--key ~/.config/padbridge/netpad.key]");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(args)
}

/// Key file: 64 hex characters (32 bytes), written by PadBridge on the Mac.
fn load_key(path: &PathBuf) -> Result<Vec<u8>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let hex = text.trim();
    if hex.len() != 64 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("{}: expected 64 hex characters", path.display()));
    }
    Ok((0..32).map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap()).collect())
}

struct Slot {
    pad: VirtualPad,
    last_packet: Instant,
    idle: bool,
}

fn main() {
    let args = parse_args().unwrap_or_else(|e| fail(&e));
    let key = load_key(&args.key_file).unwrap_or_else(|e| fail(&e));
    let socket = UdpSocket::bind((args.bind.as_str(), args.port)).unwrap_or_else(|e| fail(&format!("bind: {e}")));
    socket.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
    eprintln!("padbridge-netpad listening on {}:{}", args.bind, args.port);

    let outbox = Arc::new(Outbox {
        socket: socket.try_clone().unwrap_or_else(|e| fail(&format!("socket: {e}"))),
        key: key.clone(),
        seq: AtomicU64::new(0),
        peer: Mutex::new(None),
    });
    let midi = {
        let outbox = outbox.clone();
        match MidiPort::start(move |message| outbox.send_midi(message)) {
            Ok(port) => {
                eprintln!("ALSA MIDI port \"PadBridge\" ready");
                Some(port)
            }
            Err(e) => {
                eprintln!("MIDI disabled: {e}");
                None
            }
        }
    };

    let mut slots: HashMap<u8, Slot> = HashMap::new();
    // Highest sequence seen; anything not above it is a replay or a reordered stale packet.
    let mut last_seq = 0u64;
    let mut peer: Option<SocketAddr> = None;
    let mut buf = [0u8; 512];

    loop {
        if let Ok((len, from)) = socket.recv_from(&mut buf) {
            if len == PACKET_LEN {
                if let Some(packet) = protocol::decode(&key, &buf[..len]) {
                    if packet.seq > last_seq && packet.pad < MAX_PADS {
                        last_seq = packet.seq;
                        if peer != Some(from) {
                            eprintln!("sender {from}");
                            peer = Some(from);
                        }
                        *outbox.peer.lock().unwrap() = Some((from, Instant::now()));
                        handle(&outbox, midi.as_ref(), from, packet, &mut slots);
                    }
                }
            }
        }
        for (index, slot) in slots.iter_mut() {
            if !slot.idle && slot.last_packet.elapsed() > SILENCE_TIMEOUT {
                slot.idle = true;
                let _ = slot.pad.neutral();
                eprintln!("pad {index}: sender silent, released all inputs");
            }
        }
    }
}

fn handle(outbox: &Outbox, midi: Option<&MidiPort>, from: SocketAddr, packet: Packet, slots: &mut HashMap<u8, Slot>) {
    match packet.kind {
        TYPE_STATE => {
            let slot = match slots.entry(packet.pad) {
                std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::hash_map::Entry::Vacant(e) => match VirtualPad::create() {
                    Ok(mut pad) => {
                        let path = pad.device_path().unwrap_or_else(|| "?".into());
                        eprintln!("pad {}: created virtual Xbox 360 pad at {path}", packet.pad);
                        e.insert(Slot { pad, last_packet: Instant::now(), idle: false })
                    }
                    Err(err) => {
                        eprintln!("pad {}: cannot create uinput device: {err}", packet.pad);
                        return;
                    }
                },
            };
            slot.last_packet = Instant::now();
            slot.idle = false;
            if let Err(err) = slot.pad.apply(packet.state()) {
                eprintln!("pad {}: write failed: {err}", packet.pad);
            }
        }
        TYPE_PING => outbox.send(TYPE_PONG, packet.pad, packet.payload, from),
        TYPE_MIDI => {
            if let Some(midi) = midi {
                for message in protocol::midi_messages(&packet.payload) {
                    midi.send(message);
                }
            }
        }
        TYPE_BYE => {
            if let Some(slot) = slots.get_mut(&packet.pad) {
                slot.idle = true;
                let _ = slot.pad.neutral();
            }
        }
        _ => {}
    }
}

fn now_micros() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_micros() as u64).unwrap_or(0)
}

fn fail(message: &str) -> ! {
    eprintln!("padbridge-netpad: {message}");
    std::process::exit(1);
}
