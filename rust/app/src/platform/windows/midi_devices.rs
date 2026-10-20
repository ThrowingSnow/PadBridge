//! Listens to every MIDI input on the system (hardware such as the Launch
//! Control XL, loopMIDI ports of other apps) except our own "PadBridge" ones.
//!
//! WinMM lets only one app open a device at a time (Windows MIDI Services on
//! Windows 11 lifts that), so while PadBridge listens to a device, a DAW cannot
//! open the same one. Outputs (LEDs) are opened only once something is sent.
//!
//! WinMM sends no hotplug notices; the device list is read again every two
//! seconds. The same thread sends to devices.

use std::collections::BTreeMap;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};

use super::is_ours;
use crate::platform::{ChannelDeviceOut, Event};

const RESCAN: Duration = Duration::from_secs(2);

pub fn start(sink: impl Fn(Event) -> bool + Send + Clone + 'static) -> ChannelDeviceOut {
    let (tx, outgoing) = channel::<(String, Vec<u8>)>();
    let spawned = std::thread::Builder::new().name("midi-devices".into()).spawn(move || {
        // shown name -> connection
        let mut open: BTreeMap<String, MidiInputConnection<()>> = BTreeMap::new();
        let mut outputs: BTreeMap<String, Option<MidiOutputConnection>> = BTreeMap::new();
        // Names that failed to open, not retried until they are unplugged.
        let mut failed: Vec<String> = Vec::new();
        let mut reported: Option<Vec<String>> = None;
        loop {
            let present = input_names();
            open.retain(|name, _| present.contains(name));
            outputs.retain(|name, _| present.contains(name));
            failed.retain(|name| present.contains(name));
            for (index, name) in present.iter().enumerate() {
                if name.is_empty() || open.contains_key(name) || failed.contains(name) {
                    continue;
                }
                match connect_in(index, name, sink.clone()) {
                    Some(connection) => {
                        open.insert(name.clone(), connection);
                    }
                    None => {
                        failed.push(name.clone());
                        sink(Event::Error(format!(
                            "MIDI-Gerät „{name}“ lässt sich nicht öffnen (von einem anderen Programm belegt?)"
                        )));
                    }
                }
            }
            let names: Vec<String> = open.keys().cloned().collect();
            if reported.as_ref() != Some(&names) {
                if !sink(Event::MidiDevices(names.clone())) {
                    return;
                }
                reported = Some(names);
            }
            if !send_until(Instant::now() + RESCAN, &outgoing, &mut outputs) {
                return;
            }
        }
    });
    if let Err(e) = spawned {
        eprintln!("padbridge: midi-devices thread: {e}");
    }
    ChannelDeviceOut(tx)
}

/// Shown names of all inputs in WinMM order, ours replaced by "" so the
/// index still matches the port list.
fn input_names() -> Vec<String> {
    let Ok(input) = MidiInput::new("PadBridge Geräte") else { return Vec::new() };
    let mut names: Vec<String> = Vec::new();
    for port in input.ports() {
        let base = input.port_name(&port).unwrap_or_else(|_| "MIDI".into());
        let name = if is_ours(&base) { String::new() } else { unique_name(&base, names.iter().map(String::as_str)) };
        names.push(name);
    }
    names
}

fn connect_in(
    index: usize,
    name: &str,
    sink: impl Fn(Event) -> bool + Send + 'static,
) -> Option<MidiInputConnection<()>> {
    let mut input = MidiInput::new("PadBridge Geräte").ok()?;
    input.ignore(Ignore::None);
    let port = input.ports().into_iter().nth(index)?;
    let shown = name.to_string();
    input
        .connect(
            &port,
            name,
            move |_, message, _| {
                sink(Event::DeviceMidi(shown.clone(), message.to_vec()));
            },
            (),
        )
        .ok()
}

/// Sends what is queued until `deadline`; false once the engine is gone.
fn send_until(
    deadline: Instant,
    outgoing: &Receiver<(String, Vec<u8>)>,
    outputs: &mut BTreeMap<String, Option<MidiOutputConnection>>,
) -> bool {
    loop {
        let Some(wait) = deadline.checked_duration_since(Instant::now()) else { return true };
        match outgoing.recv_timeout(wait) {
            Ok((name, message)) => {
                let connection = outputs.entry(name.clone()).or_insert_with(|| connect_out(&name));
                if let Some(connection) = connection {
                    let _ = connection.send(&message);
                }
            }
            Err(RecvTimeoutError::Timeout) => return true,
            Err(RecvTimeoutError::Disconnected) => return false,
        }
    }
}

/// The output with the same shown name as the input.
fn connect_out(name: &str) -> Option<MidiOutputConnection> {
    let output = MidiOutput::new("PadBridge Geräte").ok()?;
    let mut names: Vec<String> = Vec::new();
    let mut found = None;
    for port in output.ports() {
        let base = output.port_name(&port).unwrap_or_else(|_| "MIDI".into());
        let shown = unique_name(&base, names.iter().map(String::as_str));
        if shown == name && !is_ours(&base) {
            found = Some(port);
            break;
        }
        names.push(shown);
    }
    output.connect(&found?, name).ok()
}

/// "Name", or "Name 2", "Name 3"... if that is taken.
fn unique_name<'a>(base: &str, taken: impl Iterator<Item = &'a str> + Clone) -> String {
    let mut name = base.to_string();
    let mut n = 2;
    while taken.clone().any(|t| t == name) {
        name = format!("{base} {n}");
        n += 1;
    }
    name
}
