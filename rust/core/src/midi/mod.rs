//! Controller -> MIDI mapping, MIDI clock following and external MIDI devices.

mod clock;
mod mapper;
mod mapping;
pub mod digitakt;
pub mod models;
pub mod surface;

pub use clock::{ClockMessage, MidiClockTracker, RingCue};
pub use mapper::{apply_deadzone, value7, MidiMapper, MidiMessage};
pub use mapping::{ButtonMode, MidiBinding, MidiMapping, MidiMessageKind, MidiTarget, VelocitySource};
