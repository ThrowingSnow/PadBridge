import AppKit
import SwiftUI

@main
struct PadBridgeApp: App {
    @StateObject private var model = AppModel()

    init() {
        // Also behave like a normal app when launched as a bare binary (swift run).
        NSApplication.shared.setActivationPolicy(.regular)
        DispatchQueue.main.async { NSApplication.shared.activate(ignoringOtherApps: true) }
    }

    var body: some Scene {
        WindowGroup("PadBridge") {
            TabView {
                LiveMonitorView().tabItem { Text("Monitor") }
                MIDIView().tabItem { Text("MIDI") }
                KeyMouseView().tabItem { Text("Tastatur/Maus") }
                NetworkView().tabItem { Text("Netzwerk") }
            }
            .environmentObject(model)
            .frame(minWidth: 980, minHeight: 620)
        }
        MenuBarExtra {
            if model.devices.isEmpty {
                Text("Kein Controller")
            } else {
                ForEach(model.devices) { Text($0.name) }
            }
            Divider()
            Button("Beenden") { NSApplication.shared.terminate(nil) }
        } label: {
            Image(systemName: model.devices.isEmpty ? "gamecontroller" : "gamecontroller.fill")
        }
    }
}
