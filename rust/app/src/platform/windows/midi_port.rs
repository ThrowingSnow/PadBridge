//! The "PadBridge" port on Windows. Windows cannot create virtual MIDI ports
//! without a driver, so PadBridge uses ports made in loopMIDI (Tobias
//! Erichsen) by name:
//!
//! - "PadBridge": we play into it, the DAW reads it.
//! - "PadBridge In" (optional): the DAW plays into it (e.g. clock), we read it.
//!   A loopMIDI port echoes what goes in, so one port cannot carry both ways.
//! - "PadBridge <device>" (optional) for each MIDI device of the peer.

use std::collections::HashMap;
use std::io;
use std::sync::Mutex;

use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};

const OUT: &str = "PadBridge";
const IN: &str = "PadBridge In";

pub struct LoopMidiPort {
    out: Mutex<MidiOutputConnection>,
    /// Ports for the peer's MIDI devices; None if no such loopMIDI port exists.
    devices: Mutex<HashMap<String, Option<MidiOutputConnection>>>,
    _input: Option<MidiInputConnection<()>>,
}

impl LoopMidiPort {
    pub fn open(mut on_input: impl FnMut(Vec<u8>) + Send + 'static) -> io::Result<Self> {
        let out = connect_out(OUT)?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "kein MIDI-Port „PadBridge“: in loopMIDI einen Port mit genau diesem Namen anlegen",
            )
        })?;
        let mut input = MidiInput::new("PadBridge").map_err(io::Error::other)?;
        input.ignore(Ignore::None);
        let port = input.ports().into_iter().find(|p| input.port_name(p).as_deref() == Ok(IN));
        let input = match port {
            Some(port) => Some(
                input
                    .connect(&port, IN, move |_, message, _| on_input(message.to_vec()), ())
                    .map_err(|e| io::Error::other(format!("„{IN}“: {e}")))?,
            ),
            None => None,
        };
        Ok(LoopMidiPort { out: Mutex::new(out), devices: Mutex::default(), _input: input })
    }
}

/// None if there is no output port of that name.
fn connect_out(name: &str) -> io::Result<Option<MidiOutputConnection>> {
    let output = MidiOutput::new("PadBridge").map_err(io::Error::other)?;
    let Some(port) = output.ports().into_iter().find(|p| output.port_name(p).as_deref() == Ok(name)) else {
        return Ok(None);
    };
    output.connect(&port, name).map(Some).map_err(|e| io::Error::other(format!("„{name}“: {e}")))
}

impl crate::platform::MidiPort for LoopMidiPort {
    fn send(&self, message: &[u8]) {
        let _ = self.out.lock().unwrap().send(message);
    }

    fn send_device(&self, device: &str, message: &[u8]) {
        let mut devices = self.devices.lock().unwrap();
        let port = devices
            .entry(device.to_string())
            .or_insert_with(|| connect_out(&crate::platform::device_port_name(device)).ok().flatten());
        if let Some(port) = port {
            let _ = port.send(message);
        }
    }
}
