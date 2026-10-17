//! Linux: pads via evdev (the kernel's xpad driver already speaks the 360
//! protocol), MIDI via an ALSA sequencer port, output via uinput.

mod alsa_devices;
mod alsa_midi;
mod evdev_pads;
mod uinput_keymouse;
mod uinput_pad;

use std::io;

use super::{EventSink, KeyMouseOut, MidiPort, VirtualKeyMouse, VirtualPad};

/// Starts hotplug scanning for game pads; events go to the engine.
pub fn start_pads(events: &EventSink) {
    evdev_pads::start(super::sink(events));
}

/// Starts listening to hardware MIDI devices.
pub fn start_midi_devices(events: &EventSink) -> Option<Box<dyn super::DeviceOut>> {
    Some(Box::new(alsa_devices::start(super::sink(events))))
}

pub fn open_midi(events: &EventSink) -> io::Result<Box<dyn MidiPort>> {
    let sink = super::sink(events);
    alsa_midi::AlsaPort::start(move |bytes| {
        sink(super::Event::MidiIn(bytes));
    })
    .map(|p| Box::new(p) as Box<dyn MidiPort>)
    .map_err(io::Error::other)
}

pub fn open_keymouse() -> io::Result<Box<dyn KeyMouseOut>> {
    Ok(Box::new(uinput_keymouse::KeyMouse::create()?))
}

pub fn create_virtual_pad() -> io::Result<Box<dyn VirtualPad>> {
    Ok(Box::new(uinput_pad::UinputPad::create()?))
}

pub fn create_virtual_keymouse() -> io::Result<Box<dyn VirtualKeyMouse>> {
    Ok(Box::new(uinput_keymouse::KeyMouse::create()?))
}
