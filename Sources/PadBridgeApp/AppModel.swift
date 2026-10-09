import Foundation
import PadCore
import PadUSB

/// Bridges `ControllerManager` (device queues, ~250 Hz) to SwiftUI (main thread).
/// Input states are coalesced and published at display rate so the UI never
/// throttles the input path.
@MainActor
final class AppModel: ObservableObject {
    @Published private(set) var devices: [ControllerInfo] = []
    @Published private(set) var states: [String: ControllerState] = [:]
    @Published private(set) var log: [String] = []
    @Published var selectedID: String?

    let manager = ControllerManager(backends: [USBBackend()])

    private let pendingLock = NSLock()
    nonisolated(unsafe) private var pending: [String: ControllerState] = [:]
    private var timer: Timer?

    init() {
        manager.addListener { [weak self] event in self?.handle(event) }
        manager.start()
        timer = Timer.scheduledTimer(withTimeInterval: 1.0 / 60, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.flushStates() }
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
        guard let id = selectedDevice?.id else { return }
        manager.device(id: id)?.setPlayerIndicator(indicator)
    }

    nonisolated private func handle(_ event: ControllerManager.Event) {
        switch event {
        case .state(let info, let state):
            pendingLock.withLock { pending[info.id] = state }
        case .connected(let info):
            Task { @MainActor in
                self.devices.append(info)
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

    private func flushStates() {
        let batch = pendingLock.withLock { () -> [String: ControllerState] in
            defer { pending.removeAll(keepingCapacity: true) }
            return pending
        }
        guard !batch.isEmpty else { return }
        states.merge(batch) { _, new in new }
    }

    private func append(_ line: String) {
        let time = Date.now.formatted(date: .omitted, time: .standard)
        log.append("\(time)  \(line)")
        if log.count > 200 { log.removeFirst(log.count - 200) }
    }
}
