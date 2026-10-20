//! Platform-independent core of PadBridge: the canonical controller model,
//! controller -> MIDI and keyboard/mouse mapping, MIDI clock following, the
//! LED ring logic, the network protocol and the settings files.
//!
//! Nothing in here touches USB, MIDI ports, input devices or sockets; the
//! platform crates do that and feed this crate plain values. That keeps it
//! fully unit-testable and identical on macOS (10.13+) and Linux.

pub mod controls;
pub mod device;
pub mod ds4;
pub mod key;
pub mod keymouse;
pub mod midi;
pub mod protocol;
pub mod ring;
pub mod settings;
pub mod state;
pub mod web;
pub mod xbox360;

pub use controls::{Control, PadAxis, PadButton};
pub use device::{Capabilities, ControllerInfo, PlayerIndicator};
pub use state::{ControlChange, ControllerState};
