use crate::controls::{Control, PadAxis, PadButton};

/// Snapshot of every control on a pad, in canonical units (see `PadAxis`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ControllerState {
    /// Bit `PadButton::index()` set = pressed.
    buttons: u16,
    axes: [f32; 6],
}

impl ControllerState {
    pub fn new(pressed: &[PadButton], axes: &[(PadAxis, f32)]) -> Self {
        let mut s = Self::default();
        for &b in pressed {
            s.set_pressed(b, true);
        }
        for &(a, v) in axes {
            s.set_axis(a, v);
        }
        s
    }

    pub fn is_pressed(&self, button: PadButton) -> bool {
        self.buttons & (1 << button.index()) != 0
    }

    pub fn set_pressed(&mut self, button: PadButton, pressed: bool) {
        if pressed {
            self.buttons |= 1 << button.index();
        } else {
            self.buttons &= !(1 << button.index());
        }
    }

    pub fn pressed(&self) -> impl Iterator<Item = PadButton> + '_ {
        PadButton::ALL.into_iter().filter(|&b| self.is_pressed(b))
    }

    pub fn axis(&self, axis: PadAxis) -> f32 {
        self.axes[axis.index()]
    }

    pub fn set_axis(&mut self, axis: PadAxis, value: f32) {
        self.axes[axis.index()] = value;
    }

    /// Controls whose value differs from `previous`, buttons first, in a stable order.
    pub fn changes(&self, previous: &ControllerState) -> Vec<ControlChange> {
        let buttons = PadButton::ALL
            .into_iter()
            .filter(|&b| self.is_pressed(b) != previous.is_pressed(b))
            .map(|b| ControlChange::Button(b, self.is_pressed(b)));
        let axes = PadAxis::ALL
            .into_iter()
            .filter(|&a| self.axis(a) != previous.axis(a))
            .map(|a| ControlChange::Axis(a, self.axis(a)));
        buttons.chain(axes).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ControlChange {
    Button(PadButton, bool),
    Axis(PadAxis, f32),
}

impl ControlChange {
    pub fn control(&self) -> Control {
        match *self {
            ControlChange::Button(b, _) => Control::Button(b),
            ControlChange::Axis(a, _) => Control::Axis(a),
        }
    }
}
