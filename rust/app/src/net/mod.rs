//! The network link. Every PadBridge can be both ends:
//! - `sender`: sends this machine's pads, MIDI and keyboard/mouse to a peer,
//!   and gets pong, rumble and MIDI back.
//! - `receiver`: accepts a peer's packets and drives virtual devices here.

pub mod receiver;
pub mod sender;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static SEQ: AtomicU64 = AtomicU64::new(0);

/// Wall-clock microseconds, but strictly increasing within this process.
/// Tracking real time means a restarted sender is never locked out by the
/// peer's replay check for longer than its clock lag.
pub fn next_seq() -> u64 {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_micros() as u64).unwrap_or(0);
    let mut current = SEQ.load(Ordering::Relaxed);
    loop {
        let next = (current + 1).max(now);
        match SEQ.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return next,
            Err(actual) => current = actual,
        }
    }
}
