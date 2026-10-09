/// MIDI real-time messages relevant to following a clock.
public enum ClockMessage: Equatable, Sendable {
    case tick
    case start
    case `continue`
    case stop
    /// Song Position Pointer, in MIDI beats (16th notes).
    case songPosition(Int)
}

/// What the LED ring should do in response to the clock.
public enum RingCue: Equatable, Sendable {
    /// Start of a quarter note. Both 0-based: `beat` within the bar (0...3),
    /// `bar` counted from song start. The ring driver picks the LED from these.
    case beat(beat: Int, bar: Int)
    /// Half a beat later: dark, so the ring blinks at the tempo.
    case off
    /// Transport stopped.
    case stopped
}

/// Follows 24 PPQN MIDI clock in 4/4: counts beats and bars and estimates BPM.
///
/// Emits a `.beat` cue on every quarter note and `.off` half a beat later, so a
/// ring driven by it blinks at the tempo.
public struct MIDIClockTracker {
    public static let ticksPerBeat = 24
    public static let beatsPerBar = 4

    public private(set) var running = false
    /// Ticks since song start; -1 means "the next tick is position 0".
    public private(set) var tick = -1
    public private(set) var bpm: Double?

    private var lastTickTime: Double?
    private var averageInterval: Double?

    public init() {}

    /// 1-based bar and beat of the current position, for display.
    public var position: (bar: Int, beat: Int) {
        let beats = max(tick, 0) / Self.ticksPerBeat
        return (beats / Self.beatsPerBar + 1, beats % Self.beatsPerBar + 1)
    }

    /// `time` is in seconds on any monotonic clock.
    public mutating func handle(_ message: ClockMessage, at time: Double) -> RingCue? {
        switch message {
        case .start:
            // Per the MIDI spec the first tick after Start is position 0.
            running = true
            tick = -1
            return nil
        case .continue:
            running = true
            return nil
        case .stop:
            running = false
            return .stopped
        case .songPosition(let sixteenths):
            tick = sixteenths * (Self.ticksPerBeat / 4) - 1
            return nil
        case .tick:
            updateTempo(at: time)
            // Many hosts send clock while stopped; only the transport moves the position.
            guard running else { return nil }
            tick += 1
            switch tick % Self.ticksPerBeat {
            case 0:
                let beats = tick / Self.ticksPerBeat
                return .beat(beat: beats % Self.beatsPerBar, bar: beats / Self.beatsPerBar)
            case Self.ticksPerBeat / 2:
                return .off
            default:
                return nil
            }
        }
    }

    private mutating func updateTempo(at time: Double) {
        defer { lastTickTime = time }
        guard let last = lastTickTime else { return }
        let interval = time - last
        // Ignore gaps (clock restarted) and garbage; 20...400 BPM.
        guard interval > 60.0 / (400 * 24), interval < 60.0 / (20 * 24) else {
            averageInterval = nil
            return
        }
        let avg = averageInterval.map { $0 + (interval - $0) * 0.08 } ?? interval
        averageInterval = avg
        bpm = 60 / (avg * Double(Self.ticksPerBeat))
    }
}
