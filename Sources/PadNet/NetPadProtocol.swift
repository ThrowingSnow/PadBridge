import CryptoKit
import Foundation
import PadCore

/// PadBridge net gamepad protocol v1. Must match `protocol.rs` in
/// padbridge-netpad on the Linux side; the shared test vector guards that.
///
/// 48-byte datagrams: 32-byte body + first 16 bytes of HMAC-SHA256(key, body).
/// Little-endian. Body: "PBG1", type, pad, 2 reserved, u64 seq, 16 payload bytes.
/// State payload: u16 buttons (Xbox 360 report layout), LT, RT, LX, LY, RX, RY
/// (i16, up/right positive), 4 reserved. Ping/pong: u64 timestamp, echoed.
/// MIDI (both directions): count byte + five 3-byte slots; sent twice, the
/// receiver's sequence check drops the duplicate.
public enum NetPadProtocol {
    public static let packetLength = 48
    static let bodyLength = 32
    static let magic: [UInt8] = Array("PBG1".utf8)

    public enum Kind: UInt8 {
        case state = 1, ping = 2, bye = 3, midi = 4
        case pong = 0x81, midiIn = 0x84
    }

    public struct Packet: Equatable {
        public var kind: Kind
        public var pad: UInt8
        public var seq: UInt64
        public var payload: [UInt8]   // 16 bytes
    }

    // MARK: Encoding

    public static func encode(_ packet: Packet, key: SymmetricKey) -> Data {
        var body = magic + [packet.kind.rawValue, packet.pad, 0, 0]
        body += withUnsafeBytes(of: packet.seq.littleEndian, Array.init)
        body += packet.payload.prefix(16) + [UInt8](repeating: 0, count: max(0, 16 - packet.payload.count))
        let tag = HMAC<SHA256>.authenticationCode(for: body, using: key)
        return Data(body + Array(tag).prefix(16))
    }

    public static func state(pad: UInt8, seq: UInt64, state: ControllerState, key: SymmetricKey) -> Data {
        encode(Packet(kind: .state, pad: pad, seq: seq, payload: statePayload(state)), key: key)
    }

    public static func ping(pad: UInt8, seq: UInt64, timestamp: UInt64, key: SymmetricKey) -> Data {
        encode(Packet(kind: .ping, pad: pad, seq: seq, payload: le(timestamp) + [UInt8](repeating: 0, count: 8)), key: key)
    }

    public static func bye(pad: UInt8, seq: UInt64, key: SymmetricKey) -> Data {
        encode(Packet(kind: .bye, pad: pad, seq: seq, payload: []), key: key)
    }

    static let midiPerPacket = 5

    /// Raw MIDI messages -> as many MIDI packets as needed (5 messages each).
    /// `nextSeq` is called once per packet.
    public static func midi(_ messages: [[UInt8]], nextSeq: () -> UInt64, key: SymmetricKey) -> [Data] {
        stride(from: 0, to: messages.count, by: midiPerPacket).map { start in
            let chunk = messages[start..<min(start + midiPerPacket, messages.count)]
            var payload = [UInt8(chunk.count)]
            for message in chunk { payload += Array(message.prefix(3)) + [UInt8](repeating: 0, count: 3 - min(message.count, 3)) }
            return encode(Packet(kind: .midi, pad: 0, seq: nextSeq(), payload: payload), key: key)
        }
    }

    /// MIDI messages carried in a `.midi` / `.midiIn` payload.
    public static func midiMessages(_ payload: [UInt8]) -> [[UInt8]] {
        guard let count = payload.first else { return [] }
        return (0..<min(Int(count), midiPerPacket)).compactMap { i in
            let slot = Array(payload.dropFirst(1 + i * 3).prefix(3))
            guard let status = slot.first, status & 0x80 != 0 else { return nil }
            return Array(slot.prefix(midiLength(status)))
        }
    }

    /// Bytes in a MIDI message with this status byte (same rule as protocol.rs).
    static func midiLength(_ status: UInt8) -> Int {
        switch status {
        case 0xF8...0xFF, 0xF6: return 1
        case 0xC0...0xDF, 0xF1, 0xF3: return 2
        default: return 3
        }
    }

    /// Canonical state -> 360 wire values.
    static func statePayload(_ s: ControllerState) -> [UInt8] {
        var buttons: UInt16 = 0
        for (mask, button) in buttonBits where s.isPressed(button) { buttons |= mask }
        func stick(_ v: Float) -> Int16 { Int16((min(max(v, -1), 1) * 32767).rounded()) }
        func trigger(_ v: Float) -> UInt8 { UInt8((min(max(v, 0), 1) * 255).rounded()) }
        var out: [UInt8] = le(buttons)
        out.append(trigger(s[.leftTrigger]))
        out.append(trigger(s[.rightTrigger]))
        for axis in [PadAxis.leftX, .leftY, .rightX, .rightY] { out += le(stick(s[axis])) }
        out += [0, 0, 0, 0]
        return out
    }

    /// Same bit layout as the Xbox 360 USB input report (byte 2 | byte 3 << 8).
    static let buttonBits: [(UInt16, PadButton)] = [
        (0x0001, .dpadUp), (0x0002, .dpadDown), (0x0004, .dpadLeft), (0x0008, .dpadRight),
        (0x0010, .start), (0x0020, .back), (0x0040, .leftStick), (0x0080, .rightStick),
        (0x0100, .leftShoulder), (0x0200, .rightShoulder), (0x0400, .guide),
        (0x1000, .a), (0x2000, .b), (0x4000, .x), (0x8000, .y),
    ]

    // MARK: Decoding

    /// Nil unless the datagram is well-formed and signed with `key`.
    public static func decode(_ data: Data, key: SymmetricKey) -> Packet? {
        let bytes = [UInt8](data)
        guard bytes.count == packetLength, Array(bytes[0..<4]) == magic,
              let kind = Kind(rawValue: bytes[4]) else { return nil }
        let body = Array(bytes[0..<bodyLength])
        let expected = Array(HMAC<SHA256>.authenticationCode(for: body, using: key)).prefix(16)
        // Constant-time compare.
        guard zip(expected, bytes[bodyLength...]).reduce(0, { $0 | ($1.0 ^ $1.1) }) == 0 else { return nil }
        let seq = bytes[8..<16].enumerated().reduce(UInt64(0)) { $0 | UInt64($1.element) << (8 * UInt64($1.offset)) }
        return Packet(kind: kind, pad: bytes[5], seq: seq, payload: Array(bytes[16..<32]))
    }

    static func timestamp(of payload: [UInt8]) -> UInt64 {
        payload.prefix(8).enumerated().reduce(UInt64(0)) { $0 | UInt64($1.element) << (8 * UInt64($1.offset)) }
    }

    private static func le<T: FixedWidthInteger>(_ v: T) -> [UInt8] {
        withUnsafeBytes(of: v.littleEndian, Array.init)
    }
}
