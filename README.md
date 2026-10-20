# PadBridge

Turn a game pad into a **MIDI controller**, a **keyboard/mouse**, or a
**network game pad** for another computer. Runs on **Linux**, on
**macOS 10.13 High Sierra and newer** (also on old Macs without Metal, it
runs on a 2010 iMac) and, new, on **Windows 10/11**. No kernel extensions and
no drivers on the Mac.

![Monitor tab: an Xbox 360 pad live, with rumble and LED ring controls](docs/images/monitor-mac.png)

Supported pads:

| Pad | macOS | Linux | Windows |
|---|---|---|---|
| Xbox 360 wired (and licensed clones) | USB, rumble, LED ring | USB, rumble, LED ring | XInput, rumble (Windows drives the LED ring) |
| Xbox One / Series and other XInput pads | – | see last row | XInput, rumble |
| DualShock 4 (PS4) | USB and Bluetooth, rumble, light bar | USB and Bluetooth, rumble, light bar | USB, rumble, light bar |
| Any other pad the Linux kernel knows | – | buttons, sticks, rumble | – |

## What it does

- **MIDI**: a virtual MIDI port "PadBridge". Every button, stick axis and
  trigger sends a CC or note of your choice: momentary or toggle, invert,
  deadzones, velocity fixed or from a trigger. Map it by clicking the
  controller picture: the outline of the pad in your hand, with a number
  field on every control that lights up when you press it.

  ![MIDI tab: the pad outline with a CC or note field on every control](docs/images/midi-picture.png)

- **MIDI devices**: PadBridge listens to MIDI controllers such as the
  Novation Launch Control XL, Akai MIDImix or APC mini mk2: a live drawing of
  the device, remapping, LED colours, and forwarding to the other computer.
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

On macOS and Windows the receiver accepts MIDI and keyboard/mouse; a virtual
game pad needs a driver there (for Windows, ViGEmBus support is planned).

## In action

A DualShock 4 on Windows: live in the monitor, then as a MIDI controller in
the picture view, its MIDI going to the Mac over the network:

![DualShock 4 on Windows: monitor and MIDI picture view, MIDI sent to the Mac](docs/images/ds4-network-windows.gif)

The pad as mouse and keyboard on Windows:

![Keyboard/mouse tab on Windows, the pad moving the mouse pointer](docs/images/keymouse-windows.gif)

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

**Windows**: build on a Mac or Linux box (needs
`rustup target add x86_64-pc-windows-gnu` and mingw-w64) and copy the `.exe`
over:

```sh
scripts/build-windows.sh         # builds target/x86_64-pc-windows-gnu/release/padbridge.exe
```

Windows has no virtual MIDI ports without a driver, so create the ports in
[loopMIDI](https://www.tobias-erichsen.de/software/loopmidi.html): one named
exactly `PadBridge` (pads → DAW), optionally `PadBridge In` (DAW → PadBridge,
e.g. MIDI clock). Start PadBridge after loopMIDI. Allow it in the firewall
prompt for private networks.

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
rust/      the app (Linux, macOS 10.13+, Windows), see rust/README.md
  core/    pads, mapping, MIDI clock, LED logic, network protocol (no OS access, unit tested)
  app/     engine + egui window; platform/linux, platform/macos and platform/windows backends
docs/      images for this page
Sources/   the original Swift app (macOS 13+, Xbox 360 only), superseded by rust/
linux/     padbridge-netpad, the original Linux receiver, superseded by rust/
```

The Swift app and `padbridge-netpad` still work and speak the same protocol,
so old and new parts understand each other.

## Roadmap

- Virtual game pad on Windows (ViGEmBus)
- A status page in the browser: pads, battery, link and MIDI devices at a
  glance, from a phone
- The receiving side shows the pad you actually hold (e.g. a PS4 pad instead
  of the Xbox 360 stand-in)
- Profiles, switched from the pad

Changes per version: [CHANGELOG.md](CHANGELOG.md).

## License

MIT, see [LICENSE](LICENSE).
