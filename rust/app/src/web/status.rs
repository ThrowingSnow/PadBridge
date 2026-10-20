//! The status as JSON for the page: what the window shows, condensed.

use padbridge_core::midi::MidiTarget;
use padbridge_core::{PadAxis, PadButton};
use serde_json::{json, Value};

use crate::engine::{DeviceView, Snapshot};

/// Log lines the page shows.
const LOG_LINES: usize = 8;

pub fn json(s: &Snapshot, host: &str) -> String {
    let pads: Vec<Value> =
        s.devices.iter().map(|d| pad(d, false)).chain(s.peer_pads.iter().map(|d| pad(d, true))).collect();
    let receiver = s.receiver.as_ref().map(|r| {
        json!({
            "listening": r.listening,
            "peer": r.peer.map(|p| p.ip().to_string()),
            "error": r.error,
            "virtualPads": r.pads.iter().map(|(i, name, active)| json!({"index": i + 1, "name": name, "active": active})).collect::<Vec<_>>(),
        })
    });
    let devices: Vec<Value> = s
        .midi_inputs
        .iter()
        .map(|i| {
            let quiet_ms = i.changed.values().map(|t| t.elapsed().as_millis() as u64).min();
            json!({"name": i.name, "remote": i.remote.is_some(), "quietMs": quiet_ms})
        })
        .collect();
    json!({
        "host": host,
        "version": env!("CARGO_PKG_VERSION"),
        "pads": pads,
        "link": {
            "active": s.link.active,
            "peer": s.link.peer,
            "answering": s.link.answering,
            "latencyMs": s.link.latency_ms,
            "error": s.link.error,
        },
        "receiver": receiver,
        "midi": {
            "enabled": s.midi.enabled,
            "target": if s.midi.target == MidiTarget::Network { "network" } else { "local" },
            "portError": s.midi_error,
            "clockRunning": s.clock.running,
            "bpm": s.clock.bpm,
        },
        "devices": devices,
        "log": s.log.iter().rev().take(LOG_LINES).rev().collect::<Vec<_>>(),
    })
    .to_string()
}

fn pad(d: &DeviceView, peer: bool) -> Value {
    let st = &d.state;
    let pressed: Vec<String> = PadButton::ALL.iter().filter(|b| st.is_pressed(**b)).map(|b| format!("{b:?}")).collect();
    let axis = |a: PadAxis| (st.axis(a) * 100.0).round() / 100.0;
    json!({
        "name": d.info.name,
        "ds4": d.info.vendor_id == 0x054c,
        "peer": peer,
        "live": d.peer_live.unwrap_or(true),
        "sentAs": d.net_pad.map(|p| p + 1),
        "pressed": pressed,
        "lx": axis(PadAxis::LeftX), "ly": axis(PadAxis::LeftY),
        "rx": axis(PadAxis::RightX), "ry": axis(PadAxis::RightY),
        "lt": axis(PadAxis::LeftTrigger), "rt": axis(PadAxis::RightTrigger),
    })
}
