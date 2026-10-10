use std::collections::{HashMap, HashSet};

use super::mapping::{ButtonMode, MidiBinding, MidiMapping, MidiMessageKind, VelocitySource};
use crate::controls::{Control, PadAxis, PadButton};
use crate::state::ControllerState;

/// A MIDI 1.0 channel message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MidiMessage {
    pub status: u8,
    pub data1: u8,
    pub data2: u8,
}

impl MidiMessage {
    pub fn control_change(channel: i32, number: i32, value: i32) -> Self {
        MidiMessage { status: 0xB0 | ch(channel), data1: d(number), data2: d(value) }
    }

    pub fn note_on(channel: i32, note: i32, velocity: i32) -> Self {
        MidiMessage { status: 0x90 | ch(channel), data1: d(note), data2: d(velocity) }
    }

    pub fn note_off(channel: i32, note: i32) -> Self {
        MidiMessage { status: 0x80 | ch(channel), data1: d(note), data2: 0 }
    }

    pub fn bytes(&self) -> [u8; 3] {
        [self.status, self.data1, self.data2]
    }
}

fn ch(channel: i32) -> u8 {
    (channel.clamp(1, 16) - 1) as u8
}

fn d(v: i32) -> u8 {
    v.clamp(0, 127) as u8
}

/// Turns controller states into MIDI messages according to a mapping.
/// Sends only what changed after deadzone and 7-bit quantisation, so stick
/// noise does not flood the port. One mapper per connected device.
#[derive(Debug, Clone)]
pub struct MidiMapper {
    mapping: MidiMapping,
    /// Last value sent per control (0..=127), for the UI's live column.
    last_sent: HashMap<Control, i32>,
    toggled: HashSet<PadButton>,
    previous: ControllerState,
}

impl MidiMapper {
    pub fn new(mapping: MidiMapping) -> Self {
        MidiMapper { mapping, last_sent: HashMap::new(), toggled: HashSet::new(), previous: ControllerState::default() }
    }

    pub fn mapping(&self) -> &MidiMapping {
        &self.mapping
    }

    /// A changed mapping re-sends everything on the next state.
    pub fn set_mapping(&mut self, mapping: MidiMapping) {
        if mapping != self.mapping {
            self.mapping = mapping;
            self.reset();
        }
    }

    pub fn last_sent(&self, control: Control) -> Option<i32> {
        self.last_sent.get(&control).copied()
    }

    /// Forget sent values so the next state re-sends everything.
    pub fn reset(&mut self) {
        self.last_sent.clear();
        self.toggled.clear();
    }

    pub fn process(&mut self, state: &ControllerState) -> Vec<MidiMessage> {
        let previous = std::mem::replace(&mut self.previous, *state);
        if !self.mapping.enabled {
            return Vec::new();
        }
        let mut out = Vec::new();
        let bindings = self.mapping.bindings.clone();
        for binding in bindings.iter().filter(|b| b.enabled) {
            match binding.control {
                Control::Axis(axis) => {
                    let value = value7(state.axis(axis), axis, &self.mapping, binding.invert);
                    if self.last_sent.get(&binding.control) == Some(&value) {
                        continue;
                    }
                    self.last_sent.insert(binding.control, value);
                    out.push(MidiMessage::control_change(self.mapping.channel, binding.number, value));
                }
                Control::Button(button) => {
                    let pressed = state.is_pressed(button);
                    let was_pressed = previous.is_pressed(button);
                    let first_send = !self.last_sent.contains_key(&binding.control);
                    if pressed == was_pressed && !first_send {
                        continue;
                    }
                    let on = match binding.button_mode {
                        ButtonMode::Momentary => pressed,
                        ButtonMode::Toggle => {
                            if pressed && !was_pressed {
                                if !self.toggled.remove(&button) {
                                    self.toggled.insert(button);
                                }
                            } else if !first_send {
                                continue;
                            }
                            self.toggled.contains(&button)
                        }
                    };
                    let value = if on { self.velocity(binding, state) } else { 0 };
                    if !first_send || on || binding.kind == MidiMessageKind::Cc {
                        out.push(self.message(binding, value));
                    }
                    self.last_sent.insert(binding.control, value);
                }
            }
        }
        out
    }

    /// `value` 0 means off; anything else is the on value / velocity.
    fn message(&self, binding: &MidiBinding, value: i32) -> MidiMessage {
        let channel = self.mapping.channel;
        match binding.kind {
            MidiMessageKind::Cc => MidiMessage::control_change(channel, binding.number, value),
            MidiMessageKind::Note if value > 0 => MidiMessage::note_on(channel, binding.number, value),
            MidiMessageKind::Note => MidiMessage::note_off(channel, binding.number),
        }
    }

    /// 1..=127: never 0, because note-on with velocity 0 means note-off.
    fn velocity(&self, binding: &MidiBinding, state: &ControllerState) -> i32 {
        let raw = match binding.velocity_source {
            VelocitySource::Fixed => binding.velocity,
            VelocitySource::LeftTrigger => {
                value7(state.axis(PadAxis::LeftTrigger), PadAxis::LeftTrigger, &self.mapping, false)
            }
            VelocitySource::RightTrigger => {
                value7(state.axis(PadAxis::RightTrigger), PadAxis::RightTrigger, &self.mapping, false)
            }
        };
        raw.clamp(1, 127)
    }
}

/// Axis value -> 0..=127. Sticks centre on 64.
pub fn value7(raw: f32, axis: PadAxis, mapping: &MidiMapping, invert: bool) -> i32 {
    let deadzone = if axis.is_bipolar() { mapping.stick_deadzone } else { mapping.trigger_deadzone };
    let mut v = apply_deadzone(raw, deadzone);
    if invert {
        v = if axis.is_bipolar() { -v } else { 1.0 - v };
    }
    let unit = if axis.is_bipolar() { (v + 1.0) / 2.0 } else { v };
    (unit * 127.0).round() as i32
}

/// Zero inside the deadzone, rescaled outside so full travel still reaches ±1.
pub fn apply_deadzone(v: f32, deadzone: f32) -> f32 {
    let dz = deadzone.clamp(0.0, 0.95);
    let magnitude = v.abs();
    if magnitude <= dz {
        return 0.0;
    }
    let scaled = ((magnitude - dz) / (1.0 - dz)).min(1.0);
    if v < 0.0 {
        -scaled
    } else {
        scaled
    }
}
