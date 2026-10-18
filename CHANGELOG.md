# Changelog

Versions follow [Semantic Versioning](https://semver.org). The Mac app and
`linux/padbridge-netpad` share one version number; the network protocol is
versioned separately by its magic (`PBG1` = protocol v1).

## [Unreleased]

### Added
- "MIDI-Geräte" tab: listens to external MIDI devices (CoreMIDI on macOS,
  ALSA hardware ports on Linux) alongside other apps, with all values seen
  and a message log, SysEx included.
- Drawings with live values for the Novation Launch Control XL (templates
  followed) and the Akai MIDImix.
- Remapping per device and template, sent through the "PadBridge" port:
  CC/note, number, channel, momentary/toggle for buttons.
- Device LEDs from the mapping: colour per control on the Launch Control XL
  (fixed, brightness, centre-bright for bipolar knobs, or a green-yellow-red
  meter), on/off on the MIDImix; toggles light up. Saved in `surfaces.json`.
- Elektron Digitakt II view: the screen follows the page and track of the
  last parameter turned (SRC, FLTR, AMP, FX), encoders A-H and the track
  level move along, trig keys light per track (and per key pressed, via the
  auto channel), muted tracks are marked.
- MIDI devices over the network: per device, the remapped MIDI can go to
  the peer instead, which plays it to a port of its own named
  "PadBridge <device>" (macOS: virtual source; Linux: sequencer port, linked
  to the next VirMIDI device, so load snd-virmidi with `midi_devs=4`).
  New packet types 6 (device MIDI) and 7 (device name); older peers ignore them.
- The peer also gets the raw values (packet type 8) and lists the device in
  its "MIDI-Geräte" tab as "<device> · Netzwerk", drawn live and read-only.

### Fixed
- macOS: MIDI devices plugged in or out while the app runs now appear and
  disappear (the device list follows CoreMIDI notifications); the tab
  switches to a device that was just plugged in.

## [0.4.2] - 2026-10-16

### Added
- MIDI tab, picture view: outline of the pad in hand (Xbox or DualShock 4,
  automatic or chosen) with a CC/note field next to every control, live
  highlighting, and a popup with all settings per control. The table
  remains as the second view.

### Changed
- README and DOC.md rewritten for the Rust app (setup on Mac and Linux,
  DualShock 4, service mode, troubleshooting).

## [0.4.1] - 2026-10-16

### Added
- DualShock 4 (054c:05c4, 09cc, 0ba0) on macOS via HID, USB and Bluetooth:
  buttons, sticks, triggers, rumble, light bar. The light bar shows the
  player indicator (P1 blue, P2 red, P3 green, P4 pink; blinking and
  rotating modes for the clock).
- Linux: DualShock 4 light bar via sysfs (hid-playstation and hid-sony),
  udev rule in DOC.md.

### Fixed
- Receiver logs an error only when it changes (no log flood on macOS, which
  cannot create a virtual pad).

## [0.4.0] - 2026-10-11

The Rust rewrite (`rust/`): one code base for Linux and macOS 10.13+.
The Swift app and `padbridge-netpad` keep working and interoperate (same
protocol v1, same settings files), but are now superseded.

### Added
- `padbridge` app (egui, OpenGL) for Linux and macOS 10.13+ (also Macs
  without Metal, e.g. a 2010 iMac). Same tabs as the Swift app.
- Every installation can **send and receive** at the same time: pads, MIDI
  and keyboard/mouse in both directions. A Mac can now receive MIDI and
  keyboard/mouse; a Linux host additionally creates virtual 360 pads.
- Linux: pads at the Linux machine itself (evdev, any pad the kernel knows,
  hotplug), rumble, xpad LED ring (udev rule in DOC.md), ALSA port with
  automatic VirMIDI link, local keyboard/mouse via uinput.
- macOS: pads via libusb (no kext), CoreMIDI ports, CGEvent keyboard/mouse;
  `rust/scripts/bundle-macos.sh` builds PadBridge.app.
- Service mode `padbridge --headless --listen --receive-only` with systemd
  unit; replaces `padbridge-netpad` (see DOC.md section 6).
- `padbridge-core` crate: all Swift tests ported, Swift settings files as
  fixtures, shared protocol test vector.

### Changed
- LED ring "Clock: Takt" goes round clockwise (1, 2, 4, 3), one LED per bar.
- Receiver replay protection is tracked per sender, so several machines can
  send to one receiver.

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
