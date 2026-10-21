# Setting up PadBridge

*Deutsch: [DOC.de.md](DOC.de.md)*

This guide walks through setting up PadBridge on a Mac, a Linux machine and
Windows. Every installation can run on its own (pad → MIDI and keyboard/mouse
on the same computer) or work together with a second installation over the
network, in both directions.

```
 Computer A (pad in hand)                        Computer B (peer)
 ┌──────────────────────────┐   UDP 48100,      ┌──────────────────────────────┐
 │ Xbox 360 / PS4 pad       │   HMAC-signed     │ virtual Xbox 360 pad         │
 │ MIDI port "PadBridge"    │ ────────────────► │ MIDI port "PadBridge"        │
 │ keyboard/mouse           │ ◄──────────────── │ virtual keyboard/mouse       │
 │ LED ring / light bar     │  rumble, MIDI     │ (games, Sunshine/Moonlight,  │
 └──────────────────────────┘  clock back       │  Wine, Bitwig, …)            │
                                                └──────────────────────────────┘
```

The virtual game pad on the receiving side exists on Linux (uinput) and on
Windows (with the free ViGEmBus driver). A Mac as receiver accepts MIDI and
keyboard/mouse; a virtual game pad would need a driver on macOS.

Ready-made builds for all three systems are on the
[Releases](https://github.com/ThrowingSnow/PadBridge/releases) page. Building
from source is only needed if you want to work on the code.

> **The user interface is in German for now.** This guide names every button
> and tab as it appears in the app, with a translation in brackets, e.g.
> **Netzwerk** (Network).

Contents:
1. [Mac](#1-mac)
2. [Linux](#2-linux)
3. [Windows](#3-windows)
4. [Using PadBridge](#4-using-padbridge)
5. [Connecting two computers](#5-connecting-two-computers)
6. [Linux service and MIDI for Bitwig](#6-linux-service-and-midi-for-bitwig)
7. [Settings](#7-settings)
8. [Troubleshooting](#8-troubleshooting)
9. [Security](#9-security)
10. [The old Swift app and padbridge-netpad](#10-the-old-swift-app-and-padbridge-netpad)

---

## 1. Mac

### Requirements

- macOS 10.13 High Sierra or newer, Intel or Apple Silicon (via Rosetta). A
  Mac without Metal is fine (tested on a 2010 iMac).
- A wired Xbox 360 controller (or a licensed clone, list in
  `rust/core/src/xbox360.rs`) or a PS4 controller (DualShock 4) over USB or
  Bluetooth.

**Remove old drivers first.** The 360Controller kext (`360Controller`,
`Xbox360ControllerManager`) holds the Xbox controller. Uninstall it with its
own uninstaller and restart the Mac. The PS4 controller needs none of this.

### Install

1. Download `PadBridge-<version>-macOS.zip` from the
   [Releases](https://github.com/ThrowingSnow/PadBridge/releases), unzip it and
   move `PadBridge.app` to Applications.
2. The app is not notarized, so macOS reports an "unidentified developer" on
   the first start: right-click the app → **Open** → **Open** again. After that
   it starts normally.
3. Plug in the controller (or pair the PS4 controller over Bluetooth): in the
   **Monitor** tab, sticks and buttons move along.

> Only one program at a time can open the Xbox controller. Quit an old version
> (including the old Swift app) before starting a new one.

### Allow keyboard/mouse

For PadBridge to send keys and move the mouse: System Settings → Privacy &
Security → **Accessibility** → allow PadBridge. The app asks the first time
and keeps retrying every few seconds, no restart needed.

> After every new build macOS asks again, because the signature changes:
> remove the old entry with "–" and add it again.

### Build from source

Needs the Xcode Command Line Tools and Rust:

```sh
xcode-select --install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

git clone https://github.com/ThrowingSnow/PadBridge.git
cd PadBridge/rust
scripts/bundle-macos.sh             # builds target/PadBridge.app
cp -R target/PadBridge.app /Applications/
```

For a stable version, check out a tag before building, e.g.
`git checkout v0.6.0`; the app's version number then comes from the tag.

---

## 2. Linux

Tested on Arch/CachyOS (KDE Plasma) and Zorin OS 18 (Ubuntu 24.04, GNOME on
Wayland). The release package runs on Ubuntu 20.04 and newer, Debian 11+,
Mint, Zorin, Pop!_OS and current Arch and Fedora.

### Release package

```sh
tar xzf padbridge-<version>-linux-x86_64.tar.gz
install -Dm755 padbridge-<version>-linux-x86_64/padbridge ~/.local/bin/padbridge
padbridge            # with window
```

Then set up the permissions below.

### Permissions for pads and virtual devices

PadBridge runs as a normal user. It needs read access to the pads
(`/dev/input/event*`) and write access to `/dev/uinput` for the virtual pad
and the virtual keyboard/mouse. Both go through the `input` group. If
Sunshine is installed, the uinput rule is usually there already. Otherwise:

```sh
echo 'KERNEL=="uinput", SUBSYSTEM=="misc", OPTIONS+="static_node=uinput", GROUP="input", MODE="0660", TAG+="uaccess"' \
  | sudo tee /etc/udev/rules.d/60-padbridge-uinput.rules
sudo usermod -aG input "$USER"     # then log out and back in
sudo udevadm control --reload && sudo udevadm trigger
```

Every game pad Linux knows shows up in the monitor, also when plugged in or
out: the Xbox 360 controller through the kernel driver xpad, the PS4
controller through hid-playstation, and other pads too.

### LED ring and light bar

By default the kernel lets only root set controller LEDs. These rules give the
`input` group write access. Replug the pad afterwards.

**Xbox 360 (LED ring):**

```sh
echo 'ACTION=="add", SUBSYSTEM=="leds", KERNEL=="xpad*", RUN+="/bin/chgrp input /sys%p/brightness", RUN+="/bin/chmod 0664 /sys%p/brightness"' \
  | sudo tee /etc/udev/rules.d/71-padbridge-xpad-led.rules
sudo udevadm control --reload
```

**PS4 controller (light bar):**

```sh
echo 'ACTION=="add", SUBSYSTEM=="leds", KERNEL=="*054C:*|*:rgb:indicator", RUN+="/bin/chgrp input /sys%p/brightness", RUN+="/bin/chmod 0664 /sys%p/brightness", RUN+="/bin/chgrp input /sys%p/multi_intensity", RUN+="/bin/chmod 0664 /sys%p/multi_intensity"' \
  | sudo tee /etc/udev/rules.d/72-padbridge-ds4-led.rules
sudo udevadm control --reload
```

Without these rules everything else still works; the monitor then shows
"Nicht verfügbar" (not available) for the LED ring.

### Build from source

PadBridge needs a current Rust. The `cargo`/`rustc` packages of Ubuntu and
Debian are too old, so install Rust with [rustup](https://rustup.rs):

```sh
# Arch / CachyOS
sudo pacman -S --needed rustup alsa-lib pkgconf && rustup default stable
# Debian / Ubuntu / Mint / Zorin
sudo apt install build-essential libasound2-dev pkg-config curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

git clone https://github.com/ThrowingSnow/PadBridge.git
cd PadBridge/rust
cargo build --release -p padbridge
install -Dm755 target/release/padbridge ~/.local/bin/padbridge
```

A binary built on a rolling-release system (Arch) needs that system's newest
glibc and does not start on Ubuntu. For a package that runs everywhere,
`scripts/build-linux.sh` builds in an Ubuntu 20.04 container (needs Docker).

---

## 3. Windows

### Requirements

- Windows 10 or 11, 64-bit.
- An Xbox controller (360, One, Series or another XInput pad) or a PS4
  controller (DualShock 4) over USB.
- For MIDI: [loopMIDI](https://www.tobias-erichsen.de/software/loopmidi.html)
  by Tobias Erichsen (free). Windows cannot create virtual MIDI ports without
  a driver; loopMIDI does that.

### Install

Put `padbridge-<version>-windows-x86_64.exe` from the Releases into a folder
of its own, e.g. `C:\PadBridge\`, and double-click it. The program is not
signed: if Windows SmartScreen warns, click **More info** → **Run anyway**.
On the first start the Windows firewall asks: allow it for **private
networks**.

### MIDI ports in loopMIDI

In loopMIDI, type the name at the bottom and click **+**:

| Port | Direction | Needed? |
|---|---|---|
| `PadBridge` | pads → DAW | yes, for MIDI |
| `PadBridge In` | DAW → PadBridge (e.g. MIDI clock for the LED ring) | only for clock |
| `PadBridge <device>` | a MIDI device of the peer, e.g. `PadBridge Launch Control XL` | only for that |

The names must match exactly. PadBridge looks for the ports when it starts, so
start loopMIDI **before** PadBridge. A loopMIDI port echoes back everything
that goes in, which is why the way back needs a port of its own.

### Virtual game pad (ViGEmBus)

For the PC to pass a peer's pads to games as a real Xbox 360 controller, it
needs the free driver [ViGEmBus](https://github.com/nefarius/ViGEmBus/releases)
(file `ViGEmBus_…_x64_x86_arm64.exe`). It is no longer developed, but the
last version works on Windows 10 and 11. Note: the "Oculus Virtual Gamepad
Emulation Bus" installed by the Meta Quest software is a different variant
and is not enough; the two coexist fine. Without the driver the log says
"ViGEmBus ist nicht installiert" (not installed); MIDI and keyboard/mouse
from the peer still work.

### What is different on Windows

- **Xbox controllers** run through XInput. Windows drives the LED ring
  itself, so the LED buttons in the monitor are greyed out. Rumble works.
- **PS4 controller:** if it is still paired over Bluetooth with another
  computer, it stays dark on the cable. Disconnect it there first.
- **MIDI devices** (Launch Control XL, MIDImix, …) can only be opened by one
  program at a time on Windows. While PadBridge reads a device, the DAW does
  not see it directly; it gets it through the `PadBridge` port. If another
  program holds the device (DAW, MadMapper), PadBridge takes it over as soon
  as it is released there.
- **Keyboard/mouse** go out as scancodes, i.e. by key position as on Mac and
  Linux. They do not reach windows running as administrator unless PadBridge
  runs as administrator too.
- **Virtual game pad** for a peer through ViGEmBus (see above). It does not
  show up in the monitor as a controller of its own; the peer's pads appear
  there as "Pad n der Gegenstelle" (pad n of the peer).

### Build from source

The `.exe` is cross-built on a Mac or Linux machine and copied to the PC:

```sh
rustup target add x86_64-pc-windows-gnu
brew install mingw-w64                 # Mac; on Linux the mingw-w64 package
cd PadBridge/rust
scripts/build-windows.sh               # prints where the .exe is
```

Without mingw-w64 installed, `build-windows.sh` builds in Docker instead.
`scripts/build-remote.sh <ssh-host>` builds the Linux and Windows release
files on another machine with Docker.

---

## 4. Using PadBridge

The window has five tabs: **Monitor**, **MIDI**, **MIDI-Geräte** (MIDI
devices), **Tastatur/Maus** (keyboard/mouse) and **Netzwerk** (network).

### Monitor

- Shows the selected pad live: sticks, triggers, all buttons. With several
  pads, pick one at the top.
- Pads a peer sends here appear in the same list as **"Pad n der
  Gegenstelle"** (pad n of the peer), for viewing only (rumble and LEDs are
  set on the computer the pad is plugged into).
- **Rumble:** two sliders for the heavy and the light motor, for testing.
- **LED-Ring** (LED ring / light bar): P1–P4, **Rotieren** (rotate),
  **Blinken** (blink), **Aus** (off), and three automatic modes:
  - **Zufall** (random): changes randomly.
  - **Clock: Beat:** blinks on every beat of the MIDI clock.
  - **Clock: Takt** (bar): one LED per bar, round in a circle.

  On the PS4 controller the light bar takes the place of the ring: P1 blue,
  P2 red, P3 green, P4 pink.

### MIDI

1. In the DAW (Bitwig, Ableton, Logic, …) enable the MIDI input
   **PadBridge**. On Linux see also
   [part 6](#midi-for-bitwig-and-other-daws-snd-virmidi).
2. In the **MIDI** tab switch on **MIDI senden** (send MIDI) and pick the
   target: **Dieser Rechner** (this computer) or **Netzwerk-Gegenstelle**
   (network peer).
3. Adjust the mapping in one of two views:
   - **Bild** (picture): the outline of the pad in your hand (Xbox or PS4,
     automatic or chosen under **Form**). Next to every control is its CC or
     note field, and beside it the current MIDI value. Pressed controls light
     up in the picture. Clicking a control opens all its settings: active, CC
     or note, momentary or toggle, velocity (fixed or from the left/right
     trigger), and for axes invert.
   - **Tabelle** (table): the same settings as a list.

   Default mapping: axes CC 20–25, buttons CC 102–116. MIDI leaves these
   numbers undefined, so they clash with nothing. Controls that send the same
   message are marked red.
4. Channel and deadzones for sticks and triggers are at the top of the tab;
   **Standard** resets everything.
5. MIDI learn in the DAW then just works.

**The peer's pads as MIDI controllers:** normally the computer the pad is
plugged into makes the MIDI. With **Pads der Gegenstelle hier zu MIDI machen**
(turn the peer's pads into MIDI here) the receiving computer does it, with
its own mapping: pick "Pad n der Gegenstelle" under **Pad**; its buttons
light up in the picture and the mapping can be edited here. Handy when the
pad is on a computer you don't want to set up. Switch off **MIDI senden** on
the peer then, or everything arrives twice.

**MIDI clock:** send clock from the DAW to **PadBridge**. The modes "Clock:
Beat" and "Clock: Takt" in the monitor then follow the tempo. With the target
"Netzwerk-Gegenstelle", the DAW's clock comes back from the peer to the pad.

### MIDI devices (MIDI-Geräte)

The MIDI controllers on this computer show up here (Novation Launch Control
XL, Akai MIDImix and APC mini mk2, Allen & Heath Xone:K2, Ableton Push 2,
Elektron Digitakt II, others as a list). For the known devices PadBridge
shows a drawing with every value live.

- **Umbelegen und weiterleiten** (remap and forward): per knob or button
  CC/note, number, channel and momentary/toggle. The result comes out of the
  **PadBridge** port.
- **Knob LEDs:** colour, brightness, "Mitte hell" (brightest in the middle)
  or traffic light (Launch Control XL).
- **Button LEDs:** two states, **gedrückt** (pressed; **an** = on for
  toggles) and **in Ruhe** (at rest), each off or a colour, dimmed where
  possible. The same setting in both rows keeps the button lit all the time;
  a different colour or a dimmed rest makes it change on press. What works
  depends on the device: Launch Control XL red/amber/yellow/green with
  dimming, APC mini pads colours with dimming, Xone:K2 red/amber/green, Push 2
  pads red/green/white and buttons on/off, MIDImix and single-colour buttons
  on/off.
- **Endless encoders** (K2, Push 2) have no position; while turning, the
  drawing shows a lit ring and the last step ("+2", "−1"). Set them up as
  relative encoders in the DAW.
- **Push 2:** the "Live Port" is drawn. When Ableton Live runs with the Push as
  its control surface, Live drives the LEDs and the display and PadBridge only
  shows values. The Push 2 display is not MIDI and stays empty.
- **Over the network:** a device can send its remapped values to the peer.
  They arrive there on a port of their own, "PadBridge <device>" (on Windows
  create a loopMIDI port with that name, part 3).

### Keyboard/mouse (Tastatur/Maus)

1. Switch on **Tastatur/Maus aktiv** (keyboard/mouse active) and pick the
   target: **Dieser Rechner** or **Netzwerk-Gegenstelle**.
2. Give every button a keyboard key or a mouse button, with modifiers (Shift,
   Ctrl, Alt/Option, Super/Cmd).
3. Use each stick as mouse, scroll wheel, WASD or arrow keys. Sensitivity and
   curve can be set (1 = linear, higher = finer around the centre, fast at
   the edge).

Keyboard/mouse runs alongside the game pad. Best to map buttons the game does
not use.

---

## 5. Connecting two computers

### Key

Both sides need the same secret key. It is created on the first start and
stored in the file `netpad.key` in the settings folder (see
[part 7](#7-settings); the **Netzwerk** tab shows the path).

1. On one computer click **Schlüssel kopieren** (copy key) in the
   **Netzwerk** tab.
2. Get the text to the other computer (e.g. in a message to yourself), paste
   it there into the field **Schlüssel der anderen Seite hier einfügen**
   (paste the other side's key here) in the **Netzwerk** tab, then click
   **Übernehmen** (apply). PadBridge checks it, saves it (the old one stays as
   `netpad.key.alt`) and reconnects right away.
3. If the background service (part 6) also runs on this computer, restart it
   afterwards: `systemctl --user restart padbridge`.

Without a window it also works through the file, e.g. on Linux:
```sh
install -m 600 /dev/stdin ~/.config/padbridge/netpad.key
# paste the key, Enter, then Ctrl-D
```
On the Mac it is `~/Library/Application Support/PadBridge/netpad.key`, on
Windows `%APPDATA%\PadBridge\netpad.key`.

### Sending and receiving

In the **Netzwerk** tab:

- **On the receiving side:** under **Empfangen** (receive) switch on "Pads,
  MIDI und Tastatur/Maus einer Gegenstelle annehmen" (accept pads, MIDI and
  keyboard/mouse from a peer). The status shows "Bereit auf Port 48100"
  (ready on port 48100).
- **On the computer with the pad:** enter the peer's IP or host name and
  switch on "Gamepad an die Gegenstelle senden" (send game pad to the peer).
  The status turns green: "Verbunden … – Gegenstelle antwortet" (connected,
  peer answers), with the latency.
- **Presets:** **+** saves the current host under a name, e.g. "Desktop LAN"
  and "Desktop Tailscale". Clicking a preset switches the host, a right click
  deletes it.
- **Rumble:** "Spiele auf der Gegenstelle dürfen den Controller vibrieren
  lassen" (games on the peer may make the controller rumble) sends the game's
  rumble back to the pad in your hand.
- **MIDI and keyboard/mouse** over the network: pick the target
  "Netzwerk-Gegenstelle" in the respective tab. Everything runs over the same
  connection at once.

Both computers can send and receive at the same time.

On a Linux peer an **Xbox 360 Controller** appears; check it with `evtest`,
in the KDE system settings → Game Controller, or with `wine control` → Game
Controllers. On Windows (with ViGEmBus) **Win+R → `joy.cpl`** shows it. This
also applies to a PS4 controller in your hand: over the network the values
arrive in Xbox format, and games recognise the virtual Xbox pad without any
setup.

### Firewall

Open UDP 48100 **only** in the LAN or VPN and **never** to the internet. The
packets are signed, but not encrypted. Example with UFW (adjust the subnet;
`ip -br a` shows it):

```sh
sudo ufw allow from <lan-subnet>/24 to any port 48100 proto udp
sudo ufw allow in on tailscale0 to any port 48100 proto udp     # if Tailscale
```

This is only needed on the **receiving** computer: sending works without a
rule, and the firewall lets the peer's answers through. Ubuntu-based systems
such as Zorin often have UFW switched on already (graphical: "gufw"). On
Windows it is enough to allow "private networks" on the first start.

On the road the connection works the same over a VPN such as Tailscale: enter
the peer's Tailscale address as host (best as a preset of its own).

### Status page for your phone

In the **Netzwerk** tab switch on **Status-Webseite** (status page). PadBridge
then shows the address, a QR code and a PIN. Scan the QR code with your
phone: the page shows pads live (also the peer's), link and latency, virtual
pads, MIDI devices and the log. "Add to Home Screen" in the browser turns it
into an app. The page only shows things; nothing can be controlled through
it.

- **In the home network** (private addresses such as 192.168.x.x, 10.x.x.x)
  it opens without a PIN.
- **From outside**, e.g. over Tailscale, it asks for the six-digit PIN from
  the **Netzwerk** tab; the browser remembers the login. **Neue PIN** (new PIN)
  signs out all browsers. After five wrong tries it waits 30 seconds.
- Port 48180 (TCP) must be open in the firewall, like 48100 only for the LAN
  or VPN. The page is unencrypted HTTP: **never** expose it to the internet
  with port forwarding.

If PadBridge runs as a service (part 6), the setting from the window applies
to it too after restarting the service. That way the phone shows the state of
a computer without a screen.

### Sunshine/Moonlight

- In **Moonlight**, switch gamepad support **off** in the settings. Otherwise
  Sunshine adds a second pad. Picture, sound, mouse and keyboard keep going
  through Moonlight.
- For games the PadBridge pad is a perfectly normal local pad (xpad,
  `045e:028e`). Steam, SDL, Wine and Proton pick it up without further
  settings.

---

## 6. Linux service and MIDI for Bitwig

### As a service without a window

If a Linux computer should always receive, also without an open window, run
PadBridge as a systemd user service:

```sh
mkdir -p ~/.config/systemd/user
cp padbridge.service ~/.config/systemd/user/     # from the release package, or rust/app/ in the repo
systemctl --user daemon-reload
systemctl --user enable --now padbridge
journalctl --user -u padbridge -f       # follow the log
```

The service runs `padbridge --headless --listen --receive-only`: no window,
always ready to receive, and it reads no pads on this computer. So the window
can run next to it without MIDI arriving twice.

When you open the window next to it, **the window takes over receiving**: the
service pauses, and the window shows everything from the peer (pads, MIDI
devices, virtual pads). When you close the window, the service receives again
after a second or two. The virtual pads are created anew once; a running game
sees that as a short unplug and replug.

Start options (`padbridge --help`):

| Option | Effect |
|---|---|
| `--headless` | no window, runs until stopped |
| `--listen` | accepts a peer regardless of the setting |
| `--receive-only` | reads no pads on this computer and sends nothing |

### MIDI for Bitwig and other DAWs (snd-virmidi)

PadBridge creates an ALSA sequencer port **PadBridge**. Some DAWs, Bitwig on
Linux for example, only list rawmidi devices. The fix is a virtual rawmidi
device, which PadBridge connects to automatically in both directions:

```sh
echo snd-virmidi | sudo tee /etc/modules-load.d/padbridge-virmidi.conf
echo 'options snd-virmidi midi_devs=1 index=9 id=PadBridge' | sudo tee /etc/modprobe.d/padbridge-virmidi.conf
sudo modprobe snd-virmidi
systemctl --user restart padbridge      # or restart the window
```

`index=9` keeps the virtual card from taking a low number ahead of the real
sound cards at boot. The log then shows "MIDI mit VirMIDI … verbunden"
(connected to VirMIDI).

In Bitwig:
- Settings → Controllers: add "Generic MIDI Keyboard", input **VirMIDI 9-0**
- Settings → Synchronization: clock output **VirMIDI 9-0**, so the LED ring or
  light bar follows the beat

Check without a DAW: `aseqdump -p PadBridge` or `amidi -p hw:9,0 -d`.

---

## 7. Settings

| System | Folder |
|---|---|
| macOS | `~/Library/Application Support/PadBridge/` |
| Linux | `~/.config/padbridge/` |
| Windows | `%APPDATA%\PadBridge\` |

| File | Contents |
|---|---|
| `midi-mapping.json` | MIDI mapping, channel, deadzones, target |
| `netpad.json` | network: host, port, rumble, receive, host presets |
| `keymouse.json` | keyboard/mouse mapping |
| `surfaces.json` | MIDI devices: remapping and LEDs |
| `web.json` | status page: on/off, port, PIN |
| `netpad.key` | secret network key (created on the first start, `chmod 600`) |

The files are compatible with the old Swift app and `padbridge-netpad`.
Copying the folder to a new computer takes all settings along, the key too,
so the peer does not need to be paired again.

---

## 8. Troubleshooting

| Symptom | Cause / fix |
|---|---|
| Monitor shows no controller (Mac) | Another program holds the Xbox controller (old PadBridge version, 360Controller kext). Try another USB cable or hub; some cables only charge. |
| Monitor shows no controller (Linux) | User not in the `input` group, or not logged in again since being added. |
| PS4 controller appears on the Mac as "(Bluetooth)" although it is on the cable | The cable only charges. Use a data cable or disconnect Bluetooth (hold the PS button for 10 s). |
| LED ring / light bar "Nicht verfügbar" (Linux) | udev rule from part 2 missing; replug the pad after setting it up. |
| Status orange "keine Antwort" (no answer) | On the peer, receiving is off or PadBridge is not running, the firewall blocks, wrong host/port, or **different keys** (the most common case with a new installation that created its own). Packets with a wrong signature are dropped silently: copy the key and paste it on the other side (part 5). |
| "Port 48100 nicht verfügbar" (not available) | The old `padbridge-netpad` or an older PadBridge holds the port. A service from 0.5 on hands it to the window by itself. |
| A second start opens no second window | On purpose: it brings the open window to the front. |
| "ViGEmBus ist nicht installiert" (Windows) | Install the driver (part 3). MIDI and keyboard/mouse work without it. |
| Status page does not load | Firewall: open TCP 48180 (Windows: allow private networks on the first start). Phone and computer must be in the same network or connected through Tailscale. |
| Pad missing on Linux | No write access to `/dev/uinput` (`input` group, log in again). |
| Two pads in the game | Switch off gamepad support in Moonlight. |
| Bitwig sees no MIDI | Load snd-virmidi (part 6) and pick **VirMIDI**, not "PadBridge". |
| LED ring does not follow the clock | The DAW must send clock to PadBridge or VirMIDI, and the MIDI target must match the DAW (this computer or peer). |
| Keyboard/mouse on the Mac has no effect | Accessibility permission missing or outdated after a new build (part 1). |
| "kein MIDI-Port ‚PadBridge'" (no MIDI port, Windows) | Create a port with exactly this name in loopMIDI and restart PadBridge (part 3). |
| MIDI device "lässt sich nicht öffnen" (cannot be opened, Windows) | Another program (DAW, vendor editor) has it open. Close it there, or use the `PadBridge` port in the DAW instead. |
| Keys stay stuck | Should not happen: the peer releases everything after 500 ms without packets. If it does, switch "aktiv" off and on again. |

---

## 9. Security

- PadBridge only accepts packets signed with the shared key (HMAC-SHA256) and
  drops replayed packets.
- The packets are **not encrypted**. Anyone listening on the network sees the
  input. So run it only in the LAN or over a VPN such as Tailscale and never
  open the port to the internet.
- The status page only shows things. Outside the home network it asks for a
  PIN, but it is unencrypted HTTP and does not belong on the internet.
- The key does not belong in a repository or in any public chat. To get a new
  key, delete `netpad.key` and restart PadBridge; then copy it to the peer
  again.

---

## 10. The old Swift app and padbridge-netpad

Before the Rust app, PadBridge consisted of a Swift app for macOS 13+
(`Sources/`, Xbox 360 only) and the Linux receiver `linux/padbridge-netpad`.
Both still work and speak the same protocol, so old and new parts understand
each other. New features (PS4 controller, picture view, sending from Linux,
Windows, MIDI devices) exist only in the Rust app.

**Build the Swift app** (macOS 13+, Swift 6): `swift build`, `swift test`;
`scripts/bundle.sh` creates `build/PadBridge.app`.

**Switch from the old daemon to the Rust app:**

```sh
systemctl --user disable --now padbridge-netpad
# then set up the service from part 6
```

Back to the old daemon: `systemctl --user disable --now padbridge` and
`systemctl --user enable --now padbridge-netpad`.
