//! Placeholder for platforms without a backend yet (macOS until stage 3).

use std::io;

use super::{EventSink, KeyMouseOut, MidiPort, VirtualKeyMouse, VirtualPad};

fn unsupported() -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, "auf diesem System noch nicht verfügbar")
}

pub fn start_pads(_events: &EventSink) {}

pub fn open_midi(_events: &EventSink) -> io::Result<Box<dyn MidiPort>> {
    Err(unsupported())
}

pub fn open_keymouse() -> io::Result<Box<dyn KeyMouseOut>> {
    Err(unsupported())
}

pub fn create_virtual_pad() -> io::Result<Box<dyn VirtualPad>> {
    Err(unsupported())
}

pub fn create_virtual_keymouse() -> io::Result<Box<dyn VirtualKeyMouse>> {
    Err(unsupported())
}
