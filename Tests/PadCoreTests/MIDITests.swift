import Foundation
import Testing
import PadCore
@testable import PadMIDI

struct MIDIMapperTests {
    private var mapping: MIDIMapping {
        var m = MIDIMapping.default
        m.channel = 2
        return m
    }

    @Test func firstStateSendsEveryCCOnce() {
        var mapper = MIDIMapper(mapping: mapping)
        let first = mapper.process(ControllerState())
        // 6 axes + 15 buttons, all CC by default.
        #expect(first.count == 21)
        #expect(first.allSatisfy { $0.status == 0xB1 })
        #expect(first.contains(.controlChange(channel: 2, number: 20, value: 64)), "stick centre is 64")
        #expect(first.contains(.controlChange(channel: 2, number: 24, value: 0)), "trigger rest is 0")
        #expect(mapper.process(ControllerState()).isEmpty, "unchanged state sends nothing")
    }

    @Test func deadzoneSwallowsDrift() {
        var mapper = MIDIMapper(mapping: mapping)
        _ = mapper.process(ControllerState())
        let drift = ControllerState(axes: [.leftX: 0.058, .leftY: 0.042, .rightX: -0.047])
        #expect(mapper.process(drift).isEmpty)
    }

    @Test func fullTravelReachesEnds() {
        #expect(MIDIMapper.value7(1, axis: .leftX, mapping: mapping, invert: false) == 127)
        #expect(MIDIMapper.value7(-1, axis: .leftX, mapping: mapping, invert: false) == 0)
        #expect(MIDIMapper.value7(1, axis: .leftX, mapping: mapping, invert: true) == 0)
        #expect(MIDIMapper.value7(1, axis: .rightTrigger, mapping: mapping, invert: false) == 127)
        #expect(MIDIMapper.value7(1, axis: .rightTrigger, mapping: mapping, invert: true) == 0)
    }

    @Test func momentaryButtonSendsPressAndRelease() {
        var mapper = MIDIMapper(mapping: mapping)
        _ = mapper.process(ControllerState())
        #expect(mapper.process(ControllerState(pressed: [.a])) == [.controlChange(channel: 2, number: 102, value: 127)])
        #expect(mapper.process(ControllerState()) == [.controlChange(channel: 2, number: 102, value: 0)])
    }

    @Test func toggleButtonFlipsOnPressOnly() {
        var m = mapping
        let index = m.bindings.firstIndex { $0.control == .button(.a) }!
        m.bindings[index].buttonMode = .toggle
        m.bindings[index].kind = .note
        m.bindings[index].number = 60
        var mapper = MIDIMapper(mapping: m)
        _ = mapper.process(ControllerState())

        #expect(mapper.process(ControllerState(pressed: [.a])) == [.noteOn(channel: 2, note: 60, velocity: 127)])
        #expect(mapper.process(ControllerState()).isEmpty, "release does nothing in toggle mode")
        #expect(mapper.process(ControllerState(pressed: [.a])) == [.noteOff(channel: 2, note: 60)])
    }

    @Test func conflictsAreDetected() {
        var m = MIDIMapping.default
        #expect(m.conflicts.isEmpty)
        m.bindings[0].number = m.bindings[1].number
        #expect(m.conflicts == [m.bindings[0].control, m.bindings[1].control])
    }
}

struct MIDIClockTests {
    /// Feeds `beats` quarter notes of clock at 120 BPM and collects ring cues.
    private func run(_ tracker: inout MIDIClockTracker, beats: Int, from time: inout Double) -> [RingCue] {
        var cues: [RingCue] = []
        for _ in 0..<(beats * 24) {
            if let cue = tracker.handle(.tick, at: time) { cues.append(cue) }
            time += 60.0 / (120 * 24)
        }
        return cues
    }

    @Test func beatsAndBarsAreCounted() {
        var tracker = MIDIClockTracker()
        var time = 0.0
        _ = tracker.handle(.start, at: time)
        let cues = run(&tracker, beats: 8, from: &time).filter { if case .beat = $0 { return true } else { return false } }
        let expected = (0..<8).map { RingCue.beat(beat: $0 % 4, bar: $0 / 4) }
        #expect(cues == expected)
    }

    @Test func blinksTwicePerBeat() {
        var tracker = MIDIClockTracker()
        var time = 0.0
        _ = tracker.handle(.start, at: time)
        #expect(run(&tracker, beats: 1, from: &time) == [.beat(beat: 0, bar: 0), .off])
    }

    @Test func bpmIsEstimated() {
        var tracker = MIDIClockTracker()
        var time = 0.0
        _ = run(&tracker, beats: 8, from: &time)
        #expect(abs((tracker.bpm ?? 0) - 120) < 0.5)
        #expect(!tracker.running, "clock without Start does not move the position")
    }

    @Test func songPositionJumps() {
        var tracker = MIDIClockTracker()
        var time = 0.0
        _ = tracker.handle(.songPosition(32), at: time)   // 32 sixteenths = bar 3
        _ = tracker.handle(.continue, at: time)
        #expect(run(&tracker, beats: 1, from: &time).first == .beat(beat: 0, bar: 2))
        #expect(tracker.position.bar == 3)
    }

    @Test func umpClockWordsDecode() {
        #expect(MIDIEndpoint.clockMessage(0x10F8_0000) == .tick)
        #expect(MIDIEndpoint.clockMessage(0x10FA_0000) == .start)
        #expect(MIDIEndpoint.clockMessage(0x10FC_0000) == .stop)
        #expect(MIDIEndpoint.clockMessage(0x10F2_0001) == .songPosition(128))
        #expect(MIDIEndpoint.clockMessage(0x20B0_1440) == nil, "channel voice is not clock")
    }
}

struct MIDIVelocityTests {
    private func mapping(_ edit: (inout MIDIBinding) -> Void) -> MIDIMapping {
        var m = MIDIMapping.default
        let index = m.bindings.firstIndex { $0.control == .button(.a) }!
        edit(&m.bindings[index])
        return m
    }

    @Test func fixedVelocityIsUsedForNotesAndCC() {
        var mapper = MIDIMapper(mapping: mapping { $0.kind = .note; $0.number = 60; $0.velocity = 90 })
        _ = mapper.process(ControllerState())
        #expect(mapper.process(ControllerState(pressed: [.a])) == [.noteOn(channel: 1, note: 60, velocity: 90)])

        var ccMapper = MIDIMapper(mapping: mapping { $0.velocity = 42 })
        _ = ccMapper.process(ControllerState())
        #expect(ccMapper.process(ControllerState(pressed: [.a])) == [.controlChange(channel: 1, number: 102, value: 42)])
    }

    @Test func triggerSetsVelocity() {
        var m = mapping { $0.kind = .note; $0.number = 60; $0.velocitySource = .rightTrigger }
        m.bindings.removeAll { if case .axis = $0.control { return true } else { return false } }
        var mapper = MIDIMapper(mapping: m)
        _ = mapper.process(ControllerState())
        let half = ControllerState(pressed: [.a], axes: [.rightTrigger: 0.5])
        let velocity = mapper.process(half).first?.data2 ?? 0
        #expect(velocity > 55 && velocity < 70)
    }

    @Test func releasedTriggerStillPlaysTheNote() {
        var mapper = MIDIMapper(mapping: mapping { $0.kind = .note; $0.number = 60; $0.velocitySource = .leftTrigger })
        _ = mapper.process(ControllerState())
        #expect(mapper.process(ControllerState(pressed: [.a])).contains(.noteOn(channel: 1, note: 60, velocity: 1)))
    }

    @Test func oldSavedMappingStillLoads() throws {
        // Shape of a midi-mapping.json written before velocity existed.
        let json = """
        {"enabled":true,"channel":3,"stickDeadzone":0.2,"triggerDeadzone":0.02,
         "bindings":[{"control":{"button":{"_0":"a"}},"enabled":true,"kind":"note","number":61,"buttonMode":"toggle","invert":false}]}
        """
        let m = try JSONDecoder().decode(MIDIMapping.self, from: Data(json.utf8))
        #expect(m.channel == 3)
        let a = m.binding(for: .button(.a))!
        #expect(a.number == 61 && a.kind == .note && a.buttonMode == .toggle)
        #expect(a.velocitySource == .fixed && a.velocity == 127)
        #expect(m.bindings.count == MIDIMapping.default.bindings.count, "missing controls are filled in")
    }
}
