import PadCore

/// Where keyboard/mouse events are produced.
public enum EmulationTarget: String, Codable, CaseIterable, Sendable {
    /// CGEvents on this Mac (Moonlight forwards them to the stream).
    case local
    /// Virtual keyboard/mouse on the Linux host via padbridge-netpad.
    case network
}

public enum StickRole: String, Codable, CaseIterable, Sendable {
    case none, mouse, scroll, wasd, arrows

    public var displayName: String {
        switch self {
        case .none: return "Aus"
        case .mouse: return "Maus"
        case .scroll: return "Scrollen"
        case .wasd: return "WASD"
        case .arrows: return "Pfeiltasten"
        }
    }

    /// Up, down, left, right keys for the key roles.
    var keys: (up: KeyCode, down: KeyCode, left: KeyCode, right: KeyCode)? {
        switch self {
        case .wasd: return (.w, .s, .a, .d)
        case .arrows: return (.up, .down, .left, .right)
        default: return nil
        }
    }
}

public enum KeyAction: Codable, Equatable, Sendable, Hashable {
    case none
    case mouse(MouseButton)
    case key(KeyCode, modifiers: [Modifier])

    public var displayName: String {
        switch self {
        case .none: return "–"
        case .mouse(let button): return button.displayName
        case .key(let key, let modifiers): return modifiers.map(\.symbol).joined() + key.displayName
        }
    }
}

public struct KeyBinding: Codable, Equatable, Sendable, Identifiable {
    /// A button, or `.axis(.leftTrigger / .rightTrigger)` used as a button.
    public var control: Control
    public var action: KeyAction
    public var id: Control { control }

    public init(control: Control, action: KeyAction) {
        self.control = control
        self.action = action
    }
}

public struct KeyMouseMapping: Codable, Equatable, Sendable {
    public var enabled: Bool
    public var target: EmulationTarget
    public var leftStick: StickRole
    public var rightStick: StickRole
    /// Pointer speed at full deflection, in points per second.
    public var mouseSpeed: Double
    /// Scroll speed at full deflection, in wheel detents per second.
    public var scrollSpeed: Double
    /// Response curve exponent: 1 = linear, higher = finer control near the centre.
    public var curve: Double
    public var stickDeadzone: Float
    public var bindings: [KeyBinding]

    /// Controls that can carry a key action: all buttons plus both triggers.
    public static let bindableControls: [Control] =
        PadButton.allCases.map { .button($0) } + [.axis(.leftTrigger), .axis(.rightTrigger)]

    /// "Couch desktop" preset: right stick pointer, left stick scroll, triggers click.
    public static let `default`: KeyMouseMapping = {
        var actions: [Control: KeyAction] = [
            .axis(.rightTrigger): .mouse(.left),
            .axis(.leftTrigger): .mouse(.right),
            .button(.rightStick): .mouse(.middle),
            .button(.a): .key(.enter, modifiers: []),
            .button(.b): .key(.escape, modifiers: []),
            .button(.x): .key(.space, modifiers: []),
            .button(.y): .key(.tab, modifiers: []),
            .button(.dpadUp): .key(.up, modifiers: []),
            .button(.dpadDown): .key(.down, modifiers: []),
            .button(.dpadLeft): .key(.left, modifiers: []),
            .button(.dpadRight): .key(.right, modifiers: []),
        ]
        actions[.button(.leftShoulder)] = .key(.tab, modifiers: [.control, .shift])
        actions[.button(.rightShoulder)] = .key(.tab, modifiers: [.control])
        return KeyMouseMapping(
            enabled: false, target: .local, leftStick: .scroll, rightStick: .mouse,
            mouseSpeed: 1400, scrollSpeed: 12, curve: 2, stickDeadzone: 0.15,
            bindings: bindableControls.map { KeyBinding(control: $0, action: actions[$0] ?? .none) }
        )
    }()

    public func action(for control: Control) -> KeyAction {
        bindings.first { $0.control == control }?.action ?? .none
    }
}

extension KeyMouseMapping {
    /// Missing fields fall back to the defaults instead of failing the whole file.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let d = KeyMouseMapping.default
        enabled = try c.decodeIfPresent(Bool.self, forKey: .enabled) ?? d.enabled
        target = try c.decodeIfPresent(EmulationTarget.self, forKey: .target) ?? d.target
        leftStick = try c.decodeIfPresent(StickRole.self, forKey: .leftStick) ?? d.leftStick
        rightStick = try c.decodeIfPresent(StickRole.self, forKey: .rightStick) ?? d.rightStick
        mouseSpeed = try c.decodeIfPresent(Double.self, forKey: .mouseSpeed) ?? d.mouseSpeed
        scrollSpeed = try c.decodeIfPresent(Double.self, forKey: .scrollSpeed) ?? d.scrollSpeed
        curve = try c.decodeIfPresent(Double.self, forKey: .curve) ?? d.curve
        stickDeadzone = try c.decodeIfPresent(Float.self, forKey: .stickDeadzone) ?? d.stickDeadzone
        bindings = try c.decodeIfPresent([KeyBinding].self, forKey: .bindings) ?? d.bindings
        for binding in d.bindings where !bindings.contains(where: { $0.control == binding.control }) {
            bindings.append(binding)
        }
    }
}
