//! Windows: Xbox pads via XInput, DualShock 4 via HID, keyboard/mouse via
//! SendInput, MIDI devices via WinMM (midir).
//!
//! Windows has no virtual MIDI ports without a driver, so the "PadBridge"
//! port is one the user creates in loopMIDI (see midi_port). A virtual game
//! pad needs ViGEmBus and is not there yet.

mod midi_devices;
mod midi_port;
mod send_input;
mod xinput_pads;

use std::io;

use super::{EventSink, KeyMouseOut, MidiPort, VirtualKeyMouse, VirtualPad};

pub fn start_pads(events: &EventSink) {
    xinput_pads::start(super::sink(events));
    super::hid_pads::start(super::sink(events));
}

/// Starts listening to the system's MIDI devices.
pub fn start_midi_devices(events: &EventSink) -> Option<Box<dyn super::DeviceOut>> {
    Some(Box::new(midi_devices::start(super::sink(events))))
}

pub fn open_midi(events: &EventSink) -> io::Result<Box<dyn MidiPort>> {
    let sink = super::sink(events);
    midi_port::LoopMidiPort::open(move |bytes| {
        sink(super::Event::MidiIn(bytes));
    })
    .map(|p| Box::new(p) as Box<dyn MidiPort>)
}

pub fn open_keymouse() -> io::Result<Box<dyn KeyMouseOut>> {
    Ok(Box::new(send_input::Poster::default()))
}

pub fn create_virtual_pad() -> io::Result<Box<dyn VirtualPad>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "virtuelles Gamepad unter Windows braucht ViGEmBus (kommt noch; MIDI und Tastatur/Maus gehen)",
    ))
}

pub fn create_virtual_keymouse() -> io::Result<Box<dyn VirtualKeyMouse>> {
    Ok(Box::new(send_input::Poster::default()))
}

/// Our own ports: "PadBridge" and the ones for a peer's devices
/// ("PadBridge <device>", "PadBridge In").
fn is_ours(name: &str) -> bool {
    name == "PadBridge" || name.starts_with("PadBridge ")
}
