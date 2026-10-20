//! The status web page for phones: one page, live over Server-Sent Events,
//! read-only. A small HTTP/1.1 server on std (a thread per connection; a
//! handful of phones at most).
//!
//! Who may look is decided in padbridge_core::web: the home network freely,
//! everyone else (Tailscale, IPv6 from outside) after entering the PIN, which
//! sets a session cookie.

mod status;

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use padbridge_core::web::is_home_address;

use crate::engine::Snapshot;

const PAGE: &str = include_str!("page.html");
const MANIFEST: &str = include_str!("manifest.json");
const ICON: &str = include_str!("icon.svg");

/// How often the page gets a new status.
const EVENT_INTERVAL: Duration = Duration::from_millis(100);
const MAX_FAILURES: u32 = 5;
const LOCKOUT: Duration = Duration::from_secs(30);
const COOKIE: &str = "pb_session";

pub struct WebServer {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

struct Shared {
    snapshot: Arc<Mutex<Snapshot>>,
    stop: Arc<AtomicBool>,
    pin: String,
    token: String,
    /// Wrong PINs per address: (count, last attempt).
    failures: Mutex<HashMap<IpAddr, (u32, Instant)>>,
}

impl WebServer {
    pub fn start(port: u16, snapshot: Arc<Mutex<Snapshot>>, pin: String, token: String) -> io::Result<Self> {
        // IPv4 only: a "::" socket is IPv6-only by default on Windows. Tailscale
        // reaches it on its 100.x address.
        let listener = TcpListener::bind(("0.0.0.0", port))?;
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Shared { snapshot, stop: stop.clone(), pin, token, failures: Mutex::default() });
        let thread = std::thread::Builder::new().name("web".into()).spawn(move || {
            while !shared.stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, from)) => {
                        let shared = shared.clone();
                        let _ = std::thread::Builder::new().name("web-client".into()).spawn(move || {
                            let _ = serve(stream, from, &shared);
                        });
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(50)),
                    Err(_) => std::thread::sleep(Duration::from_millis(200)),
                }
            }
        })?;
        Ok(WebServer { stop, thread: Some(thread) })
    }
}

impl Drop for WebServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Request {
    method: String,
    path: String,
    cookie: Option<String>,
    body: String,
}

fn read_request(stream: &TcpStream) -> io::Result<Request> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or("/").split('?').next().unwrap_or("/").to_string();
    let mut length = 0usize;
    let mut cookie = None;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
        let Some((name, value)) = header.split_once(':') else { continue };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => length = value.parse().unwrap_or(0).min(1024),
            "cookie" => {
                cookie = value
                    .split(';')
                    .filter_map(|c| c.trim().split_once('='))
                    .find(|(k, _)| *k == COOKIE)
                    .map(|(_, v)| v.to_string())
            }
            _ => {}
        }
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    Ok(Request { method, path, cookie, body: String::from_utf8_lossy(&body).into_owned() })
}

fn serve(mut stream: TcpStream, from: SocketAddr, shared: &Shared) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let request = read_request(&stream)?;
    let ip = from.ip().to_canonical();
    let allowed = is_home_address(ip) || request.cookie.as_deref() == Some(shared.token.as_str());
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") => respond(&mut stream, "200 OK", "text/html; charset=utf-8", &[], PAGE),
        ("GET", "/manifest.json") => respond(&mut stream, "200 OK", "application/manifest+json", &[], MANIFEST),
        ("GET", "/icon.svg") => respond(&mut stream, "200 OK", "image/svg+xml", &[], ICON),
        ("GET", "/events") if allowed => events(stream, shared),
        ("GET", "/events") => respond(&mut stream, "401 Unauthorized", "text/plain", &[], "PIN"),
        ("POST", "/login") => login(&mut stream, ip, &request.body, shared),
        _ => respond(&mut stream, "404 Not Found", "text/plain", &[], "nicht gefunden"),
    }
}

fn login(stream: &mut TcpStream, ip: IpAddr, body: &str, shared: &Shared) -> io::Result<()> {
    let mut failures = shared.failures.lock().unwrap();
    failures.retain(|_, (_, at)| at.elapsed() < LOCKOUT);
    if failures.get(&ip).is_some_and(|(n, _)| *n >= MAX_FAILURES) {
        drop(failures);
        return respond(stream, "429 Too Many Requests", "text/plain", &[], "Zu viele Versuche, kurz warten");
    }
    let pin = body.trim().trim_start_matches("pin=");
    if !shared.pin.is_empty() && pin == shared.pin {
        failures.remove(&ip);
        drop(failures);
        let cookie =
            format!("Set-Cookie: {COOKIE}={}; Max-Age=31536000; Path=/; HttpOnly; SameSite=Strict", shared.token);
        return respond(stream, "204 No Content", "text/plain", &[&cookie], "");
    }
    let entry = failures.entry(ip).or_insert((0, Instant::now()));
    entry.0 += 1;
    entry.1 = Instant::now();
    drop(failures);
    respond(stream, "403 Forbidden", "text/plain", &[], "PIN falsch")
}

/// Streams the status until the page goes away or the server stops.
fn events(mut stream: TcpStream, shared: &Shared) -> io::Result<()> {
    stream.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n",
    )?;
    let host = hostname();
    while !shared.stop.load(Ordering::Relaxed) {
        let json = {
            let snap = shared.snapshot.lock().unwrap();
            status::json(&snap, &host)
        };
        stream.write_all(format!("data: {json}\n\n").as_bytes())?;
        stream.flush()?;
        std::thread::sleep(EVENT_INTERVAL);
    }
    Ok(())
}

fn respond(stream: &mut TcpStream, status: &str, kind: &str, headers: &[&str], body: &str) -> io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n",
        body.len()
    );
    for h in headers {
        head.push_str(h);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(body.as_bytes())
}

/// This machine's address in the home network, for the link and QR code.
/// Connecting a UDP socket sends nothing; it only picks the outgoing interface.
pub fn lan_address() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    socket.local_addr().ok().map(|a| a.ip()).filter(|ip| !ip.is_unspecified())
}

pub fn hostname() -> String {
    #[cfg(unix)]
    {
        let mut buf = [0u8; 256];
        if unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } == 0 {
            let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            let name = String::from_utf8_lossy(&buf[..end]).into_owned();
            return name.trim_end_matches(".local").to_string();
        }
    }
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "PadBridge".into())
}
