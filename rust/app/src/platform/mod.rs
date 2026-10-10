//! Everything that touches the operating system, behind small traits so the
//! engine, network and UI are the same on every platform.
//!
//! - Linux: evdev (pads via xpad), uinput (virtual pad, keyboard/mouse), ALSA.
//! - macOS: comes in stage 3 (libusb, CoreMIDI, CGEvent); until then a stub
//!   that offers nothing, so the UI still builds and runs there.

use std::io;
use std::sync::mpsc::Sender;

use padbridge_core::keymouse::{KmEvent, WireInput};
use padbridge_core::protocol::WireState;
use padbridge_core::{ControllerInfo, ControllerState, PlayerIndicator};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(not(target_os = "linux"))]
mod stub;
#[cfg(not(target_os = "linux"))]
pub use stub::*;

/// What platform code reports to the engine.
pub enum Event {
    PadConnected(ControllerInfo, Box<dyn PadOutput>),
    PadState(String, ControllerState),
    PadGone(String),
    /// Raw MIDI from the local port (e.g. clock from a DAW).
    MidiIn(Vec<u8>),
    Error(String),
}

pub type EventSink = Sender<crate::engine::Msg>;

/// Output side of a physical pad.
pub trait PadOutput: Send {
    /// 0..=1 per motor; `low` is the heavy left motor.
    fn set_rumble(&mut self, low: f32, high: f32);
    fn set_indicator(&mut self, indicator: PlayerIndicator);
}

/// The virtual MIDI port "PadBridge" on this machine.
pub trait MidiPort: Send {
    fn send(&self, message: &[u8]);
}

/// Keyboard/mouse emulation on this machine.
pub trait KeyMouseOut: Send {
    fn post(&mut self, events: &[KmEvent]);
}

/// A virtual game pad driven by a peer (receiver side).
pub trait VirtualPad: Send {
    fn apply(&mut self, state: WireState) -> io::Result<()>;
    /// Everything released, sticks centred.
    fn neutral(&mut self) -> io::Result<()> {
        self.apply(WireState::default())
    }
    /// A descriptor that becomes readable when a game sends force feedback.
    fn poll_fd(&self) -> Option<i32>;
    /// Answers pending force-feedback requests; call when `poll_fd` is readable.
    fn process_force_feedback(&mut self);
    /// Current motor levels (heavy, light) 0..=255 requested by games.
    fn rumble(&mut self) -> (u8, u8);
    fn describe(&mut self) -> String;
}

/// A virtual keyboard/mouse driven by a peer (receiver side).
pub trait VirtualKeyMouse: Send {
    fn apply(&mut self, events: &[WireInput]) -> io::Result<()>;
    fn holding(&self) -> bool;
    fn release_all(&mut self) -> io::Result<()>;
}

/// Converts platform events into engine messages.
pub(crate) fn sink(tx: &EventSink) -> impl Fn(Event) -> bool + Send + Clone + 'static {
    let tx = tx.clone();
    move |event| tx.send(crate::engine::Msg::Platform(event)).is_ok()
}
