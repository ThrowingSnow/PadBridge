import Foundation
import PadCore
import PadUSB

// Terminal live monitor: prints connect/disconnect and one updating status line.
// Usage: padbridge-cli [--rumble]   (--rumble buzzes once on connect as an output test)

let rumbleTest = CommandLine.arguments.contains("--rumble")
let manager = ControllerManager(backends: [USBBackend()])
let out = FileHandle.standardOutput

func line(_ s: String) { out.write(("\r\u{1B}[2K" + s + "\n").data(using: .utf8)!) }

func status(_ info: ControllerInfo, _ s: ControllerState) -> String {
    func f(_ v: Float) -> String { String(format: "%+.2f", v) }
    let buttons = PadButton.allCases.filter(s.isPressed).map(\.displayName).joined(separator: " ")
    return "L(\(f(s[.leftX])),\(f(s[.leftY]))) R(\(f(s[.rightX])),\(f(s[.rightY]))) "
        + "LT \(String(format: "%.2f", s[.leftTrigger])) RT \(String(format: "%.2f", s[.rightTrigger]))  [\(buttons)]"
}

manager.addListener { event in
    switch event {
    case .connected(let info):
        line("+ connected: \(info.name) [\(String(format: "%04x:%04x", info.vendorID, info.productID))] id=\(info.id)")
        if rumbleTest, let device = manager.device(id: info.id) {
            device.setRumble(low: 0.6, high: 0.6)
            DispatchQueue.global().asyncAfter(deadline: .now() + 0.4) { device.setRumble(low: 0, high: 0) }
        }
    case .disconnected(let info):
        line("- disconnected: \(info.name)")
    case .backendError(let message):
        line("! \(message)")
    case .state(let info, let state):
        out.write(("\r\u{1B}[2K" + status(info, state)).data(using: .utf8)!)
    }
}

signal(SIGINT, SIG_IGN)
let sigint = DispatchSource.makeSignalSource(signal: SIGINT, queue: .main)
sigint.setEventHandler {
    line("")
    manager.stop()
    exit(0)
}
sigint.resume()

line("padbridge-cli: waiting for controllers (Ctrl+C to quit)")
manager.start()
dispatchMain()
