//! External MIDI devices (control surfaces such as the Launch Control XL):
//! splitting a byte stream into messages and keeping the last value of
//! every control a device has sent, for display.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{ButtonMode, MidiMessageKind, MidiTarget};

/// Splits a MIDI byte stream into complete messages, SysEx included.
/// Handles running status and real-time bytes in the middle of a message.
#[derive(Debug, Default, Clone)]
pub struct MidiStream {
    running: Option<u8>,
    pending: Vec<u8>,
    sysex: Option<Vec<u8>>,
}

/// Longer SysEx is dropped instead of growing without bound.
pub const MAX_SYSEX: usize = 4096;

impl MidiStream {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for &b in data {
            if b >= 0xF8 {
                out.push(vec![b]); // real-time: never interrupts anything else
                continue;
            }
            if let Some(sysex) = self.sysex.as_mut() {
                if b == 0xF7 {
                    let mut done = self.sysex.take().unwrap();
                    done.push(0xF7);
                    out.push(done);
                    continue;
                } else if b < 0x80 {
                    if sysex.len() < MAX_SYSEX {
                        sysex.push(b);
                    } else {
                        self.sysex = None;
                    }
                    continue;
                }
                self.sysex = None; // unterminated SysEx; the status byte starts something new
            }
            if b == 0xF0 {
                self.sysex = Some(vec![0xF0]);
                self.running = None;
                self.pending.clear();
            } else if b >= 0x80 {
                self.pending = vec![b];
                // System common messages cancel running status.
                self.running = (b < 0xF0).then_some(b);
                self.complete(&mut out);
            } else if !self.pending.is_empty() {
                self.pending.push(b);
                self.complete(&mut out);
            } else if let Some(status) = self.running {
                self.pending = vec![status, b];
                self.complete(&mut out);
            }
            // else: stray data byte, dropped
        }
        out
    }

    fn complete(&mut self, out: &mut Vec<Vec<u8>>) {
        if self.pending.len() >= crate::protocol::midi_len(self.pending[0]) {
            out.push(std::mem::take(&mut self.pending));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SurfaceKind {
    Note,
    #[default]
    Cc,
    Program,
    Pressure,
    Pitch,
}

/// One control of a device: message kind, channel 0..=15 and number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub struct SurfaceKey {
    pub kind: SurfaceKind,
    pub channel: u8,
    pub number: u8,
}

impl SurfaceKey {
    pub fn cc(channel: u8, number: u8) -> Self {
        SurfaceKey { kind: SurfaceKind::Cc, channel, number }
    }

    pub fn note(channel: u8, number: u8) -> Self {
        SurfaceKey { kind: SurfaceKind::Note, channel, number }
    }

    /// Highest value: 16383 for pitch bend, 127 otherwise.
    pub fn max_value(&self) -> u16 {
        if self.kind == SurfaceKind::Pitch {
            16383
        } else {
            127
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SurfaceValue {
    /// Last value or velocity.
    pub value: u16,
    /// Notes: held down right now. Everything else: true.
    pub on: bool,
}

/// What a channel message sets, or None for anything else (SysEx, clock...).
pub fn classify(message: &[u8]) -> Option<(SurfaceKey, SurfaceValue)> {
    let status = *message.first()?;
    if !(0x80..0xF0).contains(&status) || message.len() < crate::protocol::midi_len(status) {
        return None;
    }
    let channel = status & 0x0F;
    let d1 = message.get(1).copied().unwrap_or(0);
    let d2 = message.get(2).copied().unwrap_or(0);
    let key = |kind, number| SurfaceKey { kind, channel, number };
    Some(match status & 0xF0 {
        0x80 => (key(SurfaceKind::Note, d1), SurfaceValue { value: d2 as u16, on: false }),
        0x90 => (key(SurfaceKind::Note, d1), SurfaceValue { value: d2 as u16, on: d2 > 0 }),
        0xA0 => (key(SurfaceKind::Pressure, d1), SurfaceValue { value: d2 as u16, on: true }),
        0xB0 => (key(SurfaceKind::Cc, d1), SurfaceValue { value: d2 as u16, on: true }),
        0xC0 => (key(SurfaceKind::Program, 0), SurfaceValue { value: d1 as u16, on: true }),
        0xD0 => (key(SurfaceKind::Pressure, 128), SurfaceValue { value: d1 as u16, on: true }),
        _ => (key(SurfaceKind::Pitch, 0), SurfaceValue { value: d1 as u16 | (d2 as u16) << 7, on: true }),
    })
}

/// Last value of every control one device has sent.
#[derive(Debug, Clone, Default)]
pub struct SurfaceState {
    pub values: BTreeMap<SurfaceKey, SurfaceValue>,
    /// Channel of the latest channel message (the Launch Control XL sends
    /// each template on its own channel).
    pub channel: Option<u8>,
}

impl SurfaceState {
    /// Returns the key that changed, if the message was a channel message.
    pub fn apply(&mut self, message: &[u8]) -> Option<SurfaceKey> {
        let (key, value) = classify(message)?;
        self.values.insert(key, value);
        self.channel = Some(key.channel);
        Some(key)
    }

    pub fn get(&self, key: SurfaceKey) -> Option<SurfaceValue> {
        self.values.get(&key).copied()
    }
}

// MARK: Remapping

/// LED colours of the Launch Control XL (red and green LEDs mixed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LedColour {
    #[default]
    Off,
    Red,
    Amber,
    Yellow,
    Green,
}

impl LedColour {
    pub const ALL: [LedColour; 5] = [LedColour::Off, LedColour::Red, LedColour::Amber, LedColour::Yellow, LedColour::Green];

    /// Red and green brightness 0..=3 at full level.
    pub fn red_green(self) -> (u8, u8) {
        match self {
            LedColour::Off => (0, 0),
            LedColour::Red => (3, 0),
            LedColour::Amber => (3, 3),
            LedColour::Yellow => (2, 3),
            LedColour::Green => (0, 3),
        }
    }
}

/// How a knob's LED shows its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LedMode {
    /// Always the chosen colour.
    #[default]
    Fixed,
    /// The chosen colour, brighter the higher the value.
    Brightness,
    /// Green at 0, yellow in the middle, red at 127 (the colour is ignored).
    Meter,
    /// The chosen colour, brightest at 64, off at 0 and 127 (bipolar knobs).
    Centre,
}

/// Red and green 0..=3 for a value 0..=127 in `LedMode::Meter`: seven steps
/// green, yellow-green, yellow, amber, orange, red-orange, red.
pub fn meter_colour(value: u16) -> (u8, u8) {
    const STEPS: [(u8, u8); 7] = [(0, 3), (1, 3), (2, 3), (3, 3), (3, 2), (3, 1), (3, 0)];
    STEPS[((value.min(127) as usize * 6) + 63) / 127]
}

/// What one control of a device is turned into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SurfaceBinding {
    /// What the device sends.
    pub input: SurfaceKey,
    /// A button (press/release) rather than a knob or fader.
    pub button: bool,
    pub kind: MidiMessageKind,
    /// Output channel 0..=15.
    pub channel: u8,
    pub number: u8,
    /// Buttons only.
    pub mode: ButtonMode,
    /// Knobs: the LED colour. Buttons: the colour while pressed (momentary)
    /// or switched on (toggle).
    pub led: LedColour,
    /// Knobs only.
    pub led_mode: LedMode,
    /// Buttons: show `led` dimmed (devices that can dim).
    pub led_dim: bool,
    /// Buttons: the colour at rest; Off keeps the LED dark as before.
    pub led_idle: LedColour,
    /// Buttons: show `led_idle` dimmed.
    pub led_idle_dim: bool,
}

impl SurfaceBinding {
    /// Sends the same message the device sends.
    pub fn passthrough(input: SurfaceKey, button: bool) -> Self {
        SurfaceBinding {
            input,
            button,
            kind: if input.kind == SurfaceKind::Note { MidiMessageKind::Note } else { MidiMessageKind::Cc },
            channel: input.channel,
            number: input.number,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DeviceMapping {
    /// Remap and send to the "PadBridge" port.
    pub enabled: bool,
    /// This machine's "PadBridge" port, or a port named after the device on the peer.
    pub target: MidiTarget,
    pub bindings: Vec<SurfaceBinding>,
}

impl DeviceMapping {
    pub fn binding(&self, input: SurfaceKey) -> Option<&SurfaceBinding> {
        self.bindings.iter().find(|b| b.input == input)
    }

    /// The binding for `input`, created as a pass-through if missing.
    pub fn binding_mut(&mut self, input: SurfaceKey, button: bool) -> &mut SurfaceBinding {
        if let Some(i) = self.bindings.iter().position(|b| b.input == input) {
            return &mut self.bindings[i];
        }
        self.bindings.push(SurfaceBinding::passthrough(input, button));
        self.bindings.last_mut().unwrap()
    }
}

/// Remapping of every external device, by device name (`surfaces.json`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SurfaceMapping {
    pub devices: BTreeMap<String, DeviceMapping>,
}

/// Applies a `DeviceMapping` to a device's messages; remembers toggles.
#[derive(Debug, Clone, Default)]
pub struct SurfaceMapper {
    /// Toggle buttons that are on.
    pub toggled: BTreeSet<SurfaceKey>,
}

impl SurfaceMapper {
    /// Messages to send for one message from the device. Unbound controls
    /// pass through unchanged; SysEx and system messages are not forwarded.
    pub fn process(&mut self, mapping: &DeviceMapping, message: &[u8]) -> Vec<Vec<u8>> {
        let Some((key, v)) = classify(message) else { return Vec::new() };
        let Some(b) = mapping.binding(key) else { return vec![message.to_vec()] };
        let (ch, n) = (b.channel & 0x0F, b.number & 0x7F);
        if b.button || key.kind == SurfaceKind::Note {
            let pressed = if key.kind == SurfaceKind::Note { v.on } else { v.value > 0 };
            let on = match b.mode {
                ButtonMode::Momentary => pressed,
                ButtonMode::Toggle => {
                    if !pressed {
                        return Vec::new();
                    }
                    if !self.toggled.remove(&key) {
                        self.toggled.insert(key);
                    }
                    self.toggled.contains(&key)
                }
            };
            let velocity = if key.kind == SurfaceKind::Note && v.on && b.mode == ButtonMode::Momentary { v.value as u8 } else { 127 };
            return vec![match (b.kind, on) {
                (MidiMessageKind::Cc, on) => vec![0xB0 | ch, n, if on { 127 } else { 0 }],
                (MidiMessageKind::Note, true) => vec![0x90 | ch, n, velocity.max(1)],
                (MidiMessageKind::Note, false) => vec![0x80 | ch, n, 0],
            }];
        }
        let value = if key.max_value() > 127 { (v.value >> 7) as u8 } else { v.value as u8 };
        vec![match b.kind {
            MidiMessageKind::Cc => vec![0xB0 | ch, n, value],
            MidiMessageKind::Note => vec![0x90 | ch, n, value],
        }]
    }

    /// Forgets toggles of controls that are no longer toggles.
    pub fn retain(&mut self, mapping: &DeviceMapping) {
        self.toggled.retain(|k| mapping.binding(*k).is_some_and(|b| b.mode == ButtonMode::Toggle));
    }
}

/// One line for a message log, e.g. "K1 CC 13 = 64".
pub fn describe(message: &[u8]) -> String {
    if message.first() == Some(&0xF0) {
        let head: Vec<String> = message.iter().take(12).map(|b| format!("{b:02X}")).collect();
        let more = if message.len() > 12 { " …" } else { "" };
        return format!("SysEx {} Bytes: {}{more}", message.len(), head.join(" "));
    }
    let Some((key, v)) = classify(message) else {
        let bytes: Vec<String> = message.iter().map(|b| format!("{b:02X}")).collect();
        return bytes.join(" ");
    };
    let ch = key.channel + 1;
    match key.kind {
        SurfaceKind::Note if v.on => format!("K{ch} Note {} an ({})", key.number, v.value),
        SurfaceKind::Note => format!("K{ch} Note {} aus", key.number),
        SurfaceKind::Cc => format!("K{ch} CC {} = {}", key.number, v.value),
        SurfaceKind::Program => format!("K{ch} Programm {}", v.value),
        SurfaceKind::Pressure if key.number == 128 => format!("K{ch} Druck {}", v.value),
        SurfaceKind::Pressure => format!("K{ch} Druck Note {} = {}", key.number, v.value),
        SurfaceKind::Pitch => format!("K{ch} Pitch {}", v.value as i32 - 8192),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_stream_with_running_status_sysex_and_realtime() {
        let mut s = MidiStream::new();
        let out = s.feed(&[0xB0, 13, 64, 14, 0xF8, 70, 0xF0, 0x00, 0x20, 0xF8, 0x29, 0xF7, 0x90, 41]);
        assert_eq!(
            out,
            vec![
                vec![0xB0, 13, 64],
                vec![0xF8],
                vec![0xB0, 14, 70],
                vec![0xF8],
                vec![0xF0, 0x00, 0x20, 0x29, 0xF7],
            ]
        );
        // The note completes in the next chunk.
        assert_eq!(s.feed(&[127]), vec![vec![0x90, 41, 127]]);
    }

    #[test]
    fn note_on_with_zero_velocity_is_off() {
        let mut st = SurfaceState::default();
        st.apply(&[0x98, 41, 127]);
        assert!(st.get(SurfaceKey::note(8, 41)).unwrap().on);
        st.apply(&[0x98, 41, 0]);
        assert!(!st.get(SurfaceKey::note(8, 41)).unwrap().on);
        assert_eq!(st.channel, Some(8));
    }

    #[test]
    fn remaps_knobs_and_toggles_buttons() {
        let mut m = DeviceMapping { enabled: true, ..Default::default() };
        let knob = SurfaceKey::cc(8, 13);
        let b = m.binding_mut(knob, false);
        b.number = 20;
        b.channel = 0;
        let button = SurfaceKey::note(8, 41);
        let b = m.binding_mut(button, true);
        b.kind = MidiMessageKind::Cc;
        b.number = 64;
        b.mode = ButtonMode::Toggle;
        let mut mapper = SurfaceMapper::default();
        assert_eq!(mapper.process(&m, &[0xB8, 13, 99]), vec![vec![0xB0, 20, 99]]);
        // Unbound: unchanged.
        assert_eq!(mapper.process(&m, &[0xB8, 14, 5]), vec![vec![0xB8, 14, 5]]);
        assert_eq!(mapper.process(&m, &[0x98, 41, 127]), vec![vec![0xB8, 64, 127]]);
        assert!(mapper.process(&m, &[0x88, 41, 0]).is_empty());
        assert_eq!(mapper.process(&m, &[0x98, 41, 127]), vec![vec![0xB8, 64, 0]]);
        assert!(mapper.toggled.is_empty());
    }

    #[test]
    fn describes_messages() {
        assert_eq!(describe(&[0xB0, 13, 64]), "K1 CC 13 = 64");
        assert_eq!(describe(&[0x90, 41, 0]), "K1 Note 41 aus");
        assert_eq!(describe(&[0xE0, 0, 64]), "K1 Pitch 0");
        assert_eq!(describe(&[0xF0, 1, 0xF7]), "SysEx 3 Bytes: F0 01 F7");
    }
}
