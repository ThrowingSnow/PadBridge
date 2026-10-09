import Foundation
import Testing
import PadCore
@testable import PadKeys

struct KeyMouseMapperTests {
    private func mapper(_ edit: (inout KeyMouseMapping) -> Void = { _ in }) -> KeyMouseMapper {
        var m = KeyMouseMapping.default
        m.enabled = true
        edit(&m)
        return KeyMouseMapper(mapping: m)
    }

    @Test func buttonPressesAndReleasesKey() {
        var m = mapper()
        #expect(m.process(ControllerState(pressed: [.a])) == [.key(.enter, down: true)])
        #expect(m.process(ControllerState()) == [.key(.enter, down: false)])
    }

    @Test func comboPressesModifiersFirstAndReleasesThemLast() {
        var m = mapper()   // LB = ⌃⇧Tab
        #expect(m.process(ControllerState(pressed: [.leftShoulder])) ==
                [.key(.leftControl, down: true), .key(.leftShift, down: true), .key(.tab, down: true)])
        #expect(m.process(ControllerState()) ==
                [.key(.tab, down: false), .key(.leftShift, down: false), .key(.leftControl, down: false)])
    }

    @Test func sharedKeyIsReleasedOnlyWhenBothControlsAreUp() {
        var m = mapper { $0.rightStick = .arrows }   // D-pad up and right stick up both = ↑
        _ = m.process(ControllerState(pressed: [.dpadUp]))
        #expect(m.process(ControllerState(pressed: [.dpadUp], axes: [.rightY: 1])).isEmpty)
        #expect(m.process(ControllerState(axes: [.rightY: 1])).isEmpty, "stick still holds ↑")
        #expect(m.process(ControllerState()) == [.key(.up, down: false)])
    }

    @Test func triggerHasHysteresis() {
        var m = mapper()   // RT = left click
        #expect(m.process(ControllerState(axes: [.rightTrigger: 0.5])).isEmpty)
        #expect(m.process(ControllerState(axes: [.rightTrigger: 0.6])) == [.button(.left, down: true)])
        #expect(m.process(ControllerState(axes: [.rightTrigger: 0.5])).isEmpty, "between thresholds: stays down")
        #expect(m.process(ControllerState(axes: [.rightTrigger: 0.4])) == [.button(.left, down: false)])
    }

    @Test func pointerMovesWithStickAndRespectsDeadzone() {
        var m = mapper { $0.curve = 1 }
        #expect(m.tick(ControllerState(axes: [.rightX: 0.1]), dt: 0.01).isEmpty, "inside deadzone")
        // Full right for 0.1 s at 1400 pt/s = 140 pt; stick up = pointer up (negative y).
        var dx = 0, dy = 0
        for _ in 0..<10 {
            for case let .move(x, y) in m.tick(ControllerState(axes: [.rightX: 1]), dt: 0.01) { dx += x; dy += y }
        }
        #expect(dx == 140 && dy == 0)
        var up = 0
        for case let .move(_, y) in m.tick(ControllerState(axes: [.rightY: 1]), dt: 0.1) { up += y }
        #expect(up == -140)
    }

    @Test func diagonalIsNotFaster() {
        let m = mapper { $0.curve = 1; $0.stickDeadzone = 0 }
        let (vx, vy) = m.velocity(0.7071, 0.7071, speed: 100)
        #expect(abs((vx * vx + vy * vy).squareRoot() - 100) < 0.5)
    }

    @Test func roleChangeWhileHeldReleasesTheRightKey() {
        var m = mapper { $0.leftStick = .wasd }
        #expect(m.process(ControllerState(axes: [.leftY: 1])) == [.key(.w, down: true)])
        m.mapping.leftStick = .arrows
        #expect(m.process(ControllerState(axes: [.leftY: 1])) == [.key(.w, down: false), .key(.up, down: true)])
    }

    @Test func releaseAllLetsGoOfEverything() {
        var m = mapper()
        _ = m.process(ControllerState(pressed: [.a], axes: [.rightTrigger: 1]))
        let released = m.releaseAll()
        #expect(Set(released.map { "\($0)" }) == ["key(PadKeys.KeyCode.enter, down: false)", "button(PadKeys.MouseButton.left, down: false)"])
        #expect(!m.isHoldingAnything)
    }

    @Test func wireEncoding() {
        #expect(WireInput(.key(.a, down: true)) == WireInput(.key(.a, down: true)))
        let key = WireInput(.key(.a, down: true))
        #expect(key.type == 1 && key.a == 30 && key.b == 1)
        let click = WireInput(.button(.left, down: false))
        #expect(click.type == 1 && click.a == 0x110 && click.b == 0)
        let scroll = WireInput(.scroll(dx: 5, dy: -120))
        #expect(scroll.type == 3 && scroll.a == -120 && scroll.b == 5)
    }

    @Test func oldSettingsFileLoads() throws {
        let json = #"{"enabled":true,"rightStick":"scroll"}"#
        let m = try JSONDecoder().decode(KeyMouseMapping.self, from: Data(json.utf8))
        #expect(m.enabled && m.rightStick == .scroll && m.leftStick == KeyMouseMapping.default.leftStick)
        #expect(m.bindings.count == KeyMouseMapping.bindableControls.count)
    }
}
