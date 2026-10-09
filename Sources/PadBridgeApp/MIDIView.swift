import SwiftUI
import PadCore
import PadMIDI

/// MIDI settings: global options and the per-control CC/note table.
struct MIDIView: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            globalSettings
            Divider()
            bindingTable
        }
        .padding(20)
    }

    private var globalSettings: some View {
        HStack(alignment: .top, spacing: 32) {
            VStack(alignment: .leading, spacing: 8) {
                Toggle("MIDI senden", isOn: $model.midiMapping.enabled)
                    .toggleStyle(.switch)
                    .disabled(!model.midiAvailable)
                Text(model.midiAvailable
                     ? "Virtueller Port „PadBridge“ – in der DAW als MIDI-Eingang wählen."
                     : "CoreMIDI-Port konnte nicht erstellt werden (siehe Log).")
                    .font(.caption).foregroundStyle(.secondary)
                Picker("Kanal", selection: $model.midiMapping.channel) {
                    ForEach(1...16, id: \.self) { Text("\($0)").tag($0) }
                }
                .frame(width: 140)
            }
            VStack(alignment: .leading, spacing: 8) {
                deadzone("Deadzone Sticks", $model.midiMapping.stickDeadzone)
                deadzone("Deadzone Trigger", $model.midiMapping.triggerDeadzone)
                Button("Auf Standard zurücksetzen") { model.resetMIDIMapping() }
            }
            VStack(alignment: .leading, spacing: 6) {
                Text("Clock-Eingang").font(.headline)
                ClockBadge(clock: model.clock)
                Text("In der DAW MIDI-Clock an „PadBridge“ senden.\nLED-Ring: Monitor → „Clock: Takt“ oder „Clock: Beat“.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
    }

    private func deadzone(_ title: String, _ value: Binding<Float>) -> some View {
        HStack {
            Text(title).frame(width: 120, alignment: .leading)
            Slider(value: value, in: 0...0.5).frame(width: 160)
            Text("\(Int((value.wrappedValue * 100).rounded())) %")
                .font(.caption.monospacedDigit()).frame(width: 40, alignment: .trailing)
        }
    }

    private var bindingTable: some View {
        let conflicts = model.midiMapping.conflicts
        return ScrollView {
            Grid(alignment: .leading, horizontalSpacing: 14, verticalSpacing: 6) {
                GridRow {
                    Text("Control"); Text("Aktiv"); Text("Typ"); Text("Nr."); Text("Modus"); Text("Wert")
                }
                .font(.caption.bold())
                .foregroundStyle(.secondary)
                Divider().gridCellUnsizedAxes(.horizontal)

                ForEach($model.midiMapping.bindings) { $binding in
                    BindingRow(binding: $binding,
                               value: model.midiValues[binding.control],
                               conflict: conflicts.contains(binding.control))
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

private struct BindingRow: View {
    @Binding var binding: MIDIBinding
    let value: Int?
    let conflict: Bool

    private var isAxis: Bool { if case .axis = binding.control { return true } else { return false } }

    var body: some View {
        GridRow {
            Text(binding.control.displayName)
                .frame(width: 110, alignment: .leading)

            Toggle("", isOn: $binding.enabled).labelsHidden()

            if isAxis {
                Text("CC").foregroundStyle(.secondary).frame(width: 90, alignment: .leading)
            } else {
                Picker("", selection: $binding.kind) {
                    Text("CC").tag(MIDIMessageKind.cc)
                    Text("Note").tag(MIDIMessageKind.note)
                }
                .labelsHidden().pickerStyle(.segmented).frame(width: 90)
            }

            HStack(spacing: 4) {
                TextField("", value: $binding.number, format: .number)
                    .frame(width: 48)
                    .multilineTextAlignment(.trailing)
                    .onChange(of: binding.number) { n in
                        if n < 0 || n > 127 { binding.number = min(max(n, 0), 127) }
                    }
                if conflict {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.orange)
                        .help("Gleiche Nummer wie ein anderes Control")
                }
            }
            .frame(width: 80, alignment: .leading)

            if isAxis {
                Toggle("Invertieren", isOn: $binding.invert).frame(width: 170, alignment: .leading)
            } else {
                Picker("", selection: $binding.buttonMode) {
                    Text("Momentary").tag(ButtonMode.momentary)
                    Text("Toggle").tag(ButtonMode.toggle)
                }
                .labelsHidden().pickerStyle(.segmented).frame(width: 170)
            }

            ValueBar(value: binding.enabled ? value : nil)
        }
        .opacity(binding.enabled ? 1 : 0.45)
    }
}

private struct ValueBar: View {
    let value: Int?

    var body: some View {
        HStack(spacing: 6) {
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    RoundedRectangle(cornerRadius: 2).fill(.secondary.opacity(0.2))
                    RoundedRectangle(cornerRadius: 2).fill(Color.accentColor)
                        .frame(width: geo.size.width * CGFloat(value ?? 0) / 127)
                }
            }
            .frame(width: 110, height: 8)
            Text(value.map(String.init) ?? "–")
                .font(.caption.monospacedDigit()).frame(width: 28, alignment: .trailing)
        }
    }
}

/// "▶ 124.0 BPM · 3.2" style clock readout.
struct ClockBadge: View {
    let clock: MIDIBridge.ClockStatus

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: clock.running ? "play.fill" : "stop.fill")
                .foregroundStyle(clock.running ? Color.green : Color.secondary)
            Text(clock.bpm.map { String(format: "%.1f BPM", $0) } ?? "keine Clock")
            if clock.running { Text("· \(clock.bar).\(clock.beat)").foregroundStyle(.secondary) }
        }
        .font(.callout.monospacedDigit())
    }
}
