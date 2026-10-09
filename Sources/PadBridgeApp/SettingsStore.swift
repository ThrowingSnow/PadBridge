import Foundation
import PadMIDI

/// Persists settings as JSON in ~/Library/Application Support/PadBridge/.
/// Plain files (not UserDefaults) so profiles can later be shared or versioned.
enum SettingsStore {
    private static var directory: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("PadBridge", isDirectory: true)
    }

    private static var midiURL: URL { directory.appendingPathComponent("midi-mapping.json") }

    static func loadMIDIMapping() -> MIDIMapping {
        guard let data = try? Data(contentsOf: midiURL),
              let mapping = try? JSONDecoder().decode(MIDIMapping.self, from: data) else { return .default }
        return mapping
    }

    static func save(_ mapping: MIDIMapping) throws {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(mapping).write(to: midiURL, options: .atomic)
    }
}
