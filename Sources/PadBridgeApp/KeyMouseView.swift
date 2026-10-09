import SwiftUI
import PadCore
import PadKeys

/// Keyboard/mouse emulation: target, stick roles, speeds and per-control actions.
struct KeyMouseView: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .top, spacing: 32) {
                general
                sticks
            }
            Divider()
            bindingTable
        }
        .padding(20)
    }

    private var general: some View {
        VStack(alignment: .leading, spacing: 8) {
            Toggle("Tastatur/Maus aktiv", isOn: $model.keyMouse.enabled)
                .toggleStyle(.switch)
            Picker("Ziel", selection: $model.keyMouse.target) {
                Text("Mac").tag(EmulationTarget.local)
                Text("Tower (Netzwerk)").tag(EmulationTarget.network)
            }
            .pickerStyle(.segmented)
            .frame(width: 260)
            targetStatus
            Text("Läuft parallel zum Gamepad: belege am besten Knöpfe, die das Spiel nicht braucht.")
                .font(.caption).foregroundStyle(.secondary)
                .frame(width: 300, alignment: .leading)
            Button("Desktop-Preset laden") { model.resetKeyMouse() }
        }
    }

    @ViewBuilder
    private var targetStatus: some View {
        switch model.keyMouse.target {
        case .local:
            if model.accessibilityGranted {
                Label("Bedienungshilfen freigegeben", systemImage: "checkmark.circle.fill")
                    .font(.caption).foregroundStyle(.green)
            } else {
                VStack(alignment: .leading, spacing: 4) {
                    Label("Bedienungshilfen-Freigabe fehlt", systemImage: "exclamationmark.triangle.fill")
                        .font(.caption).foregroundStyle(.orange)
                    Text("Systemeinstellungen → Datenschutz & Sicherheit → Bedienungshilfen: "
                         + "PadBridge (bzw. Terminal bei `swift run`) erlauben.")
                        .font(.caption).foregroundStyle(.secondary)
                        .frame(width: 300, alignment: .leading)
                    Button("Freigabe anfordern") { model.requestAccessibility() }
                }
            }
        case .network:
            let s = model.netStatus
            HStack(spacing: 6) {
                Circle().fill(s.daemonAnswering ? Color.green : Color.orange).frame(width: 8, height: 8)
                Text(s.daemonAnswering
                     ? "Virtuelle Tastatur/Maus auf \(model.netSettings.host)"
                     : "Keine Antwort von \(model.netSettings.host) – Host im Netzwerk-Tab prüfen")
            }
            .font(.caption).foregroundStyle(.secondary)
        }
    }

    private var sticks: some View {
        Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 8) {
            GridRow {
                Text("Linker Stick")
                rolePicker($model.keyMouse.leftStick)
            }
            GridRow {
                Text("Rechter Stick")
                rolePicker($model.keyMouse.rightStick)
            }
            GridRow {
                Text("Maus-Tempo")
                slider($model.keyMouse.mouseSpeed, 200...4000, "%.0f pt/s")
            }
            GridRow {
                Text("Scroll-Tempo")
                slider($model.keyMouse.scrollSpeed, 2...40, "%.0f /s")
            }
            GridRow {
                Text("Kurve")
                slider($model.keyMouse.curve, 1...3, "%.1f")
                    .help("1 = linear, höher = feiner um die Mitte, schnell am Rand")
            }
            GridRow {
                Text("Deadzone")
                HStack {
                    Slider(value: $model.keyMouse.stickDeadzone, in: 0...0.4).frame(width: 160)
                    Text("\(Int((model.keyMouse.stickDeadzone * 100).rounded())) %")
                        .font(.caption.monospacedDigit()).frame(width: 60, alignment: .trailing)
                }
            }
        }
    }

    private func rolePicker(_ role: Binding<StickRole>) -> some View {
        Picker("", selection: role) {
            ForEach(StickRole.allCases, id: \.self) { Text($0.displayName).tag($0) }
        }
        .labelsHidden()
        .frame(width: 160)
    }

    private func slider(_ value: Binding<Double>, _ range: ClosedRange<Double>, _ format: String) -> some View {
        HStack {
            Slider(value: value, in: range).frame(width: 160)
            Text(String(format: format, value.wrappedValue))
                .font(.caption.monospacedDigit()).frame(width: 60, alignment: .trailing)
        }
    }

    private var bindingTable: some View {
        ScrollView {
            Grid(alignment: .leading, horizontalSpacing: 14, verticalSpacing: 6) {
                GridRow {
                    Text("Control"); Text("Aktion"); Text("Belegung"); Text("Modifier")
                }
                .font(.caption.bold())
                .foregroundStyle(.secondary)
                Divider().gridCellUnsizedAxes(.horizontal)

                ForEach($model.keyMouse.bindings) { $binding in
                    ActionRow(binding: $binding)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

/// Edits one `KeyBinding`: kind (off / mouse / key), then button or key + modifiers.
private struct ActionRow: View {
    @Binding var binding: KeyBinding

    private enum Kind: Hashable { case none, mouse, key }

    private var kind: Binding<Kind> {
        Binding(
            get: {
                switch binding.action {
                case .none: return .none
                case .mouse: return .mouse
                case .key: return .key
                }
            },
            set: { kind in
                switch kind {
                case .none: binding.action = .none
                case .mouse: binding.action = .mouse(.left)
                case .key: binding.action = .key(.space, modifiers: [])
                }
            }
        )
    }

    var body: some View {
        GridRow {
            Text(binding.control.displayName).frame(width: 110, alignment: .leading)

            Picker("", selection: kind) {
                Text("Aus").tag(Kind.none)
                Text("Maus").tag(Kind.mouse)
                Text("Taste").tag(Kind.key)
            }
            .labelsHidden().pickerStyle(.segmented).frame(width: 170)

            detail.frame(width: 170, alignment: .leading)
            modifiers.frame(width: 190, alignment: .leading)
        }
        .opacity(binding.action == .none ? 0.5 : 1)
    }

    @ViewBuilder
    private var detail: some View {
        switch binding.action {
        case .none:
            Text("–").foregroundStyle(.secondary)
        case .mouse(let button):
            Picker("", selection: Binding(get: { button }, set: { binding.action = .mouse($0) })) {
                ForEach(MouseButton.allCases, id: \.self) { Text($0.displayName).tag($0) }
            }
            .labelsHidden()
        case .key(let key, let mods):
            Picker("", selection: Binding(get: { key }, set: { binding.action = .key($0, modifiers: mods) })) {
                ForEach(KeyCode.allCases, id: \.self) { Text($0.displayName).tag($0) }
            }
            .labelsHidden()
        }
    }

    @ViewBuilder
    private var modifiers: some View {
        if case .key(let key, let mods) = binding.action {
            HStack(spacing: 4) {
                ForEach(Modifier.allCases, id: \.self) { modifier in
                    let on = mods.contains(modifier)
                    Button(modifier.symbol) {
                        // Keep a stable order (⇧⌃⌥⌘) so combos press consistently.
                        let next = on ? mods.filter { $0 != modifier } : mods + [modifier]
                        binding.action = .key(key, modifiers: Modifier.allCases.filter(next.contains))
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(on ? .accentColor : .secondary.opacity(0.35))
                }
            }
        } else {
            Text("")
        }
    }
}
