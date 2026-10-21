# PadBridge einrichten

Diese Anleitung führt Schritt für Schritt durch die Einrichtung von PadBridge
auf einem Mac, einem Linux-Rechner und unter Windows. Jede Installation kann allein
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

Das virtuelle Gamepad auf der Gegenstelle gibt es unter Linux (uinput) und
unter Windows (mit dem kostenlosen Treiber ViGEmBus). Ein Mac als Gegenstelle
nimmt MIDI und Tastatur/Maus an; für ein virtuelles Gamepad bräuchte macOS
einen Treiber.

Fertige Programme für alle drei Systeme gibt es unter
[Releases](https://github.com/ThrowingSnow/PadBridge/releases). Selbst bauen
ist nur nötig, wer am Code mitarbeiten will.

Inhalt:
1. [Mac](#1-mac)
2. [Linux](#2-linux)
3. [Windows](#3-windows)
4. [Bedienung](#4-bedienung)
5. [Zwei Rechner verbinden](#5-zwei-rechner-verbinden)
6. [Linux als Dienst und MIDI für Bitwig](#6-linux-als-dienst-und-midi-für-bitwig)
7. [Einstellungen](#7-einstellungen)
8. [Fehlersuche](#8-fehlersuche)
9. [Sicherheit](#9-sicherheit)
10. [Die alte Swift-App und padbridge-netpad](#10-die-alte-swift-app-und-padbridge-netpad)

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

Getestet unter Arch/CachyOS (KDE Plasma) und Zorin OS 18 (Ubuntu 24.04,
GNOME mit Wayland). Das fertige Paket läuft auf Ubuntu 20.04 und neuer,
Debian 11+, Mint, Zorin, Pop!_OS sowie aktuellem Arch und Fedora.

### Fertiges Paket

```sh
tar xzf padbridge-<version>-linux-x86_64.tar.gz
install -Dm755 padbridge-<version>-linux-x86_64/padbridge ~/.local/bin/padbridge
padbridge            # mit Fenster
```

Danach die Rechte unten einrichten.

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

### Selbst bauen

PadBridge braucht ein aktuelles Rust. Die Pakete `cargo`/`rustc` von Ubuntu
und Debian sind dafür zu alt, darum Rust über [rustup](https://rustup.rs):

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

Ein auf einem Rolling-Release-System (Arch) gebautes Programm braucht dessen
neueste glibc und startet auf Ubuntu nicht. Für ein Paket, das überall läuft,
baut `scripts/build-linux.sh` in einem Ubuntu-20.04-Container (braucht Docker).

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

## 3. Windows

### Voraussetzungen

- Windows 10 oder 11, 64 Bit.
- Ein Xbox-Controller (360, One, Series oder ein anderes XInput-Pad) oder ein
  PS4-Controller (DualShock 4) per USB.
- Für MIDI: [loopMIDI](https://www.tobias-erichsen.de/software/loopmidi.html)
  von Tobias Erichsen (kostenlos). Windows kann ohne Treiber keine virtuellen
  MIDI-Ports anlegen, loopMIDI übernimmt das.

### Installieren

Die `padbridge-<version>-windows-x86_64.exe` aus den Releases in einen
eigenen Ordner legen, zum Beispiel `C:\PadBridge\`, und per Doppelklick
starten. Das Programm ist nicht signiert: Warnt Windows SmartScreen, auf
**Weitere Informationen** → **Trotzdem ausführen** klicken. Beim ersten Start
fragt die Windows-Firewall nach: für **private Netzwerke** erlauben.

### Virtuelles Gamepad (ViGEmBus)

Damit der PC Pads einer Gegenstelle als echten Xbox-360-Controller an Spiele
weitergibt, braucht es den kostenlosen Treiber
[ViGEmBus](https://github.com/nefarius/ViGEmBus/releases) (Datei
`ViGEmBus_…_x64_x86_arm64.exe`). Er wird nicht mehr weiterentwickelt, die
letzte Version läuft aber unter Windows 10 und 11. Achtung: Ein mit der
Meta-Quest-Software installierter „Oculus Virtual Gamepad Emulation Bus“ ist
eine andere Abwandlung und reicht nicht, beide vertragen sich aber. Fehlt der
Treiber, meldet das Protokoll „ViGEmBus ist nicht installiert“; MIDI und
Tastatur/Maus von der Gegenstelle gehen trotzdem.

### Selbst bauen

Die `.exe` entsteht auf einem Mac oder Linux-Rechner (Cross-Build) und wird
dann auf den PC kopiert:

```sh
rustup target add x86_64-pc-windows-gnu
brew install mingw-w64                 # Mac; unter Linux das Paket mingw-w64
cd PadBridge/rust
scripts/build-windows.sh               # -> target/x86_64-pc-windows-gnu/release/padbridge.exe
```

### MIDI-Ports in loopMIDI

In loopMIDI unten den Namen eintragen und auf **+** klicken:

| Port | Richtung | nötig? |
|---|---|---|
| `PadBridge` | Pads → DAW | ja, für MIDI |
| `PadBridge In` | DAW → PadBridge (z. B. MIDI-Clock für den LED-Ring) | nur für Clock |
| `PadBridge <Gerät>` | MIDI-Gerät der Gegenstelle, z. B. `PadBridge Launch Control XL` | nur dafür |

Die Namen müssen genau stimmen. PadBridge sucht die Ports beim Start: loopMIDI
also **vor** PadBridge starten. Ein loopMIDI-Port gibt alles zurück, was
hineingeht; darum braucht der Rückweg einen eigenen Port.

### Was unter Windows anders ist

- **Xbox-Controller** laufen über XInput. Den LED-Ring steuert Windows selbst,
  darum sind die LED-Knöpfe im Monitor ausgegraut. Rumble geht.
- **PS4-Controller:** Hängt er noch per Bluetooth an einem anderen Rechner,
  bleibt er am Kabel dunkel. Dort zuerst trennen.
- **MIDI-Geräte** (Launch Control XL, MIDImix, …) kann unter Windows immer nur
  ein Programm gleichzeitig öffnen. Solange PadBridge ein Gerät liest, sieht
  die DAW es nicht direkt; sie bekommt es über den Port `PadBridge`. Hält ein
  anderes Programm das Gerät (DAW, MadMapper), übernimmt PadBridge es von
  selbst, sobald es dort freigegeben wird.
- **Tastatur/Maus** gehen als Scancodes raus, also nach Tastenposition wie auf
  Mac und Linux. In Fenster, die als Administrator laufen, kommen sie nicht
  an, außer PadBridge läuft ebenfalls als Administrator.
- **Virtuelles Gamepad** als Gegenstelle über ViGEmBus (siehe oben). Im
  Monitor erscheint es nicht als eigener Controller; die Pads der Gegenstelle
  stehen dort als „Pad n der Gegenstelle“.

---

## 4. Bedienung

Die Oberfläche hat fünf Tabs.

### Monitor

- Zeigt das gewählte Pad live: Sticks, Trigger, alle Tasten. Bei mehreren Pads
  oben auswählen.
- Pads, die eine Gegenstelle hierher sendet, stehen in derselben Auswahl als
  **„Pad n der Gegenstelle“**, nur zum Anschauen (Rumble und LEDs stellt der
  Rechner ein, an dem das Pad hängt).
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
   aktivieren. Unter Linux siehe auch [Teil 6](#midi-für-bitwig-und-andere-daws-snd-virmidi).
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

**Pads der Gegenstelle als MIDI-Controller:** Normalerweise macht der Rechner
MIDI, an dem das Pad hängt. Mit „Pads der Gegenstelle hier zu MIDI machen“
übernimmt das der empfangende Rechner, mit seiner eigenen Belegung: Bei „Pad“
das „Pad n der Gegenstelle“ wählen, im Bild leuchten dann dessen Tasten, und
die Belegung lässt sich hier bearbeiten. Praktisch, wenn das Pad an einem
Rechner hängt, den du nicht einrichten willst. Auf der Gegenstelle dann
„MIDI senden“ ausschalten, sonst kommt alles doppelt an.

**MIDI-Clock:** In der DAW Clock an **PadBridge** senden. Dann folgen die
Modi „Clock: Beat“ und „Clock: Takt“ im Monitor dem Tempo. Mit Ziel
„Netzwerk-Gegenstelle“ kommt die Clock der DAW auf der Gegenstelle zurück zum
Pad.

### MIDI-Geräte

Hier erscheinen die MIDI-Controller am Rechner (Novation Launch Control XL,
Akai MIDImix und APC mini mk2, Allen & Heath Xone:K2, Ableton Push 2,
Elektron Digitakt II, andere als Liste). Für die bekannten Geräte zeigt
PadBridge eine Zeichnung mit allen Werten live.

- **Umbelegen:** pro Regler oder Taste CC/Note, Nummer, Kanal und
  Momentary/Toggle. Das Ergebnis kommt über den Port **PadBridge** heraus.
- **LEDs von Reglern:** Farbe, Helligkeit, „Mitte hell“ oder Ampel
  (Launch Control XL).
- **LEDs von Tasten:** zwei Zustände, **gedrückt** (bzw. **an** bei Toggle) und
  **in Ruhe**, jeweils aus oder eine Farbe, wo möglich gedimmt. Dieselbe
  Einstellung in beiden Zeilen hält die Taste immer an; eine andere Farbe oder
  eine gedimmte Ruhe lässt sie beim Drücken wechseln. Was geht, hängt vom
  Gerät ab: Launch Control XL Rot/Amber/Gelb/Grün mit Dimmen, APC mini Pads
  Farben mit Dimmen, Xone:K2 Rot/Amber/Grün, Push 2 Pads Rot/Grün/Weiß und
  Tasten an/aus, MIDImix und einfarbige Tasten an/aus.
- **Endlos-Encoder** (K2, Push 2) haben keine Position; die Zeichnung zeigt
  beim Drehen einen leuchtenden Ring und den letzten Schritt („+2“, „−1“). In
  der DAW als relativen Encoder einstellen.
- **Push 2:** gezeichnet wird der „Live Port“. Läuft Ableton Live mit dem Push
  als Control Surface, steuert Live die LEDs und das Display; PadBridge zeigt
  dann nur an. Das Display des Push 2 läuft nicht über MIDI und bleibt leer.
- **Übers Netz:** Ein Gerät kann seine umbelegten Werte an die Gegenstelle
  schicken. Dort kommen sie auf einem eigenen Port „PadBridge <Gerät>“ an
  (unter Windows als loopMIDI-Port mit diesem Namen anlegen, Teil 3).

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

## 5. Zwei Rechner verbinden

### Schlüssel

Beide Seiten brauchen denselben geheimen Schlüssel. Er entsteht beim ersten
Start und liegt in der Datei `netpad.key` im Einstellungsordner (siehe
[Teil 7](#7-einstellungen); der Tab **Netzwerk** zeigt den Pfad).

1. Auf dem einen Rechner im Tab **Netzwerk** auf „Schlüssel kopieren“ klicken.
2. Den Text zum anderen Rechner bringen (zum Beispiel über eine Nachricht an
   dich selbst) und dort im Tab **Netzwerk** in das Feld „Schlüssel der
   anderen Seite hier einfügen“ einfügen, dann **Übernehmen**. PadBridge prüft
   ihn, speichert ihn (den alten als `netpad.key.alt` daneben) und verbindet
   sofort neu.
3. Läuft auf diesem Rechner auch der Hintergrund-Dienst (Teil 6), ihn danach
   neu starten: `systemctl --user restart padbridge`.

Ohne Fenster geht es auch über die Datei, zum Beispiel unter Linux:
```sh
install -m 600 /dev/stdin ~/.config/padbridge/netpad.key
# Schlüssel einfügen, Enter, dann Ctrl-D
```
Auf dem Mac ist es `~/Library/Application Support/PadBridge/netpad.key`,
unter Windows `%APPDATA%\PadBridge\netpad.key`.

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
`wine control` → Gamecontroller. Unter Windows (mit ViGEmBus) zeigt ihn
**Win+R → `joy.cpl`**. Das gilt auch für einen PS4-Controller in
der Hand: Übers Netz kommen die Werte im Xbox-Format an, und Spiele erkennen
das virtuelle Xbox-Pad ohne Einrichtung.

### Firewall

Gib UDP 48100 **nur** im LAN oder VPN frei und **nie** ins Internet. Die Pakete
sind signiert, aber nicht verschlüsselt. Beispiel mit UFW (Interface und
Subnetz anpassen, `ip -br a` zeigt sie):

```sh
sudo ufw allow from <lan-subnetz>/24 to any port 48100 proto udp
sudo ufw allow in on tailscale0 to any port 48100 proto udp     # falls Tailscale
```

Das gilt nur für den **empfangenden** Rechner: Senden geht ohne Regel, die
Antworten der Gegenstelle lässt die Firewall durch. Ubuntu-Systeme wie Zorin
haben UFW oft schon eingeschaltet (grafisch: „gufw“). Unter Windows genügt es,
beim ersten Start „private Netzwerke“ zu erlauben.

Unterwegs funktioniert die Verbindung über ein VPN wie Tailscale genauso: als
Host die Tailscale-Adresse der Gegenstelle eintragen (am besten als eigenes
Preset).

### Status-Webseite fürs Handy

Im Tab **Netzwerk** unter „Status-Webseite“ einschalten. PadBridge zeigt dann
Adresse, QR-Code und PIN. Mit dem Handy den QR-Code scannen: Die Seite zeigt
live Pads (auch die der Gegenstelle), Verbindung und Latenz, virtuelle Pads,
MIDI-Geräte und das Protokoll. Im Browser „Zum Home-Bildschirm“ macht daraus
eine App. Die Seite zeigt nur an, steuern lässt sich darüber nichts.

- **Im Heimnetz** (private Adressen wie 192.168.x.x, 10.x.x.x) öffnet sie
  sich ohne PIN.
- **Von außerhalb**, zum Beispiel über Tailscale, fragt sie nach der
  sechsstelligen PIN aus dem Netzwerk-Tab; der Browser merkt sich die
  Anmeldung. „Neue PIN“ meldet alle Browser ab. Nach fünf falschen Versuchen
  wartet sie 30 Sekunden.
- Port 48180 (TCP) muss in der Firewall offen sein, wie 48100 nur fürs LAN
  oder VPN. Die Seite ist unverschlüsseltes HTTP: **nie** per
  Portweiterleitung ins Internet stellen.

Läuft PadBridge als Dienst (Teil 6), gilt die Einstellung aus dem Fenster
nach einem Neustart des Dienstes auch für ihn. So zeigt das Handy den Stand
eines Rechners ohne Bildschirm.

### Sunshine/Moonlight

- In **Moonlight** unter Einstellungen den Gamepad-Support **ausschalten**.
  Sonst legt Sunshine ein zweites Pad an. Bild, Ton, Maus und Tastatur laufen
  weiter über Moonlight.
- Das PadBridge-Pad ist für Spiele ein ganz normales lokales Pad (xpad,
  `045e:028e`). Steam, SDL, Wine und Proton erkennen es ohne weitere Einstellungen.

---

## 6. Linux als Dienst und MIDI für Bitwig

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

Öffnest du daneben das Fenster, **übernimmt es das Empfangen**: Der Dienst
pausiert, und das Fenster zeigt alles von der Gegenstelle (Pads, MIDI-Geräte,
virtuelle Pads). Schließt du das Fenster, empfängt nach ein, zwei Sekunden
wieder der Dienst. Dabei werden die virtuellen Pads einmal neu angelegt; ein
laufendes Spiel sieht das wie kurzes Aus- und Einstecken.

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

## 7. Einstellungen

| System | Ordner |
|---|---|
| macOS | `~/Library/Application Support/PadBridge/` |
| Linux | `~/.config/padbridge/` |
| Windows | `%APPDATA%\PadBridge\` |

| Datei | Inhalt |
|---|---|
| `midi-mapping.json` | MIDI-Belegung, Kanal, Deadzones, Ziel |
| `netpad.json` | Netzwerk: Host, Port, Rumble, Empfangen, Host-Presets |
| `keymouse.json` | Tastatur/Maus-Belegung |
| `surfaces.json` | MIDI-Geräte: Umbelegung und LEDs |
| `web.json` | Status-Webseite: an/aus, Port, PIN |
| `netpad.key` | geheimer Schlüssel fürs Netzwerk (wird beim ersten Start erzeugt, `chmod 600`) |

Die Dateien sind mit der alten Swift-App und `padbridge-netpad` kompatibel.
Wer den Ordner auf einen neuen Rechner kopiert, nimmt alle Einstellungen mit,
auch den Schlüssel. Die Gegenstelle muss dann nicht neu gekoppelt werden.

---

## 8. Fehlersuche

| Symptom | Ursache / Lösung |
|---|---|
| Monitor zeigt keinen Controller (Mac) | Ein anderes Programm hält den Xbox-Controller (alte PadBridge-Version, 360Controller-Kext). USB-Kabel oder Hub wechseln; manche Kabel laden nur. |
| Monitor zeigt keinen Controller (Linux) | Benutzer nicht in der Gruppe `input`, oder seit dem Hinzufügen nicht neu angemeldet. |
| PS4-Controller erscheint am Mac als „(Bluetooth)“, obwohl er am Kabel hängt | Das Kabel lädt nur. Ein Datenkabel nehmen oder Bluetooth trennen (PS-Taste 10 s halten). |
| LED-Ring / Lightbar „Nicht verfügbar“ (Linux) | udev-Regel aus Teil 2 fehlt; nach dem Einrichten das Pad neu einstecken. |
| Status orange „keine Antwort“ | Auf der Gegenstelle ist „Empfangen“ aus oder PadBridge läuft nicht, die Firewall blockiert, falscher Host/Port oder **verschiedene Schlüssel** (der häufigste Fall bei einer neuen Installation, die ihren eigenen erzeugt hat). Pakete mit falscher Signatur werden stillschweigend verworfen: Schlüssel kopieren und auf der anderen Seite einfügen (Teil 5). |
| „Port 48100 nicht verfügbar“ | Der alte `padbridge-netpad` oder ein älteres PadBridge belegt den Port. Ein Dienst ab 0.5 gibt ihn dem Fenster selbst ab. |
| Zweiter Start öffnet kein zweites Fenster | Gewollt: Es holt das schon offene Fenster nach vorne. |
| „ViGEmBus ist nicht installiert“ (Windows) | Den Treiber installieren (Teil 3). MIDI und Tastatur/Maus gehen auch ohne. |
| Status-Webseite lädt nicht | Firewall: TCP 48180 freigeben (Windows: beim ersten Start private Netzwerke erlauben). Handy und Rechner müssen im selben Netz oder über Tailscale verbunden sein. |
| Pad auf Linux fehlt | Keine Schreibrechte auf `/dev/uinput` (Gruppe `input`, neu anmelden). |
| Zwei Pads im Spiel | Gamepad-Support in Moonlight ausschalten. |
| Bitwig sieht kein MIDI | snd-virmidi laden (Teil 6) und **VirMIDI** wählen, nicht „PadBridge“. |
| LED-Ring folgt der Clock nicht | Die DAW muss Clock an PadBridge bzw. VirMIDI senden, und das MIDI-Ziel muss zur DAW passen (dieser Rechner oder Gegenstelle). |
| Tastatur/Maus am Mac ohne Wirkung | Bedienungshilfen-Freigabe fehlt oder ist nach einem neuen Build veraltet (Teil 1). |
| „kein MIDI-Port ‚PadBridge‘“ (Windows) | In loopMIDI einen Port mit genau diesem Namen anlegen und PadBridge danach neu starten (Teil 3). |
| MIDI-Gerät „lässt sich nicht öffnen“ (Windows) | Ein anderes Programm (DAW, Hersteller-Editor) hat es schon geöffnet. Dort schließen oder in der DAW stattdessen den Port `PadBridge` nehmen. |
| Tasten bleiben hängen | Kommt nicht vor: Die Gegenstelle lässt nach 500 ms ohne Pakete alles los. Wenn doch, „aktiv“ aus- und wieder einschalten. |

---

## 9. Sicherheit

- PadBridge nimmt nur Pakete an, die mit dem gemeinsamen Schlüssel signiert
  sind (HMAC-SHA256), und verwirft wiederholte Pakete.
- Die Pakete sind **nicht verschlüsselt**. Wer im Netz mithört, sieht die
  Eingaben. Darum nur im LAN oder über ein VPN wie Tailscale betreiben und den
  Port nie ins Internet freigeben.
- Die Status-Webseite zeigt nur an. Außerhalb des Heimnetzes verlangt sie
  eine PIN; sie ist aber unverschlüsseltes HTTP und gehört nicht ins Internet.
- Der Schlüssel gehört nicht ins Repository und in keinen öffentlichen Chat. Einen neuen
  Schlüssel bekommst du, indem du `netpad.key` löschst und PadBridge neu
  startest. Danach muss er neu auf die Gegenstelle.

---

## 10. Die alte Swift-App und padbridge-netpad

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
# dann den Dienst aus Teil 6 einrichten
```

Zurück zum alten Daemon: `systemctl --user disable --now padbridge` und
`systemctl --user enable --now padbridge-netpad`.
