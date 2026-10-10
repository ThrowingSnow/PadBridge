//! Canonical controls every backend maps its hardware onto.
//!
//! Names follow the SDL GameController layout (Xbox face-button naming), so any
//! pad that SDL knows fits without new variants. Everything downstream of a
//! backend (mapping, MIDI, keyboard, network) only ever sees these, never
//! device-specific report bytes.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PadButton {
    A,
    B,
    X,
    Y,
    LeftShoulder,
    RightShoulder,
    LeftStick,
    RightStick,
    Back,
    Start,
    Guide,
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
}

impl PadButton {
    pub const ALL: [PadButton; 15] = [
        PadButton::A,
        PadButton::B,
        PadButton::X,
        PadButton::Y,
        PadButton::LeftShoulder,
        PadButton::RightShoulder,
        PadButton::LeftStick,
        PadButton::RightStick,
        PadButton::Back,
        PadButton::Start,
        PadButton::Guide,
        PadButton::DpadUp,
        PadButton::DpadDown,
        PadButton::DpadLeft,
        PadButton::DpadRight,
    ];

    /// Position in `ALL`, also the bit index in `ControllerState`.
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn display_name(self) -> &'static str {
        match self {
            PadButton::A => "A",
            PadButton::B => "B",
            PadButton::X => "X",
            PadButton::Y => "Y",
            PadButton::LeftShoulder => "LB",
            PadButton::RightShoulder => "RB",
            PadButton::LeftStick => "LS (click)",
            PadButton::RightStick => "RS (click)",
            PadButton::Back => "Back",
            PadButton::Start => "Start",
            PadButton::Guide => "Guide",
            PadButton::DpadUp => "D-Pad Up",
            PadButton::DpadDown => "D-Pad Down",
            PadButton::DpadLeft => "D-Pad Left",
            PadButton::DpadRight => "D-Pad Right",
        }
    }
}

/// Analog controls. Values in `ControllerState`: sticks -1.0..=1.0 with
/// **up and right positive**, triggers 0.0..=1.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PadAxis {
    LeftX,
    LeftY,
    RightX,
    RightY,
    LeftTrigger,
    RightTrigger,
}

impl PadAxis {
    pub const ALL: [PadAxis; 6] =
        [PadAxis::LeftX, PadAxis::LeftY, PadAxis::RightX, PadAxis::RightY, PadAxis::LeftTrigger, PadAxis::RightTrigger];

    pub fn index(self) -> usize {
        self as usize
    }

    /// Sticks are centred at 0; triggers rest at 0 and only go positive.
    pub fn is_bipolar(self) -> bool {
        !matches!(self, PadAxis::LeftTrigger | PadAxis::RightTrigger)
    }

    pub fn display_name(self) -> &'static str {
        match self {
            PadAxis::LeftX => "Left Stick X",
            PadAxis::LeftY => "Left Stick Y",
            PadAxis::RightX => "Right Stick X",
            PadAxis::RightY => "Right Stick Y",
            PadAxis::LeftTrigger => "LT",
            PadAxis::RightTrigger => "RT",
        }
    }
}

/// Either kind of control, for mapping tables and "which control moved" events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Control {
    Button(PadButton),
    Axis(PadAxis),
}

impl Control {
    pub fn display_name(self) -> &'static str {
        match self {
            Control::Button(b) => b.display_name(),
            Control::Axis(a) => a.display_name(),
        }
    }
}

// Same JSON shape as Swift's synthesized Codable: {"button":{"_0":"a"}}.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum ControlRepr {
    Button {
        #[serde(rename = "_0")]
        value: PadButton,
    },
    Axis {
        #[serde(rename = "_0")]
        value: PadAxis,
    },
}

impl Serialize for Control {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match *self {
            Control::Button(value) => ControlRepr::Button { value },
            Control::Axis(value) => ControlRepr::Axis { value },
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for Control {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match ControlRepr::deserialize(d)? {
            ControlRepr::Button { value } => Control::Button(value),
            ControlRepr::Axis { value } => Control::Axis(value),
        })
    }
}
