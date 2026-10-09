import AppKit
import SwiftUI
import PadNet

/// Network gamepad: send the pad to padbridge-netpad on a Linux host.
struct NetworkView: View {
    @EnvironmentObject var model: AppModel
    @State private var portText = ""

    private static let presets: [(String, String)] = [
        ("Tower LAN", "192.0.2.10"),
        ("Tower Tailscale", "192.0.2.20"),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Toggle("Gamepad ans Netzwerk senden", isOn: $model.netSettings.enabled)
                .toggleStyle(.switch)
                .font(.headline)

            Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 10) {
                GridRow {
                    Text("Host")
                    HStack {
                        TextField("IP oder Hostname", text: $model.netSettings.host)
                            .frame(width: 200)
                        ForEach(Self.presets, id: \.1) { preset in
                            Button(preset.0) { model.netSettings.host = preset.1 }
                        }
                    }
                }
                GridRow {
                    Text("Rumble")
                    Toggle("Spiele auf dem Host dürfen den Controller vibrieren lassen", isOn: $model.netSettings.rumble)
                }
                GridRow {
                    Text("Port")
                    TextField("48100", value: $model.netSettings.port, format: .number.grouping(.never))
                        .frame(width: 80)
                }
            }

            Divider()
            status
            Divider()
            keySection
            Spacer()
            Text("Moonlight: unter Einstellungen den Gamepad-Support ausschalten, sonst legt Sunshine ein zweites Pad an. "
                 + "Bild, Ton, Maus und Tastatur laufen weiter über Moonlight.")
                .font(.caption).foregroundStyle(.secondary)
        }
        .padding(20)
    }

    private var status: some View {
        let s = model.netStatus
        return VStack(alignment: .leading, spacing: 6) {
            Text("Status").font(.headline)
            HStack(spacing: 8) {
                Circle().fill(color(s)).frame(width: 10, height: 10)
                Text(text(s))
                if let ms = s.latencyMs {
                    Text(String(format: "· Latenz %.1f ms (Hin und zurück)", ms)).foregroundStyle(.secondary)
                }
            }
            if let error = s.error { Text(error).font(.caption).foregroundStyle(.orange) }
            if s.midiLink {
                Text("MIDI läuft über diese Verbindung (MIDI-Tab → Ziel: Tower).").font(.caption).foregroundStyle(.secondary)
            }
            ForEach(model.devices) { device in
                if let pad = s.pads[device.id] {
                    Text("\(device.name) → virtuelles Pad \(pad + 1)").font(.caption).foregroundStyle(.secondary)
                }
            }
        }
    }

    private func color(_ s: NetPadOutput.Status) -> Color {
        guard s.sending else { return .secondary }
        return s.daemonAnswering ? .green : .orange
    }

    private func text(_ s: NetPadOutput.Status) -> String {
        guard s.sending else { return "Aus" }
        return s.daemonAnswering ? "Verbunden – Daemon antwortet" : "Sende – keine Antwort vom Daemon (läuft er? UFW?)"
    }

    private var keySection: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Schlüssel").font(.headline)
            Text("Der Daemon akzeptiert nur Pakete, die mit diesem Schlüssel signiert sind. "
                 + "Auf dem Linux-Host gehört er nach ~/.config/padbridge/netpad.key (chmod 600).")
                .font(.caption).foregroundStyle(.secondary)
            HStack {
                Button("Schlüssel kopieren") {
                    guard let hex = model.netKeyHex else { return }
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(hex, forType: .string)
                }
                .disabled(model.netKeyHex == nil)
                Button("Im Finder zeigen") {
                    NSWorkspace.shared.activateFileViewerSelecting([NetPadKey.fileURL])
                }
            }
        }
    }
}
