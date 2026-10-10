use std::collections::{BTreeMap, HashMap};

use super::keycode::KeyCode;
use super::mapping::{KeyAction, KeyMouseMapping, MouseButton, StickRole};
use crate::controls::{Control, PadAxis};
use crate::state::ControllerState;

/// A keyboard/mouse event, independent of the output target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KmEvent {
    Key(KeyCode, bool),
    Button(MouseButton, bool),
    /// Pointer motion in points; x right, y **down** (screen convention).
    Move {
        dx: i32,
        dy: i32,
    },
    /// Wheel motion in 1/120 detents (hi-res units); positive = up / right.
    Scroll {
        dx: i32,
        dy: i32,
    },
}

/// One keyboard/mouse event in the network wire format: type 1 key/button
/// (a = Linux code, b = 1 down / 0 up), type 2 move (a = dx, b = dy),
/// type 3 scroll (a = vertical, b = horizontal, 1/120 detents).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WireInput {
    pub kind: u8,
    pub a: i16,
    pub b: i16,
}

fn clamp16(v: i32) -> i16 {
    v.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

impl From<KmEvent> for WireInput {
    fn from(e: KmEvent) -> Self {
        match e {
            KmEvent::Key(key, down) => WireInput { kind: 1, a: key.linux_code() as i16, b: down as i16 },
            KmEvent::Button(button, down) => WireInput { kind: 1, a: button.linux_code() as i16, b: down as i16 },
            KmEvent::Move { dx, dy } => WireInput { kind: 2, a: clamp16(dx), b: clamp16(dy) },
            KmEvent::Scroll { dx, dy } => WireInput { kind: 3, a: clamp16(dy), b: clamp16(dx) },
        }
    }
}

/// Turns controller states into keyboard/mouse events according to a mapping.
///
/// - `process` handles discrete things (buttons, triggers, stick-as-keys) and
///   is called on every input report.
/// - `tick` integrates continuous things (pointer, scroll) at a fixed rate,
///   because a pad held still at an angle sends no reports.
///
/// Keys and mouse buttons are reference counted, so two controls mapped to the
/// same key (e.g. D-pad up and a stick in arrow mode) do not release each other.
#[derive(Debug, Clone)]
pub struct KeyMouseMapper {
    pub mapping: KeyMouseMapping,
    active: HashMap<Control, KeyAction>,
    /// Stick direction slot ("Lu", "Rd", ...) -> the key it is holding.
    stick_keys: BTreeMap<&'static str, KeyCode>,
    key_count: HashMap<KeyCode, u32>,
    button_count: HashMap<MouseButton, u32>,
    move_remainder: (f64, f64),
    scroll_remainder: (f64, f64),
}

const TRIGGER_PRESS: f32 = 0.55;
const TRIGGER_RELEASE: f32 = 0.45;
const STICK_KEY_THRESHOLD: f32 = 0.5;

impl KeyMouseMapper {
    pub fn new(mapping: KeyMouseMapping) -> Self {
        KeyMouseMapper {
            mapping,
            active: HashMap::new(),
            stick_keys: BTreeMap::new(),
            key_count: HashMap::new(),
            button_count: HashMap::new(),
            move_remainder: (0.0, 0.0),
            scroll_remainder: (0.0, 0.0),
        }
    }

    pub fn is_holding_anything(&self) -> bool {
        !self.key_count.is_empty() || !self.button_count.is_empty()
    }

    // MARK: Discrete

    pub fn process(&mut self, state: &ControllerState) -> Vec<KmEvent> {
        let mut out = Vec::new();
        let bindings = self.mapping.bindings.clone();
        for binding in bindings.iter().filter(|b| b.action != KeyAction::None) {
            let was_down = self.active.contains_key(&binding.control);
            let is_down = match binding.control {
                Control::Button(b) => state.is_pressed(b),
                Control::Axis(a) => state.axis(a) > if was_down { TRIGGER_RELEASE } else { TRIGGER_PRESS },
            };
            if is_down && !was_down {
                self.active.insert(binding.control, binding.action.clone());
                out.extend(self.press(&binding.action));
            } else if !is_down {
                if let Some(action) = self.active.remove(&binding.control) {
                    out.extend(self.release(&action));
                }
            }
        }
        out.extend(self.stick_as_keys(
            state,
            PadAxis::LeftX,
            PadAxis::LeftY,
            self.mapping.left_stick,
            ["Lu", "Ld", "Ll", "Lr"],
        ));
        out.extend(self.stick_as_keys(
            state,
            PadAxis::RightX,
            PadAxis::RightY,
            self.mapping.right_stick,
            ["Ru", "Rd", "Rl", "Rr"],
        ));
        out
    }

    fn stick_as_keys(
        &mut self,
        state: &ControllerState,
        x_axis: PadAxis,
        y_axis: PadAxis,
        role: StickRole,
        slots: [&'static str; 4],
    ) -> Vec<KmEvent> {
        let keys = role.keys();
        let (x, y, t) = (state.axis(x_axis), state.axis(y_axis), STICK_KEY_THRESHOLD);
        let wanted = [y > t, y < -t, x < -t, x > t];
        let mut out = Vec::new();
        for i in 0..4 {
            let (slot, on, key) = (slots[i], wanted[i], keys.map(|k| k[i]));
            // Released, or the role changed while held: let go of what was pressed...
            if let Some(&held) = self.stick_keys.get(slot) {
                if !on || key != Some(held) {
                    self.stick_keys.remove(slot);
                    out.extend(self.key_up(held));
                }
            }
            // ...then press the current key (which may be the new role's).
            if let (true, None, Some(key)) = (on, self.stick_keys.get(slot), key) {
                self.stick_keys.insert(slot, key);
                out.extend(self.key_down(key));
            }
        }
        out
    }

    // MARK: Continuous

    pub fn tick(&mut self, state: &ControllerState, dt: f64) -> Vec<KmEvent> {
        let mut out = Vec::new();
        let sticks = [
            (self.mapping.left_stick, state.axis(PadAxis::LeftX), state.axis(PadAxis::LeftY)),
            (self.mapping.right_stick, state.axis(PadAxis::RightX), state.axis(PadAxis::RightY)),
        ];
        for (role, x, y) in sticks {
            match role {
                StickRole::Mouse => {
                    let (vx, vy) = self.velocity(x, y, self.mapping.mouse_speed);
                    self.move_remainder.0 += vx * dt;
                    self.move_remainder.1 -= vy * dt; // stick up = pointer up = negative screen y
                    let (dx, dy) = (self.move_remainder.0 as i32, self.move_remainder.1 as i32);
                    self.move_remainder.0 -= dx as f64;
                    self.move_remainder.1 -= dy as f64;
                    if dx != 0 || dy != 0 {
                        out.push(KmEvent::Move { dx, dy });
                    }
                }
                StickRole::Scroll => {
                    let (vx, vy) = self.velocity(x, y, self.mapping.scroll_speed * 120.0);
                    self.scroll_remainder.0 += vx * dt;
                    self.scroll_remainder.1 += vy * dt;
                    let (dx, dy) = (self.scroll_remainder.0 as i32, self.scroll_remainder.1 as i32);
                    self.scroll_remainder.0 -= dx as f64;
                    self.scroll_remainder.1 -= dy as f64;
                    if dx != 0 || dy != 0 {
                        out.push(KmEvent::Scroll { dx, dy });
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// Radial deadzone + response curve, so diagonals are not faster than straight lines.
    pub fn velocity(&self, x: f32, y: f32, speed: f64) -> (f64, f64) {
        let magnitude = ((x * x + y * y) as f64).sqrt().min(1.0);
        let dz = (self.mapping.stick_deadzone as f64).clamp(0.0, 0.9);
        if magnitude <= dz {
            return (0.0, 0.0);
        }
        let scaled = ((magnitude - dz) / (1.0 - dz)).powf(self.mapping.curve.max(0.1)) * speed;
        (x as f64 / magnitude * scaled, y as f64 / magnitude * scaled)
    }

    // MARK: Release

    /// Lets go of everything; call when disabling or switching target.
    pub fn release_all(&mut self) -> Vec<KmEvent> {
        let mut out: Vec<KmEvent> = self.key_count.keys().map(|&k| KmEvent::Key(k, false)).collect();
        out.extend(self.button_count.keys().map(|&b| KmEvent::Button(b, false)));
        self.active.clear();
        self.stick_keys.clear();
        self.key_count.clear();
        self.button_count.clear();
        out
    }

    // MARK: Ref counting

    fn press(&mut self, action: &KeyAction) -> Vec<KmEvent> {
        match action {
            KeyAction::None => Vec::new(),
            KeyAction::Mouse(button) => {
                let n = self.button_count.entry(*button).or_insert(0);
                *n += 1;
                if *n == 1 {
                    vec![KmEvent::Button(*button, true)]
                } else {
                    Vec::new()
                }
            }
            // Modifiers first, so the key arrives already modified.
            KeyAction::Key(key, modifiers) => {
                let mut out: Vec<KmEvent> = modifiers.iter().flat_map(|m| self.key_down(m.key())).collect();
                out.extend(self.key_down(*key));
                out
            }
        }
    }

    fn release(&mut self, action: &KeyAction) -> Vec<KmEvent> {
        match action {
            KeyAction::None => Vec::new(),
            KeyAction::Mouse(button) => match self.button_count.get(button).copied() {
                None => Vec::new(),
                Some(n) if n <= 1 => {
                    self.button_count.remove(button);
                    vec![KmEvent::Button(*button, false)]
                }
                Some(n) => {
                    self.button_count.insert(*button, n - 1);
                    Vec::new()
                }
            },
            KeyAction::Key(key, modifiers) => {
                let mut out = self.key_up(*key);
                out.extend(modifiers.iter().rev().flat_map(|m| self.key_up(m.key())));
                out
            }
        }
    }

    fn key_down(&mut self, key: KeyCode) -> Vec<KmEvent> {
        let n = self.key_count.entry(key).or_insert(0);
        *n += 1;
        if *n == 1 {
            vec![KmEvent::Key(key, true)]
        } else {
            Vec::new()
        }
    }

    fn key_up(&mut self, key: KeyCode) -> Vec<KmEvent> {
        match self.key_count.get(&key).copied() {
            None => Vec::new(),
            Some(n) if n <= 1 => {
                self.key_count.remove(&key);
                vec![KmEvent::Key(key, false)]
            }
            Some(n) => {
                self.key_count.insert(key, n - 1);
                Vec::new()
            }
        }
    }
}
