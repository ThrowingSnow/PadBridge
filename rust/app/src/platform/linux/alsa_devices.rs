//! Listens to hardware MIDI ports (USB devices such as the Launch Control XL)
//! through a second ALSA sequencer client, "PadBridge Geräte". Its only port
//! can be written to but not subscribed by others, so it does not show up as
//! a MIDI output in other apps. A DAW can use the same device at the same time.
//!
//! The source address of every event tells which device sent it. Ports come
//! and go; the thread rescans every two seconds. The same thread sends to
//! devices (e.g. LED updates) directly to their writable port.

use std::collections::BTreeMap;
use std::ffi::CString;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use alsa::poll::Descriptors;
use alsa::seq::{Addr, ClientIter, MidiEvent, PortCap, PortIter, PortSubscribe, PortType, Seq};
use alsa::Direction;

use crate::platform::{ChannelDeviceOut, Event};

const RESCAN: Duration = Duration::from_secs(2);

pub fn start(sink: impl Fn(Event) -> bool + Send + 'static) -> ChannelDeviceOut {
    let (tx, outgoing) = channel();
    let spawned = std::thread::Builder::new().name("midi-devices".into()).spawn(move || {
        if let Err(e) = run(&sink, outgoing) {
            sink(Event::Error(format!("MIDI-Geräte nicht verfügbar: {e}")));
        }
    });
    if let Err(e) = spawned {
        eprintln!("padbridge: midi-devices thread: {e}");
    }
    ChannelDeviceOut(tx)
}

fn run(sink: &impl Fn(Event) -> bool, outgoing: Receiver<(String, Vec<u8>)>) -> Result<(), alsa::Error> {
    let seq = Seq::open(None, None, true)?;
    seq.set_client_name(&CString::new("PadBridge Geräte").unwrap())?;
    let port = seq.create_simple_port(
        &CString::new("in").unwrap(),
        PortCap::WRITE | PortCap::READ,
        PortType::MIDI_GENERIC | PortType::APPLICATION,
    )?;
    let ours = Addr { client: seq.client_id()?, port };
    let decoder = MidiEvent::new(MAX_EVENT as u32)?;
    let mut encoder = MidiEvent::new(MAX_EVENT as u32)?;
    let mut targets: BTreeMap<String, Addr> = BTreeMap::new();
    decoder.enable_running_status(false);
    let mut buf = vec![0u8; MAX_EVENT];
    let mut open: BTreeMap<Addr, String> = BTreeMap::new();
    let mut reported: Option<Vec<String>> = None;
    let mut next_scan = Instant::now();

    loop {
        if Instant::now() >= next_scan {
            next_scan = Instant::now() + RESCAN;
            let found = hardware_ports(&seq, ours.client, PortCap::READ | PortCap::SUBS_READ);
            targets = hardware_ports(&seq, ours.client, PortCap::WRITE | PortCap::SUBS_WRITE)
                .into_iter()
                .map(|(addr, name)| (name, addr))
                .collect();
            for (addr, name) in &found {
                if !open.contains_key(addr) {
                    if let Ok(sub) = PortSubscribe::empty() {
                        sub.set_sender(*addr);
                        sub.set_dest(ours);
                        let _ = seq.subscribe_port(&sub); // "already subscribed" is fine
                    }
                    open.insert(*addr, name.clone());
                }
            }
            open.retain(|addr, _| found.contains_key(addr));
            let mut names: Vec<String> = open.values().cloned().collect();
            names.sort();
            if reported.as_ref() != Some(&names) {
                if !sink(Event::MidiDevices(names.clone())) {
                    return Ok(());
                }
                reported = Some(names);
            }
        }

        let mut fds = (&seq, Some(Direction::Capture)).get().unwrap_or_default();
        unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 10) };

        loop {
            match outgoing.try_recv() {
                Ok((name, message)) => {
                    let Some(&dest) = targets.get(&name) else { continue };
                    encoder.reset_encode();
                    if let Ok((_, Some(mut ev))) = encoder.encode(&message) {
                        ev.set_source(port);
                        ev.set_dest(dest);
                        ev.set_direct();
                        let _ = seq.event_output_direct(&mut ev);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(()),
            }
        }

        let mut input = seq.input();
        while input.event_input_pending(true).unwrap_or(0) > 0 {
            let Ok(mut ev) = input.event_input() else { break };
            let Some(name) = open.get(&ev.get_source()) else { continue };
            if let Ok(n) = decoder.decode(&mut buf, &mut ev) {
                if n > 0 && !sink(Event::DeviceMidi(name.clone(), buf[..n].to_vec())) {
                    return Ok(());
                }
            }
        }
    }
}

/// Largest event decoded (SysEx included); longer ones are skipped.
const MAX_EVENT: usize = 4096;

/// Hardware ports with `caps`, named after their client ("Launch Control XL"),
/// or after the port when a client has several.
fn hardware_ports(seq: &Seq, own_client: i32, caps: PortCap) -> BTreeMap<Addr, String> {
    let mut out = BTreeMap::new();
    for client in ClientIter::new(seq) {
        let id = client.get_client();
        if id == 0 || id == own_client {
            continue;
        }
        let client_name = client.get_name().unwrap_or("MIDI").to_string();
        if client_name.starts_with("Midi Through") || client_name.starts_with("Virtual Raw MIDI") {
            continue;
        }
        let ports: Vec<_> = PortIter::new(seq, id)
            .filter(|p| {
                p.get_capability().contains(caps)
                    && p.get_type().contains(PortType::HARDWARE)
                    && !p.get_name().is_ok_and(|n| n.starts_with("VirMIDI"))
            })
            .collect();
        for p in &ports {
            let name = if ports.len() == 1 { client_name.clone() } else { p.get_name().unwrap_or("MIDI").to_string() };
            out.insert(p.addr(), name);
        }
    }
    out
}
