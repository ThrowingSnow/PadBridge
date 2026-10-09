//! PadBridge net gamepad protocol v1. Must match `NetPadProtocol.swift` on the Mac.
//!
//! Every datagram is 48 bytes: a 32-byte body, then the first 16 bytes of
//! HMAC-SHA256(key, body). All integers little-endian.
//!
//! | off | size | field                                                     |
//! |-----|------|-----------------------------------------------------------|
//! | 0   | 4    | magic "PBG1"                                              |
//! | 4   | 1    | type: 1 state, 2 ping, 3 bye, 4 MIDI (Mac -> daemon),     |
//! |     |      |       0x81 pong, 0x84 MIDI (daemon -> Mac)                |
//! | 5   | 1    | pad index 0..=3                                           |
//! | 6   | 2    | reserved (0)                                              |
//! | 8   | 8    | sequence, strictly increasing per sender                  |
//! | 16  | 2    | buttons, Xbox 360 USB report layout (byte 2 | byte 3 << 8) |
//! | 18  | 1    | LT 0..=255                                                |
//! | 19  | 1    | RT 0..=255                                                |
//! | 20  | 8    | LX, LY, RX, RY as i16, up/right positive                  |
//! | 28  | 4    | reserved (0)                                              |
//!
//! Ping/pong reuse bytes 16..24 for the sender's timestamp, echoed unchanged.
//! MIDI packets: byte 16 = message count (1..=5), then five 3-byte slots, each
//! one MIDI message padded with zeros (length follows from the status byte).
//! MIDI packets are sent twice; the sequence check drops the duplicate.

use hmac::{Hmac, Mac};
use sha2::Sha256;

pub const MAGIC: &[u8; 4] = b"PBG1";
pub const BODY_LEN: usize = 32;
pub const MAC_LEN: usize = 16;
pub const PACKET_LEN: usize = BODY_LEN + MAC_LEN;

pub const TYPE_STATE: u8 = 1;
pub const TYPE_PING: u8 = 2;
pub const TYPE_BYE: u8 = 3;
pub const TYPE_MIDI: u8 = 4;
pub const TYPE_PONG: u8 = 0x81;
pub const TYPE_MIDI_IN: u8 = 0x84;

pub const MIDI_PER_PACKET: usize = 5;

/// Bytes in a MIDI message with this status byte (channel and system messages).
pub fn midi_len(status: u8) -> usize {
    match status {
        0xF8..=0xFF | 0xF6 => 1,
        0xC0..=0xDF | 0xF1 | 0xF3 => 2,
        _ => 3,
    }
}

/// MIDI messages carried in a TYPE_MIDI / TYPE_MIDI_IN payload.
pub fn midi_messages(payload: &[u8; 16]) -> Vec<Vec<u8>> {
    let count = (payload[0] as usize).min(MIDI_PER_PACKET);
    (0..count)
        .map(|i| &payload[1 + i * 3..4 + i * 3])
        .filter(|slot| slot[0] & 0x80 != 0)
        .map(|slot| slot[..midi_len(slot[0])].to_vec())
        .collect()
}

/// Packs up to five messages into one payload.
pub fn midi_payload(messages: &[Vec<u8>]) -> [u8; 16] {
    let mut payload = [0u8; 16];
    let messages = &messages[..messages.len().min(MIDI_PER_PACKET)];
    payload[0] = messages.len() as u8;
    for (i, m) in messages.iter().enumerate() {
        let n = m.len().min(3);
        payload[1 + i * 3..1 + i * 3 + n].copy_from_slice(&m[..n]);
    }
    payload
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PadState {
    pub buttons: u16,
    pub lt: u8,
    pub rt: u8,
    pub lx: i16,
    pub ly: i16,
    pub rx: i16,
    pub ry: i16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Packet {
    pub kind: u8,
    pub pad: u8,
    pub seq: u64,
    /// Raw bytes 16..32, interpreted per `kind`.
    pub payload: [u8; 16],
}

impl Packet {
    pub fn state(&self) -> PadState {
        let p = &self.payload;
        let i16_at = |o: usize| i16::from_le_bytes([p[o], p[o + 1]]);
        PadState {
            buttons: u16::from_le_bytes([p[0], p[1]]),
            lt: p[2],
            rt: p[3],
            lx: i16_at(4),
            ly: i16_at(6),
            rx: i16_at(8),
            ry: i16_at(10),
        }
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

/// Returns None for anything malformed or not signed with `key`.
pub fn decode(key: &[u8], data: &[u8]) -> Option<Packet> {
    if data.len() != PACKET_LEN || &data[..4] != MAGIC {
        return None;
    }
    let (body, tag) = data.split_at(BODY_LEN);
    let expected = mac(key, body);
    // Constant-time compare.
    if expected.iter().zip(tag).fold(0u8, |acc, (a, b)| acc | (a ^ b)) != 0 {
        return None;
    }
    let mut payload = [0u8; 16];
    payload.copy_from_slice(&body[16..32]);
    Some(Packet {
        kind: body[4],
        pad: body[5],
        seq: u64::from_le_bytes(body[8..16].try_into().unwrap()),
        payload,
    })
}

pub fn encode(key: &[u8], packet: &Packet) -> [u8; PACKET_LEN] {
    let mut out = [0u8; PACKET_LEN];
    out[..4].copy_from_slice(MAGIC);
    out[4] = packet.kind;
    out[5] = packet.pad;
    out[8..16].copy_from_slice(&packet.seq.to_le_bytes());
    out[16..32].copy_from_slice(&packet.payload);
    let tag = mac(key, &out[..BODY_LEN]);
    out[BODY_LEN..].copy_from_slice(&tag);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shared vector with the Swift tests (NetPadProtocolTests): if this
    /// changes, both sides must change together.
    pub const TEST_KEY: [u8; 32] = [7u8; 32];

    fn sample() -> Packet {
        let mut payload = [0u8; 16];
        payload[0..2].copy_from_slice(&0x1001u16.to_le_bytes()); // A + D-pad up
        payload[2] = 255;
        payload[4..6].copy_from_slice(&32767i16.to_le_bytes());
        payload[6..8].copy_from_slice(&(-32768i16).to_le_bytes());
        Packet { kind: TYPE_STATE, pad: 0, seq: 42, payload }
    }

    #[test]
    fn roundtrip() {
        let bytes = encode(&TEST_KEY, &sample());
        let decoded = decode(&TEST_KEY, &bytes).unwrap();
        assert_eq!(decoded, sample());
        let s = decoded.state();
        assert_eq!((s.buttons, s.lt, s.lx, s.ly), (0x1001, 255, 32767, -32768));
    }

    #[test]
    fn rejects_wrong_key_and_tampering() {
        let mut bytes = encode(&TEST_KEY, &sample());
        assert!(decode(&[8u8; 32], &bytes).is_none());
        bytes[18] ^= 1;
        assert!(decode(&TEST_KEY, &bytes).is_none());
    }

    #[test]
    fn midi_payload_roundtrip() {
        let msgs = vec![vec![0xB0, 20, 64], vec![0xF8], vec![0xF2, 0x00, 0x01], vec![0xC1, 5]];
        assert_eq!(midi_messages(&midi_payload(&msgs)), msgs);
        assert_eq!(midi_payload(&[vec![0x90, 60, 127]])[..4], [1, 0x90, 60, 127]);
    }

    #[test]
    fn shared_vector() {
        let hex: String = encode(&TEST_KEY, &sample()).iter().map(|b| format!("{b:02x}")).collect();
        println!("vector: {hex}");
        assert_eq!(&hex[..64], "50424731010000002a000000000000000110ff00ff7f00800000000000000000");
    }
}
