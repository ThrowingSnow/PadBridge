//! Ported from MIDITests.swift (mapper, velocity, clock, clock bytes) plus ring logic.

use padbridge_core::midi::{
    value7, ButtonMode, ClockMessage, MidiBinding, MidiClockTracker, MidiMapper, MidiMapping, MidiMessage,
    MidiMessageKind, RingCue, VelocitySource,
};
use padbridge_core::ring::{Ring, RingMode};
use padbridge_core::{Control, ControllerState, PadAxis, PadButton, PlayerIndicator};

fn mapping() -> MidiMapping {
    MidiMapping { channel: 2, ..MidiMapping::default() }
}

fn idle() -> ControllerState {
    ControllerState::default()
}

fn pressed(b: &[PadButton]) -> ControllerState {
    ControllerState::new(b, &[])
}

fn edit_a(edit: impl FnOnce(&mut MidiBinding)) -> MidiMapping {
    let mut m = MidiMapping::default();
    edit(m.binding_mut(Control::Button(PadButton::A)).unwrap());
    m
}

// MARK: Mapper

#[test]
fn first_state_sends_every_cc_once() {
    let mut mapper = MidiMapper::new(mapping());
    let first = mapper.process(&idle());
    assert_eq!(first.len(), 21, "6 axes + 15 buttons, all CC by default");
    assert!(first.iter().all(|m| m.status == 0xB1));
    assert!(first.contains(&MidiMessage::control_change(2, 20, 64)), "stick centre is 64");
    assert!(first.contains(&MidiMessage::control_change(2, 24, 0)), "trigger rest is 0");
    assert!(mapper.process(&idle()).is_empty(), "unchanged state sends nothing");
}

#[test]
fn deadzone_swallows_drift() {
    let mut mapper = MidiMapper::new(mapping());
    mapper.process(&idle());
    let drift =
        ControllerState::new(&[], &[(PadAxis::LeftX, 0.058), (PadAxis::LeftY, 0.042), (PadAxis::RightX, -0.047)]);
    assert!(mapper.process(&drift).is_empty());
}

#[test]
fn full_travel_reaches_ends() {
    let m = mapping();
    assert_eq!(value7(1.0, PadAxis::LeftX, &m, false), 127);
    assert_eq!(value7(-1.0, PadAxis::LeftX, &m, false), 0);
    assert_eq!(value7(1.0, PadAxis::LeftX, &m, true), 0);
    assert_eq!(value7(1.0, PadAxis::RightTrigger, &m, false), 127);
    assert_eq!(value7(1.0, PadAxis::RightTrigger, &m, true), 0);
}

#[test]
fn momentary_button_sends_press_and_release() {
    let mut mapper = MidiMapper::new(mapping());
    mapper.process(&idle());
    assert_eq!(mapper.process(&pressed(&[PadButton::A])), vec![MidiMessage::control_change(2, 102, 127)]);
    assert_eq!(mapper.process(&idle()), vec![MidiMessage::control_change(2, 102, 0)]);
}

#[test]
fn toggle_button_flips_on_press_only() {
    let mut m = mapping();
    let a = m.binding_mut(Control::Button(PadButton::A)).unwrap();
    a.button_mode = ButtonMode::Toggle;
    a.kind = MidiMessageKind::Note;
    a.number = 60;
    let mut mapper = MidiMapper::new(m);
    mapper.process(&idle());
    assert_eq!(mapper.process(&pressed(&[PadButton::A])), vec![MidiMessage::note_on(2, 60, 127)]);
    assert!(mapper.process(&idle()).is_empty(), "release does nothing in toggle mode");
    assert_eq!(mapper.process(&pressed(&[PadButton::A])), vec![MidiMessage::note_off(2, 60)]);
}

#[test]
fn conflicts_are_detected() {
    let mut m = MidiMapping::default();
    assert!(m.conflicts().is_empty());
    m.bindings[0].number = m.bindings[1].number;
    let expected: Vec<_> = vec![m.bindings[0].control, m.bindings[1].control];
    assert_eq!(m.conflicts().into_iter().collect::<Vec<_>>(), expected);
}

#[test]
fn mapping_change_resends_everything() {
    let mut mapper = MidiMapper::new(mapping());
    mapper.process(&idle());
    let mut changed = mapping();
    changed.channel = 3;
    mapper.set_mapping(changed);
    assert_eq!(mapper.process(&idle()).len(), 21);
}

// MARK: Velocity

#[test]
fn fixed_velocity_is_used_for_notes_and_cc() {
    let mut mapper = MidiMapper::new(edit_a(|a| {
        a.kind = MidiMessageKind::Note;
        a.number = 60;
        a.velocity = 90;
    }));
    mapper.process(&idle());
    assert_eq!(mapper.process(&pressed(&[PadButton::A])), vec![MidiMessage::note_on(1, 60, 90)]);

    let mut cc = MidiMapper::new(edit_a(|a| a.velocity = 42));
    cc.process(&idle());
    assert_eq!(cc.process(&pressed(&[PadButton::A])), vec![MidiMessage::control_change(1, 102, 42)]);
}

#[test]
fn trigger_sets_velocity() {
    let mut m = edit_a(|a| {
        a.kind = MidiMessageKind::Note;
        a.number = 60;
        a.velocity_source = VelocitySource::RightTrigger;
    });
    m.bindings.retain(|b| !matches!(b.control, Control::Axis(_)));
    let mut mapper = MidiMapper::new(m);
    mapper.process(&idle());
    let half = ControllerState::new(&[PadButton::A], &[(PadAxis::RightTrigger, 0.5)]);
    let velocity = mapper.process(&half).first().map_or(0, |m| m.data2);
    assert!(velocity > 55 && velocity < 70, "got {velocity}");
}

#[test]
fn released_trigger_still_plays_the_note() {
    let mut mapper = MidiMapper::new(edit_a(|a| {
        a.kind = MidiMessageKind::Note;
        a.number = 60;
        a.velocity_source = VelocitySource::LeftTrigger;
    }));
    mapper.process(&idle());
    assert!(mapper.process(&pressed(&[PadButton::A])).contains(&MidiMessage::note_on(1, 60, 1)));
}

#[test]
fn old_saved_mapping_still_loads() {
    // Shape of a midi-mapping.json written before velocity existed.
    let json = r#"{"enabled":true,"channel":3,"stickDeadzone":0.2,"triggerDeadzone":0.02,
        "bindings":[{"control":{"button":{"_0":"a"}},"enabled":true,"kind":"note","number":61,"buttonMode":"toggle","invert":false}]}"#;
    let m: MidiMapping = serde_json::from_str(json).unwrap();
    assert_eq!(m.channel, 3);
    let a = m.binding(Control::Button(PadButton::A)).unwrap();
    assert!(a.number == 61 && a.kind == MidiMessageKind::Note && a.button_mode == ButtonMode::Toggle);
    assert!(a.velocity_source == VelocitySource::Fixed && a.velocity == 127);
    assert_eq!(m.bindings.len(), MidiMapping::default().bindings.len(), "missing controls are filled in");
}

// MARK: Clock

/// Feeds `beats` quarter notes of clock at 120 BPM and collects ring cues.
fn run(tracker: &mut MidiClockTracker, beats: usize, time: &mut f64) -> Vec<RingCue> {
    let mut cues = Vec::new();
    for _ in 0..beats * 24 {
        cues.extend(tracker.handle(ClockMessage::Tick, *time));
        *time += 60.0 / (120.0 * 24.0);
    }
    cues
}

#[test]
fn beats_and_bars_are_counted() {
    let mut t = MidiClockTracker::new();
    let mut time = 0.0;
    t.handle(ClockMessage::Start, time);
    let beats: Vec<_> = run(&mut t, 8, &mut time).into_iter().filter(|c| matches!(c, RingCue::Beat { .. })).collect();
    let expected: Vec<_> = (0..8).map(|i| RingCue::Beat { beat: i % 4, bar: i / 4 }).collect();
    assert_eq!(beats, expected);
}

#[test]
fn blinks_twice_per_beat() {
    let mut t = MidiClockTracker::new();
    let mut time = 0.0;
    t.handle(ClockMessage::Start, time);
    assert_eq!(run(&mut t, 1, &mut time), vec![RingCue::Beat { beat: 0, bar: 0 }, RingCue::Off]);
}

#[test]
fn bpm_is_estimated() {
    let mut t = MidiClockTracker::new();
    let mut time = 0.0;
    run(&mut t, 8, &mut time);
    assert!((t.bpm().unwrap_or(0.0) - 120.0).abs() < 0.5);
    assert!(!t.running(), "clock without Start does not move the position");
}

#[test]
fn song_position_jumps() {
    let mut t = MidiClockTracker::new();
    let mut time = 0.0;
    t.handle(ClockMessage::SongPosition(32), time); // 32 sixteenths = bar 3
    t.handle(ClockMessage::Continue, time);
    assert_eq!(run(&mut t, 1, &mut time).first(), Some(&RingCue::Beat { beat: 0, bar: 2 }));
    assert_eq!(t.position().0, 3);
}

#[test]
fn raw_clock_bytes_decode() {
    assert_eq!(ClockMessage::from_bytes(&[0xF8]), Some(ClockMessage::Tick));
    assert_eq!(ClockMessage::from_bytes(&[0xFA]), Some(ClockMessage::Start));
    assert_eq!(ClockMessage::from_bytes(&[0xFB]), Some(ClockMessage::Continue));
    assert_eq!(ClockMessage::from_bytes(&[0xFC]), Some(ClockMessage::Stop));
    assert_eq!(ClockMessage::from_bytes(&[0xF2, 0x00, 0x01]), Some(ClockMessage::SongPosition(128)));
    assert_eq!(ClockMessage::from_bytes(&[0xB0, 20, 64]), None);
}

// MARK: Ring

#[test]
fn ring_beat_mode_goes_round_clockwise() {
    let mut ring = Ring::new();
    ring.set_mode(RingMode::ClockBeat);
    let leds: Vec<_> = (0..4).map(|beat| ring.clock_cue(RingCue::Beat { beat, bar: 0 })).collect();
    let q = PlayerIndicator::Quadrant;
    assert_eq!(leds, vec![Some(q(1)), Some(q(2)), Some(q(4)), Some(q(3))]);
}

#[test]
fn ring_bar_mode_follows_bars() {
    let mut ring = Ring::new();
    ring.set_mode(RingMode::ClockBar);
    let leds: Vec<_> = (0..5).map(|bar| ring.clock_cue(RingCue::Beat { beat: 3, bar })).collect();
    let q = PlayerIndicator::Quadrant;
    assert_eq!(leds, vec![Some(q(1)), Some(q(2)), Some(q(4)), Some(q(3)), Some(q(1))], "clockwise, one LED per bar");
    assert_eq!(ring.clock_cue(RingCue::Off), Some(PlayerIndicator::Off));
    assert_eq!(ring.clock_cue(RingCue::Stopped), Some(PlayerIndicator::Quadrant(1)));
}

#[test]
fn ring_ignores_clock_outside_clock_modes() {
    let mut ring = Ring::new();
    ring.set_manual(PlayerIndicator::Player(2));
    assert_eq!(ring.clock_cue(RingCue::Beat { beat: 0, bar: 0 }), None);
    assert_eq!(ring.current(), Some(PlayerIndicator::Player(2)));
}

#[test]
fn ring_random_never_repeats_a_quadrant_and_waits_60_to_280_ms() {
    let mut ring = Ring::new();
    ring.set_mode(RingMode::Random);
    let mut last = None;
    for _ in 0..500 {
        let (indicator, wait) = ring.random_step();
        assert!((60..=280).contains(&wait.as_millis()));
        if let PlayerIndicator::Quadrant(q) = indicator {
            assert!((1..=4).contains(&q));
            assert_ne!(Some(q), last);
            last = Some(q);
        } else {
            assert_eq!(indicator, PlayerIndicator::Off);
            last = None;
        }
    }
}
