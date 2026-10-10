//! Ported from Xbox360ProtocolTests.swift.

use padbridge_core::xbox360::{led_report, parse_input, rumble_report};
use padbridge_core::{ControlChange, ControllerState, PadAxis, PadButton, PlayerIndicator};

#[allow(clippy::too_many_arguments)]
fn report(b2: u8, b3: u8, lt: u8, rt: u8, lx: i16, ly: i16, rx: i16, ry: i16) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x14, b2, b3, lt, rt];
    for v in [lx, ly, rx, ry] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.extend_from_slice(&[0; 6]);
    bytes
}

fn idle() -> Vec<u8> {
    report(0, 0, 0, 0, 0, 0, 0, 0)
}

#[test]
fn idle_report_is_neutral() {
    assert_eq!(parse_input(&idle()), Some(ControllerState::default()));
}

#[test]
fn button_bits() {
    let state = parse_input(&report(0x01 | 0x10 | 0x80, 0x04 | 0x10 | 0x80, 0, 0, 0, 0, 0, 0)).unwrap();
    let pressed: Vec<_> = state.pressed().collect();
    let mut expected =
        vec![PadButton::DpadUp, PadButton::Start, PadButton::RightStick, PadButton::Guide, PadButton::A, PadButton::Y];
    expected.sort();
    let mut got = pressed.clone();
    got.sort();
    assert_eq!(got, expected);
}

#[test]
fn axes_are_normalised() {
    let s = parse_input(&report(0, 0, 255, 0, 32767, -32768, 0, -16384)).unwrap();
    assert_eq!(s.axis(PadAxis::LeftTrigger), 1.0);
    assert_eq!(s.axis(PadAxis::RightTrigger), 0.0);
    assert_eq!(s.axis(PadAxis::LeftX), 1.0);
    assert_eq!(s.axis(PadAxis::LeftY), -1.0, "-32768 must clamp to exactly -1");
    assert!((s.axis(PadAxis::RightY) + 0.5).abs() < 0.001);
}

#[test]
fn non_input_messages_are_ignored() {
    assert_eq!(parse_input(&[0x01, 0x03, 0x0E]), None); // LED status
    assert_eq!(parse_input(&[0x03, 0x03, 0x00]), None); // rumble status
    let mut wrong_type = idle();
    wrong_type[0] = 0x08;
    assert_eq!(parse_input(&wrong_type), None);
}

#[test]
fn output_reports() {
    assert_eq!(rumble_report(1.0, 0.0), [0x00, 0x08, 0x00, 0xFF, 0x00, 0x00, 0x00, 0x00]);
    assert_eq!(led_report(PlayerIndicator::Player(1)), [0x01, 0x03, 0x02]);
    assert_eq!(led_report(PlayerIndicator::Player(4)), [0x01, 0x03, 0x05]);
    assert_eq!(led_report(PlayerIndicator::Off), [0x01, 0x03, 0x00]);
    assert_eq!(led_report(PlayerIndicator::Quadrant(1)), [0x01, 0x03, 0x06]);
    assert_eq!(led_report(PlayerIndicator::Quadrant(4)), [0x01, 0x03, 0x09]);
}

#[test]
fn changes_report_only_what_moved() {
    let before = ControllerState::new(&[PadButton::A], &[(PadAxis::LeftX, 0.5)]);
    let after = ControllerState::new(&[PadButton::B], &[(PadAxis::LeftX, 0.5), (PadAxis::RightTrigger, 1.0)]);
    assert_eq!(
        after.changes(&before),
        vec![
            ControlChange::Button(PadButton::A, false),
            ControlChange::Button(PadButton::B, true),
            ControlChange::Axis(PadAxis::RightTrigger, 1.0),
        ]
    );
}
