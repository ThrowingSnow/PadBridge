import CryptoKit
import Foundation
import Testing
import PadCore
@testable import PadNet

struct NetPadProtocolTests {
    private let key = SymmetricKey(data: [UInt8](repeating: 7, count: 32))

    private func hex(_ data: Data) -> String { data.map { String(format: "%02x", $0) }.joined() }

    /// Same packet and expected bytes as `shared_vector` in protocol.rs.
    @Test func matchesRustSharedVector() {
        var payload = [UInt8](repeating: 0, count: 16)
        payload[0] = 0x01; payload[1] = 0x10        // A + D-pad up
        payload[2] = 255                             // LT
        payload[4] = 0xFF; payload[5] = 0x7F         // LX 32767
        payload[6] = 0x00; payload[7] = 0x80         // LY -32768
        let data = NetPadProtocol.encode(.init(kind: .state, pad: 0, seq: 42, payload: payload), key: key)
        #expect(hex(data) == "50424731010000002a000000000000000110ff00ff7f00800000000000000000"
                           + "035c93a75504bde728414e7c6c6e4fc1")
    }

    @Test func stateConversion() {
        let state = ControllerState(pressed: [.a, .dpadUp], axes: [.leftTrigger: 1, .leftX: 1, .leftY: -1])
        let payload = NetPadProtocol.statePayload(state)
        #expect(payload[0] == 0x01 && payload[1] == 0x10)
        #expect(payload[2] == 255)
        #expect(Array(payload[4..<8]) == [0xFF, 0x7F, 0x01, 0x80], "LX 32767, LY -32767")
    }

    @Test func decodeVerifiesSignature() {
        let data = NetPadProtocol.ping(pad: 0, seq: 9, timestamp: 123_456, key: key)
        let packet = NetPadProtocol.decode(data, key: key)
        #expect(packet?.kind == .ping && packet?.seq == 9)
        #expect(NetPadProtocol.timestamp(of: packet!.payload) == 123_456)

        #expect(NetPadProtocol.decode(data, key: SymmetricKey(data: [UInt8](repeating: 8, count: 32))) == nil)
        var tampered = data
        tampered[20] ^= 1
        #expect(NetPadProtocol.decode(tampered, key: key) == nil)
    }

    @Test func keyParsing() {
        #expect(NetPadKey.parse(String(repeating: "07", count: 32)) != nil)
        #expect(NetPadKey.parse("abc") == nil)
        #expect(NetPadKey.parse(String(repeating: "zz", count: 32)) == nil)
    }
}

struct NetPadMIDITests {
    private let key = SymmetricKey(data: [UInt8](repeating: 7, count: 32))

    @Test func midiPacketsRoundTrip() {
        let messages: [[UInt8]] = [[0xB0, 20, 64], [0xF8], [0xF2, 0x00, 0x01], [0xC1, 5], [0x90, 60, 100], [0x80, 60, 0]]
        var seq: UInt64 = 0
        let packets = NetPadProtocol.midi(messages, nextSeq: { seq += 1; return seq }, key: key)
        #expect(packets.count == 2, "5 messages per packet")
        let decoded = packets.flatMap { NetPadProtocol.midiMessages(NetPadProtocol.decode($0, key: key)!.payload) }
        #expect(decoded == messages)
    }

    /// Payload layout must match protocol.rs `midi_payload`.
    @Test func midiPayloadLayout() {
        let data = NetPadProtocol.midi([[0x90, 60, 127]], nextSeq: { 1 }, key: key)[0]
        #expect(Array(data[16..<20]) == [1, 0x90, 60, 127])
        #expect(data[4] == 4, "type MIDI")
    }
}

struct NetPadSettingsTests {
    @Test func oldSettingsFileKeepsValuesAndEnablesRumble() throws {
        let json = #"{"enabled":true,"host":"192.0.2.20","port":48100}"#
        let s = try JSONDecoder().decode(NetPadOutput.Settings.self, from: Data(json.utf8))
        #expect(s.enabled && s.host == "192.0.2.20" && s.rumble)
    }

    @Test func rumblePacketDecodes() {
        let key = SymmetricKey(data: [UInt8](repeating: 7, count: 32))
        let data = NetPadProtocol.encode(.init(kind: .rumble, pad: 1, seq: 5, payload: [200, 40]), key: key)
        let packet = NetPadProtocol.decode(data, key: key)
        #expect(packet?.kind == .rumble && packet?.pad == 1 && packet?.payload.prefix(2) == [200, 40])
    }
}

struct NetPadInputTests {
    private let key = SymmetricKey(data: [UInt8](repeating: 7, count: 32))

    /// Layout must match protocol.rs `input_events`.
    @Test func inputPayloadLayout() {
        var seq: UInt64 = 0
        let packets = NetPadProtocol.input([(1, 30, 1), (2, -5, 300), (3, 120, 0), (1, 0x110, 0)],
                                           nextSeq: { seq += 1; return seq }, key: key)
        #expect(packets.count == 2)
        let p = [UInt8](packets[0])
        #expect(p[4] == 5, "type input")
        #expect(Array(p[16..<27]) == [3, 1, 30, 0, 1, 0, 2, 0xFB, 0xFF, 0x2C, 0x01])
    }

    @Test func emptyInputIsAKeepalive() {
        let packets = NetPadProtocol.input([], nextSeq: { 1 }, key: key)
        #expect(packets.count == 1 && [UInt8](packets[0])[16] == 0)
    }
}
