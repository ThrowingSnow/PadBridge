import Foundation
import PadCore
import PadMIDI

/// Decides what the LED ring of every connected pad shows.
/// Thread-safe: clock cues arrive on the CoreMIDI thread and go straight to the
/// devices (no main-thread hop) to keep the blink tight to the beat.
final class RingDriver {
    enum Mode: String, CaseIterable, Identifiable {
        case manual, random
        /// Clock: LED = bar within 4 bars (bar 1 -> LED 1 ... bar 4 -> LED 4), blinking on every beat.
        case clockBar
        /// Clock: LED = beat within the bar, so every beat the next LED blinks.
        case clockBeat
        var id: String { rawValue }
        var isClock: Bool { self == .clockBar || self == .clockBeat }
    }

    private let manager: ControllerManager
    private let lock = NSLock()
    private var mode: Mode = .manual
    private var manualIndicator: PlayerIndicator = .player(1)
    private var randomTimer: DispatchSourceTimer?
    private var lastRandom = 0
    private let randomQueue = DispatchQueue(label: "padbridge.ring.random")

    init(manager: ControllerManager) {
        self.manager = manager
    }

    var currentMode: Mode { lock.withLock { mode } }

    func setManual(_ indicator: PlayerIndicator) {
        lock.withLock {
            mode = .manual
            manualIndicator = indicator
        }
        stopRandom()
        show(indicator)
    }

    func setMode(_ newMode: Mode) {
        lock.withLock { mode = newMode }
        switch newMode {
        case .manual:
            stopRandom()
            show(lock.withLock { manualIndicator })
        case .random:
            startRandom()
        case .clockBar, .clockBeat:
            stopRandom()
            // Until the first beat arrives, show the ring is waiting for clock.
            show(.quadrant(1))
        }
    }

    /// Newly connected pads pick up the current mode.
    func deviceConnected(_ id: String) {
        guard let device = manager.device(id: id) else { return }
        switch currentMode {
        case .manual: device.setPlayerIndicator(lock.withLock { manualIndicator })
        case .clockBar, .clockBeat: device.setPlayerIndicator(.quadrant(1))
        case .random: break
        }
    }

    func clockCue(_ cue: RingCue) {
        let mode = currentMode
        guard mode.isClock else { return }
        switch cue {
        case .beat(let beat, let bar): show(.quadrant((mode == .clockBar ? bar : beat) % 4 + 1))
        case .off: show(.off)
        case .stopped: show(.quadrant(1))
        }
    }

    private func show(_ indicator: PlayerIndicator) {
        for device in manager.connected { device.setPlayerIndicator(indicator) }
    }

    // MARK: Random

    private func startRandom() {
        stopRandom()
        let timer = DispatchSource.makeTimerSource(queue: randomQueue)
        timer.setEventHandler { [weak self] in self?.randomStep() }
        timer.schedule(deadline: .now())
        lock.withLock { randomTimer = timer }
        timer.resume()
    }

    private func stopRandom() {
        lock.withLock {
            randomTimer?.cancel()
            randomTimer = nil
        }
    }

    /// Jumps to a different quadrant (sometimes dark) after a random 60...280 ms.
    private func randomStep() {
        guard currentMode == .random else { return }
        if Int.random(in: 0..<5) == 0 {
            lastRandom = 0
            show(.off)
        } else {
            var next = Int.random(in: 1...4)
            while next == lastRandom { next = Int.random(in: 1...4) }
            lastRandom = next
            show(.quadrant(next))
        }
        lock.withLock { randomTimer }?.schedule(deadline: .now() + .milliseconds(Int.random(in: 60...280)))
    }
}
