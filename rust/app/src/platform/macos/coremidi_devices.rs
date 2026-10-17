//! Listens to every MIDI source on the system (hardware such as the Launch
//! Control XL, other apps' virtual ports) except our own "PadBridge" port.
//! CoreMIDI lets several clients read one source, so a DAW can use the same
//! device at the same time.
//!
//! One input port per source, so every callback knows which device it is.
//! Sources come and go; a thread rescans every two seconds. The same thread
//! sends to devices (destinations with the same name), e.g. LED updates.

use std::collections::BTreeMap;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

use core_foundation::runloop::{kCFRunLoopDefaultMode, CFRunLoop};
use coremidi::{Client, Destination, Destinations, InputPort, PacketBuffer, Sources};
use padbridge_core::midi::surface::MidiStream;

use crate::platform::{ChannelDeviceOut, Event};

const RESCAN: Duration = Duration::from_secs(2);
/// How often queued output is sent.
const SEND_TICK: Duration = Duration::from_millis(10);

pub fn start(sink: impl Fn(Event) -> bool + Send + Clone + 'static) -> ChannelDeviceOut {
    let (tx, outgoing) = channel::<(String, Vec<u8>)>();
    let spawned = std::thread::Builder::new().name("midi-devices".into()).spawn(move || {
        let client = match Client::new("PadBridge Geräte") {
            Ok(c) => c,
            Err(status) => {
                sink(Event::Error(format!("MIDI-Geräte nicht verfügbar (OSStatus {status})")));
                return;
            }
        };
        let output = match client.output_port("out") {
            Ok(p) => p,
            Err(status) => {
                sink(Event::Error(format!("MIDI-Ausgang zu Geräten nicht verfügbar (OSStatus {status})")));
                return;
            }
        };
        // unique id -> (shown name, port)
        let mut open: BTreeMap<u32, (String, InputPort)> = BTreeMap::new();
        let mut reported: Option<Vec<String>> = None;
        loop {
            let destinations = find_destinations();
            let mut present = Vec::new();
            for source in Sources {
                let Some(id) = source.unique_id() else { continue };
                let base = source.display_name().unwrap_or_else(|| "MIDI".into());
                if base == "PadBridge" {
                    continue;
                }
                present.push(id);
                if open.contains_key(&id) {
                    continue;
                }
                let name = unique_name(&base, open.values().map(|(n, _)| n.as_str()));
                let tx = sink.clone();
                let shown = name.clone();
                let mut stream = MidiStream::new();
                let port = client.input_port(&format!("in {id}"), move |packets: &coremidi::PacketList| {
                    for packet in packets.iter() {
                        for message in stream.feed(packet.data()) {
                            tx(Event::DeviceMidi(shown.clone(), message));
                        }
                    }
                });
                match port {
                    Ok(port) if port.connect_source(&source).is_ok() => {
                        open.insert(id, (name, port));
                    }
                    _ => {
                        sink(Event::Error(format!("MIDI-Gerät „{base}“ lässt sich nicht öffnen")));
                    }
                }
            }
            open.retain(|id, _| present.contains(id));
            let mut names: Vec<String> = open.values().map(|(n, _)| n.clone()).collect();
            names.sort();
            if reported.as_ref() != Some(&names) {
                if !sink(Event::MidiDevices(names.clone())) {
                    return;
                }
                reported = Some(names);
            }
            // Let CoreMIDI deliver setup changes to this thread, and send what is queued.
            let next_scan = Instant::now() + RESCAN;
            while Instant::now() < next_scan {
                let started = Instant::now();
                CFRunLoop::run_in_mode(unsafe { kCFRunLoopDefaultMode }, SEND_TICK, false);
                if !send_queued(&outgoing, &output, &destinations) {
                    return;
                }
                if let Some(rest) = SEND_TICK.checked_sub(started.elapsed()) {
                    std::thread::sleep(rest);
                }
            }
        }
    });
    if let Err(e) = spawned {
        eprintln!("padbridge: midi-devices thread: {e}");
    }
    ChannelDeviceOut(tx)
}

/// False once the engine is gone.
fn send_queued(outgoing: &Receiver<(String, Vec<u8>)>, output: &coremidi::OutputPort, to: &BTreeMap<String, Destination>) -> bool {
    loop {
        match outgoing.try_recv() {
            Ok((name, message)) => {
                if let Some(dest) = to.get(&name) {
                    let _ = output.send(dest, &PacketBuffer::new(0, &message));
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => return true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return false,
        }
    }
}

/// Destinations by the same names the sources get.
fn find_destinations() -> BTreeMap<String, Destination> {
    let mut out: BTreeMap<String, Destination> = BTreeMap::new();
    for dest in Destinations {
        let base = dest.display_name().unwrap_or_else(|| "MIDI".into());
        if base == "PadBridge" {
            continue;
        }
        let name = unique_name(&base, out.keys().map(String::as_str));
        out.insert(name, dest);
    }
    out
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
