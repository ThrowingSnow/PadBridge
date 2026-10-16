# PadBridge (Rust)

One code base for Linux and macOS (10.13+), replacing the Swift app and the
`padbridge-netpad` daemon. Setup for users: [../DOC.md](../DOC.md) (German).

```
core/    padbridge-core: controls, state, MIDI and keyboard/mouse mapping, MIDI
         clock, LED ring logic, network protocol v1, settings and key files.
         No OS access; fully unit tested (incl. the Swift app's JSON files and
         the shared protocol test vector).
app/     padbridge: engine thread + egui UI (glow/OpenGL, so it runs on Macs
         without Metal). `--headless` runs the engine without a window.
  engine.rs       owns devices, mappers, clock/ring, links; UI reads snapshots
  net/sender.rs   sends this machine's pads/MIDI/keyboard to a peer
  net/receiver.rs accepts a peer's packets, drives virtual devices (poll loop)
  platform/linux  evdev pads (hotplug, FF rumble, xpad LED, DS4 light bar), uinput, ALSA
  platform/macos  libusb Xbox 360 pads, hidapi DualShock 4, CoreMIDI ports,
                  CGEvent keyboard/mouse
  ui/pad_map.rs   MIDI tab picture view (pad outline with a field per control)
  platform/stub   placeholder for other systems
spike/   stage-0 feasibility test for macOS 10.13 (egui/glow + libusb + CoreMIDI)
```

Every installation can send and receive at the same time; both directions use
the same protocol, so the Swift app and the old daemon interoperate.

## Build

```sh
cargo test -p padbridge-core
cargo build --release -p padbridge          # Linux: needs alsa-lib headers
MACOSX_DEPLOYMENT_TARGET=10.13 cargo build --release -p padbridge   # macOS
```

## Status

- [x] Stage 0: macOS 10.13 feasibility (iMac 2010, OpenGL only)
- [x] Stage 1: core crate, all Swift tests ported
- [x] Stage 2: Linux app (pads, MIDI, LED, rumble, keyboard/mouse, send + receive)
- [x] Stage 3: macOS backend (libusb pad, CoreMIDI, CGEvent), app bundle
      (`scripts/bundle-macos.sh`); receiver on macOS: MIDI + keyboard/mouse
      (a virtual game pad needs a driver there)
- [x] DualShock 4 (macOS via HID, USB and Bluetooth; Linux via evdev + sysfs LEDs)
- [x] MIDI tab picture view
- [ ] Stage 4: forward MIDI controllers (per device, both directions), SysEx
- [ ] Retire the Swift app and padbridge-netpad
