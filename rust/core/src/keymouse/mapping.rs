use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::keycode::KeyCode;
use crate::controls::{Control, PadAxis, PadButton};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Modifier {
    Shift,
    Control,
    Option,
    Command,
}

impl Modifier {
    /// Stable press order (⇧⌃⌥⌘) so combos press consistently.
    pub const ALL: [Modifier; 4] = [Modifier::Shift, Modifier::Control, Modifier::Option, Modifier::Command];

    pub fn key(self) -> KeyCode {
        match self {
            Modifier::Shift => KeyCode::LeftShift,
            Modifier::Control => KeyCode::LeftControl,
            Modifier::Option => KeyCode::LeftOption,
            Modifier::Command => KeyCode::LeftCommand,
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            Modifier::Shift => "⇧",
            Modifier::Control => "⌃",
            Modifier::Option => "⌥",
            Modifier::Command => "⌘",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

impl MouseButton {
    pub const ALL: [MouseButton; 3] = [MouseButton::Left, MouseButton::Right, MouseButton::Middle];

    /// Linux BTN_LEFT / BTN_RIGHT / BTN_MIDDLE (sent as key events).
    pub fn linux_code(self) -> u16 {
        match self {
            MouseButton::Left => 0x110,
            MouseButton::Right => 0x111,
            MouseButton::Middle => 0x112,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            MouseButton::Left => "Linksklick",
            MouseButton::Right => "Rechtsklick",
            MouseButton::Middle => "Mittelklick",
        }
    }
}

/// Where keyboard/mouse events are produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EmulationTarget {
    /// This machine.
    #[default]
    Local,
    /// The peer at the other end of the network link.
    Network,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StickRole {
    #[default]
    None,
    Mouse,
    Scroll,
    Wasd,
    Arrows,
}

impl StickRole {
    pub const ALL: [StickRole; 5] =
        [StickRole::None, StickRole::Mouse, StickRole::Scroll, StickRole::Wasd, StickRole::Arrows];

    pub fn display_name(self) -> &'static str {
        match self {
            StickRole::None => "Aus",
            StickRole::Mouse => "Maus",
            StickRole::Scroll => "Scrollen",
            StickRole::Wasd => "WASD",
            StickRole::Arrows => "Pfeiltasten",
        }
    }

    /// (up, down, left, right) keys for the key roles.
    pub fn keys(self) -> Option<[KeyCode; 4]> {
        match self {
            StickRole::Wasd => Some([KeyCode::W, KeyCode::S, KeyCode::A, KeyCode::D]),
            StickRole::Arrows => Some([KeyCode::Up, KeyCode::Down, KeyCode::Left, KeyCode::Right]),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum KeyAction {
    #[default]
    None,
    Mouse(MouseButton),
    Key(KeyCode, Vec<Modifier>),
}

impl KeyAction {
    pub fn key(key: KeyCode) -> Self {
        KeyAction::Key(key, Vec::new())
    }

    pub fn display_name(&self) -> String {
        match self {
            KeyAction::None => "–".into(),
            KeyAction::Mouse(b) => b.display_name().into(),
            KeyAction::Key(k, mods) => mods.iter().map(|m| m.symbol()).collect::<String>() + k.display_name(),
        }
    }
}

// Same JSON shape as Swift's synthesized Codable:
// {"none":{}}, {"mouse":{"_0":"left"}}, {"key":{"_0":"enter","modifiers":[]}}.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum KeyActionRepr {
    None {},
    Mouse {
        #[serde(rename = "_0")]
        button: MouseButton,
    },
    Key {
        #[serde(rename = "_0")]
        key: KeyCode,
        #[serde(default)]
        modifiers: Vec<Modifier>,
    },
}

impl Serialize for KeyAction {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            KeyAction::None => KeyActionRepr::None {},
            KeyAction::Mouse(button) => KeyActionRepr::Mouse { button: *button },
            KeyAction::Key(key, modifiers) => KeyActionRepr::Key { key: *key, modifiers: modifiers.clone() },
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for KeyAction {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match KeyActionRepr::deserialize(d)? {
            KeyActionRepr::None {} => KeyAction::None,
            KeyActionRepr::Mouse { button } => KeyAction::Mouse(button),
            KeyActionRepr::Key { key, modifiers } => KeyAction::Key(key, modifiers),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyBinding {
    /// A button, or a trigger used as a button.
    pub control: Control,
    pub action: KeyAction,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyMouseMapping {
    pub enabled: bool,
    pub target: EmulationTarget,
    pub left_stick: StickRole,
    pub right_stick: StickRole,
    /// Pointer speed at full deflection, in points per second.
    pub mouse_speed: f64,
    /// Scroll speed at full deflection, in wheel detents per second.
    pub scroll_speed: f64,
    /// Response curve exponent: 1 = linear, higher = finer control near the centre.
    pub curve: f64,
    pub stick_deadzone: f32,
    pub bindings: Vec<KeyBinding>,
}

impl KeyMouseMapping {
    /// Controls that can carry a key action: all buttons plus both triggers.
    pub fn bindable_controls() -> Vec<Control> {
        PadButton::ALL
            .iter()
            .map(|&b| Control::Button(b))
            .chain([Control::Axis(PadAxis::LeftTrigger), Control::Axis(PadAxis::RightTrigger)])
            .collect()
    }

    pub fn action(&self, control: Control) -> &KeyAction {
        static NONE: KeyAction = KeyAction::None;
        self.bindings.iter().find(|b| b.control == control).map_or(&NONE, |b| &b.action)
    }
}

impl Default for KeyMouseMapping {
    /// "Couch desktop" preset: right stick pointer, left stick scroll, triggers click.
    fn default() -> Self {
        use KeyCode as K;
        use PadButton as B;
        let preset = |control: Control| -> KeyAction {
            match control {
                Control::Axis(PadAxis::RightTrigger) => KeyAction::Mouse(MouseButton::Left),
                Control::Axis(PadAxis::LeftTrigger) => KeyAction::Mouse(MouseButton::Right),
                Control::Button(B::RightStick) => KeyAction::Mouse(MouseButton::Middle),
                Control::Button(B::A) => KeyAction::key(K::Enter),
                Control::Button(B::B) => KeyAction::key(K::Escape),
                Control::Button(B::X) => KeyAction::key(K::Space),
                Control::Button(B::Y) => KeyAction::key(K::Tab),
                Control::Button(B::DpadUp) => KeyAction::key(K::Up),
                Control::Button(B::DpadDown) => KeyAction::key(K::Down),
                Control::Button(B::DpadLeft) => KeyAction::key(K::Left),
                Control::Button(B::DpadRight) => KeyAction::key(K::Right),
                Control::Button(B::LeftShoulder) => KeyAction::Key(K::Tab, vec![Modifier::Control, Modifier::Shift]),
                Control::Button(B::RightShoulder) => KeyAction::Key(K::Tab, vec![Modifier::Control]),
                _ => KeyAction::None,
            }
        };
        KeyMouseMapping {
            enabled: false,
            target: EmulationTarget::Local,
            left_stick: StickRole::Scroll,
            right_stick: StickRole::Mouse,
            mouse_speed: 1400.0,
            scroll_speed: 12.0,
            curve: 2.0,
            stick_deadzone: 0.15,
            bindings: Self::bindable_controls()
                .into_iter()
                .map(|c| KeyBinding { control: c, action: preset(c) })
                .collect(),
        }
    }
}

// Missing fields fall back to the defaults instead of failing the whole file.
impl<'de> Deserialize<'de> for KeyMouseMapping {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Partial {
            enabled: Option<bool>,
            target: Option<EmulationTarget>,
            left_stick: Option<StickRole>,
            right_stick: Option<StickRole>,
            mouse_speed: Option<f64>,
            scroll_speed: Option<f64>,
            curve: Option<f64>,
            stick_deadzone: Option<f32>,
            bindings: Option<Vec<KeyBinding>>,
        }
        let p = Partial::deserialize(d)?;
        let def = KeyMouseMapping::default();
        let mut bindings = p.bindings.unwrap_or_else(|| def.bindings.clone());
        for b in &def.bindings {
            if !bindings.iter().any(|x| x.control == b.control) {
                bindings.push(b.clone());
            }
        }
        Ok(KeyMouseMapping {
            enabled: p.enabled.unwrap_or(def.enabled),
            target: p.target.unwrap_or(def.target),
            left_stick: p.left_stick.unwrap_or(def.left_stick),
            right_stick: p.right_stick.unwrap_or(def.right_stick),
            mouse_speed: p.mouse_speed.unwrap_or(def.mouse_speed),
            scroll_speed: p.scroll_speed.unwrap_or(def.scroll_speed),
            curve: p.curve.unwrap_or(def.curve),
            stick_deadzone: p.stick_deadzone.unwrap_or(def.stick_deadzone),
            bindings,
        })
    }
}
