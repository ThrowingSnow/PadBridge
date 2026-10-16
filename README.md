# PadBridge

Turn a game pad into a **MIDI controller**, a **keyboard/mouse**, or a
**network game pad** for another computer. Runs on **Linux** and on
**macOS 10.13 High Sierra and newer**, also on old Macs without Metal (it
runs on a 2010 iMac). No kernel extensions and no drivers on the Mac.

Supported pads:

| Pad | macOS | Linux |
|---|---|---|
| Xbox 360 wired (and licensed clones) | USB, rumble, LED ring | USB, rumble, LED ring |
| DualShock 4 (PS4) | USB and Bluetooth, rumble, light bar | USB and Bluetooth, rumble, light bar |
| Any other pad the Linux kernel knows | – | buttons, sticks, rumble |

## What it does

- **MIDI**: a virtual MIDI port "PadBridge". Every button, stick axis and
  trigger sends a CC or note of your choice: momentary or toggle, invert,
  deadzones, velocity fixed or from a trigger. Map it by clicking the
  controller picture: the outline of the pad in your hand, with a number
  field on every control that lights up when you press it.
- **MIDI clock in**: send clock to "PadBridge" and the LED ring (or the PS4
  light bar) blinks on the beat or steps through the bars.
- **Keyboard/mouse**: buttons become keys, a stick becomes the mouse.
- **Network**: send the pad, MIDI and keyboard/mouse to another machine over
  UDP (signed with HMAC-SHA256). On Linux the receiver creates a virtual Xbox
  360 pad that Steam, SDL, Wine/Proton and Sunshine/Moonlight recognise
  without any setup. Rumble from the game goes back to the pad in your hand.
- **Both directions at once**: every installation can send and receive. Play
  on the Linux box with a pad on the Mac, or play Bitwig on Linux with
  MIDI-learn from a pad on the laptop.

```
 Laptop / Mac                                  Linux desktop
 ┌──────────────────────┐   UDP 48100,         ┌─────────────────────────────┐
 │ pad (USB/Bluetooth)  │   HMAC-signed        │ virtual Xbox 360 pad        │
 │ MIDI port "PadBridge"│ ───────────────────► │ ALSA MIDI port "PadBridge"  │
 │ keyboard/mouse       │ ◄─────────────────── │ virtual keyboard/mouse      │
 │ LED ring, rumble     │  rumble, MIDI clock  │ (games, Bitwig, Moonlight)  │
 └──────────────────────┘                      └─────────────────────────────┘
```

On macOS the receiver accepts MIDI and keyboard/mouse; a virtual game pad
would need a driver there.

## Getting started

You need [Rust](https://rustup.rs). Then:

```sh
git clone https://github.com/ThrowingSnow/PadBridge.git
cd PadBridge/rust
```

**Linux** (needs the ALSA headers: `alsa-lib` on Arch, `libasound2-dev` on
Debian/Ubuntu):

```sh
cargo build --release -p padbridge
install -Dm755 target/release/padbridge ~/.local/bin/padbridge
padbridge
```

Add yourself to the `input` group and allow `/dev/uinput`; the udev rules for
the LED ring and the light bar are in [DOC.md](DOC.md).

**macOS** (Xcode Command Line Tools are enough):

```sh
scripts/bundle-macos.sh          # builds target/PadBridge.app
open target/PadBridge.app
```

The app is ad-hoc signed: on the first start, right-click it and choose
"Open". For keyboard/mouse output, allow PadBridge under System Settings →
Privacy & Security → Accessibility.

**Network**: on both machines open the "Netzwerk" tab. Both sides need the
same key: it is created on the first start; copy it with "Schlüssel kopieren"
and put it into the key file on the other machine (the tab shows the path).
Enter the other machine's address on the sending side and switch on
receiving on the other. Only use it in your LAN or over a VPN such as Tailscale: packets are
signed, not encrypted.

**Headless service on Linux** (receives only, no window):

```sh
cp app/padbridge.service ~/.config/systemd/user/
systemctl --user enable --now padbridge
```

Step-by-step setup, troubleshooting and the udev rules are in
**[DOC.md](DOC.md)** (German).

The user interface is in German for now.

## Project layout

```
rust/      the app (Linux and macOS 10.13+), see rust/README.md
  core/    pads, mapping, MIDI clock, LED logic, network protocol (no OS access, unit tested)
  app/     engine + egui window; platform/linux and platform/macos backends
Sources/   the original Swift app (macOS 13+, Xbox 360 only), superseded by rust/
linux/     padbridge-netpad, the original Linux receiver, superseded by rust/
```

The Swift app and `padbridge-netpad` still work and speak the same protocol,
so old and new parts understand each other.

## Roadmap

- Forward real MIDI controllers over the network (per device, both
  directions, SysEx)
- The receiving side shows the pad you actually hold (e.g. a PS4 pad instead
  of the Xbox 360 stand-in)
- Profiles, switched from the pad

Changes per version: [CHANGELOG.md](CHANGELOG.md).

## License

MIT, see [LICENSE](LICENSE).
