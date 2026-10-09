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
