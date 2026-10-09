import CoreMIDI
import Foundation

/// A virtual CoreMIDI device: one source (we send controller data) and one
/// destination (hosts send us clock). Both show up as "<name>" in DAWs.
public final class MIDIEndpoint {
    public enum SetupError: Error, CustomStringConvertible {
        case coreMIDI(String, OSStatus)
        public var description: String {
            switch self { case .coreMIDI(let call, let status): return "\(call) failed (\(status))" }
        }
    }

    /// Called on CoreMIDI's receive thread with the time the message arrived.
    public var onClock: ((ClockMessage, Double) -> Void)?

    private var client = MIDIClientRef()
    private var source = MIDIEndpointRef()
    private var destination = MIDIEndpointRef()

    public init(name: String) throws {
        try check("MIDIClientCreateWithBlock", MIDIClientCreateWithBlock(name as CFString, &client, nil))
        try check("MIDISourceCreateWithProtocol",
                  MIDISourceCreateWithProtocol(client, name as CFString, ._1_0, &source))
        try check("MIDIDestinationCreateWithProtocol",
                  MIDIDestinationCreateWithProtocol(client, name as CFString, ._1_0, &destination) { [weak self] list, _ in
                      self?.receive(list)
                  })
    }

    deinit {
        MIDIEndpointDispose(source)
        MIDIEndpointDispose(destination)
        MIDIClientDispose(client)
    }

    public func send(_ messages: [MIDIMessage]) {
        guard !messages.isEmpty else { return }
        // A MIDIEventList packet holds 64 words; one MIDI 1.0 UMP is one word.
        for chunk in stride(from: 0, to: messages.count, by: 48).map({ messages[$0..<min($0 + 48, messages.count)] }) {
            var list = MIDIEventList()
            var packet = MIDIEventListInit(&list, ._1_0)
            for message in chunk {
                // UMP type 2 (MIDI 1.0 channel voice), group 0.
                var word: UInt32 = 0x2000_0000 | UInt32(message.status) << 16 | UInt32(message.data1) << 8 | UInt32(message.data2)
                packet = MIDIEventListAdd(&list, MemoryLayout<MIDIEventList>.size, packet, 0, 1, &word)
            }
            MIDIReceivedEventList(source, &list)
        }
    }

    private func receive(_ list: UnsafePointer<MIDIEventList>) {
        guard let onClock else { return }
        let now = Double(clock_gettime_nsec_np(CLOCK_UPTIME_RAW)) / 1e9
        for packet in list.unsafeSequence() {
            let count = Int(packet.pointee.wordCount)
            withUnsafeBytes(of: packet.pointee.words) { raw in
                let words = raw.bindMemory(to: UInt32.self)
                for i in 0..<min(count, words.count) {
                    if let message = Self.clockMessage(words[i]) { onClock(message, now) }
                }
            }
        }
    }

    /// Decodes UMP type 1 (system real-time / common) words.
    static func clockMessage(_ word: UInt32) -> ClockMessage? {
        guard word >> 28 == 0x1 else { return nil }
        switch UInt8((word >> 16) & 0xFF) {
        case 0xF8: return .tick
        case 0xFA: return .start
        case 0xFB: return .continue
        case 0xFC: return .stop
        case 0xF2:
            let lsb = Int((word >> 8) & 0x7F)
            let msb = Int(word & 0x7F)
            return .songPosition(msb << 7 | lsb)
        default: return nil
        }
    }

    private func check(_ call: String, _ status: OSStatus) throws {
        guard status == noErr else { throw SetupError.coreMIDI(call, status) }
    }
}
