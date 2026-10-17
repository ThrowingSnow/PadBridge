//! macOS (10.13+): Xbox 360 pads via libusb (no kernel extension; the 360 pad
//! is not a HID device, so nothing else claims it), DualShock 4 via HID, MIDI via CoreMIDI virtual ports,
//! keyboard/mouse via CGEvent (needs the Accessibility permission).
//!
//! A virtual game pad for a peer is not possible on macOS without a driver,
//! so the receiver here accepts MIDI and keyboard/mouse only.

mod cg_events;
mod coremidi_devices;
mod coremidi_port;
mod hid_pads;
mod usb_pads;

use std::io;

use super::{EventSink, KeyMouseOut, MidiPort, VirtualKeyMouse, VirtualPad};

pub fn start_pads(events: &EventSink) {
    usb_pads::start(super::sink(events));
    hid_pads::start(super::sink(events));
}

/// Starts listening to the system's MIDI devices.
pub fn start_midi_devices(events: &EventSink) -> Option<Box<dyn super::DeviceOut>> {
    Some(Box::new(coremidi_devices::start(super::sink(events))))
}

pub fn open_midi(events: &EventSink) -> io::Result<Box<dyn MidiPort>> {
    let sink = super::sink(events);
    coremidi_port::CoreMidiPort::open(move |bytes| {
        sink(super::Event::MidiIn(bytes));
    })
    .map(|p| Box::new(p) as Box<dyn MidiPort>)
}

pub fn open_keymouse() -> io::Result<Box<dyn KeyMouseOut>> {
    Ok(Box::new(cg_events::Poster::open()?))
}

pub fn create_virtual_pad() -> io::Result<Box<dyn VirtualPad>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "macOS kann ohne Treiber kein virtuelles Gamepad anlegen (MIDI und Tastatur/Maus gehen)",
    ))
}

pub fn create_virtual_keymouse() -> io::Result<Box<dyn VirtualKeyMouse>> {
    Ok(Box::new(cg_events::Poster::open()?))
}
