import CryptoKit
import Foundation
import Network
import PadCore

/// Sends every connected controller to padbridge-netpad over UDP, and carries
/// MIDI both ways when MIDI is routed to the network.
///
/// - state packets go out immediately on every change,
/// - the last state is repeated every 100 ms so a lost datagram heals quickly
///   (the daemon releases everything after 500 ms of silence),
/// - a ping every second measures round-trip time and shows whether the
///   daemon is answering at all.
public final class NetPadOutput {
    public struct Settings: Codable, Equatable, Sendable {
        /// Send the gamepad. The link also opens for MIDI alone (`setMIDILink`).
        public var enabled: Bool
        public var host: String
        public var port: UInt16
        /// Let games on the host drive the pad's motors.
        public var rumble: Bool

        public init(enabled: Bool = false, host: String = "192.0.2.10", port: UInt16 = 48100, rumble: Bool = true) {
            self.enabled = enabled
            self.host = host
            self.port = port
            self.rumble = rumble
        }

        // Missing fields (older settings files) take their defaults.
        public init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            let d = Settings()
            enabled = try c.decodeIfPresent(Bool.self, forKey: .enabled) ?? d.enabled
            host = try c.decodeIfPresent(String.self, forKey: .host) ?? d.host
            port = try c.decodeIfPresent(UInt16.self, forKey: .port) ?? d.port
            rumble = try c.decodeIfPresent(Bool.self, forKey: .rumble) ?? d.rumble
        }
    }

    public struct Status: Equatable, Sendable {
        public var sending: Bool
        /// Round-trip time of the last answered ping.
        public var latencyMs: Double?
        /// True if a pong arrived within the last 3 seconds.
        public var daemonAnswering: Bool
        public var pads: [String: Int]   // device id -> pad index
        public var error: String?
        public var midiLink: Bool
        public var inputLink: Bool

        public static let off = Status(sending: false, latencyMs: nil, daemonAnswering: false, pads: [:], error: nil,
                                       midiLink: false, inputLink: false)
    }

    /// Called on an internal queue whenever the status changes.
    public var onStatus: ((Status) -> Void)?
    /// MIDI received from the Linux host (e.g. clock from the DAW), on an internal queue.
    public var onMIDI: (([UInt8]) -> Void)?

    private let key: SymmetricKey
    private weak var manager: ControllerManager?
    /// Pads currently rumbling at the host's request, and when we last heard so.
    private var rumbleHeard: [UInt8: Date] = [:]
    /// Without a refresh for this long the motors are stopped (a lost "off").
    private static let rumbleTimeout: TimeInterval = 0.5
    private let queue = DispatchQueue(label: "padbridge.netpad", qos: .userInteractive)
    private var settings = Settings()
    private var midiLink = false
    private var inputLink = false
    private var ready = false
    /// MIDI sent before the connection is ready (e.g. the initial value burst
    /// on connect); flushed once it is. Gamepad state needs no buffer, it repeats.
    private var pendingMIDI: [[UInt8]] = []
    private var lastInboundSeq: UInt64 = 0
    private var connection: NWConnection?
    private var timer: DispatchSourceTimer?
    private var seq: UInt64
    private var pads: [String: UInt8] = [:]
    private var lastStates: [UInt8: ControllerState] = [:]
    private var ticks = 0
    private var lastPong: Date?
    private var latencyMs: Double?
    private var lastError: String?
    private var lastStatus: Status?

    public init(manager: ControllerManager, key: SymmetricKey) {
        self.key = key
        self.manager = manager
        // Start above anything sent by a previous run: the daemon only accepts
        // increasing sequence numbers (replay protection).
        seq = UInt64(Date().timeIntervalSince1970 * 1_000_000)
        manager.addListener { [weak self] event in self?.handle(event) }
    }

    public func apply(_ newSettings: Settings) {
        queue.async {
            let endpointChanged = newSettings.host != self.settings.host || newSettings.port != self.settings.port
            let wasSendingPad = self.settings.enabled
            self.settings = newSettings
            if wasSendingPad && !newSettings.enabled { self.sendByes() }
            if !newSettings.rumble || !newSettings.enabled { self.stopAllRumble() }
            self.reconcile(restart: endpointChanged)
        }
    }

    /// Keeps the link open for MIDI even when the gamepad is not being sent.
    public func setMIDILink(_ on: Bool) {
        queue.async {
            self.midiLink = on
            self.reconcile(restart: false)
        }
    }

    /// Keeps the link open for keyboard/mouse emulation on the host.
    public func setInputLink(_ on: Bool) {
        queue.async {
            self.inputLink = on
            self.reconcile(restart: false)
        }
    }

    /// Keyboard/mouse events for the host; empty = keepalive for held keys.
    public func sendInput(_ events: [(type: UInt8, a: Int16, b: Int16)]) {
        queue.async {
            guard self.connection != nil, self.ready else { return }
            for datagram in NetPadProtocol.input(events, nextSeq: self.nextSeq, key: self.key) {
                // Twice, like MIDI: a lost key-up would leave a key stuck.
                self.send(datagram)
                self.send(datagram)
            }
        }
    }

    public func sendMIDI(_ messages: [[UInt8]]) {
        queue.async {
            guard self.connection != nil, !messages.isEmpty else { return }
            guard self.ready else {
                self.pendingMIDI = Array((self.pendingMIDI + messages).suffix(256))
                return
            }
            self.transmitMIDI(messages)
        }
    }

    private func transmitMIDI(_ messages: [[UInt8]]) {
        for datagram in NetPadProtocol.midi(messages, nextSeq: nextSeq, key: key) {
            // Twice: the daemon drops the duplicate, but a single loss no longer loses a note-off.
            send(datagram)
            send(datagram)
        }
    }

    private func reconcile(restart: Bool) {
        let wanted = settings.enabled || midiLink || inputLink
        if connection != nil, !wanted || restart { stopOnQueue(sendBye: settings.enabled) }
        if connection == nil, wanted { startOnQueue() }
        publishStatus()
    }

    private func sendByes() {
        for pad in pads.values { send(NetPadProtocol.bye(pad: pad, seq: nextSeq(), key: key)) }
    }

    // MARK: Connection

    private func startOnQueue() {
        guard let port = NWEndpoint.Port(rawValue: settings.port), !settings.host.isEmpty else {
            lastError = "Ungültiger Host oder Port"
            return
        }
        let params = NWParameters.udp
        params.serviceClass = .interactiveVideo   // low-latency DSCP marking
        let connection = NWConnection(host: NWEndpoint.Host(settings.host), port: port, using: params)
        connection.stateUpdateHandler = { [weak self] state in
            guard let self else { return }
            switch state {
            case .failed(let error), .waiting(let error):
                self.ready = false
                self.lastError = error.localizedDescription
            case .ready:
                self.ready = true
                self.lastError = nil
                let pending = self.pendingMIDI
                self.pendingMIDI.removeAll()
                self.transmitMIDI(pending)
            default:
                break
            }
            self.publishStatus()
        }
        connection.start(queue: queue)
        self.connection = connection
        receive(on: connection)

        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now(), repeating: .milliseconds(100), leeway: .milliseconds(5))
        timer.setEventHandler { [weak self] in self?.tick() }
        timer.resume()
        self.timer = timer
        // Push current states right away so the virtual pads appear.
        if settings.enabled {
            for (pad, state) in lastStates { send(NetPadProtocol.state(pad: pad, seq: nextSeq(), state: state, key: key)) }
        }
    }

    private func stopOnQueue(sendBye: Bool) {
        if sendBye, connection != nil { sendByes() }
        stopAllRumble()
        timer?.cancel()
        timer = nil
        connection?.cancel()
        connection = nil
        ready = false
        pendingMIDI.removeAll()
        lastPong = nil
        latencyMs = nil
        lastError = nil
    }

    private func receive(on connection: NWConnection) {
        connection.receiveMessage { [weak self] data, _, _, error in
            guard let self, error == nil, self.connection === connection else { return }
            if let data, let packet = NetPadProtocol.decode(data, key: self.key), packet.seq > self.lastInboundSeq {
                self.lastInboundSeq = packet.seq
                switch packet.kind {
                case .pong:
                    let sent = NetPadProtocol.timestamp(of: packet.payload)
                    let now = DispatchTime.now().uptimeNanoseconds
                    if now >= sent { self.latencyMs = Double(now - sent) / 1_000_000 }
                    self.lastPong = Date()
                    self.publishStatus()
                case .midiIn:
                    for message in NetPadProtocol.midiMessages(packet.payload) { self.onMIDI?(message) }
                case .rumble:
                    self.applyRumble(pad: packet.pad, heavy: packet.payload[0], light: packet.payload[1])
                default:
                    break
                }
            }
            self.receive(on: connection)
        }
    }

    private func applyRumble(pad: UInt8, heavy: UInt8, light: UInt8) {
        guard settings.rumble, settings.enabled, let device = device(forPad: pad) else { return }
        device.setRumble(low: Float(heavy) / 255, high: Float(light) / 255)
        if heavy == 0 && light == 0 { rumbleHeard[pad] = nil } else { rumbleHeard[pad] = Date() }
    }

    private func stopAllRumble() {
        for pad in rumbleHeard.keys { device(forPad: pad)?.setRumble(low: 0, high: 0) }
        rumbleHeard.removeAll()
    }

    private func device(forPad pad: UInt8) -> ControllerDevice? {
        guard let id = pads.first(where: { $0.value == pad })?.key else { return nil }
        return manager?.device(id: id)
    }

    private func tick() {
        let now = Date()
        for (pad, heard) in rumbleHeard where now.timeIntervalSince(heard) > Self.rumbleTimeout {
            device(forPad: pad)?.setRumble(low: 0, high: 0)
            rumbleHeard[pad] = nil
        }
        for (pad, state) in lastStates where settings.enabled {
            send(NetPadProtocol.state(pad: pad, seq: nextSeq(), state: state, key: key))
        }
        ticks += 1
        if ticks % 10 == 0 {
            send(NetPadProtocol.ping(pad: 0, seq: nextSeq(), timestamp: DispatchTime.now().uptimeNanoseconds, key: key))
            publishStatus()
        }
    }

    private func send(_ datagram: Data) {
        connection?.send(content: datagram, completion: .idempotent)
    }

    /// Wall-clock microseconds, but always increasing. Tracking real time
    /// means a restarted sender (or another tool using the same key) is never
    /// locked out by the daemon's replay check for longer than its clock lag.
    private func nextSeq() -> UInt64 {
        seq = max(seq + 1, UInt64(Date().timeIntervalSince1970 * 1_000_000))
        return seq
    }

    // MARK: Controller events

    private func handle(_ event: ControllerManager.Event) {
        queue.async {
            switch event {
            case .connected(let info):
                let used = Set(self.pads.values)
                guard let free = (0..<4).map(UInt8.init).first(where: { !used.contains($0) }) else { return }
                self.pads[info.id] = free
                self.publishStatus()
            case .disconnected(let info):
                guard let pad = self.pads.removeValue(forKey: info.id) else { return }
                self.lastStates[pad] = nil
                self.send(NetPadProtocol.bye(pad: pad, seq: self.nextSeq(), key: self.key))
                self.publishStatus()
            case .state(let info, let state):
                guard let pad = self.pads[info.id] else { return }
                self.lastStates[pad] = state
                guard self.settings.enabled else { return }
                self.send(NetPadProtocol.state(pad: pad, seq: self.nextSeq(), state: state, key: self.key))
            case .backendError:
                break
            }
        }
    }

    private func publishStatus() {
        let answering = lastPong.map { Date().timeIntervalSince($0) < 3 } ?? false
        let status = Status(sending: connection != nil, latencyMs: answering ? latencyMs : nil,
                            daemonAnswering: answering, pads: settings.enabled ? pads.mapValues(Int.init) : [:],
                            error: lastError, midiLink: midiLink, inputLink: inputLink)
        guard status != lastStatus else { return }
        lastStatus = status
        onStatus?(status)
    }
}
