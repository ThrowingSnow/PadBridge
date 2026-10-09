import Testing
import PadCore
@testable import PadUSB

struct Xbox360ProtocolTests {
    private func report(buttons2: UInt8 = 0, buttons3: UInt8 = 0, lt: UInt8 = 0, rt: UInt8 = 0,
                        lx: Int16 = 0, ly: Int16 = 0, rx: Int16 = 0, ry: Int16 = 0) -> [UInt8] {
        func le(_ v: Int16) -> [UInt8] { let u = UInt16(bitPattern: v); return [UInt8(u & 0xFF), UInt8(u >> 8)] }
        var bytes: [UInt8] = [0x00, 0x14, buttons2, buttons3, lt, rt]
        for axis in [lx, ly, rx, ry] { bytes += le(axis) }
        bytes += [UInt8](repeating: 0, count: 6)
        return bytes
    }

    @Test func testIdleReportIsNeutral() {
        let state = Xbox360Protocol.parseInput(report())
        #expect(state == ControllerState())
    }

    @Test func testButtonBits() {
        let state = Xbox360Protocol.parseInput(report(buttons2: 0x01 | 0x10 | 0x80, buttons3: 0x04 | 0x10 | 0x80))!
        #expect(state.pressed == [.dpadUp, .start, .rightStick, .guide, .a, .y])
    }

    @Test func testAxesAreNormalised() {
        let state = Xbox360Protocol.parseInput(report(lt: 255, rt: 0, lx: 32767, ly: -32768, rx: 0, ry: -16384))!
        #expect(state[.leftTrigger] == 1)
        #expect(state[.rightTrigger] == 0)
        #expect(state[.leftX] == 1)
        #expect(state[.leftY] == -1, "-32768 must clamp to exactly -1")
        #expect(abs(state[.rightY] - -0.5) < 0.001)
    }

    @Test func testNonInputMessagesAreIgnored() {
        #expect(Xbox360Protocol.parseInput([0x01, 0x03, 0x0E]) == nil)   // LED status
        #expect(Xbox360Protocol.parseInput([0x03, 0x03, 0x00]) == nil)   // rumble status
        var wrongType = report()
        wrongType[0] = 0x08
        #expect(Xbox360Protocol.parseInput(wrongType) == nil)
    }

    @Test func testOutputReports() {
        #expect(Xbox360Protocol.rumbleReport(low: 1, high: 0) == [0x00, 0x08, 0x00, 0xFF, 0x00, 0x00, 0x00, 0x00])
        #expect(Xbox360Protocol.ledReport(.player(1)) == [0x01, 0x03, 0x02])
        #expect(Xbox360Protocol.ledReport(.player(4)) == [0x01, 0x03, 0x05])
        #expect(Xbox360Protocol.ledReport(.off) == [0x01, 0x03, 0x00])
    }

    @Test func testChangesReportOnlyWhatMoved() {
        let before = ControllerState(pressed: [.a], axes: [.leftX: 0.5])
        let after = ControllerState(pressed: [.b], axes: [.leftX: 0.5, .rightTrigger: 1])
        #expect(after.changes(since: before) == [
            .button(.a, pressed: false),
            .button(.b, pressed: true),
            .axis(.rightTrigger, value: 1),
        ])
    }
}
