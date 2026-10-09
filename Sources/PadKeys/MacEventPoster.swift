import ApplicationServices
import CoreGraphics

/// Posts keyboard/mouse events on this Mac via CGEvent.
/// Needs Accessibility permission (System Settings → Privacy & Security →
/// Accessibility) for the app, or for Terminal when run with `swift run`.
public final class MacEventPoster {
    private let source = CGEventSource(stateID: .hidSystemState)
    private var heldButtons: Set<MouseButton> = []
    private var flags: CGEventFlags = []

    /// One wheel detent in pixels (macOS has no detents for synthetic pixel scrolls).
    private static let pixelsPerDetent = 40.0
    private var scrollRemainder = (x: 0.0, y: 0.0)

    public init() {}

    public static var isTrusted: Bool { AXIsProcessTrusted() }

    /// Shows the system prompt that leads to the Accessibility settings.
    public static func requestTrust() {
        let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue(): true] as CFDictionary
        _ = AXIsProcessTrustedWithOptions(options)
    }

    public func post(_ events: [KMEvent]) {
        for event in events {
            switch event {
            case .key(let key, let down): postKey(key, down: down)
            case .button(let button, let down): postButton(button, down: down)
            case .move(let dx, let dy): postMove(dx: dx, dy: dy)
            case .scroll(let dx, let dy): postScroll(dx: dx, dy: dy)
            }
        }
    }

    private func postKey(_ key: KeyCode, down: Bool) {
        if let flag = Self.flag(for: key) {
            if down { flags.insert(flag) } else { flags.remove(flag) }
        }
        guard let event = CGEvent(keyboardEventSource: source, virtualKey: key.macCode, keyDown: down) else { return }
        event.flags = flags
        event.post(tap: .cghidEventTap)
    }

    private static func flag(for key: KeyCode) -> CGEventFlags? {
        switch key {
        case .leftShift: return .maskShift
        case .leftControl: return .maskControl
        case .leftOption: return .maskAlternate
        case .leftCommand: return .maskCommand
        default: return nil
        }
    }

    private func postButton(_ button: MouseButton, down: Bool) {
        if down { heldButtons.insert(button) } else { heldButtons.remove(button) }
        let type: CGEventType
        let cgButton: CGMouseButton
        switch button {
        case .left: (type, cgButton) = (down ? .leftMouseDown : .leftMouseUp, .left)
        case .right: (type, cgButton) = (down ? .rightMouseDown : .rightMouseUp, .right)
        case .middle: (type, cgButton) = (down ? .otherMouseDown : .otherMouseUp, .center)
        }
        guard let event = CGEvent(mouseEventSource: source, mouseType: type,
                                  mouseCursorPosition: currentLocation(), mouseButton: cgButton) else { return }
        event.setIntegerValueField(.mouseEventClickState, value: 1)
        event.flags = flags
        event.post(tap: .cghidEventTap)
    }

    private func postMove(dx: Int, dy: Int) {
        let from = currentLocation()
        let to = clampToDisplays(CGPoint(x: from.x + CGFloat(dx), y: from.y + CGFloat(dy)))
        // While a button is held, macOS expects drag events (e.g. selecting text).
        let (type, button): (CGEventType, CGMouseButton) =
            heldButtons.contains(.left) ? (.leftMouseDragged, .left)
            : heldButtons.contains(.right) ? (.rightMouseDragged, .right)
            : heldButtons.contains(.middle) ? (.otherMouseDragged, .center)
            : (.mouseMoved, .left)
        guard let event = CGEvent(mouseEventSource: source, mouseType: type, mouseCursorPosition: to, mouseButton: button) else { return }
        // Relative deltas: apps with a captured pointer (Moonlight in game mode) read only these.
        event.setIntegerValueField(.mouseEventDeltaX, value: Int64(dx))
        event.setIntegerValueField(.mouseEventDeltaY, value: Int64(dy))
        event.flags = flags
        event.post(tap: .cghidEventTap)
    }

    private func postScroll(dx: Int, dy: Int) {
        scrollRemainder.x += Double(dx) / 120 * Self.pixelsPerDetent
        scrollRemainder.y += Double(dy) / 120 * Self.pixelsPerDetent
        let px = Int32(scrollRemainder.x), py = Int32(scrollRemainder.y)
        scrollRemainder.x -= Double(px)
        scrollRemainder.y -= Double(py)
        guard px != 0 || py != 0 else { return }
        // wheel2 positive scrolls left, ours means right.
        guard let event = CGEvent(scrollWheelEvent2Source: source, units: .pixel, wheelCount: 2,
                                  wheel1: py, wheel2: -px, wheel3: 0) else { return }
        event.post(tap: .cghidEventTap)
    }

    private func currentLocation() -> CGPoint {
        CGEvent(source: nil)?.location ?? .zero
    }

    /// Keeps the pointer on some display (CGEvent coordinates are global, top-left origin).
    private func clampToDisplays(_ point: CGPoint) -> CGPoint {
        var count: UInt32 = 0
        CGGetActiveDisplayList(0, nil, &count)
        var ids = [CGDirectDisplayID](repeating: 0, count: Int(count))
        CGGetActiveDisplayList(count, &ids, &count)
        let bounds = ids.map(CGDisplayBounds)
        if bounds.contains(where: { $0.contains(point) }) { return point }
        // Off-screen: clamp into the nearest display.
        guard let nearest = bounds.min(by: { distance($0, point) < distance($1, point) }) else { return point }
        return CGPoint(x: min(max(point.x, nearest.minX), nearest.maxX - 1),
                       y: min(max(point.y, nearest.minY), nearest.maxY - 1))
    }

    private func distance(_ rect: CGRect, _ p: CGPoint) -> CGFloat {
        let dx = max(rect.minX - p.x, 0, p.x - rect.maxX)
        let dy = max(rect.minY - p.y, 0, p.y - rect.maxY)
        return dx * dx + dy * dy
    }
}
