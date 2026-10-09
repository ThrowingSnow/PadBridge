import CryptoKit
import Foundation
import Security

/// The shared secret between PadBridge and padbridge-netpad: 32 random bytes,
/// stored as 64 hex characters in a file only the user can read. The same file
/// format is what the Linux daemon reads from ~/.config/padbridge/netpad.key.
public enum NetPadKey {
    public static var fileURL: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("PadBridge/netpad.key")
    }

    /// Loads the key, creating one on first use.
    public static func loadOrCreate() throws -> SymmetricKey {
        if let hex = try? String(contentsOf: fileURL, encoding: .utf8), let key = parse(hex) {
            return key
        }
        var bytes = [UInt8](repeating: 0, count: 32)
        guard SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) == errSecSuccess else {
            throw CocoaError(.fileWriteUnknown)
        }
        let hex = bytes.map { String(format: "%02x", $0) }.joined()
        try FileManager.default.createDirectory(at: fileURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try (hex + "\n").write(to: fileURL, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: fileURL.path)
        return SymmetricKey(data: bytes)
    }

    /// Hex text of the current key, for copying to the Linux host.
    public static func hexString() -> String? {
        (try? String(contentsOf: fileURL, encoding: .utf8))?.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    static func parse(_ text: String) -> SymmetricKey? {
        let hex = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard hex.count == 64 else { return nil }
        var bytes: [UInt8] = []
        var index = hex.startIndex
        while index < hex.endIndex {
            let next = hex.index(index, offsetBy: 2)
            guard let byte = UInt8(hex[index..<next], radix: 16) else { return nil }
            bytes.append(byte)
            index = next
        }
        return SymmetricKey(data: bytes)
    }
}
