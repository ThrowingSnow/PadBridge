import Foundation
import PadCore

/// One keyboard/mouse event in the network wire format (see protocol.rs):
/// type 1 key/button (a = Linux code, b = 1 down / 0 up), type 2 move (a = dx,
/// b = dy), type 3 scroll (a = vertical, b = horizontal, 1/120 detents).
public struct WireInput: Equatable, Sendable {
    public let type: UInt8
    public let a: Int16
    public let b: Int16

    public init(_ event: KMEvent) {
        func clamp(_ v: Int) -> Int16 { Int16(clamping: v) }
        switch event {
        case .key(let key, let down): (type, a, b) = (1, Int16(key.linuxCode), down ? 1 : 0)
        case .button(let button, let down): (type, a, b) = (1, Int16(button.linuxCode), down ? 1 : 0)
        case .move(let dx, let dy): (type, a, b) = (2, clamp(dx), clamp(dy))
        case .scroll(let dx, let dy): (type, a, b) = (3, clamp(dy), clamp(dx))
        }
    }
}

/// Drives keyboard/mouse emulation from all connected pads, on its own queue.
/// Discrete input reacts to every report; pointer and scroll are integrated
/// at 100 Hz. Output goes to this Mac or, via `networkSend`, to the Linux host.
public final class KeyMouseBridge {
    /// Sends events to padbridge-netpad. Called with an empty array as a
    /// keepalive while keys are held, so the daemon does not release them.
    public var networkSend: (([WireInput]) -> Void)?

    private let queue = DispatchQueue(label: "padbridge.keys", qos: .userInteractive)
    private let poster = MacEventPoster()
    private var mapping: KeyMouseMapping
    private var mappers: [String: KeyMouseMapper] = [:]
    private var states: [String: ControllerState] = [:]
    private var timer: DispatchSourceTimer?
    private var lastTick = DispatchTime.now()
    private var lastKeepalive = DispatchTime.now()

    private static let rate = 100.0
    private static let keepaliveInterval: UInt64 = 200_000_000

    public init(manager: ControllerManager, mapping: KeyMouseMapping) {
        self.mapping = mapping
        manager.addListener { [weak self] event in self?.handle(event) }
        queue.async { self.updateTimer() }
    }

    public func setMapping(_ newMapping: KeyMouseMapping) {
        queue.async {
            let old = self.mapping
            // Turning off or moving to the other target: let go on the old side first.
            if old.enabled && (!newMapping.enabled || newMapping.target != old.target) {
                for id in self.mappers.keys {
                    let released = self.mappers[id]!.releaseAll()
                    self.emit(released, to: old.target)
                }
            }
            self.mapping = newMapping
            for id in self.mappers.keys { self.mappers[id]!.mapping = newMapping }
            self.updateTimer()
        }
    }

    private func handle(_ event: ControllerManager.Event) {
        queue.async {
            switch event {
            case .connected(let info):
                self.mappers[info.id] = KeyMouseMapper(mapping: self.mapping)
            case .disconnected(let info):
                if var mapper = self.mappers.removeValue(forKey: info.id) { self.emit(mapper.releaseAll()) }
                self.states[info.id] = nil
            case .state(let info, let state):
                self.states[info.id] = state
                guard self.mapping.enabled, var mapper = self.mappers[info.id] else { return }
                let events = mapper.process(state)
                self.mappers[info.id] = mapper
                self.emit(events)
            case .backendError:
                break
            }
        }
    }

    private func updateTimer() {
        if mapping.enabled, timer == nil {
            let timer = DispatchSource.makeTimerSource(queue: queue)
            timer.schedule(deadline: .now(), repeating: 1 / Self.rate, leeway: .milliseconds(1))
            timer.setEventHandler { [weak self] in self?.tick() }
            lastTick = .now()
            timer.resume()
            self.timer = timer
        } else if !mapping.enabled, let timer {
            timer.cancel()
            self.timer = nil
        }
    }

    private func tick() {
        let now = DispatchTime.now()
        let dt = min(Double(now.uptimeNanoseconds - lastTick.uptimeNanoseconds) / 1e9, 0.1)
        lastTick = now
        var events: [KMEvent] = []
        for (id, state) in states {
            guard var mapper = mappers[id] else { continue }
            events += mapper.tick(state, dt: dt)
            mappers[id] = mapper
        }
        emit(events)

        if mapping.target == .network, mappers.values.contains(where: \.isHoldingAnything),
           now.uptimeNanoseconds - lastKeepalive.uptimeNanoseconds > Self.keepaliveInterval {
            lastKeepalive = now
            networkSend?([])
        }
    }

    private func emit(_ events: [KMEvent], to target: EmulationTarget? = nil) {
        guard !events.isEmpty else { return }
        switch target ?? mapping.target {
        case .local: poster.post(events)
        case .network:
            lastKeepalive = .now()
            networkSend?(events.map(WireInput.init))
        }
    }
}
