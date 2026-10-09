//! ALSA sequencer client "PadBridge" with one port used in both directions:
//! messages from the Mac are played out to subscribers (e.g. Bitwig's input),
//! and whatever apps send to the port (e.g. clock) is handed to `on_input`.
//!
//! `Seq` must not be used from two threads at once, so one thread owns it and
//! does both directions; outgoing messages reach it through a channel plus a
//! wake-up socket that is polled together with the sequencer.

use std::ffi::CString;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixDatagram;
use std::sync::mpsc::{channel, Receiver, Sender};

use alsa::poll::Descriptors;
use alsa::seq::{MidiEvent, PortCap, PortType, Seq};
use alsa::Direction;

pub struct MidiPort {
    tx: Sender<Vec<u8>>,
    wake: UnixDatagram,
}

impl MidiPort {
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

    pub fn send(&self, message: Vec<u8>) {
        if self.tx.send(message).is_ok() {
            let _ = self.wake.send(&[1]);
        }
    }
}

fn run(seq: Seq, port: i32, rx: Receiver<Vec<u8>>, wake: UnixDatagram, mut on_input: impl FnMut(Vec<u8>)) {
    let mut encoder = MidiEvent::new(64).expect("snd_midi_event");
    let decoder = MidiEvent::new(64).expect("snd_midi_event");
    decoder.enable_running_status(false);
    let mut buf = [0u8; 64];

    loop {
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
