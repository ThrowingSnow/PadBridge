import AppKit
import SwiftUI
import PadNet

/// Network gamepad: send the pad to padbridge-netpad on a Linux host.
struct NetworkView: View {
    @EnvironmentObject var model: AppModel
    @State private var showSavePreset = false
    @State private var presetName = ""

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
                        presetButtons
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

    /// Saved hosts: click to use, right-click to delete, "+" saves the current host.
    private var presetButtons: some View {
        HStack {
            ForEach(model.netSettings.presets) { preset in
                Button(preset.name) { model.netSettings.host = preset.host }
                    .help(preset.host)
                    .tint(preset.host == model.netSettings.host ? .accentColor : nil)
                    .buttonStyle(.bordered)
                    .contextMenu {
                        Button("Preset „\(preset.name)“ löschen", role: .destructive) {
                            model.netSettings.presets.removeAll { $0 == preset }
                        }
                    }
            }
            Button {
                presetName = ""
                showSavePreset = true
            } label: {
                Image(systemName: "plus")
            }
            .help("Aktuellen Host als Preset speichern (Rechtsklick auf ein Preset löscht es)")
            .disabled(model.netSettings.host.trimmingCharacters(in: .whitespaces).isEmpty)
            .popover(isPresented: $showSavePreset, arrowEdge: .bottom) { savePresetForm }
        }
    }

    private var savePresetForm: some View {
        let name = presetName.trimmingCharacters(in: .whitespaces)
        return VStack(alignment: .leading, spacing: 10) {
            Text("Preset für \(model.netSettings.host)").font(.headline)
            TextField("Name, z.B. Desktop LAN", text: $presetName)
                .frame(width: 220)
                .onSubmit(savePreset)
            HStack {
                Spacer()
                Button("Abbrechen") { showSavePreset = false }
                Button("Speichern", action: savePreset)
                    .keyboardShortcut(.defaultAction)
                    .disabled(name.isEmpty)
            }
        }
        .padding(14)
    }

    private func savePreset() {
        let name = presetName.trimmingCharacters(in: .whitespaces)
        let host = model.netSettings.host.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty, !host.isEmpty else { return }
        // Same name again = update that preset's host.
        model.netSettings.presets.removeAll { $0.name == name }
        model.netSettings.presets.append(.init(name: name, host: host))
        showSavePreset = false
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
                Text("MIDI läuft über diese Verbindung (MIDI-Tab → Ziel: Linux-Host).").font(.caption).foregroundStyle(.secondary)
            }
            if s.inputLink {
                Text("Tastatur/Maus läuft über diese Verbindung.").font(.caption).foregroundStyle(.secondary)
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
