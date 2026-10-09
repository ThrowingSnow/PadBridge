// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "PadBridge",
    platforms: [.macOS(.v13)],
    products: [
        .library(name: "PadCore", targets: ["PadCore"]),
        .executable(name: "padbridge-cli", targets: ["padbridge-cli"]),
        .executable(name: "PadBridgeApp", targets: ["PadBridgeApp"]),
    ],
    targets: [
        // Device-agnostic model: controls, state, device/backend protocols.
        .target(name: "PadCore"),

        // Input backend: raw USB via IOUSBHost (Xbox 360 wired and clones).
        .target(
            name: "PadUSB",
            dependencies: ["PadCore"],
            linkerSettings: [
                .linkedFramework("IOKit"),
                .linkedFramework("IOUSBHost"),
            ]
        ),

        // Output: virtual CoreMIDI port, control mapping, clock following.
        .target(
            name: "PadMIDI",
            dependencies: ["PadCore"],
            linkerSettings: [.linkedFramework("CoreMIDI")]
        ),

        .executableTarget(name: "padbridge-cli", dependencies: ["PadCore", "PadUSB"]),
        .executableTarget(name: "PadBridgeApp", dependencies: ["PadCore", "PadUSB", "PadMIDI"]),

        .testTarget(name: "PadCoreTests", dependencies: ["PadCore", "PadUSB", "PadMIDI"]),
    ]
)
