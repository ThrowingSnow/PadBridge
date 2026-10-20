//! PadBridge: game controllers as MIDI, keyboard/mouse and network gamepads.
//!
//! `padbridge`                     window
//! `padbridge --headless --listen --receive-only`   service mode (replaces padbridge-netpad)

mod engine;
mod net;
mod platform;
mod ui;
mod web;

const USAGE: &str = "\
usage: padbridge [--headless] [--listen] [--receive-only]

  --headless       no window; runs until stopped (e.g. as a systemd user service)
  --listen         accept a peer's pads, MIDI and keyboard/mouse regardless of the setting
  --receive-only   do not read local pads and send nothing (for a service next to the window)
  --version        print the version";

fn main() {
    let mut headless = false;
    let mut options = engine::Options::default();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--headless" => headless = true,
            "--listen" => options.listen_forced = true,
            "--receive-only" => options.receive_only = true,
            "--version" => {
                println!("padbridge {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return;
            }
            other => {
                eprintln!("padbridge: unknown argument {other}\n\n{USAGE}");
                std::process::exit(2);
            }
        }
    }

    options.service = headless;
    let handle = engine::Engine::spawn(options);
    if headless {
        // The engine thread does all the work; virtual devices vanish with the process.
        loop {
            std::thread::park();
        }
    }
    if let Err(e) = ui::run(handle) {
        eprintln!("padbridge: window failed: {e}");
        std::process::exit(1);
    }
}
