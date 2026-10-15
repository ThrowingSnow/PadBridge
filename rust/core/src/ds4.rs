//! Sony DualShock 4 (PS4 controller): HID input report parsing and output
//! reports for rumble and the light bar, over USB and Bluetooth.
//!
//! Buttons map by position onto the Xbox layout, so MIDI and keyboard/mouse
//! mappings work unchanged: Cross = A, Circle = B, Square = X, Triangle = Y,
//! Share = Back, Options = Start, PS = Guide.

use crate::device::PlayerIndicator;
use crate::{ControllerState, PadAxis, PadButton};

pub const VENDOR_SONY: u16 = 0x054C;

/// (vendor, product, name) of the DualShock 4 variants.
pub const KNOWN_DEVICES: [(u16, u16, &str); 3] = [
    (VENDOR_SONY, 0x05C4, "DualShock 4"),
    (VENDOR_SONY, 0x09CC, "DualShock 4 (v2)"),
    (VENDOR_SONY, 0x0BA0, "DualShock 4 (USB-Adapter)"),
];

pub fn is_known(vendor: u16, product: u16) -> Option<&'static str> {
    KNOWN_DEVICES.iter().find(|(v, p, _)| *v == vendor && *p == product).map(|&(_, _, n)| n)
}

/// Parses an input report including its report ID: 0x01 (USB, and the basic
/// Bluetooth report) or 0x11 (full Bluetooth report, two bytes further in).
pub fn parse_input(report: &[u8]) -> Option<ControllerState> {
    let base = match report.first()? {
        0x01 => 1,
        0x11 => 3,
        _ => return None,
    };
    let r = report.get(base..base + 9)?;
    let mut state = ControllerState::default();

    let stick = |v: u8| ((v as f32 - 128.0) / 127.0).clamp(-1.0, 1.0);
    state.set_axis(PadAxis::LeftX, stick(r[0]));
    state.set_axis(PadAxis::LeftY, -stick(r[1]));
    state.set_axis(PadAxis::RightX, stick(r[2]));
    state.set_axis(PadAxis::RightY, -stick(r[3]));
    state.set_axis(PadAxis::LeftTrigger, r[7] as f32 / 255.0);
    state.set_axis(PadAxis::RightTrigger, r[8] as f32 / 255.0);

    // Hat: 0 = up, clockwise in eighths, 8 = released.
    let hat = r[4] & 0x0F;
    let dirs = [(0, 1, 7), (2, 1, 3), (4, 3, 5), (6, 5, 7)];
    for (button, (center, a, b)) in
        [PadButton::DpadUp, PadButton::DpadRight, PadButton::DpadDown, PadButton::DpadLeft].into_iter().zip(dirs)
    {
        state.set_pressed(button, hat < 8 && (hat == center || hat == a || hat == b));
    }

    let bit = |byte: u8, n: u8| byte & (1 << n) != 0;
    for (button, pressed) in [
        (PadButton::X, bit(r[4], 4)), // Square
        (PadButton::A, bit(r[4], 5)), // Cross
        (PadButton::B, bit(r[4], 6)), // Circle
        (PadButton::Y, bit(r[4], 7)), // Triangle
        (PadButton::LeftShoulder, bit(r[5], 0)),
        (PadButton::RightShoulder, bit(r[5], 1)),
        (PadButton::Back, bit(r[5], 4)),  // Share
        (PadButton::Start, bit(r[5], 5)), // Options
        (PadButton::LeftStick, bit(r[5], 6)),
        (PadButton::RightStick, bit(r[5], 7)),
        (PadButton::Guide, bit(r[6], 0)), // PS
    ] {
        state.set_pressed(button, pressed);
    }
    Some(state)
}

/// Light bar colour and blinking, derived from the 360-style indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LightBar {
    pub rgb: [u8; 3],
    /// Blink on/off time in units of 10 ms; 0/0 = steady.
    pub flash: [u8; 2],
}

/// Player colours as on the PS4: blue, red, green, pink.
const PLAYER_RGB: [[u8; 3]; 4] = [[0, 0, 160], [160, 0, 0], [0, 160, 0], [160, 0, 80]];

impl LightBar {
    pub fn from_indicator(indicator: PlayerIndicator) -> Self {
        let steady = |rgb| LightBar { rgb, flash: [0, 0] };
        match indicator {
            PlayerIndicator::Off => steady([0, 0, 0]),
            PlayerIndicator::Player(n) | PlayerIndicator::Quadrant(n) => {
                steady(PLAYER_RGB[(n.clamp(1, 4) - 1) as usize])
            }
            PlayerIndicator::BlinkAll => LightBar { rgb: [160, 160, 160], flash: [25, 25] },
            // No ring to run round: a slow purple pulse stands in for it.
            PlayerIndicator::Rotate => LightBar { rgb: [100, 0, 160], flash: [50, 50] },
        }
    }
}

/// Output report for rumble (`low` = heavy left motor) and the light bar.
/// USB: report 0x05, 32 bytes. Bluetooth: report 0x11, 78 bytes with CRC.
pub fn output_report(bluetooth: bool, low: f32, high: f32, bar: LightBar) -> Vec<u8> {
    let motor = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let effects = [motor(high), motor(low), bar.rgb[0], bar.rgb[1], bar.rgb[2], bar.flash[0], bar.flash[1]];
    if bluetooth {
        let mut r = vec![0u8; 78];
        r[0] = 0x11;
        r[1] = 0xC0; // HID report with CRC
        r[3] = 0x07; // rumble + light bar + flash
        r[6..13].copy_from_slice(&effects);
        let crc = crc32(&[&[0xA2], &r[..74]]);
        r[74..].copy_from_slice(&crc.to_le_bytes());
        r
    } else {
        let mut r = vec![0u8; 32];
        r[0] = 0x05;
        r[1] = 0x07;
        r[2] = 0x04;
        r[4..11].copy_from_slice(&effects);
        r
    }
}

/// CRC-32 (IEEE) over several slices, as the DS4 expects on Bluetooth.
fn crc32(parts: &[&[u8]]) -> u32 {
    let mut crc = !0u32;
    for byte in parts.iter().flat_map(|p| p.iter()) {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usb_report(lx: u8, ly: u8, b5: u8, b6: u8, b7: u8, l2: u8, r2: u8) -> Vec<u8> {
        let mut r = vec![0u8; 64];
        r[0] = 0x01;
        r[1..5].copy_from_slice(&[lx, ly, 128, 128]);
        r[5] = b5;
        r[6] = b6;
        r[7] = b7;
        r[8] = l2;
        r[9] = r2;
        r
    }

    #[test]
    fn neutral_report() {
        let s = parse_input(&usb_report(128, 128, 0x08, 0, 0, 0, 0)).unwrap();
        assert_eq!(s.pressed().count(), 0);
        assert_eq!(s.axis(PadAxis::LeftX), 0.0);
        assert_eq!(s.axis(PadAxis::LeftY), 0.0);
    }

    #[test]
    fn sticks_up_and_right_are_positive() {
        let s = parse_input(&usb_report(255, 0, 0x08, 0, 0, 255, 0)).unwrap();
        assert_eq!(s.axis(PadAxis::LeftX), 1.0);
        assert_eq!(s.axis(PadAxis::LeftY), 1.0);
        assert_eq!(s.axis(PadAxis::LeftTrigger), 1.0);
        assert_eq!(s.axis(PadAxis::RightTrigger), 0.0);
    }

    #[test]
    fn buttons_map_by_position() {
        // Cross + Triangle, hat up-right, L1, Options, PS.
        let s = parse_input(&usb_report(128, 128, 0x20 | 0x80 | 1, 0x01 | 0x20, 0x01, 0, 0)).unwrap();
        let pressed: Vec<_> = s.pressed().collect();
        assert_eq!(
            pressed,
            vec![
                PadButton::A,
                PadButton::Y,
                PadButton::LeftShoulder,
                PadButton::Start,
                PadButton::Guide,
                PadButton::DpadUp,
                PadButton::DpadRight
            ]
        );
    }

    #[test]
    fn bluetooth_report_is_offset() {
        let usb = usb_report(10, 200, 0x40 | 6, 0x80, 0, 30, 40);
        let mut bt = vec![0x11, 0xC0, 0x00];
        bt.extend_from_slice(&usb[1..]);
        assert_eq!(parse_input(&bt), parse_input(&usb));
        assert!(parse_input(&[0x05, 0, 0]).is_none());
    }

    #[test]
    fn output_reports() {
        let bar = LightBar::from_indicator(PlayerIndicator::Quadrant(2));
        let usb = output_report(false, 1.0, 0.0, bar);
        assert_eq!(&usb[..11], &[0x05, 0x07, 0x04, 0, 0, 255, 160, 0, 0, 0, 0]);
        let bt = output_report(true, 0.0, 0.0, LightBar::from_indicator(PlayerIndicator::Off));
        assert_eq!(bt.len(), 78);
        assert_eq!(crc32(&[b"123456789"]), 0xCBF4_3926);
    }
}
