//! padbridge-netpad: UDP -> uinput bridge for PadBridge (see protocol.rs).

mod keymouse;
mod midi;
mod pad;
mod protocol;

use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use keymouse::KeyMouse;
use midi::MidiPort;
use pad::VirtualPad;
use protocol::{Packet, PACKET_LEN, TYPE_BYE, TYPE_INPUT, TYPE_MIDI, TYPE_MIDI_IN, TYPE_PING, TYPE_PONG, TYPE_RUMBLE, TYPE_STATE};
use std::os::unix::io::AsRawFd;

/// No packet for this long: release everything so nothing keeps "running".
const SILENCE_TIMEOUT: Duration = Duration::from_millis(500);
const MAX_PADS: u8 = 4;
/// MIDI from ALSA is only forwarded to a Mac heard from this recently.
const PEER_TIMEOUT: Duration = Duration::from_secs(5);
/// Upper bound for one loop iteration: silence checks and rumble refresh.
const TICK: Duration = Duration::from_millis(20);
/// While a pad rumbles, repeat its level this often (the Mac stops after 500 ms of silence).
const RUMBLE_REFRESH: Duration = Duration::from_millis(200);

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

    /// The Mac, if heard from recently.
    fn live_peer(&self) -> Option<SocketAddr> {
        let (peer, seen) = (*self.peer.lock().unwrap())?;
        (seen.elapsed() <= PEER_TIMEOUT).then_some(peer)
    }

    /// Sends a MIDI message from ALSA to the Mac (twice, see protocol.rs).
    fn send_midi(&self, message: Vec<u8>) {
        let Some(peer) = self.live_peer() else { return };
        let payload = protocol::midi_payload(&[message]);
        for _ in 0..2 {
            self.send(TYPE_MIDI_IN, 0, payload, peer);
        }
    }

    fn send_rumble(&self, pad: u8, (strong, weak): (u8, u8), copies: usize) {
        let Some(peer) = self.live_peer() else { return };
        let mut payload = [0u8; 16];
        payload[0] = strong;
        payload[1] = weak;
        for _ in 0..copies {
            self.send(TYPE_RUMBLE, pad, payload, peer);
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

/// Keyboard/mouse emulation, created on the first input packet.
#[derive(Default)]
struct Input {
    device: Option<KeyMouse>,
    last_packet: Option<Instant>,
}

struct Slot {
    pad: VirtualPad,
    last_packet: Instant,
    idle: bool,
    rumble_sent: (u8, u8),
    rumble_sent_at: Instant,
}

fn main() {
    let args = parse_args().unwrap_or_else(|e| fail(&e));
    let key = load_key(&args.key_file).unwrap_or_else(|e| fail(&e));
    let socket = UdpSocket::bind((args.bind.as_str(), args.port)).unwrap_or_else(|e| fail(&format!("bind: {e}")));
    socket.set_nonblocking(true).unwrap();
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
    let mut input = Input::default();
    // Highest sequence seen; anything not above it is a replay or a reordered stale packet.
    let mut last_seq = 0u64;
    let mut peer: Option<SocketAddr> = None;
    let mut buf = [0u8; 512];

    loop {
        // Sleep until a datagram arrives, a game sends a rumble request, or the tick passes.
        let mut fds: Vec<libc::pollfd> = std::iter::once(socket.as_raw_fd())
            .chain(slots.values().map(|s| s.pad.fd()))
            .map(|fd| libc::pollfd { fd, events: libc::POLLIN, revents: 0 })
            .collect();
        unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, TICK.as_millis() as i32) };

        while let Ok((len, from)) = socket.recv_from(&mut buf) {
            if len == PACKET_LEN {
                if let Some(packet) = protocol::decode(&key, &buf[..len]) {
                    if packet.seq > last_seq && packet.pad < MAX_PADS {
                        last_seq = packet.seq;
                        if peer != Some(from) {
                            eprintln!("sender {from}");
                            peer = Some(from);
                        }
                        *outbox.peer.lock().unwrap() = Some((from, Instant::now()));
                        handle(&outbox, midi.as_ref(), from, packet, &mut slots, &mut input);
                    }
                }
            }
        }
        if let (Some(device), Some(last)) = (input.device.as_mut(), input.last_packet) {
            if device.holding() && last.elapsed() > SILENCE_TIMEOUT {
                let _ = device.release_all();
                eprintln!("keyboard/mouse: sender silent, released all keys");
            }
        }
        for (index, slot) in slots.iter_mut() {
            if !slot.idle && slot.last_packet.elapsed() > SILENCE_TIMEOUT {
                slot.idle = true;
                let _ = slot.pad.neutral();
                eprintln!("pad {index}: sender silent, released all inputs");
            }
            slot.pad.process_force_feedback();
            let level = slot.pad.rumble();
            let changed = level != slot.rumble_sent;
            if changed || (level != (0, 0) && slot.rumble_sent_at.elapsed() >= RUMBLE_REFRESH) {
                // "Off" goes out twice: losing it would leave the motors running until the Mac's timeout.
                outbox.send_rumble(*index, level, if level == (0, 0) { 2 } else { 1 });
                slot.rumble_sent = level;
                slot.rumble_sent_at = Instant::now();
            }
        }
    }
}

fn handle(outbox: &Outbox, midi: Option<&MidiPort>, from: SocketAddr, packet: Packet, slots: &mut HashMap<u8, Slot>, input: &mut Input) {
    match packet.kind {
        TYPE_STATE => {
            let slot = match slots.entry(packet.pad) {
                std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::hash_map::Entry::Vacant(e) => match VirtualPad::create() {
                    Ok(mut pad) => {
                        let path = pad.device_path().unwrap_or_else(|| "?".into());
                        eprintln!("pad {}: created virtual Xbox 360 pad at {path}", packet.pad);
                        e.insert(Slot { pad, last_packet: Instant::now(), idle: false, rumble_sent: (0, 0), rumble_sent_at: Instant::now() })
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
        TYPE_INPUT => {
            input.last_packet = Some(Instant::now());
            if input.device.is_none() {
                match KeyMouse::create() {
                    Ok(device) => {
                        eprintln!("created virtual keyboard/mouse");
                        input.device = Some(device);
                    }
                    Err(err) => {
                        eprintln!("cannot create keyboard/mouse device: {err}");
                        return;
                    }
                }
            }
            if let Some(device) = input.device.as_mut() {
                if let Err(err) = device.apply(&protocol::input_events(&packet.payload)) {
                    eprintln!("keyboard/mouse write failed: {err}");
                }
            }
        }
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
