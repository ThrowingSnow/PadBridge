import PadCore

/// Wire format of the wired Xbox 360 controller (and its licensed clones).
/// Pure byte <-> model conversion, no IOKit, so it is unit-testable.
///
/// Reference: Linux `drivers/input/joystick/xpad.c` (XTYPE_XBOX360).
public enum Xbox360Protocol {
    /// Gamepad interface: vendor-specific class, subclass 0x5D, protocol 0x01.
    /// Interfaces 1-3 on the same device (headset, security) use other protocols.
    public static let interfaceClass: UInt8 = 0xFF
    public static let interfaceSubclass: UInt8 = 0x5D
    public static let interfaceProtocol: UInt8 = 0x01

    public static let inputReportLength = 20

    /// Byte 2 / byte 3 bit layout of the input report.
    private static let byte2: [(UInt8, PadButton)] = [
        (0x01, .dpadUp), (0x02, .dpadDown), (0x04, .dpadLeft), (0x08, .dpadRight),
        (0x10, .start), (0x20, .back), (0x40, .leftStick), (0x80, .rightStick),
    ]
    private static let byte3: [(UInt8, PadButton)] = [
        (0x01, .leftShoulder), (0x02, .rightShoulder), (0x04, .guide),
        (0x10, .a), (0x20, .b), (0x40, .x), (0x80, .y),
    ]

    /// Parses an input report. Returns nil for other message types
    /// (LED status 0x01, rumble/headset status 0x02/0x03, ...).
    ///
    /// Layout: [0]=0x00 type, [1]=0x14 length, [2..3] buttons, [4] LT, [5] RT,
    /// [6..13] LX, LY, RX, RY as little-endian Int16 (up/right positive).
    public static func parseInput<C: Collection>(_ report: C) -> ControllerState? where C.Element == UInt8, C.Index == Int {
        guard report.count >= inputReportLength else { return nil }
        let r = Array(report.prefix(inputReportLength))
        guard r[0] == 0x00, r[1] == 0x14 else { return nil }

        var state = ControllerState()
        for (mask, button) in byte2 where r[2] & mask != 0 { state.pressed.insert(button) }
        for (mask, button) in byte3 where r[3] & mask != 0 { state.pressed.insert(button) }

        state[.leftTrigger] = Float(r[4]) / 255
        state[.rightTrigger] = Float(r[5]) / 255
        state[.leftX] = stick(r[6], r[7])
        state[.leftY] = stick(r[8], r[9])
        state[.rightX] = stick(r[10], r[11])
        state[.rightY] = stick(r[12], r[13])
        return state
    }

    /// Int16 LE -> -1...1. -32768 is clamped so both directions reach exactly 1.
    private static func stick(_ lo: UInt8, _ hi: UInt8) -> Float {
        let raw = Int16(bitPattern: UInt16(lo) | UInt16(hi) << 8)
        return max(-1, Float(raw) / 32767)
    }

    /// Output report driving both rumble motors.
    public static func rumbleReport(low: Float, high: Float) -> [UInt8] {
        [0x00, 0x08, 0x00, motor(low), motor(high), 0x00, 0x00, 0x00]
    }

    private static func motor(_ v: Float) -> UInt8 { UInt8((min(max(v, 0), 1) * 255).rounded()) }

    /// Output report for the ring of four LEDs around the Guide button.
    public static func ledReport(_ indicator: PlayerIndicator) -> [UInt8] {
        let pattern: UInt8
        switch indicator {
        case .off: pattern = 0x00
        case .blinkAll: pattern = 0x01
        // 0x02...0x05: quadrant 1...4 flashes, then stays on.
        case .player(let n): pattern = UInt8(0x02 + (min(max(n, 1), 4) - 1))
        case .rotate: pattern = 0x0A
        }
        return [0x01, 0x03, pattern]
    }
}
