# padbridge-netpad

Linux side of PadBridge's network gamepad: receives signed UDP packets
(protocol in `src/protocol.rs`) and drives a uinput device that looks exactly
like an xpad-driven Xbox 360 pad (`045e:028e`), so Wine/Proton, Steam and SDL
treat it as a real controller.

Step-by-step setup (German): [../../DOC.md](../../DOC.md)

## Install (as the desktop user, no root)

```sh
cargo build --release
install -Dm755 target/release/padbridge-netpad ~/.local/bin/padbridge-netpad
mkdir -p ~/.config/padbridge
install -m 600 /dev/stdin ~/.config/padbridge/netpad.key   # paste the key from PadBridge → Netzwerk → "Schlüssel kopieren"
cp padbridge-netpad.service ~/.config/systemd/user/
systemctl --user enable --now padbridge-netpad
```

Needs write access to `/dev/uinput` (group `input`; Sunshine's udev rule already grants it).

Firewall: allow UDP 48100 only from LAN / Tailscale, e.g.

```sh
sudo ufw allow in on <lan-interface> from <lan-subnet>/24 to any port 48100 proto udp
sudo ufw allow in on tailscale0 to any port 48100 proto udp
```

Bitwig (and other apps that only list ALSA rawmidi devices) do not see the
sequencer port "PadBridge". Load `snd-virmidi`; the daemon links to it
automatically and the DAW shows "VirMIDI 9-0":

```sh
echo snd-virmidi | sudo tee /etc/modules-load.d/padbridge-virmidi.conf
echo 'options snd-virmidi midi_devs=1 index=9 id=PadBridge' | sudo tee /etc/modprobe.d/padbridge-virmidi.conf
sudo modprobe snd-virmidi
```

(`index=9` keeps the virtual card from taking a low index before the audio cards at boot.)

Behaviour: pads are created on the first packet and kept for the daemon's
lifetime (games enumerate once); 500 ms without packets releases all inputs.
