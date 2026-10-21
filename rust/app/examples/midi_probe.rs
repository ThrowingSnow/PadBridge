//! Prints every MIDI message from sources whose name contains the argument
//! (macOS), one line each with milliseconds since start. For working out a
//! controller's map: press its controls in a known order.
//!
//!   cargo run --example midi_probe -- "Push 2"

#[cfg(target_os = "macos")]
fn main() {
    use std::time::Instant;

    use coremidi::{Client, PacketList, Sources};

    let filter = std::env::args().nth(1).unwrap_or_default().to_lowercase();
    let client = Client::new("midi_probe").expect("CoreMIDI client");
    let start = Instant::now();
    let mut ports = Vec::new();
    for source in Sources {
        let name = source.display_name().unwrap_or_default();
        if !name.to_lowercase().contains(&filter) {
            continue;
        }
        println!("listening: {name}");
        let shown = name.clone();
        let port = client
            .input_port(&format!("probe {name}"), move |packets: &PacketList| {
                for p in packets.iter() {
                    let data = p.data();
                    // Active sensing and clock would drown everything else.
                    if data == [0xFE] || data == [0xF8] {
                        continue;
                    }
                    let hex: Vec<String> = data.iter().map(|b| format!("{b:02X}")).collect();
                    println!("{:>8} {shown}: {}", start.elapsed().as_millis(), hex.join(" "));
                }
            })
            .expect("input port");
        port.connect_source(&source).expect("connect");
        ports.push(port);
    }
    if ports.is_empty() {
        eprintln!("no source matches {filter:?}");
        return;
    }
    loop {
        std::thread::park();
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("midi_probe: macOS only");
}
