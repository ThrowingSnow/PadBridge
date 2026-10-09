import SwiftUI
import PadCore

/// Live view of one controller: sticks, triggers, every button, plus output tests.
struct LiveMonitorView: View {
    @EnvironmentObject var model: AppModel
    @State private var rumbleLow: Float = 0
    @State private var rumbleHigh: Float = 0

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            header
            if let device = model.selectedDevice {
                let state = model.state(for: device.id)
                HStack(alignment: .top, spacing: 24) {
                    VStack(spacing: 12) {
                        TriggerBar(label: "LT", value: state[.leftTrigger])
                        StickView(label: "Left", x: state[.leftX], y: state[.leftY], clicked: state.isPressed(.leftStick))
                    }
                    VStack(spacing: 12) {
                        DPadView(state: state)
                        ButtonRow(buttons: [.back, .guide, .start], state: state)
                        ButtonRow(buttons: [.leftShoulder, .rightShoulder], state: state)
                    }
                    VStack(spacing: 12) {
                        TriggerBar(label: "RT", value: state[.rightTrigger])
                        StickView(label: "Right", x: state[.rightX], y: state[.rightY], clicked: state.isPressed(.rightStick))
                    }
                    FaceButtons(state: state)
                }
                Divider()
                outputTests(device)
            } else {
                ContentUnavailable()
            }
            Divider()
            LogView(lines: model.log)
        }
        .padding(20)
        .frame(minWidth: 760, minHeight: 560)
    }

    private var header: some View {
        HStack {
            Text("PadBridge").font(.title2.bold())
            Spacer()
            if model.devices.count > 1 {
                Picker("Controller", selection: $model.selectedID) {
                    ForEach(model.devices) { Text($0.name).tag(Optional($0.id)) }
                }
                .frame(maxWidth: 300)
            } else if let device = model.selectedDevice {
                Text("\(device.name)  ·  \(String(format: "%04x:%04x", device.vendorID, device.productID))")
                    .foregroundStyle(.secondary)
            }
        }
    }

    @ViewBuilder
    private func outputTests(_ device: ControllerInfo) -> some View {
        HStack(alignment: .top, spacing: 32) {
            if device.capabilities.contains(.rumble) {
                VStack(alignment: .leading) {
                    Text("Rumble").font(.headline)
                    slider("Links (schwer)", $rumbleLow)
                    slider("Rechts (leicht)", $rumbleHigh)
                    Button("Stopp") { rumbleLow = 0; rumbleHigh = 0 }
                }
                .frame(width: 280)
                .onChange(of: rumbleLow) { _ in model.setRumble(low: rumbleLow, high: rumbleHigh) }
                .onChange(of: rumbleHigh) { _ in model.setRumble(low: rumbleLow, high: rumbleHigh) }
            }
            if device.capabilities.contains(.playerIndicator) {
                VStack(alignment: .leading) {
                    Text("LED-Ring").font(.headline)
                    HStack {
                        ForEach(1...4, id: \.self) { n in
                            Button("P\(n)") { model.setPlayerIndicator(.player(n)) }
                        }
                    }
                    HStack {
                        Button("Rotieren") { model.setPlayerIndicator(.rotate) }
                        Button("Blinken") { model.setPlayerIndicator(.blinkAll) }
                        Button("Aus") { model.setPlayerIndicator(.off) }
                    }
                }
            }
        }
    }

    private func slider(_ title: String, _ value: Binding<Float>) -> some View {
        HStack {
            Text(title).frame(width: 110, alignment: .leading)
            Slider(value: value, in: 0...1)
        }
    }
}

private struct ContentUnavailable: View {
    var body: some View {
        VStack(spacing: 8) {
            Image(systemName: "gamecontroller").font(.system(size: 48)).foregroundStyle(.secondary)
            Text("Kein Controller gefunden").font(.headline)
            Text("Xbox-360-Controller per USB einstecken.").foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, minHeight: 280)
    }
}

// MARK: - Controls

private struct StickView: View {
    let label: String
    let x: Float
    let y: Float
    let clicked: Bool
    private let size: CGFloat = 130

    var body: some View {
        VStack(spacing: 4) {
            ZStack {
                Circle().stroke(.secondary.opacity(0.5), lineWidth: 1)
                Path { p in
                    p.move(to: CGPoint(x: size / 2, y: 0)); p.addLine(to: CGPoint(x: size / 2, y: size))
                    p.move(to: CGPoint(x: 0, y: size / 2)); p.addLine(to: CGPoint(x: size, y: size / 2))
                }
                .stroke(.secondary.opacity(0.2))
                Circle()
                    .fill(clicked ? Color.accentColor : Color.primary)
                    .frame(width: 14, height: 14)
                    // Up is positive in the model, but screen Y grows downward.
                    .offset(x: CGFloat(x) * (size / 2 - 7), y: CGFloat(-y) * (size / 2 - 7))
            }
            .frame(width: size, height: size)
            Text(label).font(.caption)
            Text(String(format: "%+.3f  %+.3f", x, y)).font(.caption.monospacedDigit()).foregroundStyle(.secondary)
        }
    }
}

private struct TriggerBar: View {
    let label: String
    let value: Float

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack {
                Text(label).font(.caption.bold())
                Spacer()
                Text(String(format: "%.2f", value)).font(.caption.monospacedDigit()).foregroundStyle(.secondary)
            }
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    RoundedRectangle(cornerRadius: 3).fill(.secondary.opacity(0.2))
                    RoundedRectangle(cornerRadius: 3).fill(Color.accentColor)
                        .frame(width: geo.size.width * CGFloat(value))
                }
            }
            .frame(height: 10)
        }
        .frame(width: 130)
    }
}

private struct ButtonChip: View {
    let button: PadButton
    let pressed: Bool

    var body: some View {
        Text(button.displayName)
            .font(.caption.bold())
            .padding(.horizontal, 8).padding(.vertical, 4)
            .frame(minWidth: 34)
            .background(RoundedRectangle(cornerRadius: 6).fill(pressed ? Color.accentColor : Color.secondary.opacity(0.15)))
            .foregroundStyle(pressed ? Color.white : Color.primary)
    }
}

private struct ButtonRow: View {
    let buttons: [PadButton]
    let state: ControllerState

    var body: some View {
        HStack(spacing: 6) {
            ForEach(buttons, id: \.self) { ButtonChip(button: $0, pressed: state.isPressed($0)) }
        }
    }
}

private struct DPadView: View {
    let state: ControllerState

    var body: some View {
        VStack(spacing: 2) {
            arrow(.dpadUp, "arrowtriangle.up.fill")
            HStack(spacing: 22) {
                arrow(.dpadLeft, "arrowtriangle.left.fill")
                arrow(.dpadRight, "arrowtriangle.right.fill")
            }
            arrow(.dpadDown, "arrowtriangle.down.fill")
        }
    }

    private func arrow(_ button: PadButton, _ symbol: String) -> some View {
        Image(systemName: symbol)
            .foregroundStyle(state.isPressed(button) ? Color.accentColor : Color.secondary.opacity(0.4))
            .font(.title3)
    }
}

private struct FaceButtons: View {
    let state: ControllerState

    var body: some View {
        VStack(spacing: 4) {
            chip(.y)
            HStack(spacing: 28) { chip(.x); chip(.b) }
            chip(.a)
        }
    }

    private func chip(_ button: PadButton) -> some View {
        ButtonChip(button: button, pressed: state.isPressed(button))
    }
}

private struct LogView: View {
    let lines: [String]

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 2) {
                ForEach(Array(lines.enumerated()), id: \.offset) { Text($0.element) }
            }
            .font(.caption.monospaced())
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .frame(height: 90)
        .foregroundStyle(.secondary)
    }
}
