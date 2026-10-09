/// Canonical controls every backend maps its hardware onto.
///
/// Names follow the SDL GameController layout (Xbox face-button naming), so any
/// pad that SDL or Apple's GameController framework knows fits without new cases.
/// Everything downstream of a backend (mapping, MIDI, keyboard, network) only
/// ever sees these, never device-specific report bytes.
public enum PadButton: String, CaseIterable, Codable, Sendable, Hashable {
    case a, b, x, y
    case leftShoulder, rightShoulder
    case leftStick, rightStick
    case back, start, guide
    case dpadUp, dpadDown, dpadLeft, dpadRight

    public var displayName: String {
        switch self {
        case .a: return "A"
        case .b: return "B"
        case .x: return "X"
        case .y: return "Y"
        case .leftShoulder: return "LB"
        case .rightShoulder: return "RB"
        case .leftStick: return "LS (click)"
        case .rightStick: return "RS (click)"
        case .back: return "Back"
        case .start: return "Start"
        case .guide: return "Guide"
        case .dpadUp: return "D-Pad Up"
        case .dpadDown: return "D-Pad Down"
        case .dpadLeft: return "D-Pad Left"
        case .dpadRight: return "D-Pad Right"
        }
    }
}

/// Analog controls.
///
/// Value ranges in `ControllerState`:
/// - sticks: -1.0 ... +1.0, **up and right are positive**
/// - triggers: 0.0 ... 1.0
public enum PadAxis: String, CaseIterable, Codable, Sendable, Hashable {
    case leftX, leftY, rightX, rightY
    case leftTrigger, rightTrigger

    /// Sticks are centred at 0, triggers rest at 0 and only go positive.
    public var isBipolar: Bool {
        switch self {
        case .leftTrigger, .rightTrigger: return false
        default: return true
        }
    }

    public var displayName: String {
        switch self {
        case .leftX: return "Left Stick X"
        case .leftY: return "Left Stick Y"
        case .rightX: return "Right Stick X"
        case .rightY: return "Right Stick Y"
        case .leftTrigger: return "LT"
        case .rightTrigger: return "RT"
        }
    }
}

/// Either kind of control, for mapping tables and "which control moved" events.
public enum Control: Hashable, Codable, Sendable {
    case button(PadButton)
    case axis(PadAxis)

    public var displayName: String {
        switch self {
        case .button(let b): return b.displayName
        case .axis(let a): return a.displayName
        }
    }
}
