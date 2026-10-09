import Foundation
import PadCore
import PadMIDI
import PadUSB

/// Bridges the engine (device and CoreMIDI threads, ~250 Hz) to SwiftUI (main thread).
/// High-rate data (input states, sent MIDI values) is coalesced and published
/// at display rate so the UI never throttles the input path.
@MainActor
final class AppModel: ObservableObject {
    @Published private(set) var devices: [ControllerInfo] = []
    @Published private(set) var states: [String: ControllerState] = [:]
    @Published private(set) var log: [String] = []
    @Published var selectedID: String?

    @Published var midiMapping: MIDIMapping {
        didSet { midiMappingChanged() }
    }
    @Published private(set) var midiValues: [Control: Int] = [:]
    @Published private(set) var midiAvailable = false
    @Published private(set) var clock = MIDIBridge.ClockStatus(running: false, bpm: nil, bar: 1, beat: 1)
    @Published private(set) var ringMode: RingDriver.Mode = .manual

    let manager = ControllerManager(backends: [USBBackend()])
    private let ring: RingDriver
    private var midi: MIDIBridge?

    private let pendingLock = NSLock()
    nonisolated(unsafe) private var pendingStates: [String: ControllerState] = [:]
    nonisolated(unsafe) private var pendingMIDI: [String: [Control: Int]] = [:]
    private var timer: Timer?
    private var saveWork: DispatchWorkItem?

    init() {
        midiMapping = SettingsStore.loadMIDIMapping()
        ring = RingDriver(manager: manager)

        manager.addListener { [weak self] event in self?.handle(event) }
        do {
            let bridge = try MIDIBridge(manager: manager, mapping: midiMapping)
            bridge.onSent = { [weak self] id, values in
                guard let self else { return }
                self.pendingLock.withLock { self.pendingMIDI[id] = values }
            }
            bridge.onRingCue = { [ring] cue in ring.clockCue(cue) }
            bridge.onClockStatus = { [weak self] status in
                Task { @MainActor in self?.clock = status }
            }
            midi = bridge
            midiAvailable = true
        } catch {
            append("MIDI nicht verfügbar: \(error)")
        }
        manager.start()

        timer = Timer.scheduledTimer(withTimeInterval: 1.0 / 60, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.flush() }
        }
    }

    var selectedDevice: ControllerInfo? {
        devices.first { $0.id == selectedID } ?? devices.first
    }

    func state(for id: String) -> ControllerState { states[id] ?? ControllerState() }

    func setRumble(low: Float, high: Float) {
        guard let id = selectedDevice?.id else { return }
        manager.device(id: id)?.setRumble(low: low, high: high)
    }

    func setPlayerIndicator(_ indicator: PlayerIndicator) {
        ring.setManual(indicator)
        ringMode = .manual
    }

    func setRingMode(_ mode: RingDriver.Mode) {
        ring.setMode(mode)
        ringMode = mode
    }

    func resetMIDIMapping() { midiMapping = .default }

    // MARK: Engine events

    nonisolated private func handle(_ event: ControllerManager.Event) {
        switch event {
        case .state(let info, let state):
            pendingLock.withLock { pendingStates[info.id] = state }
        case .connected(let info):
            Task { @MainActor in
                self.devices.append(info)
                self.ring.deviceConnected(info.id)
                self.append("Verbunden: \(info.name)")
            }
        case .disconnected(let info):
            Task { @MainActor in
                self.devices.removeAll { $0.id == info.id }
                self.states[info.id] = nil
                self.append("Getrennt: \(info.name)")
            }
        case .backendError(let message):
            Task { @MainActor in self.append("Fehler: \(message)") }
        }
    }

    private func flush() {
        let (stateBatch, midiBatch) = pendingLock.withLock { () -> ([String: ControllerState], [String: [Control: Int]]) in
            defer {
                pendingStates.removeAll(keepingCapacity: true)
                pendingMIDI.removeAll(keepingCapacity: true)
            }
            return (pendingStates, pendingMIDI)
        }
        if !stateBatch.isEmpty { states.merge(stateBatch) { _, new in new } }
        if let id = selectedDevice?.id, let values = midiBatch[id] { midiValues = values }
    }

    private func midiMappingChanged() {
        midi?.setMapping(midiMapping)
        // Debounced: typing a CC number should not write the file per keystroke.
        saveWork?.cancel()
        let mapping = midiMapping
        let work = DispatchWorkItem { [weak self] in
            do { try SettingsStore.save(mapping) } catch {
                Task { @MainActor in self?.append("Speichern fehlgeschlagen: \(error.localizedDescription)") }
            }
        }
        saveWork = work
        DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + 0.5, execute: work)
    }

    private func append(_ line: String) {
        let time = Date.now.formatted(date: .omitted, time: .standard)
        log.append("\(time)  \(line)")
        if log.count > 200 { log.removeFirst(log.count - 200) }
    }
}
