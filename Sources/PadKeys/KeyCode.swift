/// Keys PadBridge can emit, with their codes on both output targets:
/// macOS virtual key codes (Carbon `kVK_*`, US layout positions) and Linux
/// input event codes (`KEY_*` in linux/input-event-codes.h).
public enum KeyCode: String, Codable, CaseIterable, Sendable, Hashable {
    case a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p, q, r, s, t, u, v, w, x, y, z
    case digit1, digit2, digit3, digit4, digit5, digit6, digit7, digit8, digit9, digit0
    case f1, f2, f3, f4, f5, f6, f7, f8, f9, f10, f11, f12
    case escape, tab, space, enter, backspace, forwardDelete
    case up, down, left, right, home, end, pageUp, pageDown
    case minus, equal, leftBracket, rightBracket, semicolon, quote, grave, backslash, comma, period, slash
    case leftShift, leftControl, leftOption, leftCommand, capsLock

    /// (macOS kVK, Linux KEY_*)
    private var codes: (mac: UInt16, linux: UInt16) {
        switch self {
        case .a: return (0, 30); case .b: return (11, 48); case .c: return (8, 46); case .d: return (2, 32)
        case .e: return (14, 18); case .f: return (3, 33); case .g: return (5, 34); case .h: return (4, 35)
        case .i: return (34, 23); case .j: return (38, 36); case .k: return (40, 37); case .l: return (37, 38)
        case .m: return (46, 50); case .n: return (45, 49); case .o: return (31, 24); case .p: return (35, 25)
        case .q: return (12, 16); case .r: return (15, 19); case .s: return (1, 31); case .t: return (17, 20)
        case .u: return (32, 22); case .v: return (9, 47); case .w: return (13, 17); case .x: return (7, 45)
        case .y: return (16, 21); case .z: return (6, 44)
        case .digit1: return (18, 2); case .digit2: return (19, 3); case .digit3: return (20, 4)
        case .digit4: return (21, 5); case .digit5: return (23, 6); case .digit6: return (22, 7)
        case .digit7: return (26, 8); case .digit8: return (28, 9); case .digit9: return (25, 10)
        case .digit0: return (29, 11)
        case .f1: return (122, 59); case .f2: return (120, 60); case .f3: return (99, 61); case .f4: return (118, 62)
        case .f5: return (96, 63); case .f6: return (97, 64); case .f7: return (98, 65); case .f8: return (100, 66)
        case .f9: return (101, 67); case .f10: return (109, 68); case .f11: return (103, 87); case .f12: return (111, 88)
        case .escape: return (53, 1); case .tab: return (48, 15); case .space: return (49, 57)
        case .enter: return (36, 28); case .backspace: return (51, 14); case .forwardDelete: return (117, 111)
        case .up: return (126, 103); case .down: return (125, 108); case .left: return (123, 105)
        case .right: return (124, 106); case .home: return (115, 102); case .end: return (119, 107)
        case .pageUp: return (116, 104); case .pageDown: return (121, 109)
        case .minus: return (27, 12); case .equal: return (24, 13); case .leftBracket: return (33, 26)
        case .rightBracket: return (30, 27); case .semicolon: return (41, 39); case .quote: return (39, 40)
        case .grave: return (50, 41); case .backslash: return (42, 43); case .comma: return (43, 51)
        case .period: return (47, 52); case .slash: return (44, 53)
        case .leftShift: return (56, 42); case .leftControl: return (59, 29); case .leftOption: return (58, 56)
        case .leftCommand: return (55, 125); case .capsLock: return (57, 58)
        }
    }

    public var macCode: UInt16 { codes.mac }
    public var linuxCode: UInt16 { codes.linux }

    public var displayName: String {
        switch self {
        case .digit1, .digit2, .digit3, .digit4, .digit5, .digit6, .digit7, .digit8, .digit9, .digit0:
            return String(rawValue.last!)
        case .f1, .f2, .f3, .f4, .f5, .f6, .f7, .f8, .f9, .f10, .f11, .f12: return rawValue.uppercased()
        case .escape: return "Esc"
        case .tab: return "Tab"
        case .space: return "Leertaste"
        case .enter: return "Enter"
        case .backspace: return "⌫ Backspace"
        case .forwardDelete: return "⌦ Entf"
        case .up: return "↑"
        case .down: return "↓"
        case .left: return "←"
        case .right: return "→"
        case .home: return "Home"
        case .end: return "End"
        case .pageUp: return "Page Up"
        case .pageDown: return "Page Down"
        case .minus: return "-"
        case .equal: return "="
        case .leftBracket: return "["
        case .rightBracket: return "]"
        case .semicolon: return ";"
        case .quote: return "'"
        case .grave: return "`"
        case .backslash: return "\\"
        case .comma: return ","
        case .period: return "."
        case .slash: return "/"
        case .leftShift: return "⇧ Shift"
        case .leftControl: return "⌃ Ctrl"
        case .leftOption: return "⌥ Alt/Option"
        case .leftCommand: return "⌘ Cmd/Super"
        case .capsLock: return "Caps Lock"
        default: return rawValue.uppercased()
        }
    }
}

public enum Modifier: String, Codable, CaseIterable, Sendable, Hashable {
    case shift, control, option, command

    public var key: KeyCode {
        switch self {
        case .shift: return .leftShift
        case .control: return .leftControl
        case .option: return .leftOption
        case .command: return .leftCommand
        }
    }

    public var symbol: String {
        switch self {
        case .shift: return "⇧"
        case .control: return "⌃"
        case .option: return "⌥"
        case .command: return "⌘"
        }
    }
}

public enum MouseButton: String, Codable, CaseIterable, Sendable, Hashable {
    case left, right, middle

    /// Linux BTN_LEFT / BTN_RIGHT / BTN_MIDDLE (sent as key events).
    public var linuxCode: UInt16 {
        switch self {
        case .left: return 0x110
        case .right: return 0x111
        case .middle: return 0x112
        }
    }

    public var displayName: String {
        switch self {
        case .left: return "Linksklick"
        case .right: return "Rechtsklick"
        case .middle: return "Mittelklick"
        }
    }
}
