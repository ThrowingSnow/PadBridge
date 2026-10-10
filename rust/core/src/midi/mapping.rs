use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::controls::{Control, PadAxis, PadButton};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MidiMessageKind {
    #[default]
    Cc,
    Note,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ButtonMode {
    /// Press sends on, release sends off.
    #[default]
    Momentary,
    /// Each press flips between on and off; release sends nothing.
    Toggle,
}

/// Where a button's "on" value (note velocity, or CC value) comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VelocitySource {
    /// `MidiBinding::velocity`.
    #[default]
    Fixed,
    /// How far the trigger is pulled at the moment of the press.
    LeftTrigger,
    RightTrigger,
}

/// Where controller MIDI goes and where clock is taken from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MidiTarget {
    /// Virtual port "PadBridge" on this machine.
    #[default]
    Local,
    /// The peer at the other end of the network link.
    Network,
}

/// How one control is sent. Axes always send CC; `kind`, `button_mode` and the
/// velocity fields only apply to buttons. Fields added after the first release
/// default when missing, so older files keep loading.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MidiBinding {
    pub control: Control,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub kind: MidiMessageKind,
    /// CC or note number, 0..=127.
    pub number: i32,
    #[serde(default)]
    pub button_mode: ButtonMode,
    /// Axes only: flip direction (sticks) or range (triggers).
    #[serde(default)]
    pub invert: bool,
    #[serde(default)]
    pub velocity_source: VelocitySource,
    /// 1..=127, used when `velocity_source` is `Fixed`.
    #[serde(default = "full_velocity")]
    pub velocity: i32,
}

fn yes() -> bool {
    true
}

fn full_velocity() -> i32 {
    127
}

impl MidiBinding {
    pub fn new(control: Control, number: i32) -> Self {
        MidiBinding {
            control,
            enabled: true,
            kind: MidiMessageKind::Cc,
            number,
            button_mode: ButtonMode::Momentary,
            invert: false,
            velocity_source: VelocitySource::Fixed,
            velocity: 127,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MidiMapping {
    pub enabled: bool,
    pub target: MidiTarget,
    /// 1..=16
    pub channel: i32,
    /// Fraction of travel ignored around the centre of each stick axis (drift).
    pub stick_deadzone: f32,
    /// Fraction ignored at the resting end of each trigger.
    pub trigger_deadzone: f32,
    pub bindings: Vec<MidiBinding>,
}

impl Default for MidiMapping {
    /// CC numbers MIDI leaves undefined, so they clash with nothing:
    /// axes on 20..=25, buttons on 102..=116.
    fn default() -> Self {
        let axes = PadAxis::ALL.iter().enumerate().map(|(i, &a)| MidiBinding::new(Control::Axis(a), 20 + i as i32));
        let buttons =
            PadButton::ALL.iter().enumerate().map(|(i, &b)| MidiBinding::new(Control::Button(b), 102 + i as i32));
        MidiMapping {
            enabled: true,
            target: MidiTarget::Local,
            channel: 1,
            stick_deadzone: 0.12,
            trigger_deadzone: 0.02,
            bindings: axes.chain(buttons).collect(),
        }
    }
}

impl MidiMapping {
    pub fn binding(&self, control: Control) -> Option<&MidiBinding> {
        self.bindings.iter().find(|b| b.control == control)
    }

    pub fn binding_mut(&mut self, control: Control) -> Option<&mut MidiBinding> {
        self.bindings.iter_mut().find(|b| b.control == control)
    }

    /// Enabled bindings that would send the same message as another one.
    pub fn conflicts(&self) -> BTreeSet<Control> {
        let mut seen: HashMap<(MidiMessageKind, i32), Control> = HashMap::new();
        let mut out = BTreeSet::new();
        for b in self.bindings.iter().filter(|b| b.enabled) {
            let kind = if matches!(b.control, Control::Axis(_)) { MidiMessageKind::Cc } else { b.kind };
            match seen.get(&(kind, b.number)) {
                Some(&other) => {
                    out.insert(other);
                    out.insert(b.control);
                }
                None => {
                    seen.insert((kind, b.number), b.control);
                }
            }
        }
        out
    }
}

// Missing fields fall back to the defaults instead of failing the whole file,
// and controls added in newer versions get their default binding.
impl<'de> Deserialize<'de> for MidiMapping {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Partial {
            enabled: Option<bool>,
            target: Option<MidiTarget>,
            channel: Option<i32>,
            stick_deadzone: Option<f32>,
            trigger_deadzone: Option<f32>,
            bindings: Option<Vec<MidiBinding>>,
        }
        let p = Partial::deserialize(d)?;
        let def = MidiMapping::default();
        let mut bindings = p.bindings.unwrap_or_else(|| def.bindings.clone());
        for b in &def.bindings {
            if !bindings.iter().any(|x| x.control == b.control) {
                bindings.push(*b);
            }
        }
        Ok(MidiMapping {
            enabled: p.enabled.unwrap_or(def.enabled),
            target: p.target.unwrap_or(def.target),
            channel: p.channel.unwrap_or(def.channel),
            stick_deadzone: p.stick_deadzone.unwrap_or(def.stick_deadzone),
            trigger_deadzone: p.trigger_deadzone.unwrap_or(def.trigger_deadzone),
            bindings,
        })
    }
}
