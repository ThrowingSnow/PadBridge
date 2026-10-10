//! Feasibility spike for the Rust port (stage 0): does a single binary built on
//! a current Mac run on macOS 10.13 (iMac 2010, OpenGL only, no Metal) with
//! - an egui window (glow / OpenGL backend),
//! - a wired Xbox 360 pad read via libusb (no kext),
//! - a CoreMIDI virtual source?
//! Throwaway code: the real crates come in stage 1.

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use eframe::egui;

const PADS: &[(u16, u16)] = &[(0x045e, 0x028e), (0x045e, 0x028f), (0x0e6f, 0x0213), (0x24c6, 0x5300)];

#[derive(Default, Clone)]
struct Pad {
    status: String,
    buttons: u16,
    lt: u8,
    rt: u8,
    sticks: [i16; 4],
    reports: u64,
}

fn usb_thread(shared: Arc<Mutex<Pad>>, midi: Arc<Mutex<Option<coremidi::VirtualSource>>>) {
    loop {
        if let Err(e) = run_pad(&shared, &midi) {
            shared.lock().unwrap().status = format!("kein Pad: {e}");
        }
        thread::sleep(Duration::from_secs(1));
    }
}

fn run_pad(shared: &Mutex<Pad>, midi: &Mutex<Option<coremidi::VirtualSource>>) -> Result<(), String> {
    let handle = PADS.iter().find_map(|&(v, p)| rusb::open_device_with_vid_pid(v, p)).ok_or("nicht gefunden")?;
    let device = handle.device();
    let config =
        device.active_config_descriptor().or_else(|_| device.config_descriptor(0)).map_err(|e| e.to_string())?;

    // Interface 0 (class FF/5D/01) carries the gamepad; find its interrupt endpoints.
    let iface = config.interfaces().next().ok_or("kein Interface")?;
    let desc = iface.descriptors().next().ok_or("kein Descriptor")?;
    let mut ep_in = 0x81;
    let mut ep_out = 0x01;
    for ep in desc.endpoint_descriptors() {
        if ep.transfer_type() == rusb::TransferType::Interrupt {
            match ep.direction() {
                rusb::Direction::In => ep_in = ep.address(),
                rusb::Direction::Out => ep_out = ep.address(),
            }
        }
    }
    let _ = handle.set_auto_detach_kernel_driver(true);
    if handle.active_configuration().unwrap_or(0) == 0 {
        handle.set_active_configuration(1).map_err(|e| format!("config: {e}"))?;
    }
    handle.claim_interface(desc.interface_number()).map_err(|e| format!("claim: {e}"))?;
    let _ = handle.write_interrupt(ep_out, &[0x01, 0x03, 0x06], Duration::from_millis(200)); // LED quadrant 1

    shared.lock().unwrap().status = format!("Pad offen (in {ep_in:#04x}, out {ep_out:#04x})");
    let mut buf = [0u8; 64];
    let mut last_lx: Option<u8> = None;
    loop {
        match handle.read_interrupt(ep_in, &mut buf, Duration::from_millis(500)) {
            Ok(n) if n >= 14 && buf[0] == 0x00 && buf[1] == 0x14 => {
                let s = |i: usize| i16::from_le_bytes([buf[i], buf[i + 1]]);
                let mut pad = shared.lock().unwrap();
                pad.buttons = u16::from_le_bytes([buf[2], buf[3]]);
                pad.lt = buf[4];
                pad.rt = buf[5];
                pad.sticks = [s(6), s(8), s(10), s(12)];
                pad.reports += 1;
                // Left stick X -> CC 20, only on change.
                let cc = ((pad.sticks[0] as i32 + 32768) >> 9) as u8;
                drop(pad);
                if last_lx != Some(cc) {
                    last_lx = Some(cc);
                    if let Some(source) = midi.lock().unwrap().as_ref() {
                        let _ = source.received(&coremidi::PacketBuffer::new(0, &[0xB0, 20, cc]));
                    }
                }
            }
            Ok(_) => {}
            Err(rusb::Error::Timeout) => {}
            Err(e) => return Err(format!("read: {e}")),
        }
    }
}

struct App {
    pad: Arc<Mutex<Pad>>,
    midi_status: String,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let pad = self.pad.lock().unwrap().clone();
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("PadBridge – Rust-Machbarkeitstest");
            ui.label(format!("System: {}", os_version()));
            ui.label(format!("MIDI: {}", self.midi_status));
            ui.label(format!("USB: {}", pad.status));
            ui.separator();
            ui.monospace(format!("Reports: {}", pad.reports));
            ui.monospace(format!("Buttons: {:016b}", pad.buttons));
            ui.monospace(format!("LT {:3}  RT {:3}", pad.lt, pad.rt));
            ui.monospace(format!(
                "LX {:6} LY {:6}  RX {:6} RY {:6}",
                pad.sticks[0], pad.sticks[1], pad.sticks[2], pad.sticks[3]
            ));
            ui.add(egui::ProgressBar::new((pad.sticks[0] as f32 + 32768.0) / 65535.0).text("Linker Stick X → CC 20"));
        });
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }
}

fn os_version() -> String {
    std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn main() -> eframe::Result {
    let pad = Arc::new(Mutex::new(Pad { status: "suche…".into(), ..Default::default() }));

    // The client must outlive the source; both live for the whole program.
    let client = coremidi::Client::new("PadBridge Spike").ok();
    let source = client.as_ref().and_then(|c| c.virtual_source("PadBridge Spike").ok());
    let midi_status = if source.is_some() {
        "virtueller Port „PadBridge Spike“ aktiv".into()
    } else {
        "CoreMIDI-Fehler".to_string()
    };
    let midi = Arc::new(Mutex::new(source));
    std::mem::forget(client);

    {
        let (pad, midi) = (pad.clone(), midi.clone());
        thread::spawn(move || usb_thread(pad, midi));
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([520.0, 300.0]),
        ..Default::default()
    };
    eframe::run_native("PadBridge Spike", options, Box::new(|_cc| Ok(Box::new(App { pad, midi_status }))))
}
