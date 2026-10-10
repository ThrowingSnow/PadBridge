//! Controller -> MIDI mapping and MIDI clock following.

mod clock;
mod mapper;
mod mapping;

pub use clock::{ClockMessage, MidiClockTracker, RingCue};
pub use mapper::{apply_deadzone, value7, MidiMapper, MidiMessage};
pub use mapping::{ButtonMode, MidiBinding, MidiMapping, MidiMessageKind, MidiTarget, VelocitySource};
