//! Ported from KeyMouseTests.swift.

use std::collections::HashSet;

use padbridge_core::keymouse::{KeyCode, KeyMouseMapper, KeyMouseMapping, KmEvent, MouseButton, StickRole, WireInput};
use padbridge_core::{ControllerState, PadAxis, PadButton};

fn mapper(edit: impl FnOnce(&mut KeyMouseMapping)) -> KeyMouseMapper {
    let mut m = KeyMouseMapping { enabled: true, ..KeyMouseMapping::default() };
    edit(&mut m);
    KeyMouseMapper::new(m)
}

fn state(pressed: &[PadButton], axes: &[(PadAxis, f32)]) -> ControllerState {
    ControllerState::new(pressed, axes)
}

fn idle() -> ControllerState {
    ControllerState::default()
}

#[test]
fn button_presses_and_releases_key() {
    let mut m = mapper(|_| {});
    assert_eq!(m.process(&state(&[PadButton::A], &[])), vec![KmEvent::Key(KeyCode::Enter, true)]);
    assert_eq!(m.process(&idle()), vec![KmEvent::Key(KeyCode::Enter, false)]);
}

#[test]
fn combo_presses_modifiers_first_and_releases_them_last() {
    let mut m = mapper(|_| {}); // LB = ⌃⇧Tab
    assert_eq!(
        m.process(&state(&[PadButton::LeftShoulder], &[])),
        vec![
            KmEvent::Key(KeyCode::LeftControl, true),
            KmEvent::Key(KeyCode::LeftShift, true),
            KmEvent::Key(KeyCode::Tab, true)
        ]
    );
    assert_eq!(
        m.process(&idle()),
        vec![
            KmEvent::Key(KeyCode::Tab, false),
            KmEvent::Key(KeyCode::LeftShift, false),
            KmEvent::Key(KeyCode::LeftControl, false)
        ]
    );
}

#[test]
fn shared_key_is_released_only_when_both_controls_are_up() {
    let mut m = mapper(|m| m.right_stick = StickRole::Arrows); // D-pad up and right stick up both = ↑
    m.process(&state(&[PadButton::DpadUp], &[]));
    assert!(m.process(&state(&[PadButton::DpadUp], &[(PadAxis::RightY, 1.0)])).is_empty());
    assert!(m.process(&state(&[], &[(PadAxis::RightY, 1.0)])).is_empty(), "stick still holds ↑");
    assert_eq!(m.process(&idle()), vec![KmEvent::Key(KeyCode::Up, false)]);
}

#[test]
fn trigger_has_hysteresis() {
    let mut m = mapper(|_| {}); // RT = left click
    let rt = |v| state(&[], &[(PadAxis::RightTrigger, v)]);
    assert!(m.process(&rt(0.5)).is_empty());
    assert_eq!(m.process(&rt(0.6)), vec![KmEvent::Button(MouseButton::Left, true)]);
    assert!(m.process(&rt(0.5)).is_empty(), "between thresholds: stays down");
    assert_eq!(m.process(&rt(0.4)), vec![KmEvent::Button(MouseButton::Left, false)]);
}

#[test]
fn pointer_moves_with_stick_and_respects_deadzone() {
    let mut m = mapper(|m| m.curve = 1.0);
    assert!(m.tick(&state(&[], &[(PadAxis::RightX, 0.1)]), 0.01).is_empty(), "inside deadzone");
    // Full right for 0.1 s at 1400 pt/s = 140 pt; stick up = pointer up (negative y).
    let (mut dx, mut dy) = (0, 0);
    for _ in 0..10 {
        for e in m.tick(&state(&[], &[(PadAxis::RightX, 1.0)]), 0.01) {
            if let KmEvent::Move { dx: x, dy: y } = e {
                dx += x;
                dy += y;
            }
        }
    }
    assert_eq!((dx, dy), (140, 0));
    let up: i32 = m
        .tick(&state(&[], &[(PadAxis::RightY, 1.0)]), 0.1)
        .into_iter()
        .map(|e| if let KmEvent::Move { dy, .. } = e { dy } else { 0 })
        .sum();
    assert_eq!(up, -140);
}

#[test]
fn diagonal_is_not_faster() {
    let m = mapper(|m| {
        m.curve = 1.0;
        m.stick_deadzone = 0.0;
    });
    let (vx, vy) = m.velocity(std::f32::consts::FRAC_1_SQRT_2, std::f32::consts::FRAC_1_SQRT_2, 100.0);
    assert!(((vx * vx + vy * vy).sqrt() - 100.0).abs() < 0.5);
}

#[test]
fn role_change_while_held_releases_the_right_key() {
    let mut m = mapper(|m| m.left_stick = StickRole::Wasd);
    let up = state(&[], &[(PadAxis::LeftY, 1.0)]);
    assert_eq!(m.process(&up), vec![KmEvent::Key(KeyCode::W, true)]);
    m.mapping.left_stick = StickRole::Arrows;
    assert_eq!(m.process(&up), vec![KmEvent::Key(KeyCode::W, false), KmEvent::Key(KeyCode::Up, true)]);
}

#[test]
fn release_all_lets_go_of_everything() {
    let mut m = mapper(|_| {});
    m.process(&state(&[PadButton::A], &[(PadAxis::RightTrigger, 1.0)]));
    let released: HashSet<_> = m.release_all().into_iter().collect();
    assert_eq!(
        released,
        HashSet::from([KmEvent::Key(KeyCode::Enter, false), KmEvent::Button(MouseButton::Left, false)])
    );
    assert!(!m.is_holding_anything());
}

#[test]
fn wire_encoding() {
    assert_eq!(WireInput::from(KmEvent::Key(KeyCode::A, true)), WireInput { kind: 1, a: 30, b: 1 });
    assert_eq!(WireInput::from(KmEvent::Button(MouseButton::Left, false)), WireInput { kind: 1, a: 0x110, b: 0 });
    assert_eq!(WireInput::from(KmEvent::Scroll { dx: 5, dy: -120 }), WireInput { kind: 3, a: -120, b: 5 });
    assert_eq!(WireInput::from(KmEvent::Move { dx: 100_000, dy: -3 }), WireInput { kind: 2, a: i16::MAX, b: -3 });
}

#[test]
fn old_settings_file_loads() {
    let m: KeyMouseMapping = serde_json::from_str(r#"{"enabled":true,"rightStick":"scroll"}"#).unwrap();
    assert!(m.enabled && m.right_stick == StickRole::Scroll);
    assert_eq!(m.left_stick, KeyMouseMapping::default().left_stick);
    assert_eq!(m.bindings.len(), KeyMouseMapping::bindable_controls().len());
}
