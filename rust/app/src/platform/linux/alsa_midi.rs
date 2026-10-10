//! ALSA sequencer client "PadBridge" with one port used in both directions:
//! messages from pads or a peer are played out to subscribers (e.g. Bitwig's input),
//! and whatever apps send to the port (e.g. clock) is handed to `on_input`.
//!
//! Bitwig on Linux only lists ALSA *rawmidi* devices, not sequencer ports, so
//! the port is also linked both ways to a `snd-virmidi` device ("VirMIDI n-0")
//! whenever one exists; DAWs then see that as a normal MIDI interface.
//!
//! `Seq` must not be used from two threads at once, so one thread owns it and
//! does both directions; outgoing messages reach it through a channel plus a
//! wake-up socket that is polled together with the sequencer.

use std::ffi::CString;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixDatagram;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

use alsa::poll::Descriptors;
use alsa::seq::{Addr, ClientIter, MidiEvent, PortCap, PortIter, PortSubscribe, PortType, Seq};
use alsa::Direction;

pub struct AlsaPort {
    tx: Sender<Vec<u8>>,
    wake: UnixDatagram,
}

impl AlsaPort {
    pub fn start(on_input: impl FnMut(Vec<u8>) + Send + 'static) -> Result<Self, String> {
        fn err(what: &'static str) -> impl Fn(alsa::Error) -> String {
            move |e| format!("{what}: {e}")
        }
        let seq = Seq::open(None, None, true).map_err(err("open sequencer"))?;
        let name = CString::new("PadBridge").unwrap();
        seq.set_client_name(&name).map_err(err("set client name"))?;
        let port = seq
            .create_simple_port(
                &name,
                PortCap::READ | PortCap::SUBS_READ | PortCap::WRITE | PortCap::SUBS_WRITE,
                PortType::MIDI_GENERIC | PortType::APPLICATION,
            )
            .map_err(err("create port"))?;

        let (tx, rx) = channel();
        let (wake, wake_rx) = UnixDatagram::pair().map_err(|e| format!("wake socket: {e}"))?;
        wake_rx.set_nonblocking(true).map_err(|e| format!("wake socket: {e}"))?;
        std::thread::Builder::new()
            .name("alsa-midi".into())
            .spawn(move || run(seq, port, rx, wake_rx, on_input))
            .map_err(|e| format!("spawn: {e}"))?;
        Ok(Self { tx, wake })
    }
}

impl crate::platform::MidiPort for AlsaPort {
    fn send(&self, message: &[u8]) {
        if self.tx.send(message.to_vec()).is_ok() {
            let _ = self.wake.send(&[1]);
        }
    }
}

fn run(seq: Seq, port: i32, rx: Receiver<Vec<u8>>, wake: UnixDatagram, mut on_input: impl FnMut(Vec<u8>)) {
    let mut encoder = MidiEvent::new(64).expect("snd_midi_event");
    let decoder = MidiEvent::new(64).expect("snd_midi_event");
    decoder.enable_running_status(false);
    let mut buf = [0u8; 64];
    let ours = Addr { client: seq.client_id().unwrap_or(-1), port };
    let mut virmidi: Option<Addr> = None;
    let mut next_scan = Instant::now();

    loop {
        if Instant::now() >= next_scan {
            next_scan = Instant::now() + VIRMIDI_SCAN;
            let found = find_virmidi(&seq);
            if found != virmidi {
                if let Some(target) = found {
                    link(&seq, ours, target);
                    eprintln!("padbridge: MIDI mit VirMIDI {}:{} verbunden", target.client, target.port);
                }
                virmidi = found;
            }
        }

        let mut fds = (&seq, Some(Direction::Capture)).get().unwrap_or_default();
        fds.push(libc::pollfd { fd: wake.as_raw_fd(), events: libc::POLLIN, revents: 0 });
        unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 1000) };
        while wake.recv(&mut buf).is_ok() {}

        while let Ok(message) = rx.try_recv() {
            encoder.reset_encode();
            if let Ok((_, Some(mut ev))) = encoder.encode(&message) {
                ev.set_source(port);
                ev.set_subs();
                ev.set_direct();
                let _ = seq.event_output_direct(&mut ev);
            }
        }

        let mut input = seq.input();
        while input.event_input_pending(true).unwrap_or(0) > 0 {
            let Ok(mut ev) = input.event_input() else { break };
            if let Ok(n) = decoder.decode(&mut buf, &mut ev) {
                if n > 0 {
                    on_input(buf[..n].to_vec());
                }
            }
        }
    }
}

/// How often to look for a snd-virmidi device (module loaded later, reboot...).
const VIRMIDI_SCAN: Duration = Duration::from_secs(3);

/// First port of a snd-virmidi card; its ports are named "VirMIDI <card>-<n>".
fn find_virmidi(seq: &Seq) -> Option<Addr> {
    ClientIter::new(seq)
        .flat_map(|client| PortIter::new(seq, client.get_client()).collect::<Vec<_>>())
        .find(|port| port.get_name().is_ok_and(|name| name.starts_with("VirMIDI")))
        .map(|port| port.addr())
}

/// Subscribes both directions; "already subscribed" errors are fine.
fn link(seq: &Seq, ours: Addr, target: Addr) {
    for (from, to) in [(ours, target), (target, ours)] {
        if let Ok(sub) = PortSubscribe::empty() {
            sub.set_sender(from);
            sub.set_dest(to);
            let _ = seq.subscribe_port(&sub);
        }
    }
}
