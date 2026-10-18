//! PadBridge network protocol v1. Wire-compatible with `NetPadProtocol.swift`
//! and the original `padbridge-netpad`; the shared test vector guards that.
//!
//! Every datagram is 48 bytes: a 32-byte body, then the first 16 bytes of
//! HMAC-SHA256(key, body). All integers little-endian.
//!
//! | off | size | field                                                     |
//! |-----|------|-----------------------------------------------------------|
//! | 0   | 4    | magic "PBG1"                                              |
//! | 4   | 1    | type (see `Kind`)                                         |
//! | 5   | 1    | pad index 0..=3                                           |
//! | 6   | 2    | reserved (0)                                              |
//! | 8   | 8    | sequence, strictly increasing per sender                  |
//! | 16  | 16   | payload, per type                                         |
//!
//! - State: u16 buttons (Xbox 360 report layout), LT, RT (u8), LX, LY, RX, RY
//!   (i16, up/right positive), 4 reserved.
//! - Ping/pong: u64 sender timestamp, echoed unchanged.
//! - MIDI (both directions): count byte (1..=5) + five 3-byte slots, each one
//!   message padded with zeros. Sent twice; the sequence check drops the duplicate.
//! - Input (keyboard/mouse): count byte (0..=3, 0 = keepalive for held keys) +
//!   three 5-byte events (type u8, a i16, b i16), see `WireInput`. Sent twice.
//! - Rumble: heavy motor, light motor (0..=255), repeated every 200 ms while
//!   non-zero; the receiver stops the motors after 500 ms of silence.
//! - Device MIDI: like MIDI, from one MIDI device of the sender (control
//!   surface); the pad byte is the device slot. Sent twice.
//! - Device name: names a slot, repeated every second. Byte 0 = chunk index
//!   (low nibble) and chunk count (high nibble), then 15 bytes of UTF-8 name.
//! - Device raw: like device MIDI, but what the device itself sent, for a live
//!   view on the peer. The slot `F0 77 tt` means: template changed to `tt`.

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::controls::PadAxis;
use crate::keymouse::WireInput;
use crate::state::ControllerState;
use crate::xbox360;

pub const MAGIC: &[u8; 4] = b"PBG1";
pub const BODY_LEN: usize = 32;
pub const MAC_LEN: usize = 16;
pub const PACKET_LEN: usize = BODY_LEN + MAC_LEN;
pub const DEFAULT_PORT: u16 = 48100;

pub const MIDI_PER_PACKET: usize = 5;
pub const INPUTS_PER_PACKET: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    // Sender (pad side) -> receiver (virtual pad side).
    State = 1,
    Ping = 2,
    Bye = 3,
    Midi = 4,
    Input = 5,
    DeviceMidi = 6,
    DeviceName = 7,
    DeviceRaw = 8,
    // Receiver -> sender.
    Pong = 0x81,
    Rumble = 0x82,
    MidiIn = 0x84,
}

impl Kind {
    pub fn from_u8(v: u8) -> Option<Kind> {
        Some(match v {
            1 => Kind::State,
            2 => Kind::Ping,
            3 => Kind::Bye,
            4 => Kind::Midi,
            5 => Kind::Input,
            6 => Kind::DeviceMidi,
            7 => Kind::DeviceName,
            8 => Kind::DeviceRaw,
            0x81 => Kind::Pong,
            0x82 => Kind::Rumble,
            0x84 => Kind::MidiIn,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Packet {
    pub kind: Kind,
    pub pad: u8,
    pub seq: u64,
    pub payload: [u8; 16],
}

impl Packet {
    pub fn new(kind: Kind, pad: u8, seq: u64, payload: &[u8]) -> Self {
        let mut p = [0u8; 16];
        let n = payload.len().min(16);
        p[..n].copy_from_slice(&payload[..n]);
        Packet { kind, pad, seq, payload: p }
    }
}

fn mac(key: &[u8], body: &[u8]) -> [u8; MAC_LEN] {
    let mut m = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("HMAC accepts any key length");
    m.update(body);
    let full = m.finalize().into_bytes();
    let mut out = [0u8; MAC_LEN];
    out.copy_from_slice(&full[..MAC_LEN]);
    out
}

pub fn encode(key: &[u8], packet: &Packet) -> [u8; PACKET_LEN] {
    let mut out = [0u8; PACKET_LEN];
    out[..4].copy_from_slice(MAGIC);
    out[4] = packet.kind as u8;
    out[5] = packet.pad;
    out[8..16].copy_from_slice(&packet.seq.to_le_bytes());
    out[16..32].copy_from_slice(&packet.payload);
    let tag = mac(key, &out[..BODY_LEN]);
    out[BODY_LEN..].copy_from_slice(&tag);
    out
}

/// None for anything malformed, of unknown type, or not signed with `key`.
pub fn decode(key: &[u8], data: &[u8]) -> Option<Packet> {
    if data.len() != PACKET_LEN || &data[..4] != MAGIC {
        return None;
    }
    let (body, tag) = data.split_at(BODY_LEN);
    // Constant-time compare.
    if mac(key, body).iter().zip(tag).fold(0u8, |acc, (a, b)| acc | (a ^ b)) != 0 {
        return None;
    }
    let mut payload = [0u8; 16];
    payload.copy_from_slice(&body[16..32]);
    Some(Packet {
        kind: Kind::from_u8(body[4])?,
        pad: body[5],
        seq: u64::from_le_bytes(body[8..16].try_into().unwrap()),
        payload,
    })
}

// MARK: State

pub fn state_payload(s: &ControllerState) -> [u8; 16] {
    let mut p = [0u8; 16];
    p[0..2].copy_from_slice(&xbox360::bits_from_buttons(s).to_le_bytes());
    p[2] = xbox360::trigger_to_u8(s.axis(PadAxis::LeftTrigger));
    p[3] = xbox360::trigger_to_u8(s.axis(PadAxis::RightTrigger));
    for (i, axis) in [PadAxis::LeftX, PadAxis::LeftY, PadAxis::RightX, PadAxis::RightY].into_iter().enumerate() {
        p[4 + i * 2..6 + i * 2].copy_from_slice(&xbox360::stick_to_i16(s.axis(axis)).to_le_bytes());
    }
    p
}

/// Raw 360 wire values of a state payload, for receivers that drive a
/// virtual 360 pad directly (uinput) without going through f32.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WireState {
    pub buttons: u16,
    pub lt: u8,
    pub rt: u8,
    pub lx: i16,
    pub ly: i16,
    pub rx: i16,
    pub ry: i16,
}

pub fn wire_state(p: &[u8; 16]) -> WireState {
    let i16_at = |o: usize| i16::from_le_bytes([p[o], p[o + 1]]);
    WireState {
        buttons: u16::from_le_bytes([p[0], p[1]]),
        lt: p[2],
        rt: p[3],
        lx: i16_at(4),
        ly: i16_at(6),
        rx: i16_at(8),
        ry: i16_at(10),
    }
}

pub fn state_from_payload(p: &[u8; 16]) -> ControllerState {
    let w = wire_state(p);
    let mut s = ControllerState::default();
    xbox360::buttons_from_bits(w.buttons, &mut s);
    s.set_axis(PadAxis::LeftTrigger, w.lt as f32 / 255.0);
    s.set_axis(PadAxis::RightTrigger, w.rt as f32 / 255.0);
    s.set_axis(PadAxis::LeftX, xbox360::stick_from_i16(w.lx));
    s.set_axis(PadAxis::LeftY, xbox360::stick_from_i16(w.ly));
    s.set_axis(PadAxis::RightX, xbox360::stick_from_i16(w.rx));
    s.set_axis(PadAxis::RightY, xbox360::stick_from_i16(w.ry));
    s
}

// MARK: Ping

pub fn timestamp_payload(timestamp: u64) -> [u8; 16] {
    let mut p = [0u8; 16];
    p[..8].copy_from_slice(&timestamp.to_le_bytes());
    p
}

pub fn timestamp(p: &[u8; 16]) -> u64 {
    u64::from_le_bytes(p[..8].try_into().unwrap())
}

// MARK: MIDI

/// Bytes in a MIDI message with this status byte (channel and system messages).
pub fn midi_len(status: u8) -> usize {
    match status {
        0xF8..=0xFF | 0xF6 => 1,
        0xC0..=0xDF | 0xF1 | 0xF3 => 2,
        _ => 3,
    }
}

/// Packs up to five messages into one payload (longer messages are cut to 3 bytes).
pub fn midi_payload(messages: &[Vec<u8>]) -> [u8; 16] {
    let mut p = [0u8; 16];
    let messages = &messages[..messages.len().min(MIDI_PER_PACKET)];
    p[0] = messages.len() as u8;
    for (i, m) in messages.iter().enumerate() {
        let n = m.len().min(3);
        p[1 + i * 3..1 + i * 3 + n].copy_from_slice(&m[..n]);
    }
    p
}

/// MIDI messages carried in a `Midi` / `MidiIn` payload.
pub fn midi_messages(p: &[u8; 16]) -> Vec<Vec<u8>> {
    let count = (p[0] as usize).min(MIDI_PER_PACKET);
    (0..count)
        .map(|i| &p[1 + i * 3..4 + i * 3])
        .filter(|slot| slot[0] & 0x80 != 0)
        .map(|slot| slot[..midi_len(slot[0])].to_vec())
        .collect()
}

/// Messages -> as many payloads as needed, five messages each.
pub fn midi_payloads(messages: &[Vec<u8>]) -> Vec<[u8; 16]> {
    messages.chunks(MIDI_PER_PACKET).map(midi_payload).collect()
}

// MARK: Device name

/// Stand-in for a template change (SysEx) in the device raw stream.
pub fn template_message(template: u8) -> Vec<u8> {
    vec![0xF0, 0x77, template]
}

pub fn template_of(message: &[u8]) -> Option<u8> {
    match message {
        [0xF0, 0x77, t] => Some(*t),
        _ => None,
    }
}

const NAME_CHUNK: usize = 15;
/// Longest device name sent (4 chunks; the count must fit a nibble).
pub const NAME_MAX: usize = 4 * NAME_CHUNK;

/// A device name -> chunk payloads (cut at a char boundary to `NAME_MAX` bytes).
pub fn name_payloads(name: &str) -> Vec<[u8; 16]> {
    let mut end = name.len().min(NAME_MAX);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    let bytes = &name.as_bytes()[..end];
    let chunks: Vec<&[u8]> = if bytes.is_empty() { vec![&[]] } else { bytes.chunks(NAME_CHUNK).collect() };
    let count = chunks.len() as u8;
    chunks
        .iter()
        .enumerate()
        .map(|(i, chunk)| {
            let mut p = [0u8; 16];
            p[0] = i as u8 | count << 4;
            p[1..1 + chunk.len()].copy_from_slice(chunk);
            p
        })
        .collect()
}

/// Collects name chunks; `add` returns the full name once all chunks are in.
#[derive(Debug, Default, Clone)]
pub struct NameAssembler {
    chunks: Vec<Option<Vec<u8>>>,
}

impl NameAssembler {
    pub fn add(&mut self, p: &[u8; 16]) -> Option<String> {
        let (index, count) = ((p[0] & 0x0F) as usize, (p[0] >> 4) as usize);
        if count == 0 || index >= count {
            return None;
        }
        if self.chunks.len() != count {
            self.chunks = vec![None; count];
        }
        let text = &p[1..];
        let len = text.iter().position(|&b| b == 0).unwrap_or(text.len());
        self.chunks[index] = Some(text[..len].to_vec());
        if self.chunks.iter().any(Option::is_none) {
            return None;
        }
        let bytes: Vec<u8> = self.chunks.iter().flatten().flatten().copied().collect();
        self.chunks.clear();
        String::from_utf8(bytes).ok().filter(|s| !s.is_empty())
    }
}

// MARK: Input

/// Events -> payloads, three events each. No events gives one empty keepalive.
pub fn input_payloads(events: &[WireInput]) -> Vec<[u8; 16]> {
    if events.is_empty() {
        return vec![[0u8; 16]];
    }
    events
        .chunks(INPUTS_PER_PACKET)
        .map(|chunk| {
            let mut p = [0u8; 16];
            p[0] = chunk.len() as u8;
            for (i, e) in chunk.iter().enumerate() {
                let o = 1 + i * 5;
                p[o] = e.kind;
                p[o + 1..o + 3].copy_from_slice(&e.a.to_le_bytes());
                p[o + 3..o + 5].copy_from_slice(&e.b.to_le_bytes());
            }
            p
        })
        .collect()
}

pub fn input_events(p: &[u8; 16]) -> Vec<WireInput> {
    let count = (p[0] as usize).min(INPUTS_PER_PACKET);
    (0..count)
        .map(|i| {
            let e = &p[1 + i * 5..6 + i * 5];
            WireInput { kind: e[0], a: i16::from_le_bytes([e[1], e[2]]), b: i16::from_le_bytes([e[3], e[4]]) }
        })
        .collect()
}

// MARK: Rumble

pub fn rumble_payload(heavy: u8, light: u8) -> [u8; 16] {
    let mut p = [0u8; 16];
    p[0] = heavy;
    p[1] = light;
    p
}

/// (heavy, light) motor levels as 0..=1.
pub fn rumble_levels(p: &[u8; 16]) -> (f32, f32) {
    (p[0] as f32 / 255.0, p[1] as f32 / 255.0)
}
