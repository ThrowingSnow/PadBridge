# Changelog

Versions follow [Semantic Versioning](https://semver.org). The Mac app and
`linux/padbridge-netpad` share one version number; the network protocol is
versioned separately by its magic (`PBG1` = protocol v1).

## [Unreleased]

### Added
- Rumble from games on the Linux host: the virtual pad accepts FF_RUMBLE effects
  (upload/erase/play/stop/gain), the daemon forwards motor levels (packet 0x82),
  the Mac drives the pad's motors; 500 ms safety stop if updates stop.
  Toggle in the Network tab.

### Changed
- padbridge-netpad waits on the socket and all virtual pads with `poll()`
  instead of a 100 ms receive timeout (faster FF answers and rumble).

## [0.1.0] - 2026-10-09

First usable version.

### Input
- Wired Xbox 360 controller (and clones) read directly over USB via IOUSBHost,
  no kernel extension; hotplug; rumble and LED ring output.
- Device-agnostic core (`PadButton`, `PadAxis`, backends) prepared for more controllers.

### App
- SwiftUI app with live monitor (sticks, triggers, buttons), rumble test,
  LED ring control, menu bar icon. Settings in `~/Library/Application Support/PadBridge/`.
- LED ring modes: manual (P1–P4, rotate, blink, off), random, MIDI clock synced
  by bar (bar 1–4 → LED 1–4) or by beat (clockwise chase).

### MIDI
- Virtual CoreMIDI port "PadBridge"; editable table per control: on/off, CC/note,
  number, momentary/toggle, invert, velocity (fixed or from LT/RT), conflict warning.
- Channel, stick and trigger deadzones; MIDI clock input with BPM and position.
- Target switch: this Mac or the Linux host (ALSA port "PadBridge" via the network link).

### Network gamepad
- HMAC-SHA256 signed UDP protocol v1 (port 48100), keepalive, ping/latency,
  replay protection, 500 ms silence → all inputs released.
- `padbridge-netpad` (Rust): uinput device identical to an xpad Xbox 360 pad,
  ALSA sequencer port for MIDI both ways; systemd user service.
