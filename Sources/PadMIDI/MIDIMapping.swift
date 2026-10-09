import PadCore

public enum MIDIMessageKind: String, Codable, CaseIterable, Sendable {
    case cc, note
}

public enum ButtonMode: String, Codable, CaseIterable, Sendable {
    /// Press sends on (127), release sends off (0).
    case momentary
    /// Each press flips between on and off; release sends nothing.
    case toggle
}

/// Where a button's "on" value (note velocity, or CC value) comes from.
public enum VelocitySource: String, Codable, CaseIterable, Sendable {
    /// `MIDIBinding.velocity`.
    case fixed
    /// How far the trigger is pulled at the moment of the press.
    case leftTrigger, rightTrigger
}

/// How one control is sent. Axes always send CC; `kind`, `buttonMode` and the
/// velocity fields only apply to buttons.
public struct MIDIBinding: Codable, Equatable, Sendable, Identifiable {
    public var control: Control
    public var enabled: Bool
    public var kind: MIDIMessageKind
    /// CC or note number, 0...127.
    public var number: Int
    public var buttonMode: ButtonMode
    /// Axes only: flip direction (sticks) or range (triggers).
    public var invert: Bool
    public var velocitySource: VelocitySource
    /// 1...127, used when `velocitySource == .fixed`.
    public var velocity: Int

    public var id: Control { control }

    public init(control: Control, number: Int, kind: MIDIMessageKind = .cc, enabled: Bool = true,
                buttonMode: ButtonMode = .momentary, invert: Bool = false,
                velocitySource: VelocitySource = .fixed, velocity: Int = 127) {
        self.control = control
        self.enabled = enabled
        self.kind = kind
        self.number = number
        self.buttonMode = buttonMode
        self.invert = invert
        self.velocitySource = velocitySource
        self.velocity = velocity
    }

    // Fields added after the first release default when missing, so older
    // saved mappings keep loading instead of falling back to the defaults.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        control = try c.decode(Control.self, forKey: .control)
        number = try c.decode(Int.self, forKey: .number)
        enabled = try c.decodeIfPresent(Bool.self, forKey: .enabled) ?? true
        kind = try c.decodeIfPresent(MIDIMessageKind.self, forKey: .kind) ?? .cc
        buttonMode = try c.decodeIfPresent(ButtonMode.self, forKey: .buttonMode) ?? .momentary
        invert = try c.decodeIfPresent(Bool.self, forKey: .invert) ?? false
        velocitySource = try c.decodeIfPresent(VelocitySource.self, forKey: .velocitySource) ?? .fixed
        velocity = try c.decodeIfPresent(Int.self, forKey: .velocity) ?? 127
    }
}

public struct MIDIMapping: Codable, Equatable, Sendable {
    public var enabled: Bool
    /// 1...16
    public var channel: Int
    /// Fraction of travel ignored around the centre of each stick axis (drift).
    public var stickDeadzone: Float
    /// Fraction ignored at the resting end of each trigger.
    public var triggerDeadzone: Float
    public var bindings: [MIDIBinding]

    /// Defaults use CC numbers MIDI leaves undefined, so they clash with nothing:
    /// axes on 20...25, buttons on 102...116.
    public static let `default` = MIDIMapping(
        enabled: true,
        channel: 1,
        stickDeadzone: 0.12,
        triggerDeadzone: 0.02,
        bindings: PadAxis.allCases.enumerated().map { MIDIBinding(control: .axis($1), number: 20 + $0) }
            + PadButton.allCases.enumerated().map { MIDIBinding(control: .button($1), number: 102 + $0) }
    )

    public func binding(for control: Control) -> MIDIBinding? {
        bindings.first { $0.control == control }
    }

    /// Bindings that would send the same message as another enabled binding.
    public var conflicts: Set<Control> {
        var seen: [String: Control] = [:]
        var out: Set<Control> = []
        for b in bindings where b.enabled {
            let kind: MIDIMessageKind = { if case .axis = b.control { return .cc } else { return b.kind } }()
            let key = "\(kind.rawValue)-\(b.number)"
            if let other = seen[key] {
                out.insert(other)
                out.insert(b.control)
            } else {
                seen[key] = b.control
            }
        }
        return out
    }
}

/// A MIDI 1.0 channel message.
public struct MIDIMessage: Equatable, Sendable {
    public let status: UInt8
    public let data1: UInt8
    public let data2: UInt8

    public static func controlChange(channel: Int, number: Int, value: Int) -> MIDIMessage {
        MIDIMessage(status: 0xB0 | ch(channel), data1: d(number), data2: d(value))
    }

    public static func noteOn(channel: Int, note: Int, velocity: Int) -> MIDIMessage {
        MIDIMessage(status: 0x90 | ch(channel), data1: d(note), data2: d(velocity))
    }

    public static func noteOff(channel: Int, note: Int) -> MIDIMessage {
        MIDIMessage(status: 0x80 | ch(channel), data1: d(note), data2: 0)
    }

    private static func ch(_ channel: Int) -> UInt8 { UInt8(min(max(channel, 1), 16) - 1) }
    private static func d(_ v: Int) -> UInt8 { UInt8(min(max(v, 0), 127)) }
}

/// Turns controller states into MIDI messages according to a mapping.
/// Sends only what changed after deadzone and 7-bit quantisation, so stick
/// noise does not flood the port. One mapper per connected device.
public struct MIDIMapper {
    public var mapping: MIDIMapping {
        didSet { if mapping != oldValue { reset() } }
    }

    /// Last value sent per control (0...127), for the UI's live column.
    public private(set) var lastSent: [Control: Int] = [:]
    private var toggled: Set<PadButton> = []
    private var previousPressed: Set<PadButton> = []

    public init(mapping: MIDIMapping) {
        self.mapping = mapping
    }

    /// Forget sent values so the next state re-sends everything (e.g. after a mapping edit).
    public mutating func reset() {
        lastSent.removeAll()
        toggled.removeAll()
    }

    public mutating func process(_ state: ControllerState) -> [MIDIMessage] {
        defer { previousPressed = state.pressed }
        guard mapping.enabled else { return [] }
        var out: [MIDIMessage] = []

        for binding in mapping.bindings where binding.enabled {
            switch binding.control {
            case .axis(let axis):
                let value = Self.value7(state[axis], axis: axis, mapping: mapping, invert: binding.invert)
                guard lastSent[binding.control] != value else { continue }
                lastSent[binding.control] = value
                out.append(.controlChange(channel: mapping.channel, number: binding.number, value: value))

            case .button(let button):
                let pressed = state.isPressed(button)
                let wasPressed = previousPressed.contains(button)
                let firstSend = lastSent[binding.control] == nil
                guard pressed != wasPressed || firstSend else { continue }

                let on: Bool
                switch binding.buttonMode {
                case .momentary:
                    on = pressed
                case .toggle:
                    if pressed && !wasPressed {
                        if toggled.contains(button) { toggled.remove(button) } else { toggled.insert(button) }
                    } else if !firstSend {
                        continue
                    }
                    on = toggled.contains(button)
                }
                let value = on ? velocity(for: binding, state: state) : 0
                if !firstSend || on || binding.kind == .cc {
                    out.append(message(binding, value: value))
                }
                lastSent[binding.control] = value
            }
        }
        return out
    }

    /// `value` 0 means off; anything else is the on value / velocity.
    private func message(_ binding: MIDIBinding, value: Int) -> MIDIMessage {
        switch binding.kind {
        case .cc: return .controlChange(channel: mapping.channel, number: binding.number, value: value)
        case .note: return value > 0 ? .noteOn(channel: mapping.channel, note: binding.number, velocity: value)
                                     : .noteOff(channel: mapping.channel, note: binding.number)
        }
    }

    /// 1...127: never 0, because note-on with velocity 0 means note-off.
    func velocity(for binding: MIDIBinding, state: ControllerState) -> Int {
        let raw: Int
        switch binding.velocitySource {
        case .fixed: raw = binding.velocity
        case .leftTrigger: raw = Self.value7(state[.leftTrigger], axis: .leftTrigger, mapping: mapping, invert: false)
        case .rightTrigger: raw = Self.value7(state[.rightTrigger], axis: .rightTrigger, mapping: mapping, invert: false)
        }
        return min(max(raw, 1), 127)
    }

    /// Axis value -> 0...127. Sticks centre on 64.
    static func value7(_ raw: Float, axis: PadAxis, mapping: MIDIMapping, invert: Bool) -> Int {
        let deadzone = axis.isBipolar ? mapping.stickDeadzone : mapping.triggerDeadzone
        var v = applyDeadzone(raw, deadzone)
        if invert { v = axis.isBipolar ? -v : 1 - v }
        let unit = axis.isBipolar ? (v + 1) / 2 : v
        return Int((unit * 127).rounded())
    }

    /// Zero inside the deadzone, rescaled outside so full travel still reaches ±1.
    static func applyDeadzone(_ v: Float, _ deadzone: Float) -> Float {
        let dz = min(max(deadzone, 0), 0.95)
        let magnitude = abs(v)
        guard magnitude > dz else { return 0 }
        let scaled = min((magnitude - dz) / (1 - dz), 1)
        return v < 0 ? -scaled : scaled
    }
}

// In an extension so the memberwise initializer stays available.
extension MIDIMapping {
    /// Missing fields fall back to the defaults instead of failing the whole file.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let d = MIDIMapping.default
        enabled = try c.decodeIfPresent(Bool.self, forKey: .enabled) ?? d.enabled
        channel = try c.decodeIfPresent(Int.self, forKey: .channel) ?? d.channel
        stickDeadzone = try c.decodeIfPresent(Float.self, forKey: .stickDeadzone) ?? d.stickDeadzone
        triggerDeadzone = try c.decodeIfPresent(Float.self, forKey: .triggerDeadzone) ?? d.triggerDeadzone
        bindings = try c.decodeIfPresent([MIDIBinding].self, forKey: .bindings) ?? d.bindings
        // Controls added in newer versions get their default binding.
        for binding in d.bindings where !bindings.contains(where: { $0.control == binding.control }) {
            bindings.append(binding)
        }
    }
}
