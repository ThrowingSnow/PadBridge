//! Control surfaces PadBridge knows by name: their controls (factory
//! layout), where they sit in the drawing, and how their LEDs are set.
//!
//! - Novation Launch Control XL: every template sends on its own channel
//!   (user 1-8 on channels 1-8, factory 1-8 on 9-16) with the same numbers
//!   unless changed with Novation Components. Red/green LEDs under the 24
//!   knobs and in the buttons, set per template with SysEx
//!   `F0 00 20 29 02 11 78 <template> <index> <value> F7`.
//! - Akai MIDImix: channel 1; the Mute, Rec Arm and Bank buttons have
//!   single-colour LEDs, switched with a note on (127 on, 0 off).

use std::collections::BTreeSet;

use super::surface::{meter_colour, DeviceMapping, LedColour, LedMode, SurfaceKey, SurfaceState};
use super::ButtonMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Model {
    LaunchControlXl,
    MidiMix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CtlKind {
    Knob,
    Fader,
    Button,
}

/// One control: what it sends, its LED and its place in the drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct Ctl {
    pub kind: CtlKind,
    /// Sends a note (else a CC; buttons sending CCs use 127/0).
    pub note: bool,
    pub number: u8,
    pub name: String,
    /// Index into the model's LEDs.
    pub led: Option<usize>,
    /// Centre and size in drawing units.
    pub at: (f32, f32),
    pub size: (f32, f32),
    /// Text printed on the button.
    pub caption: Option<&'static str>,
    /// Drawn as a triangle: 0 up, 1 down, 2 left, 3 right.
    pub arrow: Option<u8>,
    /// Shown in the settings, e.g. which colour the LED has.
    pub led_hint: Option<&'static str>,
}

impl Ctl {
    pub fn input(&self, channel: u8) -> SurfaceKey {
        if self.note {
            SurfaceKey::note(channel, self.number)
        } else {
            SurfaceKey::cc(channel, self.number)
        }
    }

    pub fn is_button(&self) -> bool {
        self.kind == CtlKind::Button
    }
}

/// A caption in the drawing; `centred` or left-aligned at `x`.
pub struct Caption {
    pub text: &'static str,
    pub x: f32,
    pub y: f32,
    pub centred: bool,
}

impl Model {
    pub fn of(device_name: &str) -> Option<Model> {
        let lower = device_name.to_lowercase();
        if lower.contains("launch control xl") {
            Some(Model::LaunchControlXl)
        } else if lower.contains("midi mix") || lower.contains("midimix") {
            Some(Model::MidiMix)
        } else {
            None
        }
    }

    /// Channel assumed before the device has sent anything.
    pub fn default_channel(self) -> u8 {
        match self {
            Model::LaunchControlXl => 8, // factory template 1, the power-on default
            Model::MidiMix => 0,
        }
    }

    /// Red and green LEDs (else single-colour, only on/off).
    pub fn two_colour_leds(self) -> bool {
        self == Model::LaunchControlXl
    }

    /// Size of the drawing in units.
    pub fn body(self) -> (f32, f32) {
        match self {
            Model::LaunchControlXl => (660.0, 424.0),
            Model::MidiMix => (660.0, 440.0),
        }
    }

    /// "Factory 1" etc., for devices with templates.
    pub fn template_name(self, channel: u8) -> Option<String> {
        match self {
            Model::LaunchControlXl if channel < 8 => Some(format!("User {}", channel + 1)),
            Model::LaunchControlXl => Some(format!("Factory {}", channel - 7)),
            Model::MidiMix => None,
        }
    }

    /// The template the device reports after switching.
    pub fn template_change(self, message: &[u8]) -> Option<u8> {
        match (self, message) {
            (Model::LaunchControlXl, [0xF0, 0x00, 0x20, 0x29, 0x02, 0x11, 0x77, t, 0xF7]) if *t < 16 => Some(*t),
            _ => None,
        }
    }

    pub fn captions(self) -> Vec<Caption> {
        let c = |text, x, y, centred| Caption { text, x, y, centred };
        match self {
            Model::LaunchControlXl => vec![
                c("Send A", 10.0, LCXL_KNOB_Y[0], false),
                c("Send B", 10.0, LCXL_KNOB_Y[1], false),
                c("Pan", 10.0, LCXL_KNOB_Y[2], false),
                c("Send Select", 590.0, 26.0, true),
                c("Track Select", 590.0, 128.0, true),
            ],
            Model::MidiMix => vec![
                c("Mute", 10.0, MIX_MUTE_Y, false),
                c("Rec Arm", 10.0, MIX_ARM_Y, false),
                c("Bank", 590.0, 98.0, true),
                c("Master", 590.0, 238.0, true),
            ],
        }
    }

    pub fn controls(self) -> Vec<Ctl> {
        match self {
            Model::LaunchControlXl => lcxl_controls(),
            Model::MidiMix => midimix_controls(),
        }
    }

    pub fn led_count(self) -> usize {
        match self {
            Model::LaunchControlXl => 48,
            Model::MidiMix => 18,
        }
    }

    /// The message that sets LED `index` to `value` (from `leds`).
    pub fn led_message(self, channel: u8, index: usize, value: u8) -> Vec<u8> {
        match self {
            Model::LaunchControlXl => vec![0xF0, 0x00, 0x20, 0x29, 0x02, 0x11, 0x78, channel & 0x0F, index as u8, value, 0xF7],
            Model::MidiMix => vec![0x90 | (channel & 0x0F), MIX_LED_NOTES[index], value],
        }
    }

    /// Messages that turn every LED off.
    pub fn reset(self, channel: u8) -> Vec<Vec<u8>> {
        match self {
            Model::LaunchControlXl => vec![vec![0xB0 | (channel & 0x0F), 0, 0]],
            Model::MidiMix => (0..self.led_count()).map(|i| self.led_message(channel, i, 0)).collect(),
        }
    }

    /// Wanted LED values for the controls on `channel`.
    pub fn leds(self, mapping: &DeviceMapping, state: &SurfaceState, toggled: &BTreeSet<SurfaceKey>, channel: u8) -> Vec<u8> {
        let mut out = vec![self.encode(0, 0); self.led_count()];
        for ctl in self.controls() {
            let Some(index) = ctl.led else { continue };
            let key = ctl.input(channel);
            let Some(b) = mapping.binding(key) else { continue };
            let value = state.get(key);
            let (r, g) = if ctl.is_button() {
                let lit = match b.mode {
                    ButtonMode::Toggle => toggled.contains(&key),
                    ButtonMode::Momentary => value.is_some_and(|v| v.on && v.value > 0),
                };
                scaled(b.led, if lit { 3 } else { 0 })
            } else {
                let v = value.map_or(0, |v| v.value);
                match b.led_mode {
                    LedMode::Fixed => scaled(b.led, 3),
                    // Never fully dark, so a closed knob still shows its colour.
                    LedMode::Brightness => scaled(b.led, 1 + ((v as u32 * 2 + 63) / 127) as u8),
                    LedMode::Centre => scaled(b.led, centre_level(v)),
                    LedMode::Meter => meter_colour(v),
                }
            };
            out[index] = self.encode(r, g);
        }
        out
    }

    fn encode(self, red: u8, green: u8) -> u8 {
        match self {
            // Plus the "copy and clear" flags the device expects.
            Model::LaunchControlXl => (green.min(3) << 4) | red.min(3) | 0x0C,
            Model::MidiMix => {
                if red > 0 || green > 0 {
                    127
                } else {
                    0
                }
            }
        }
    }
}

/// Colour at brightness `level` 0..=3, as red and green 0..=3.
pub fn scaled(colour: LedColour, level: u8) -> (u8, u8) {
    let (r, g) = colour.red_green();
    let scale = |x: u8| (x as u16 * level.min(3) as u16).div_ceil(3) as u8;
    (scale(r), scale(g))
}

/// `LedMode::Centre`: full at 64, dimmer towards the ends, off at 0 and 127.
pub fn centre_level(value: u16) -> u8 {
    let distance = (value.min(127) as i32 - 64).unsigned_abs().min(64);
    (3 - (distance * 3 + 21) / 64).min(3) as u8
}

// MARK: Launch Control XL

const LCXL_SEND_A: [u8; 8] = [13, 14, 15, 16, 17, 18, 19, 20];
const LCXL_SEND_B: [u8; 8] = [29, 30, 31, 32, 33, 34, 35, 36];
const LCXL_PAN: [u8; 8] = [49, 50, 51, 52, 53, 54, 55, 56];
const LCXL_FADERS: [u8; 8] = [77, 78, 79, 80, 81, 82, 83, 84];
const LCXL_FOCUS: [u8; 8] = [41, 42, 43, 44, 57, 58, 59, 60];
const LCXL_CONTROL: [u8; 8] = [73, 74, 75, 76, 89, 90, 91, 92];
const LCXL_KNOB_Y: [f32; 3] = [48.0, 110.0, 172.0];

fn lcxl_controls() -> Vec<Ctl> {
    let x = |c: usize| 62.0 + c as f32 * 58.0;
    let base = |kind, note, number, name: String, led, at, size| Ctl {
        kind,
        note,
        number,
        name,
        led,
        at,
        size,
        caption: None,
        arrow: None,
        led_hint: None,
    };
    let mut out = Vec::new();
    for (row, (numbers, row_name)) in [(LCXL_SEND_A, "Send A"), (LCXL_SEND_B, "Send B"), (LCXL_PAN, "Pan")].iter().enumerate() {
        for c in 0..8 {
            out.push(base(
                CtlKind::Knob,
                false,
                numbers[c],
                format!("{row_name} {}", c + 1),
                Some(row * 8 + c),
                (x(c), LCXL_KNOB_Y[row]),
                (38.0, 38.0),
            ));
        }
    }
    for c in 0..8 {
        out.push(base(CtlKind::Fader, false, LCXL_FADERS[c], format!("Fader {}", c + 1), None, (x(c), 276.0), (30.0, 122.0)));
    }
    for c in 0..8 {
        out.push(base(CtlKind::Button, true, LCXL_FOCUS[c], format!("Track Focus {}", c + 1), Some(24 + c), (x(c), 364.0), (46.0, 24.0)));
    }
    for c in 0..8 {
        out.push(base(CtlKind::Button, true, LCXL_CONTROL[c], format!("Track Control {}", c + 1), Some(32 + c), (x(c), 396.0), (46.0, 24.0)));
    }
    for (i, (note, caption)) in [(105, "Device"), (106, "Mute"), (107, "Solo"), (108, "Rec Arm")].into_iter().enumerate() {
        let mut ctl = base(CtlKind::Button, true, note, caption.into(), Some(40 + i), (590.0, 240.0 + i as f32 * 36.0), (76.0, 26.0));
        ctl.caption = Some(caption);
        ctl.led_hint = Some("Diese Taste hat nur eine gelbe LED.");
        out.push(ctl);
    }
    let arrows = [
        (104, "Send Select hoch", (590.0, 50.0)),
        (105, "Send Select runter", (590.0, 82.0)),
        (106, "Track Select links", (568.0, 156.0)),
        (107, "Track Select rechts", (612.0, 156.0)),
    ];
    for (i, (cc, name, at)) in arrows.into_iter().enumerate() {
        let mut ctl = base(CtlKind::Button, false, cc, name.into(), Some(44 + i), at, (38.0, 26.0));
        ctl.arrow = Some(i as u8);
        ctl.led_hint = Some("Diese Taste hat nur eine rote LED.");
        out.push(ctl);
    }
    out
}

// MARK: MIDImix

const MIX_KNOBS: [[u8; 3]; 8] =
    [[16, 17, 18], [20, 21, 22], [24, 25, 26], [28, 29, 30], [46, 47, 48], [50, 51, 52], [54, 55, 56], [58, 59, 60]];
const MIX_FADERS: [u8; 8] = [19, 23, 27, 31, 49, 53, 57, 61];
const MIX_MASTER: u8 = 62;
const MIX_MUTE: [u8; 8] = [1, 4, 7, 10, 13, 16, 19, 22];
const MIX_ARM: [u8; 8] = [3, 6, 9, 12, 15, 18, 21, 24];
const MIX_BANK_LEFT: u8 = 25;
const MIX_BANK_RIGHT: u8 = 26;
const MIX_SOLO: u8 = 27;
/// LED index -> note: 8 mute, 8 rec arm, bank left, bank right.
const MIX_LED_NOTES: [u8; 18] = [1, 4, 7, 10, 13, 16, 19, 22, 3, 6, 9, 12, 15, 18, 21, 24, 25, 26];
const MIX_MUTE_Y: f32 = 206.0;
const MIX_ARM_Y: f32 = 238.0;

fn midimix_controls() -> Vec<Ctl> {
    let x = |c: usize| 92.0 + c as f32 * 60.0;
    let base = |kind, note, number, name: String, led, at, size| Ctl {
        kind,
        note,
        number,
        name,
        led,
        at,
        size,
        caption: None,
        arrow: None,
        led_hint: None,
    };
    let mut out = Vec::new();
    for row in 0..3 {
        for c in 0..8 {
            out.push(base(
                CtlKind::Knob,
                false,
                MIX_KNOBS[c][row],
                format!("Kanal {} Regler {}", c + 1, row + 1),
                None,
                (x(c), 44.0 + row as f32 * 54.0),
                (38.0, 38.0),
            ));
        }
    }
    for c in 0..8 {
        let mut mute = base(CtlKind::Button, true, MIX_MUTE[c], format!("Mute {}", c + 1), Some(c), (x(c), MIX_MUTE_Y), (44.0, 24.0));
        mute.led_hint = Some("Nur an/aus (die Farbe gibt das Gerät vor).");
        out.push(mute);
        let mut arm = base(CtlKind::Button, true, MIX_ARM[c], format!("Rec Arm {}", c + 1), Some(8 + c), (x(c), MIX_ARM_Y), (44.0, 24.0));
        arm.led_hint = Some("Nur an/aus (die Farbe gibt das Gerät vor).");
        out.push(arm);
    }
    for c in 0..8 {
        out.push(base(CtlKind::Fader, false, MIX_FADERS[c], format!("Fader {}", c + 1), None, (x(c), 340.0), (30.0, 136.0)));
    }
    out.push(base(CtlKind::Fader, false, MIX_MASTER, "Master".into(), None, (590.0, 340.0), (30.0, 136.0)));
    for (i, note) in [MIX_BANK_LEFT, MIX_BANK_RIGHT].into_iter().enumerate() {
        let name = if i == 0 { "Bank links" } else { "Bank rechts" };
        let mut ctl = base(CtlKind::Button, true, note, name.into(), Some(16 + i), (568.0 + i as f32 * 44.0, 124.0), (38.0, 26.0));
        ctl.arrow = Some(2 + i as u8);
        ctl.led_hint = Some("Nur an/aus (die Farbe gibt das Gerät vor).");
        out.push(ctl);
    }
    let mut solo = base(CtlKind::Button, true, MIX_SOLO, "Solo".into(), None, (590.0, 176.0), (76.0, 26.0));
    solo.caption = Some("Solo");
    out.push(solo);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lcxl_led_values_match_the_programmers_reference() {
        let m = Model::LaunchControlXl;
        assert_eq!(m.encode(0, 0), 0x0C);
        assert_eq!(m.encode(3, 0), 0x0F);
        assert_eq!(m.encode(0, 3), 0x3C);
        assert_eq!(m.encode(3, 3), 0x3F);
        assert_eq!(m.led_message(8, 24, 0x3C), vec![0xF0, 0x00, 0x20, 0x29, 0x02, 0x11, 0x78, 8, 24, 0x3C, 0xF7]);
    }

    #[test]
    fn centre_is_brightest_in_the_middle() {
        assert_eq!(centre_level(64), 3);
        assert_eq!(centre_level(0), 0);
        assert_eq!(centre_level(127), 0);
        assert!(centre_level(40) > 0 && centre_level(40) < 3);
    }

    #[test]
    fn toggled_button_lights() {
        let m = Model::LaunchControlXl;
        let ctl = m.controls().into_iter().find(|c| c.name == "Track Focus 1").unwrap();
        let mut mapping = DeviceMapping { enabled: true, ..Default::default() };
        let b = mapping.binding_mut(ctl.input(8), true);
        b.mode = ButtonMode::Toggle;
        b.led = LedColour::Green;
        let mut toggled = BTreeSet::new();
        let st = SurfaceState::default();
        assert_eq!(m.leds(&mapping, &st, &toggled, 8)[24], 0x0C);
        toggled.insert(ctl.input(8));
        assert_eq!(m.leds(&mapping, &st, &toggled, 8)[24], 0x3C);
    }

    #[test]
    fn midimix_mute_led_is_a_note() {
        let m = Model::MidiMix;
        let ctl = m.controls().into_iter().find(|c| c.name == "Mute 2").unwrap();
        assert_eq!(m.led_message(0, ctl.led.unwrap(), 127), vec![0x90, 4, 127]);
        assert_eq!(Model::of("MIDI Mix"), Some(Model::MidiMix));
    }

    #[test]
    fn leds_are_unique_per_model() {
        for m in [Model::LaunchControlXl, Model::MidiMix] {
            let mut seen = BTreeSet::new();
            for ctl in m.controls() {
                if let Some(i) = ctl.led {
                    assert!(i < m.led_count() && seen.insert(i), "{m:?} {}", ctl.name);
                }
            }
        }
    }
}
