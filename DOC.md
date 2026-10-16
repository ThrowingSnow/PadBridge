# PadBridge einrichten

Diese Anleitung führt Schritt für Schritt durch die Einrichtung von PadBridge
auf einem Mac und auf einem Linux-Rechner. Jede Installation kann allein
laufen (Pad → MIDI und Tastatur/Maus auf demselben Rechner) oder mit einer
zweiten Installation im Netzwerk zusammenarbeiten, in beide Richtungen.

```
 Rechner A (Pad in der Hand)                    Rechner B (Gegenstelle)
 ┌──────────────────────────┐   UDP 48100,      ┌──────────────────────────────┐
 │ Xbox 360 / PS4-Pad       │   HMAC-signiert   │ virtuelles Xbox-360-Pad      │
 │ MIDI-Port "PadBridge"    │ ────────────────► │ MIDI-Port "PadBridge"        │
 │ Tastatur/Maus            │ ◄──────────────── │ virtuelle Tastatur/Maus      │
 │ LED-Ring / Lightbar      │  Rumble, MIDI-    │ (Spiele, Sunshine/Moonlight, │
 └──────────────────────────┘  Clock zurück     │  Wine, Bitwig, …)            │
                                                └──────────────────────────────┘
```

Das virtuelle Gamepad auf der Gegenstelle gibt es nur unter Linux. Ein Mac als
Gegenstelle nimmt MIDI und Tastatur/Maus an; für ein virtuelles Gamepad
bräuchte macOS einen Treiber.

Inhalt:
1. [Mac](#1-mac)
2. [Linux](#2-linux)
3. [Bedienung](#3-bedienung)
4. [Zwei Rechner verbinden](#4-zwei-rechner-verbinden)
5. [Linux als Dienst und MIDI für Bitwig](#5-linux-als-dienst-und-midi-für-bitwig)
6. [Einstellungen](#6-einstellungen)
7. [Fehlersuche](#7-fehlersuche)
8. [Sicherheit](#8-sicherheit)
9. [Die alte Swift-App und padbridge-netpad](#9-die-alte-swift-app-und-padbridge-netpad)

---

## 1. Mac

### Voraussetzungen

- macOS 10.13 High Sierra oder neuer, Intel oder Apple Silicon. Ein Mac ohne
  Metal reicht (getestet auf einem iMac von 2010).
- Ein Xbox-360-Controller mit Kabel (oder ein lizenzierter Klon, Liste in
  `rust/core/src/xbox360.rs`) oder ein PS4-Controller (DualShock 4) per USB
  oder Bluetooth.
- Xcode Command Line Tools und Rust:
  ```sh
  xcode-select --install
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

**Alte Treiber zuerst entfernen.** Der 360Controller-Kext (`360Controller`,
`Xbox360ControllerManager`) belegt den Xbox-Controller. Deinstalliere ihn über
das mitgelieferte Uninstall-Programm und starte den Mac danach neu. Für den
PS4-Controller braucht es nichts davon.

### Bauen und installieren

```sh
git clone https://github.com/ThrowingSnow/PadBridge.git
cd PadBridge/rust
scripts/bundle-macos.sh             # erzeugt target/PadBridge.app
cp -R target/PadBridge.app /Applications/
open /Applications/PadBridge.app
```

Für eine stabile Version vor dem Bauen einen Tag auschecken, zum Beispiel
`git checkout v0.4.1`. Die Versionsnummer der App kommt dann aus dem Tag.

Die App ist nur ad-hoc signiert. Beim ersten Start meldet macOS deshalb einen
„nicht verifizierten Entwickler“: Rechtsklick auf die App → **Öffnen** →
nochmals **Öffnen**. Danach startet sie normal.

Controller einstecken (oder den PS4-Controller per Bluetooth koppeln): Im Tab
**Monitor** bewegen sich Sticks und Tasten mit.

> Es kann immer nur ein Programm den Xbox-Controller öffnen. Vor dem Start
> einer neuen Version die alte beenden, auch die alte Swift-App.

### Tastatur/Maus auf dem Mac erlauben

Damit PadBridge Tasten und Maus steuern darf: Systemeinstellungen →
Datenschutz & Sicherheit → **Bedienungshilfen** → PadBridge erlauben. Die App
fragt beim ersten Mal selbst danach und versucht es danach alle paar Sekunden
erneut, ein Neustart ist nicht nötig.

> Nach jedem neuen Build verlangt macOS die Freigabe nochmals, weil die
> Signatur wechselt: den alten Eintrag mit „–“ entfernen und neu hinzufügen.

---

## 2. Linux

Getestet unter Arch/CachyOS mit KDE Plasma. Andere Distributionen
funktionieren genauso, nur die Paketnamen unterscheiden sich.

### Pakete

```sh
# Arch / CachyOS
sudo pacman -S --needed rust alsa-lib pkgconf
# Debian / Ubuntu
sudo apt install cargo libasound2-dev pkg-config
```

### Rechte für Pads und virtuelle Geräte

PadBridge läuft als normaler Benutzer. Es braucht Lesezugriff auf die Pads
(`/dev/input/event*`) und Schreibzugriff auf `/dev/uinput` für das virtuelle
Pad und die virtuelle Tastatur/Maus. Beides geht über die Gruppe `input`.
Wenn Sunshine installiert ist, ist die uinput-Regel meist schon vorhanden.
Sonst so:

```sh
echo 'KERNEL=="uinput", SUBSYSTEM=="misc", OPTIONS+="static_node=uinput", GROUP="input", MODE="0660", TAG+="uaccess"' \
  | sudo tee /etc/udev/rules.d/60-padbridge-uinput.rules
sudo usermod -aG input "$USER"     # danach ab- und wieder anmelden
sudo udevadm control --reload && sudo udevadm trigger
```

### Bauen und starten

```sh
git clone https://github.com/ThrowingSnow/PadBridge.git
cd PadBridge/rust
cargo build --release -p padbridge
install -Dm755 target/release/padbridge ~/.local/bin/padbridge
padbridge            # mit Fenster
```

Jedes Gamepad, das Linux kennt, erscheint im Monitor, auch beim Ein- und
Ausstecken: der Xbox-360-Controller über den Kernel-Treiber xpad, der
PS4-Controller über hid-playstation, aber auch andere Pads.

### LED-Ring und Lightbar

Der Kernel erlaubt das Setzen der Controller-LEDs standardmässig nur root.
Diese Regeln geben der Gruppe `input` Schreibrecht. Danach das Pad neu
einstecken.

**Xbox 360 (LED-Ring):**

```sh
echo 'ACTION=="add", SUBSYSTEM=="leds", KERNEL=="xpad*", RUN+="/bin/chgrp input /sys%p/brightness", RUN+="/bin/chmod 0664 /sys%p/brightness"' \
  | sudo tee /etc/udev/rules.d/71-padbridge-xpad-led.rules
sudo udevadm control --reload
```

**PS4-Controller (Lightbar):**

```sh
echo 'ACTION=="add", SUBSYSTEM=="leds", KERNEL=="*054C:*|*:rgb:indicator", RUN+="/bin/chgrp input /sys%p/brightness", RUN+="/bin/chmod 0664 /sys%p/brightness", RUN+="/bin/chgrp input /sys%p/multi_intensity", RUN+="/bin/chmod 0664 /sys%p/multi_intensity"' \
  | sudo tee /etc/udev/rules.d/72-padbridge-ds4-led.rules
sudo udevadm control --reload
```

Ohne diese Regeln funktioniert alles andere trotzdem; im Monitor steht dann
beim LED-Ring „Nicht verfügbar“.

---

## 3. Bedienung

Die Oberfläche hat vier Tabs.

### Monitor

- Zeigt das gewählte Pad live: Sticks, Trigger, alle Tasten. Bei mehreren Pads
  oben auswählen.
- **Rumble:** zwei Regler für den schweren und den leichten Motor, zum Testen.
- **LED-Ring / Lightbar:** P1–P4, Rotieren, Blinken, Aus, sowie drei
  automatische Modi:
  - **Zufall:** wechselt zufällig.
  - **Clock: Beat:** blinkt auf jedem Schlag der MIDI-Clock.
  - **Clock: Takt:** eine LED pro Takt, im Kreis.

  Beim PS4-Controller ersetzt die Lightbar den Ring: P1 blau, P2 rot, P3 grün,
  P4 pink.

### MIDI

1. In der DAW (Bitwig, Ableton, Logic, …) den MIDI-Eingang **PadBridge**
   aktivieren. Unter Linux siehe auch [Teil 5](#midi-für-bitwig-und-andere-daws-snd-virmidi).
2. Im Tab **MIDI** „MIDI senden“ einschalten und das Ziel wählen: **Dieser
   Rechner** oder **Netzwerk-Gegenstelle**.
3. Belegung anpassen, in einer von zwei Ansichten:
   - **Bild:** der Umriss des Pads in deiner Hand (Xbox oder PS4, automatisch
     oder bei „Form“ gewählt). Neben jeder Taste steht ihr Feld für die CC-
     oder Notennummer; daneben der aktuelle MIDI-Wert. Gedrückte Tasten
     leuchten im Bild. Ein Klick auf eine Taste öffnet alle Einstellungen:
     Aktiv, CC oder Note, Momentary oder Toggle, Velocity (fix oder vom
     linken/rechten Trigger), bei Achsen Invertieren.
   - **Tabelle:** dieselben Einstellungen als Liste.

   Standardbelegung: Achsen CC 20–25, Tasten CC 102–116. Diese Nummern sind
   in MIDI nicht vergeben und kollidieren mit nichts. Rot markiert sind
   Controls, die dieselbe Nachricht senden.
4. Kanal und Deadzones für Sticks und Trigger stehen oben im Tab;
   „Standard“ setzt alles zurück.
5. Danach klappt MIDI-Learn in der DAW.

**MIDI-Clock:** In der DAW Clock an **PadBridge** senden. Dann folgen die
Modi „Clock: Beat“ und „Clock: Takt“ im Monitor dem Tempo. Mit Ziel
„Netzwerk-Gegenstelle“ kommt die Clock der DAW auf der Gegenstelle zurück zum
Pad.

### Tastatur/Maus

1. „Tastatur/Maus aktiv“ einschalten und das Ziel wählen: **Dieser Rechner**
   oder **Netzwerk-Gegenstelle**.
2. Jeder Taste eine Taste der Tastatur oder einen Mausknopf zuweisen, mit
   Modifier (Shift, Ctrl, Alt/Option, Super/Cmd).
3. Jeden Stick als Maus, Mausrad, WASD oder Pfeiltasten verwenden.
   Empfindlichkeit und Kurve sind einstellbar (1 = linear, höher = feiner um
   die Mitte, schnell am Rand).

Tastatur/Maus läuft parallel zum Gamepad. Am besten Tasten belegen, die das
Spiel nicht braucht.

---

## 4. Zwei Rechner verbinden

### Schlüssel

Beide Seiten brauchen denselben geheimen Schlüssel. Er entsteht beim ersten
Start und liegt in der Datei `netpad.key` im Einstellungsordner (siehe
[Teil 6](#6-einstellungen); der Tab **Netzwerk** zeigt den Pfad).

1. Auf dem einen Rechner im Tab **Netzwerk** auf „Schlüssel kopieren“ klicken.
2. Auf dem anderen Rechner den Schlüssel in dessen `netpad.key` schreiben,
   zum Beispiel unter Linux:
   ```sh
   mkdir -p ~/.config/padbridge
   install -m 600 /dev/stdin ~/.config/padbridge/netpad.key
   # Schlüssel einfügen, Enter, dann Ctrl-D
   ```
   Auf dem Mac ist es `~/Library/Application Support/PadBridge/netpad.key`.
3. PadBridge auf diesem Rechner neu starten.

### Senden und empfangen

Im Tab **Netzwerk**:

- **Auf der Gegenstelle:** unter „Empfangen“ „Pads, MIDI und Tastatur/Maus
  einer Gegenstelle annehmen“ einschalten. Der Status zeigt „Bereit auf Port
  48100“.
- **Auf dem Rechner mit dem Pad:** die IP oder den Hostnamen der Gegenstelle
  eintragen und „Gamepad an die Gegenstelle senden“ einschalten. Der Status
  wird grün: „Verbunden … – Gegenstelle antwortet“, mit der Latenz.
- **Presets:** Mit **+** speicherst du den aktuellen Host unter einem Namen,
  zum Beispiel „Desktop LAN“ und „Desktop Tailscale“. Ein Klick auf ein Preset
  wechselt den Host, ein Rechtsklick löscht es.
- **Rumble:** „Spiele auf der Gegenstelle dürfen den Controller vibrieren
  lassen“ schickt das Rumble aus dem Spiel zurück zum Pad in deiner Hand.
- **MIDI und Tastatur/Maus** übers Netz: im jeweiligen Tab das Ziel
  „Netzwerk-Gegenstelle“ wählen. Alles läuft gleichzeitig über dieselbe
  Verbindung.

Beide Rechner können gleichzeitig senden und empfangen.

Auf einer Linux-Gegenstelle erscheint ein **Xbox 360 Controller**, prüfbar mit
`evtest`, in den KDE-Systemeinstellungen → Gamecontroller oder mit
`wine control` → Gamecontroller. Das gilt auch für einen PS4-Controller in
der Hand: Übers Netz kommen die Werte im Xbox-Format an, und Spiele erkennen
das virtuelle Xbox-Pad ohne Einrichtung.

### Firewall

Gib UDP 48100 **nur** im LAN oder VPN frei und **nie** ins Internet. Die Pakete
sind signiert, aber nicht verschlüsselt. Beispiel mit UFW (Interface und
Subnetz anpassen, `ip -br a` zeigt sie):

```sh
sudo ufw allow in on <lan-interface> from <lan-subnetz>/24 to any port 48100 proto udp
sudo ufw allow in on tailscale0 to any port 48100 proto udp     # falls Tailscale
```

Unterwegs funktioniert die Verbindung über ein VPN wie Tailscale genauso: als
Host die Tailscale-Adresse der Gegenstelle eintragen (am besten als eigenes
Preset).

### Sunshine/Moonlight

- In **Moonlight** unter Einstellungen den Gamepad-Support **ausschalten**.
  Sonst legt Sunshine ein zweites Pad an. Bild, Ton, Maus und Tastatur laufen
  weiter über Moonlight.
- Das PadBridge-Pad ist für Spiele ein ganz normales lokales Pad (xpad,
  `045e:028e`). Steam, SDL, Wine und Proton erkennen es ohne weitere Einstellungen.

---

## 5. Linux als Dienst und MIDI für Bitwig

### Als Dienst ohne Fenster

Soll ein Linux-Rechner immer empfangen, auch ohne geöffnetes Fenster, läuft
PadBridge als systemd-User-Dienst:

```sh
mkdir -p ~/.config/systemd/user
cp PadBridge/rust/app/padbridge.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now padbridge
journalctl --user -u padbridge -f       # Log anschauen
```

Der Dienst startet `padbridge --headless --listen --receive-only`: ohne
Fenster, immer empfangsbereit, und er liest keine Pads an diesem Rechner. So
kann das Fenster daneben laufen, ohne dass MIDI doppelt ankommt.

> Läuft der Dienst, belegt er Port 48100. Im Fenster daneben „Empfangen“
> auslassen, sonst meldet es „Port 48100 nicht verfügbar“. Pads am Rechner,
> MIDI und Senden macht das Fenster wie gewohnt.

Startoptionen (`padbridge --help`):

| Option | Wirkung |
|---|---|
| `--headless` | kein Fenster, läuft bis zum Beenden |
| `--listen` | nimmt eine Gegenstelle an, unabhängig von der Einstellung |
| `--receive-only` | liest keine Pads an diesem Rechner und sendet nichts |

### MIDI für Bitwig und andere DAWs (snd-virmidi)

PadBridge legt einen ALSA-Sequencer-Port **PadBridge** an. Manche DAWs, zum
Beispiel Bitwig unter Linux, zeigen aber nur Rawmidi-Geräte an. Die Lösung ist
ein virtuelles Rawmidi-Gerät, mit dem sich PadBridge automatisch in beide
Richtungen verbindet:

```sh
echo snd-virmidi | sudo tee /etc/modules-load.d/padbridge-virmidi.conf
echo 'options snd-virmidi midi_devs=1 index=9 id=PadBridge' | sudo tee /etc/modprobe.d/padbridge-virmidi.conf
sudo modprobe snd-virmidi
systemctl --user restart padbridge      # bzw. das Fenster neu starten
```

`index=9` sorgt dafür, dass die virtuelle Karte beim Booten keine niedrige
Nummer vor den echten Audiokarten belegt. Im Log erscheint danach „MIDI mit
VirMIDI … verbunden“.

In Bitwig:
- Einstellungen → Controller: „Generic MIDI Keyboard“ hinzufügen, Eingang **VirMIDI 9-0**
- Einstellungen → Synchronisation: Clock-Ausgang **VirMIDI 9-0**, damit LED-Ring
  oder Lightbar dem Takt folgen

Prüfen ohne DAW: `aseqdump -p PadBridge` oder `amidi -p hw:9,0 -d`.

---

## 6. Einstellungen

| System | Ordner |
|---|---|
| macOS | `~/Library/Application Support/PadBridge/` |
| Linux | `~/.config/padbridge/` |

| Datei | Inhalt |
|---|---|
| `midi-mapping.json` | MIDI-Belegung, Kanal, Deadzones, Ziel |
| `netpad.json` | Netzwerk: Host, Port, Rumble, Empfangen, Host-Presets |
| `keymouse.json` | Tastatur/Maus-Belegung |
| `netpad.key` | geheimer Schlüssel fürs Netzwerk (wird beim ersten Start erzeugt, `chmod 600`) |

Die Dateien sind mit der alten Swift-App und `padbridge-netpad` kompatibel.
Wer den Ordner auf einen neuen Rechner kopiert, nimmt alle Einstellungen mit,
auch den Schlüssel. Die Gegenstelle muss dann nicht neu gekoppelt werden.

---

## 7. Fehlersuche

| Symptom | Ursache / Lösung |
|---|---|
| Monitor zeigt keinen Controller (Mac) | Ein anderes Programm hält den Xbox-Controller (alte PadBridge-Version, 360Controller-Kext). USB-Kabel oder Hub wechseln; manche Kabel laden nur. |
| Monitor zeigt keinen Controller (Linux) | Benutzer nicht in der Gruppe `input`, oder seit dem Hinzufügen nicht neu angemeldet. |
| PS4-Controller erscheint am Mac als „(Bluetooth)“, obwohl er am Kabel hängt | Das Kabel lädt nur. Ein Datenkabel nehmen oder Bluetooth trennen (PS-Taste 10 s halten). |
| LED-Ring / Lightbar „Nicht verfügbar“ (Linux) | udev-Regel aus Teil 2 fehlt; nach dem Einrichten das Pad neu einstecken. |
| Status orange „keine Antwort“ | Auf der Gegenstelle ist „Empfangen“ aus oder PadBridge läuft nicht, die Firewall blockiert, falscher Host/Port oder falscher Schlüssel. Pakete mit falscher Signatur werden stillschweigend verworfen: Schlüssel neu kopieren und beide Seiten neu starten. |
| „Port 48100 nicht verfügbar“ | Ein anderes PadBridge (Dienst, zweites Fenster) oder der alte `padbridge-netpad` belegt den Port. Dort „Empfangen“ ausschalten. |
| Pad auf Linux fehlt | Keine Schreibrechte auf `/dev/uinput` (Gruppe `input`, neu anmelden). |
| Zwei Pads im Spiel | Gamepad-Support in Moonlight ausschalten. |
| Bitwig sieht kein MIDI | snd-virmidi laden (Teil 5) und **VirMIDI** wählen, nicht „PadBridge“. |
| LED-Ring folgt der Clock nicht | Die DAW muss Clock an PadBridge bzw. VirMIDI senden, und das MIDI-Ziel muss zur DAW passen (dieser Rechner oder Gegenstelle). |
| Tastatur/Maus am Mac ohne Wirkung | Bedienungshilfen-Freigabe fehlt oder ist nach einem neuen Build veraltet (Teil 1). |
| Tasten bleiben hängen | Kommt nicht vor: Die Gegenstelle lässt nach 500 ms ohne Pakete alles los. Wenn doch, „aktiv“ aus- und wieder einschalten. |

---

## 8. Sicherheit

- PadBridge nimmt nur Pakete an, die mit dem gemeinsamen Schlüssel signiert
  sind (HMAC-SHA256), und verwirft wiederholte Pakete.
- Die Pakete sind **nicht verschlüsselt**. Wer im Netz mithört, sieht die
  Eingaben. Darum nur im LAN oder über ein VPN wie Tailscale betreiben und den
  Port nie ins Internet freigeben.
- Der Schlüssel gehört nicht ins Repository und in keinen Chat. Einen neuen
  Schlüssel bekommst du, indem du `netpad.key` löschst und PadBridge neu
  startest. Danach muss er neu auf die Gegenstelle.

---

## 9. Die alte Swift-App und padbridge-netpad

Vor der Rust-App bestand PadBridge aus einer Swift-App für macOS 13+
(`Sources/`, nur Xbox 360) und dem Linux-Empfänger `linux/padbridge-netpad`.
Beide funktionieren weiter und sprechen dasselbe Protokoll, alte und neue
Teile verstehen sich also. Neue Funktionen (PS4-Controller, Bildansicht,
Senden von Linux aus) gibt es nur in der Rust-App.

**Swift-App bauen** (macOS 13+, Swift 6): `swift build`, `swift test`,
`scripts/bundle.sh` erzeugt `build/PadBridge.app`.

**Vom alten Daemon zur Rust-App wechseln:**

```sh
systemctl --user disable --now padbridge-netpad
# dann den Dienst aus Teil 5 einrichten
```

Zurück zum alten Daemon: `systemctl --user disable --now padbridge` und
`systemctl --user enable --now padbridge-netpad`.
