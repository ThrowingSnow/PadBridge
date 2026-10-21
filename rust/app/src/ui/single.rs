//! One window per user. The first window holds a localhost port; a second
//! start knocks there, the first one comes to the front and the second exits
//! before it opens anything. If some other program holds the port, nobody
//! answers like PadBridge and the window simply starts without the guard.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use eframe::egui;

const PORT: u16 = 48179;
const KNOCK: &[u8] = b"show\n";
const ANSWER: &str = "padbridge";

/// The guard for this window, or Err(()) if a PadBridge window already runs
/// (it has been asked to come to the front).
pub fn claim() -> Result<Option<TcpListener>, ()> {
    if let Ok(listener) = TcpListener::bind(("127.0.0.1", PORT)) {
        return Ok(Some(listener));
    }
    if knock() {
        Err(())
    } else {
        Ok(None)
    }
}

/// True if a PadBridge window answered.
fn knock() -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], PORT));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(300)) else { return false };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
    if stream.write_all(KNOCK).is_err() {
        return false;
    }
    let mut answer = String::new();
    BufReader::new(stream).read_line(&mut answer).is_ok() && answer.trim() == ANSWER
}

/// Answers knocks by bringing this window to the front.
pub fn serve(listener: TcpListener, ctx: egui::Context) {
    let spawned = std::thread::Builder::new().name("single-window".into()).spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
            let mut line = String::new();
            if BufReader::new(&stream).read_line(&mut line).is_err() || line.as_bytes() != KNOCK {
                continue;
            }
            let _ = stream.write_all(format!("{ANSWER}\n").as_bytes());
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            ctx.request_repaint();
        }
    });
    if let Err(e) = spawned {
        eprintln!("padbridge: single-window thread: {e}");
    }
}
