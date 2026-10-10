# PadBridge

Gamepads on macOS without kernel extensions. Reads a wired Xbox 360 controller
directly over USB (IOUSBHost, user space) and routes it to:

- **MIDI**: virtual CoreMIDI port "PadBridge", every control mapped to a CC/note;
  MIDI clock in drives the LED ring (beat blink, bar 1-4 -> LED 1-4)
- **Network gamepad**: signed UDP to `linux/padbridge-netpad`, which creates a real
  uinput Xbox 360 pad on Linux, e.g. for Wine/Proton games streamed with Sunshine/Moonlight
- **Keyboard/mouse** emulation on the Mac (CGEvent) or on the Linux host (uinput)
- **Rumble** from games on the Linux host back to the pad

**Setup on a new machine: see [DOC.md](DOC.md)** (step by step, German).

Why: the old 360Controller kext is blocked on macOS 14 (IOHIDFamily kexts are
classified "Unsupported"), and Moonlight cannot see the pad because it is not a
HID device.

## Architecture

```
Backends (input)            Canonical state          Mapping              Outputs
USBBackend  ─┐                                                            ┌─► MIDI
  Xbox360Device              PadButton + PadAxis      profiles, layers,    ├─► Keyboard/Mouse
HID (later) ─┼──► ControllerManager ──► events ──►     curves, deadzones ─┼─► Net gamepad (UDP)
GCController (later) ─┘      (+ rumble/LED back)                          └─► OSC
```

- `PadCore`: device-agnostic model (`PadButton`, `PadAxis`, `ControllerState`,
  `ControllerDevice`, `ControllerBackend`, `ControllerManager`). Nothing in here
  knows about USB or a specific pad.
- `PadMIDI`: mapping (`MIDIMapper`, pure), clock following (`MIDIClockTracker`, pure),
  CoreMIDI port (`MIDIEndpoint`), glue (`MIDIBridge`). Settings: `~/Library/Application Support/PadBridge/`.
- `PadNet`: net gamepad protocol (`NetPadProtocol`, shares a test vector with the
  Rust side), key handling, UDP sender with keepalive and ping.
- `PadUSB`: raw USB backend. `Xbox360Protocol` is pure byte parsing (unit tested);
  `Xbox360Device` does the IOUSBHost I/O; `USBBackend` handles hotplug.
- **Adding a controller family** means adding a backend (or a protocol + device
  class inside `PadUSB`) that emits `ControllerState`. Mapping and outputs do not change.

Units: sticks -1…+1 with **up/right positive**, triggers 0…1.

## Build & run

Needs only the Xcode Command Line Tools (Swift 6).

```sh
swift build
swift test
swift run padbridge-cli            # terminal live monitor (--rumble: buzz on connect)
swift run PadBridgeApp             # SwiftUI live monitor + menu bar icon
scripts/bundle.sh                  # build/PadBridge.app
```

## Roadmap

1. ✅ Core model, USB backend (Xbox 360 wired + clones), live monitor
2. ✅ MIDI output: per-control CC/note table, channel, deadzones, momentary/toggle, invert;
   MIDI clock in -> LED ring sync; random LED mode
   - later: 14-bit, relative mode, shift layer, rumble from MIDI
3. ✅ Network gamepad: HMAC-signed UDP protocol + Rust uinput daemon, latency display
   - MIDI over the same link: ALSA port "PadBridge" on the Linux host, clock back to the ring;
     MIDI tab switches the target between Mac and Linux host
   - ✅ rumble from the game back to the pad, Bitwig on Linux via snd-virmidi
   - ✅ host presets (name + address) editable in the app
4. ✅ Keyboard/mouse output (Mac or Linux host, alongside the gamepad)
5. Profiles (Guide + D-Pad to switch, shown on the LED ring)
6. More backends: IOHID, GameController.framework

## License

MIT, see [LICENSE](LICENSE).
