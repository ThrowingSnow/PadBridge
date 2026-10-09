# padbridge-netpad

Linux side of PadBridge's network gamepad: receives signed UDP packets
(protocol in `src/protocol.rs`) and drives a uinput device that looks exactly
like an xpad-driven Xbox 360 pad (`045e:028e`), so Wine/Proton, Steam and SDL
treat it as a real controller.

## Install (as the desktop user, no root)

```sh
cargo build --release
install -m 755 target/release/padbridge-netpad ~/.local/bin/
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

Behaviour: pads are created on the first packet and kept for the daemon's
lifetime (games enumerate once); 500 ms without packets releases all inputs.
