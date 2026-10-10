//! PadBridge: game controllers as MIDI, keyboard/mouse and network gamepads.
//!
//! `padbridge`                     window
//! `padbridge --headless --listen` service mode (replaces padbridge-netpad)

mod engine;
mod net;
mod platform;
mod ui;

const USAGE: &str = "\
usage: padbridge [--headless] [--listen]

  --headless   no window; runs until stopped (e.g. as a systemd user service)
  --listen     accept a peer's pads, MIDI and keyboard/mouse regardless of the setting
  --version    print the version";

fn main() {
    let mut headless = false;
    let mut listen = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--headless" => headless = true,
            "--listen" => listen = true,
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

    let handle = engine::Engine::spawn(listen);
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
