//! padbridge-netpad: UDP -> uinput bridge for PadBridge (see protocol.rs).

mod pad;
mod protocol;

use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use pad::VirtualPad;
use protocol::{Packet, PACKET_LEN, TYPE_BYE, TYPE_PING, TYPE_PONG, TYPE_STATE};

/// No packet for this long: release everything so nothing keeps "running".
const SILENCE_TIMEOUT: Duration = Duration::from_millis(500);
const MAX_PADS: u8 = 4;

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

    let mut slots: HashMap<u8, Slot> = HashMap::new();
    // Highest sequence seen; anything not above it is a replay or a reordered stale packet.
    let mut last_seq = 0u64;
    let mut own_seq = 0u64;
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
                        handle(&socket, &key, from, packet, &mut slots, &mut own_seq);
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

fn handle(socket: &UdpSocket, key: &[u8], from: SocketAddr, packet: Packet, slots: &mut HashMap<u8, Slot>, own_seq: &mut u64) {
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
        TYPE_PING => {
            *own_seq += 1;
            let pong = Packet { kind: TYPE_PONG, pad: packet.pad, seq: *own_seq, payload: packet.payload };
            let _ = socket.send_to(&protocol::encode(key, &pong), from);
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

fn fail(message: &str) -> ! {
    eprintln!("padbridge-netpad: {message}");
    std::process::exit(1);
}
