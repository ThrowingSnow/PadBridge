//! Controller -> keyboard/mouse emulation, independent of the output target.

mod keycode;
mod mapper;
mod mapping;

pub use keycode::KeyCode;
pub use mapper::{KeyMouseMapper, KmEvent, WireInput};
pub use mapping::{EmulationTarget, KeyAction, KeyBinding, KeyMouseMapping, Modifier, MouseButton, StickRole};
