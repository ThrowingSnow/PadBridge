import Foundation

/// Owns all backends and the set of connected devices, and fans their events
/// out to listeners. UI-framework agnostic: callbacks arrive on device queues,
/// listeners hop to the main thread themselves if they need to.
public final class ControllerManager: ControllerBackendDelegate {
    public enum Event {
        case connected(ControllerInfo)
        case disconnected(ControllerInfo)
        case state(ControllerInfo, ControllerState)
        case backendError(String)
    }

    private let backends: [ControllerBackend]
    private let lock = NSLock()
    private var devices: [String: ControllerDevice] = [:]
    private var listeners: [UUID: (Event) -> Void] = [:]

    public init(backends: [ControllerBackend]) {
        self.backends = backends
    }

    public func start() { backends.forEach { $0.start(delegate: self) } }

    public func stop() {
        backends.forEach { $0.stop() }
        lock.withLock { devices }.values.forEach { $0.close() }
        lock.withLock { devices.removeAll() }
    }

    public var connected: [ControllerDevice] {
        lock.withLock { Array(devices.values) }.sorted { $0.info.id < $1.info.id }
    }

    public func device(id: String) -> ControllerDevice? { lock.withLock { devices[id] } }

    /// Returns a token; pass it to `removeListener` to unsubscribe.
    @discardableResult
    public func addListener(_ listener: @escaping (Event) -> Void) -> UUID {
        let token = UUID()
        lock.withLock { listeners[token] = listener }
        return token
    }

    public func removeListener(_ token: UUID) { _ = lock.withLock { listeners.removeValue(forKey: token) } }

    private func emit(_ event: Event) {
        for listener in lock.withLock({ Array(listeners.values) }) { listener(event) }
    }

    // MARK: ControllerBackendDelegate

    public func backend(_ backend: ControllerBackend, didConnect device: ControllerDevice) {
        lock.withLock { devices[device.info.id] = device }
        let info = device.info
        device.onState = { [weak self] state in self?.emit(.state(info, state)) }
        emit(.connected(info))
        // Pads like the Xbox 360 only report on change, so an untouched pad would
        // stay silent: publish the current state once so outputs start right away.
        emit(.state(info, device.state))
    }

    public func backend(_ backend: ControllerBackend, didDisconnect device: ControllerDevice) {
        _ = lock.withLock { devices.removeValue(forKey: device.info.id) }
        device.onState = nil
        emit(.disconnected(device.info))
    }

    public func backend(_ backend: ControllerBackend, didFail message: String) {
        emit(.backendError("[\(backend.name)] \(message)"))
    }
}
