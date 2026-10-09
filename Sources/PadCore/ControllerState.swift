/// Snapshot of every control on a pad, in canonical units (see `PadAxis`).
public struct ControllerState: Equatable, Sendable {
    public var pressed: Set<PadButton>
    private var axisValues: [Float]

    public init(pressed: Set<PadButton> = [], axes: [PadAxis: Float] = [:]) {
        self.pressed = pressed
        self.axisValues = Array(repeating: 0, count: PadAxis.allCases.count)
        for (axis, value) in axes { self[axis] = value }
    }

    public subscript(axis: PadAxis) -> Float {
        get { axisValues[axis.index] }
        set { axisValues[axis.index] = newValue }
    }

    public func isPressed(_ button: PadButton) -> Bool { pressed.contains(button) }

    /// Controls whose value differs from `previous`, in a stable order.
    /// This is what outputs like MIDI consume: only send what moved.
    public func changes(since previous: ControllerState) -> [ControlChange] {
        var out: [ControlChange] = []
        for button in PadButton.allCases {
            let now = pressed.contains(button)
            if now != previous.pressed.contains(button) {
                out.append(.button(button, pressed: now))
            }
        }
        for axis in PadAxis.allCases where self[axis] != previous[axis] {
            out.append(.axis(axis, value: self[axis]))
        }
        return out
    }
}

public enum ControlChange: Equatable, Sendable {
    case button(PadButton, pressed: Bool)
    case axis(PadAxis, value: Float)

    public var control: Control {
        switch self {
        case .button(let b, _): return .button(b)
        case .axis(let a, _): return .axis(a)
        }
    }
}

private extension PadAxis {
    var index: Int { PadAxis.allCases.firstIndex(of: self)! }
}
