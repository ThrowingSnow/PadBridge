import Foundation
import PadCore

/// Connects controllers to the virtual MIDI port: controller states go out as
/// CC/notes, incoming clock is tracked and reported as ring cues.
///
/// Runs on the device and CoreMIDI threads, never on main; the UI reads results
/// through the callbacks and must hop to main itself.
public final class MIDIBridge {
    public struct ClockStatus: Equatable, Sendable {
        public var running: Bool
        public var bpm: Double?
        public var bar: Int
        public var beat: Int

        public init(running: Bool, bpm: Double?, bar: Int, beat: Int) {
            self.running = running
            self.bpm = bpm
            self.bar = bar
            self.beat = beat
        }
    }

    /// Latest sent values per device, for live display.
    public var onSent: ((String, [Control: Int]) -> Void)?
    /// Called on every clock cue (beat/half beat/stop) with the derived ring state.
    public var onRingCue: ((RingCue) -> Void)?
    /// Clock status, throttled to roughly once per beat.
    public var onClockStatus: ((ClockStatus) -> Void)?

    private let endpoint: MIDIEndpoint
    private let lock = NSLock()
    private var mapping: MIDIMapping
    private var mappers: [String: MIDIMapper] = [:]
    private var clock = MIDIClockTracker()
    private var lastStatus: ClockStatus?

    public init(manager: ControllerManager, mapping: MIDIMapping, portName: String = "PadBridge") throws {
        self.mapping = mapping
        endpoint = try MIDIEndpoint(name: portName)
        endpoint.onClock = { [weak self] message, time in self?.handleClock(message, at: time) }
        manager.addListener { [weak self] event in self?.handle(event) }
    }

    public func setMapping(_ mapping: MIDIMapping) {
        lock.withLock {
            self.mapping = mapping
            for id in mappers.keys { mappers[id]?.mapping = mapping }
        }
    }

    private func handle(_ event: ControllerManager.Event) {
        switch event {
        case .connected(let info):
            lock.withLock { mappers[info.id] = MIDIMapper(mapping: mapping) }
        case .disconnected(let info):
            _ = lock.withLock { mappers.removeValue(forKey: info.id) }
        case .state(let info, let state):
            let (messages, sent) = lock.withLock { () -> ([MIDIMessage], [Control: Int]) in
                guard var mapper = mappers[info.id] else { return ([], [:]) }
                let messages = mapper.process(state)
                mappers[info.id] = mapper
                return (messages, mapper.lastSent)
            }
            guard !messages.isEmpty else { return }
            endpoint.send(messages)
            onSent?(info.id, sent)
        case .backendError:
            break
        }
    }

    private func handleClock(_ message: ClockMessage, at time: Double) {
        let (cue, status) = lock.withLock { () -> (RingCue?, ClockStatus?) in
            let cue = clock.handle(message, at: time)
            let position = clock.position
            let status = ClockStatus(running: clock.running, bpm: clock.bpm.map { ($0 * 10).rounded() / 10 },
                                     bar: position.bar, beat: position.beat)
            // Report on beat boundaries and transport changes, not 24x per beat.
            let report = message != .tick || cue != nil || lastStatus == nil
            guard report, status != lastStatus else { return (cue, nil) }
            lastStatus = status
            return (cue, status)
        }
        if let cue { onRingCue?(cue) }
        if let status { onClockStatus?(status) }
    }
}
