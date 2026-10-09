# Changelog

Versions follow [Semantic Versioning](https://semver.org). The Mac app and
`linux/padbridge-netpad` share one version number; the network protocol is
versioned separately by its magic (`PBG1` = protocol v1).

## [0.3.0] - 2026-10-10

### Added
- Host presets in the Network tab: save the current host under a name ("+"),
  click to switch, right-click to delete. Stored in `netpad.json`.
- `DOC.md`: step-by-step setup for a new Mac and Linux host.

### Changed
- No built-in hosts any more; the host field starts empty.
- Network target is called "Linux-Host" in the UI.

## [0.2.0] - 2026-10-10

### Added
- Rumble from games on the Linux host: the virtual pad accepts FF_RUMBLE effects
  (upload/erase/play/stop/gain), the daemon forwards motor levels (packet 0x82),
  the Mac drives the pad's motors; 500 ms safety stop if updates stop.
  Toggle in the Network tab.
- Keyboard/mouse emulation (new tab), running alongside the gamepad:
  per stick mouse / scroll / WASD / arrows, per button or trigger a mouse click
  or a key with modifiers, speed/curve/deadzone, "desktop" preset.
  Target switch: this Mac (CGEvent, needs Accessibility permission; pointer
  deltas set for captured-mouse apps like Moonlight) or the Linux host
  (virtual "PadBridge Keyboard/Mouse" via uinput, hi-res scrolling,
  keys released after 500 ms of silence).
- padbridge-netpad links its ALSA port both ways to a `snd-virmidi` device
  when present, so DAWs that only list rawmidi devices (Bitwig on Linux) see it.

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
