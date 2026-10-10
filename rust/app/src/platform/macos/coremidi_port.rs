//! Virtual CoreMIDI ports named "PadBridge": a source (pads -> DAW) and a
//! destination (DAW -> us, e.g. clock). Uses the MIDI 1.0 packet API, which
//! exists on 10.13 (the MIDIEventList API needs macOS 11).

use std::io;

use coremidi::{Client, PacketBuffer, PacketList, VirtualDestination, VirtualSource};
use padbridge_core::protocol::midi_len;

pub struct CoreMidiPort {
    // Kept alive for as long as the ports exist.
    _client: Client,
    source: VirtualSource,
    _destination: VirtualDestination,
}

// CoreMIDI object references may be used from any thread.
unsafe impl Send for CoreMidiPort {}

impl CoreMidiPort {
    pub fn open(mut on_input: impl FnMut(Vec<u8>) + Send + 'static) -> io::Result<Self> {
        let err = |what: &str, status: i32| io::Error::other(format!("{what} (OSStatus {status})"));
        let client = Client::new("PadBridge").map_err(|s| err("MIDI-Client", s))?;
        let source = client.virtual_source("PadBridge").map_err(|s| err("MIDI-Quelle", s))?;
        let destination = client
            .virtual_destination("PadBridge", move |packets: &PacketList| {
                for packet in packets.iter() {
                    for message in split(packet.data()) {
                        on_input(message);
                    }
                }
            })
            .map_err(|s| err("MIDI-Ziel", s))?;
        Ok(CoreMidiPort { _client: client, source, _destination: destination })
    }
}

impl crate::platform::MidiPort for CoreMidiPort {
    fn send(&self, message: &[u8]) {
        let _ = self.source.received(&PacketBuffer::new(0, message));
    }
}

/// One CoreMIDI packet may hold several messages; split them. SysEx is skipped
/// (nothing here uses it yet), real-time bytes may sit anywhere.
fn split(data: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let status = data[i];
        if status & 0x80 == 0 {
            i += 1; // stray data byte (running status is not used by CoreMIDI senders)
        } else if status == 0xF0 {
            while i < data.len() && data[i] != 0xF7 {
                i += 1;
            }
            i += 1;
        } else {
            let n = midi_len(status);
            if i + n <= data.len() {
                out.push(data[i..i + n].to_vec());
            }
            i += n;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::split;

    #[test]
    fn splits_packets_into_messages() {
        let data = [0xF8, 0xB0, 20, 64, 0xF0, 1, 2, 0xF7, 0xF2, 0, 1, 0xFA];
        assert_eq!(split(&data), vec![vec![0xF8], vec![0xB0, 20, 64], vec![0xF2, 0, 1], vec![0xFA]]);
    }
}
