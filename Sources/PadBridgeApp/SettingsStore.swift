import Foundation
import PadMIDI
import PadNet

/// Persists settings as JSON in ~/Library/Application Support/PadBridge/.
/// Plain files (not UserDefaults) so profiles can later be shared or versioned.
enum SettingsStore {
    private static var directory: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("PadBridge", isDirectory: true)
    }

    private static var midiURL: URL { directory.appendingPathComponent("midi-mapping.json") }
    private static var netURL: URL { directory.appendingPathComponent("netpad.json") }

    static func loadNetSettings() -> NetPadOutput.Settings {
        guard let data = try? Data(contentsOf: netURL),
              let settings = try? JSONDecoder().decode(NetPadOutput.Settings.self, from: data) else { return .init() }
        return settings
    }

    static func save(_ settings: NetPadOutput.Settings) throws {
        try write(settings, to: netURL)
    }

    static func loadMIDIMapping() -> MIDIMapping {
        guard let data = try? Data(contentsOf: midiURL),
              let mapping = try? JSONDecoder().decode(MIDIMapping.self, from: data) else { return .default }
        return mapping
    }

    static func save(_ mapping: MIDIMapping) throws {
        try write(mapping, to: midiURL)
    }

    private static func write<T: Encodable>(_ value: T, to url: URL) throws {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(value).write(to: url, options: .atomic)
    }
}
