//! Wire format of the wired Xbox 360 controller and its licensed clones.
//! Pure byte <-> model conversion, shared by the libusb backend (macOS) and tests.
//!
//! Reference: Linux `drivers/input/joystick/xpad.c` (XTYPE_XBOX360).

use crate::controls::{PadAxis, PadButton};
use crate::device::PlayerIndicator;
use crate::state::ControllerState;

/// Gamepad interface: vendor-specific class, subclass 0x5D, protocol 0x01.
/// Interfaces 1-3 (headset, security) use other protocols.
pub const INTERFACE_CLASS: u8 = 0xFF;
pub const INTERFACE_SUBCLASS: u8 = 0x5D;
pub const INTERFACE_PROTOCOL: u8 = 0x01;

pub const INPUT_REPORT_LEN: usize = 20;

/// Known wired 360-protocol pads: (vendor, product, name). Same list as KnownDevices.swift.
pub const KNOWN_DEVICES: &[(u16, u16, &str)] = &[
    (0x045e, 0x028e, "Xbox 360 Controller"),
    (0x045e, 0x028f, "Xbox 360 Controller (Play & Charge)"),
    (0x046d, 0xc21d, "Logitech Gamepad F310"),
    (0x046d, 0xc21e, "Logitech Gamepad F510"),
    (0x046d, 0xc21f, "Logitech Gamepad F710"),
    (0x0738, 0x4716, "Mad Catz Wired Xbox 360 Controller"),
    (0x0738, 0x4726, "Mad Catz Xbox 360 Controller"),
    (0x1bad, 0xf016, "Mad Catz Xbox 360 Controller"),
    (0x0e6f, 0x0105, "HSM3 Xbox 360 Dancepad"),
    (0x0e6f, 0x0113, "Afterglow AX.1 Gamepad"),
    (0x0e6f, 0x0201, "Pelican PL-3601 'TSZ' Wired Xbox 360 Controller"),
    (0x0e6f, 0x0213, "Afterglow Gamepad for Xbox 360"),
    (0x0f0d, 0x000a, "Hori Co. DOA4 FightStick"),
    (0x0f0d, 0x000c, "Hori PadEX Turbo"),
    (0x1532, 0x0037, "Razer Sabertooth"),
    (0x24c6, 0x5300, "PowerA MINI PROEX Controller"),
    (0x24c6, 0x5303, "Xbox Airflo wired controller"),
];

/// Byte 2 | byte 3 << 8 of the input report. Also the button layout of the
/// network protocol, which reuses it.
pub const BUTTON_BITS: [(u16, PadButton); 15] = [
    (0x0001, PadButton::DpadUp),
    (0x0002, PadButton::DpadDown),
    (0x0004, PadButton::DpadLeft),
    (0x0008, PadButton::DpadRight),
    (0x0010, PadButton::Start),
    (0x0020, PadButton::Back),
    (0x0040, PadButton::LeftStick),
    (0x0080, PadButton::RightStick),
    (0x0100, PadButton::LeftShoulder),
    (0x0200, PadButton::RightShoulder),
    (0x0400, PadButton::Guide),
    (0x1000, PadButton::A),
    (0x2000, PadButton::B),
    (0x4000, PadButton::X),
    (0x8000, PadButton::Y),
];

pub fn buttons_from_bits(bits: u16, state: &mut ControllerState) {
    for (mask, button) in BUTTON_BITS {
        state.set_pressed(button, bits & mask != 0);
    }
}

pub fn bits_from_buttons(state: &ControllerState) -> u16 {
    BUTTON_BITS.iter().filter(|(_, b)| state.is_pressed(*b)).fold(0, |acc, (mask, _)| acc | mask)
}

/// i16 -> -1..=1. -32768 is clamped so both directions reach exactly 1.
pub fn stick_from_i16(raw: i16) -> f32 {
    (raw as f32 / 32767.0).max(-1.0)
}

pub fn stick_to_i16(v: f32) -> i16 {
    (v.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

pub fn trigger_to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Parses an input report; None for other message types
/// (LED status 0x01, rumble/headset status 0x02/0x03, ...).
///
/// Layout: [0]=0x00 type, [1]=0x14 length, [2..3] buttons, [4] LT, [5] RT,
/// [6..13] LX, LY, RX, RY as little-endian i16 (up/right positive).
pub fn parse_input(report: &[u8]) -> Option<ControllerState> {
    if report.len() < INPUT_REPORT_LEN || report[0] != 0x00 || report[1] != 0x14 {
        return None;
    }
    let mut state = ControllerState::default();
    buttons_from_bits(u16::from_le_bytes([report[2], report[3]]), &mut state);
    state.set_axis(PadAxis::LeftTrigger, report[4] as f32 / 255.0);
    state.set_axis(PadAxis::RightTrigger, report[5] as f32 / 255.0);
    let stick = |i: usize| stick_from_i16(i16::from_le_bytes([report[i], report[i + 1]]));
    state.set_axis(PadAxis::LeftX, stick(6));
    state.set_axis(PadAxis::LeftY, stick(8));
    state.set_axis(PadAxis::RightX, stick(10));
    state.set_axis(PadAxis::RightY, stick(12));
    Some(state)
}

/// Output report driving both motors, 0..=1 each (`low` = heavy left motor).
pub fn rumble_report(low: f32, high: f32) -> [u8; 8] {
    [0x00, 0x08, 0x00, trigger_to_u8(low), trigger_to_u8(high), 0x00, 0x00, 0x00]
}

/// Output report for the ring of four LEDs around the Guide button.
pub fn led_report(indicator: PlayerIndicator) -> [u8; 3] {
    let pattern = match indicator {
        PlayerIndicator::Off => 0x00,
        PlayerIndicator::BlinkAll => 0x01,
        // 0x02..=0x05: quadrant 1..=4 flashes, then stays on.
        PlayerIndicator::Player(n) => 0x02 + (n.clamp(1, 4) - 1),
        // 0x06..=0x09: quadrant 1..=4 on, without the flash.
        PlayerIndicator::Quadrant(n) => 0x06 + (n.clamp(1, 4) - 1),
        PlayerIndicator::Rotate => 0x0A,
    };
    [0x01, 0x03, pattern]
}
