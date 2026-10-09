# PadBridge einrichten

Diese Anleitung führt Schritt für Schritt durch die Einrichtung auf einem neuen
Mac und optional auf einem Linux-Rechner, der das Gamepad, MIDI und
Tastatur/Maus über das Netzwerk bekommt.

```
 Mac (PadBridge.app)                          Linux-Host (padbridge-netpad)
 ┌───────────────────────┐   UDP 48100,      ┌──────────────────────────────┐
 │ Xbox-360-Pad per USB  │   HMAC-signiert   │ virtuelles Xbox-360-Pad      │
 │ MIDI-Port "PadBridge" │ ────────────────► │ ALSA-MIDI "PadBridge"        │
 │ Tastatur/Maus         │ ◄──────────────── │ virtuelle Tastatur/Maus      │
 │ LED-Ring, Rumble      │  Rumble, MIDI-    │ (Sunshine/Moonlight, Wine,   │
 └───────────────────────┘  Clock zurück     │  Bitwig, …)                  │
                                             └──────────────────────────────┘
```

Der Linux-Teil ist optional. Für MIDI und Tastatur/Maus auf dem Mac allein
reicht Teil 1.

---

## 1. Mac

### Voraussetzungen

- macOS 13 oder neuer, Apple Silicon oder Intel
- Xcode Command Line Tools mit Swift 6. Ein volles Xcode ist nicht nötig:
  ```sh
  xcode-select --install
  ```
- Ein kabelgebundener Xbox-360-Controller oder ein kompatibler Klon
  (Liste in `Sources/PadUSB/KnownDevices.swift`)

**Alte Treiber zuerst entfernen.** Der 360Controller-Kext (`360Controller`,
`Xbox360ControllerManager`) belegt das Gerät. Unter macOS 14 lässt er sich
ohnehin nicht mehr laden. Deinstalliere ihn über das mitgelieferte
Uninstall-Programm und starte den Mac danach neu.

### Bauen und installieren

```sh
git clone https://github.com/<user>/PadBridge.git
cd PadBridge
swift test                    # optional: alle Tests sollten grün sein
scripts/bundle.sh             # erzeugt build/PadBridge.app
cp -R build/PadBridge.app /Applications/
open /Applications/PadBridge.app
```

Die Versionsnummer der App kommt aus dem Git-Tag. Für eine stabile Version
checkst du vor dem Bauen einen Tag aus, zum Beispiel `git checkout v0.3.0`.

Controller einstecken: Im Tab **Monitor** sollten sich Sticks und Knöpfe
bewegen. Die App braucht keine Treiber und keine Systemerweiterung.

> Es kann immer nur eine Instanz den Controller öffnen. Wenn du eine neue
> Version startest, beende vorher die alte (auch `swift run …` im Terminal).

### Einstellungen

Alle Einstellungen liegen in `~/Library/Application Support/PadBridge/`:

| Datei | Inhalt |
|---|---|
| `midi-mapping.json` | MIDI-Tabelle, Kanal, Deadzones, Ziel |
| `netpad.json` | Netzwerk: Host, Port, Rumble, Host-Presets |
| `keymouse.json` | Tastatur/Maus-Belegung |
| `netpad.key` | geheimer Schlüssel für das Netzwerk (wird beim ersten Start erzeugt, `chmod 600`) |

Wenn du diesen Ordner auf einen neuen Mac kopierst, nimmst du alle
Einstellungen mit, auch den Schlüssel. Der Linux-Host muss dann nicht neu
gekoppelt werden.

### MIDI auf dem Mac

1. In der DAW (Bitwig, Ableton, Logic, …) den MIDI-Eingang **PadBridge** aktivieren.
2. Im Tab **MIDI** jedem Control eine CC- oder Notennummer geben. Die Standardbelegung:
   Achsen CC 20–25, Knöpfe CC 102–116. Danach klappt MIDI-Learn in der DAW.
3. Optional: In der DAW MIDI-Clock an **PadBridge** senden. Dann blinkt der
   LED-Ring im Takt (Modus „Clock: Takt“ oder „Clock: Beat“ im Tab Monitor).

### Tastatur/Maus auf dem Mac

1. Im Tab **Tastatur/Maus** die Option „aktiv“ einschalten und als Ziel **Mac** wählen.
2. Unter Systemeinstellungen → Datenschutz & Sicherheit → **Bedienungshilfen**
   PadBridge erlauben (der Knopf „Freigabe anfordern“ öffnet die Stelle).

> Die App ist nur ad-hoc signiert. Nach jedem neuen Build verlangt macOS die
> Freigabe deshalb nochmals: den alten Eintrag mit „–“ entfernen und neu hinzufügen.

---

## 2. Linux-Host (optional)

Getestet unter Arch/CachyOS mit KDE Plasma. Andere Distributionen funktionieren
genauso, nur die Paketnamen unterscheiden sich.

### Pakete

```sh
# Arch / CachyOS
sudo pacman -S --needed rust alsa-lib pkgconf
# Debian / Ubuntu
sudo apt install cargo libasound2-dev pkg-config
```

### Zugriff auf /dev/uinput

Der Daemon läuft als normaler Benutzer und braucht Schreibrechte auf `/dev/uinput`.
Wenn Sunshine installiert ist, ist das meist schon erledigt: Dessen udev-Regel
gibt der Gruppe `input` Zugriff. Sonst so:

```sh
echo 'KERNEL=="uinput", SUBSYSTEM=="misc", OPTIONS+="static_node=uinput", GROUP="input", MODE="0660", TAG+="uaccess"' \
  | sudo tee /etc/udev/rules.d/60-padbridge-uinput.rules
sudo usermod -aG input "$USER"     # danach ab- und wieder anmelden
sudo udevadm control --reload && sudo udevadm trigger
```

### Daemon bauen und starten

```sh
cd PadBridge/linux/padbridge-netpad
cargo build --release
install -Dm755 target/release/padbridge-netpad ~/.local/bin/padbridge-netpad
```

**Den Schlüssel übertragen:** Auf dem Mac im Tab **Netzwerk** auf
„Schlüssel kopieren“ klicken. Danach auf dem Linux-Host:

```sh
mkdir -p ~/.config/padbridge
install -m 600 /dev/stdin ~/.config/padbridge/netpad.key
# Schlüssel einfügen, Enter, dann Ctrl-D
```

**Als systemd-User-Service einrichten:**

```sh
mkdir -p ~/.config/systemd/user
cp padbridge-netpad.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now padbridge-netpad
journalctl --user -u padbridge-netpad -f       # Log anschauen
```

Optionen: `padbridge-netpad --bind 0.0.0.0 --port 48100 --key ~/.config/padbridge/netpad.key`.

### Firewall

Gib UDP 48100 **nur** im LAN oder VPN frei und **nie** ins Internet. Die Pakete
sind zwar signiert, aber nicht verschlüsselt. Beispiel mit UFW (Interface und
Subnetz anpassen, `ip -br a` zeigt sie):

```sh
sudo ufw allow in on <lan-interface> from <lan-subnetz>/24 to any port 48100 proto udp
sudo ufw allow in on tailscale0 to any port 48100 proto udp     # falls Tailscale
```

### MIDI für Bitwig und andere DAWs (snd-virmidi)

Der Daemon legt einen ALSA-Sequencer-Port **PadBridge** an. Manche DAWs, zum
Beispiel Bitwig unter Linux, zeigen aber nur Rawmidi-Geräte an. Die Lösung
ist ein virtuelles Rawmidi-Gerät, mit dem sich der Daemon automatisch verbindet:

```sh
echo snd-virmidi | sudo tee /etc/modules-load.d/padbridge-virmidi.conf
echo 'options snd-virmidi midi_devs=1 index=9 id=PadBridge' | sudo tee /etc/modprobe.d/padbridge-virmidi.conf
sudo modprobe snd-virmidi
systemctl --user restart padbridge-netpad
```

`index=9` sorgt dafür, dass die virtuelle Karte beim Booten keine niedrige
Nummer vor den echten Audiokarten belegt. Im Log erscheint danach
„MIDI linked to rawmidi bridge“.

In Bitwig:
- Einstellungen → Controller: „Generic MIDI Keyboard“ hinzufügen, Eingang **VirMIDI 9-0**
- Einstellungen → Synchronisation: Clock-Ausgang **VirMIDI 9-0**, damit der LED-Ring folgt

Prüfen ohne DAW: `aseqdump -p PadBridge` oder `amidi -p hw:9,0 -d`.

---

## 3. Mac mit dem Linux-Host verbinden

1. Im Tab **Netzwerk** die IP oder den Hostnamen des Linux-Hosts eintragen.
2. Mit **+** speicherst du den aktuellen Host als Preset unter einem eigenen
   Namen, zum Beispiel „Desktop LAN“ oder „Desktop Tailscale“. Ein Klick auf das
   Preset wechselt den Host, ein Rechtsklick löscht es. Gleicher Name überschreibt.
3. „Gamepad ans Netzwerk senden“ einschalten. Der Status sollte grün
   „Verbunden – Daemon antwortet“ mit Latenz zeigen.
4. Auf dem Linux-Host erscheint ein **Xbox 360 Controller**, prüfbar mit
   `evtest`, in den KDE-Systemeinstellungen → Gamecontroller oder mit `wine control` → Gamecontroller.
5. MIDI über das Netzwerk: im Tab **MIDI** das Ziel „Linux-Host“ wählen.
   Tastatur/Maus über das Netzwerk: im Tab **Tastatur/Maus** das Ziel „Linux-Host“ wählen.
   Gamepad, MIDI und Tastatur/Maus laufen gleichzeitig über dieselbe Verbindung.
6. Rumble: Die Option „Spiele auf dem Host dürfen den Controller vibrieren lassen“
   lässt Spiele auf dem Host den Controller am Mac vibrieren.

### Sunshine/Moonlight

- In **Moonlight** unter Einstellungen den Gamepad-Support **ausschalten**.
  Sonst legt Sunshine ein zweites Pad an. Bild, Ton, Maus und Tastatur laufen
  weiter über Moonlight.
- Das PadBridge-Pad ist für Spiele ein ganz normales lokales Pad (xpad,
  `045e:028e`). Steam, SDL, Wine und Proton erkennen es ohne weitere Einstellungen.

---

## 4. Fehlersuche

| Symptom | Ursache / Lösung |
|---|---|
| Monitor zeigt keinen Controller | Eine andere PadBridge-Instanz läuft noch, oder der 360Controller-Kext ist noch installiert. USB-Kabel oder Hub wechseln. |
| Status orange „keine Antwort vom Daemon“ | Daemon läuft nicht (`systemctl --user status padbridge-netpad`), die Firewall blockiert, falscher Host/Port oder falscher Schlüssel. Pakete mit falscher Signatur verwirft der Daemon stillschweigend. Im Log erscheint dann kein „sender …“. Den Schlüssel neu kopieren und `systemctl --user restart padbridge-netpad` ausführen. |
| Pad auf Linux fehlt | Keine Schreibrechte auf `/dev/uinput` (Gruppe `input`, neu anmelden). |
| Zwei Pads im Spiel | Gamepad-Support in Moonlight ausschalten. |
| Bitwig sieht kein MIDI | snd-virmidi laden (siehe oben) und **VirMIDI** wählen, nicht „PadBridge“. |
| LED-Ring folgt der Clock nicht | Die DAW muss Clock an PadBridge/VirMIDI senden, und das MIDI-Ziel in der App muss zur DAW passen (Mac oder Linux-Host). |
| Tastatur/Maus am Mac ohne Wirkung | Bedienungshilfen-Freigabe fehlt oder ist nach einem neuen Build veraltet. |
| Tasten bleiben hängen | Kommt nicht vor: Der Daemon lässt nach 500 ms ohne Pakete alles los. Wenn doch, „aktiv“ aus- und wieder einschalten. |

---

## 5. Sicherheit

- Der Daemon nimmt nur Pakete an, die mit dem gemeinsamen Schlüssel signiert
  sind (HMAC-SHA256). Er erkennt wiederholte Pakete und verwirft sie.
- Die Pakete sind **nicht verschlüsselt**. Wer im Netz mithört, sieht die
  Eingaben. Darum nur im LAN oder über ein VPN wie Tailscale betreiben und den
  Port nie ins Internet freigeben.
- Der Schlüssel gehört nicht ins Repository. Ein neuer Schlüssel entsteht,
  wenn du `netpad.key` auf dem Mac löschst und die App neu startest. Danach
  musst du ihn neu auf den Linux-Host übertragen.
