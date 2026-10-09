import Foundation
import PadCore

/// A keyboard/mouse event, independent of the output target.
public enum KMEvent: Equatable, Sendable {
    case key(KeyCode, down: Bool)
    case button(MouseButton, down: Bool)
    /// Pointer motion in points; x right, y **down** (screen convention).
    case move(dx: Int, dy: Int)
    /// Wheel motion in 1/120 detents (hi-res units); positive = up / right.
    case scroll(dx: Int, dy: Int)
}

/// Turns controller states into keyboard/mouse events according to a mapping.
///
/// - `process` handles discrete things (buttons, triggers, stick-as-keys) and is
///   called on every input report.
/// - `tick` integrates continuous things (pointer, scroll) and is called at a
///   fixed rate, because a pad held still at an angle sends no reports.
///
/// Keys and mouse buttons are reference counted, so two controls mapped to the
/// same key (e.g. D-pad up and a stick in arrow mode) do not release each other.
public struct KeyMouseMapper {
    public var mapping: KeyMouseMapping

    private var active: [Control: KeyAction] = [:]
    /// Stick direction slot (e.g. "Lu") -> the key it is holding.
    private var stickKeys: [String: KeyCode] = [:]
    private var keyCount: [KeyCode: Int] = [:]
    private var buttonCount: [MouseButton: Int] = [:]
    private var moveRemainder = (x: 0.0, y: 0.0)
    private var scrollRemainder = (x: 0.0, y: 0.0)

    static let triggerPress: Float = 0.55
    static let triggerRelease: Float = 0.45
    static let stickKeyThreshold: Float = 0.5

    public init(mapping: KeyMouseMapping) {
        self.mapping = mapping
    }

    public var isHoldingAnything: Bool { !keyCount.isEmpty || !buttonCount.isEmpty }

    // MARK: Discrete

    public mutating func process(_ state: ControllerState) -> [KMEvent] {
        var out: [KMEvent] = []
        for binding in mapping.bindings where binding.action != .none {
            let wasDown = active[binding.control] != nil
            let isDown: Bool
            switch binding.control {
            case .button(let button):
                isDown = state.isPressed(button)
            case .axis(let axis):
                isDown = state[axis] > (wasDown ? Self.triggerRelease : Self.triggerPress)
            }
            if isDown, !wasDown {
                active[binding.control] = binding.action
                out += press(binding.action)
            } else if !isDown, let action = active.removeValue(forKey: binding.control) {
                out += release(action)
            }
        }
        out += stickAsKeys(state, stick: (.leftX, .leftY), role: mapping.leftStick, tag: "L")
        out += stickAsKeys(state, stick: (.rightX, .rightY), role: mapping.rightStick, tag: "R")
        return out
    }

    private mutating func stickAsKeys(_ state: ControllerState, stick: (x: PadAxis, y: PadAxis), role: StickRole, tag: String) -> [KMEvent] {
        let keys = role.keys
        let x = state[stick.x], y = state[stick.y]
        let t = Self.stickKeyThreshold
        let wanted: [(String, KeyCode?, Bool)] = [
            ("\(tag)u", keys?.up, y > t), ("\(tag)d", keys?.down, y < -t),
            ("\(tag)l", keys?.left, x < -t), ("\(tag)r", keys?.right, x > t),
        ]
        var out: [KMEvent] = []
        for (id, key, on) in wanted {
            // Released, or the role changed while held: let go of what was pressed...
            if let held = stickKeys[id], !on || key != held {
                stickKeys[id] = nil
                out += keyUp(held)
            }
            // ...then press the current key (which may be the new role's).
            if on, stickKeys[id] == nil, let key {
                stickKeys[id] = key
                out += keyDown(key)
            }
        }
        return out
    }

    // MARK: Continuous

    public mutating func tick(_ state: ControllerState, dt: Double) -> [KMEvent] {
        var out: [KMEvent] = []
        for (role, x, y) in [(mapping.leftStick, state[.leftX], state[.leftY]),
                             (mapping.rightStick, state[.rightX], state[.rightY])] {
            switch role {
            case .mouse:
                let (vx, vy) = velocity(x, y, speed: mapping.mouseSpeed)
                moveRemainder.x += vx * dt
                moveRemainder.y -= vy * dt   // stick up = pointer up = negative screen y
                let dx = Int(moveRemainder.x), dy = Int(moveRemainder.y)
                moveRemainder.x -= Double(dx)
                moveRemainder.y -= Double(dy)
                if dx != 0 || dy != 0 { out.append(.move(dx: dx, dy: dy)) }
            case .scroll:
                let (vx, vy) = velocity(x, y, speed: mapping.scrollSpeed * 120)
                scrollRemainder.x += vx * dt
                scrollRemainder.y += vy * dt
                let dx = Int(scrollRemainder.x), dy = Int(scrollRemainder.y)
                scrollRemainder.x -= Double(dx)
                scrollRemainder.y -= Double(dy)
                if dx != 0 || dy != 0 { out.append(.scroll(dx: dx, dy: dy)) }
            default:
                break
            }
        }
        return out
    }

    /// Radial deadzone + response curve, so diagonals are not faster than straight lines.
    func velocity(_ x: Float, _ y: Float, speed: Double) -> (Double, Double) {
        let magnitude = min(Double((x * x + y * y).squareRoot()), 1)
        let dz = Double(min(max(mapping.stickDeadzone, 0), 0.9))
        guard magnitude > dz else { return (0, 0) }
        let scaled = pow((magnitude - dz) / (1 - dz), max(mapping.curve, 0.1)) * speed
        return (Double(x) / magnitude * scaled, Double(y) / magnitude * scaled)
    }

    // MARK: Release

    /// Lets go of everything; call when disabling or switching target.
    public mutating func releaseAll() -> [KMEvent] {
        let out = keyCount.keys.map { KMEvent.key($0, down: false) } + buttonCount.keys.map { KMEvent.button($0, down: false) }
        active.removeAll()
        stickKeys.removeAll()
        keyCount.removeAll()
        buttonCount.removeAll()
        return out
    }

    // MARK: Ref counting

    private mutating func press(_ action: KeyAction) -> [KMEvent] {
        switch action {
        case .none: return []
        case .mouse(let button):
            buttonCount[button, default: 0] += 1
            return buttonCount[button] == 1 ? [.button(button, down: true)] : []
        case .key(let key, let modifiers):
            // Modifiers first, so the key arrives already modified.
            return modifiers.flatMap { keyDown($0.key) } + keyDown(key)
        }
    }

    private mutating func release(_ action: KeyAction) -> [KMEvent] {
        switch action {
        case .none: return []
        case .mouse(let button):
            guard let n = buttonCount[button] else { return [] }
            if n <= 1 { buttonCount[button] = nil; return [.button(button, down: false)] }
            buttonCount[button] = n - 1
            return []
        case .key(let key, let modifiers):
            return keyUp(key) + modifiers.reversed().flatMap { keyUp($0.key) }
        }
    }

    private mutating func keyDown(_ key: KeyCode) -> [KMEvent] {
        keyCount[key, default: 0] += 1
        return keyCount[key] == 1 ? [.key(key, down: true)] : []
    }

    private mutating func keyUp(_ key: KeyCode) -> [KMEvent] {
        guard let n = keyCount[key] else { return [] }
        if n <= 1 { keyCount[key] = nil; return [.key(key, down: false)] }
        keyCount[key] = n - 1
        return []
    }
}

