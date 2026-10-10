//! Ported from NetPadProtocolTests.swift and padbridge-netpad's protocol tests.

use padbridge_core::key::NetKey;
use padbridge_core::keymouse::WireInput;
use padbridge_core::protocol::*;
use padbridge_core::{ControllerState, PadAxis, PadButton};

const KEY: [u8; 32] = [7u8; 32];

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sample() -> Packet {
    let mut payload = [0u8; 16];
    payload[0..2].copy_from_slice(&0x1001u16.to_le_bytes()); // A + D-pad up
    payload[2] = 255;
    payload[4..6].copy_from_slice(&32767i16.to_le_bytes());
    payload[6..8].copy_from_slice(&(-32768i16).to_le_bytes());
    Packet { kind: Kind::State, pad: 0, seq: 42, payload }
}

/// Shared with NetPadProtocolTests.swift: if this changes, every side must change.
#[test]
fn shared_vector() {
    assert_eq!(
        hex(&encode(&KEY, &sample())),
        "50424731010000002a000000000000000110ff00ff7f00800000000000000000035c93a75504bde728414e7c6c6e4fc1"
    );
}

#[test]
fn roundtrip_and_wire_state() {
    let decoded = decode(&KEY, &encode(&KEY, &sample())).unwrap();
    assert_eq!(decoded, sample());
    let w = wire_state(&decoded.payload);
    assert_eq!((w.buttons, w.lt, w.lx, w.ly), (0x1001, 255, 32767, -32768));
}

#[test]
fn rejects_wrong_key_tampering_and_unknown_types() {
    let mut bytes = encode(&KEY, &sample());
    assert!(decode(&[8u8; 32], &bytes).is_none());
    bytes[18] ^= 1;
    assert!(decode(&KEY, &bytes).is_none());
    assert!(decode(&KEY, &bytes[..47]).is_none());
}

#[test]
fn state_conversion() {
    let s = ControllerState::new(
        &[PadButton::A, PadButton::DpadUp],
        &[(PadAxis::LeftTrigger, 1.0), (PadAxis::LeftX, 1.0), (PadAxis::LeftY, -1.0)],
    );
    let p = state_payload(&s);
    assert_eq!((p[0], p[1], p[2]), (0x01, 0x10, 255));
    assert_eq!(p[4..8], [0xFF, 0x7F, 0x01, 0x80], "LX 32767, LY -32767");
    assert_eq!(state_from_payload(&p), s);
}

#[test]
fn ping_carries_timestamp() {
    let packet =
        decode(&KEY, &encode(&KEY, &Packet { kind: Kind::Ping, pad: 0, seq: 9, payload: timestamp_payload(123_456) }))
            .unwrap();
    assert_eq!((packet.kind, packet.seq, timestamp(&packet.payload)), (Kind::Ping, 9, 123_456));
}

#[test]
fn midi_payloads_roundtrip() {
    let messages = vec![
        vec![0xB0, 20, 64],
        vec![0xF8],
        vec![0xF2, 0x00, 0x01],
        vec![0xC1, 5],
        vec![0x90, 60, 100],
        vec![0x80, 60, 0],
    ];
    let payloads = midi_payloads(&messages);
    assert_eq!(payloads.len(), 2, "5 messages per packet");
    assert_eq!(payloads.iter().flat_map(midi_messages).collect::<Vec<_>>(), messages);
    // Layout must match NetPadProtocol.midi on the Mac.
    assert_eq!(midi_payload(&[vec![0x90, 60, 127]])[..4], [1, 0x90, 60, 127]);
}

#[test]
fn input_payload_layout() {
    // Same packet as the Mac's inputPayloadLayout test.
    let events =
        [WireInput { kind: 1, a: 30, b: 1 }, WireInput { kind: 2, a: -5, b: 300 }, WireInput { kind: 3, a: 120, b: 0 }];
    let payloads = input_payloads(&events);
    assert_eq!(payloads, vec![[3, 1, 30, 0, 1, 0, 2, 0xFB, 0xFF, 0x2C, 0x01, 3, 120, 0, 0, 0]]);
    assert_eq!(input_events(&payloads[0]), events);
    assert_eq!(input_payloads(&[]), vec![[0u8; 16]], "no events = one keepalive");
}

#[test]
fn rumble_levels_roundtrip() {
    assert_eq!(rumble_levels(&rumble_payload(255, 0)), (1.0, 0.0));
}

#[test]
fn key_parsing() {
    assert!(NetKey::parse(&"07".repeat(32)).is_some());
    assert!(NetKey::parse(&format!("{}\n", "ab".repeat(32))).is_some(), "trailing newline is fine");
    assert!(NetKey::parse("abc").is_none());
    assert!(NetKey::parse(&"zz".repeat(32)).is_none());
    assert_eq!(NetKey::parse(&"07".repeat(32)).unwrap().0, KEY);
    assert_eq!(format!("{:?}", NetKey(KEY)), "NetKey(…)", "Debug never prints the secret");
}
