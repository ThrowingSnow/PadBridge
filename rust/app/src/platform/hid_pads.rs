//! HID game pads (DualShock 4, USB and Bluetooth) via hidapi, on macOS
//! (IOHIDManager underneath) and Windows. The 360 pad is not HID; usb_pads
//! (macOS) and xinput_pads (Windows) cover it.

use std::collections::HashSet;
use std::ffi::CString;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hidapi::{BusType, HidApi, HidDevice};
use padbridge_core::ds4::{self, LightBar};
use padbridge_core::{Capabilities, ControllerInfo, ControllerState, PadAxis, PadButton, PlayerIndicator};

use crate::platform::{Event, PadOutput};

const SCAN_INTERVAL: Duration = Duration::from_secs(2);
/// Short, so a rumble/LED write waiting for the lock is never held up long.
const READ_TIMEOUT_MS: i32 = 10;

pub fn start(sink: impl Fn(Event) -> bool + Send + Clone + 'static) {
    std::thread::Builder::new()
        .name("hid-scan".into())
        .spawn(move || {
            let mut api = match HidApi::new() {
                Ok(api) => api,
                Err(e) => {
                    sink(Event::Error(format!("HID nicht verfügbar: {e}")));
                    return;
                }
            };
            let open: Arc<Mutex<HashSet<CString>>> = Arc::default();
            let mut failed: HashSet<CString> = HashSet::new();
            loop {
                let _ = api.refresh_devices();
                for info in api.device_list() {
                    let Some(name) = ds4::is_known(info.vendor_id(), info.product_id()) else { continue };
                    let path = info.path().to_owned();
                    if open.lock().unwrap().contains(&path) || failed.contains(&path) {
                        continue;
                    }
                    let bluetooth = matches!(info.bus_type(), BusType::Bluetooth);
                    let device = match info.open_device(&api) {
                        Ok(d) => d,
                        Err(e) => {
                            failed.insert(path);
                            sink(Event::Error(format!("{name}: HID nicht geöffnet ({e})")));
                            continue;
                        }
                    };
                    open.lock().unwrap().insert(path.clone());
                    let id = format!("hid-{}", path.to_string_lossy());
                    let info = ControllerInfo {
                        id: id.clone(),
                        name: if bluetooth { format!("{name} (Bluetooth)") } else { name.to_string() },
                        vendor_id: info.vendor_id(),
                        product_id: info.product_id(),
                        backend: "hid".into(),
                        capabilities: Capabilities { rumble: true, player_indicator: true },
                        buttons: PadButton::ALL.to_vec(),
                        axes: PadAxis::ALL.to_vec(),
                    };
                    let device = Arc::new(Shared { device: Mutex::new(device), writers: AtomicUsize::new(0) });
                    let mut output = Output {
                        device: device.clone(),
                        bluetooth,
                        low: 0.0,
                        high: 0.0,
                        bar: LightBar::from_indicator(PlayerIndicator::Off),
                    };
                    // Bluetooth pads send full reports only after the first output report.
                    output.send();
                    if !sink(Event::PadConnected(info, Box::new(output))) {
                        return;
                    }
                    let (sink, open) = (sink.clone(), open.clone());
                    let _ = std::thread::Builder::new().name(id.clone()).spawn(move || {
                        read_loop(&device, &id, &sink);
                        open.lock().unwrap().remove(&path);
                        sink(Event::PadGone(id));
                    });
                }
                // Forget failures of pads that are gone, so replugging retries.
                let present: HashSet<CString> = api.device_list().map(|i| i.path().to_owned()).collect();
                failed.retain(|p| present.contains(p));
                std::thread::sleep(SCAN_INTERVAL);
            }
        })
        .expect("hid scan thread");
}

fn read_loop(shared: &Shared, id: &str, sink: &impl Fn(Event) -> bool) {
    let mut buf = [0u8; 128];
    let mut sent = ControllerState::default();
    sink(Event::PadState(id.to_string(), sent));
    loop {
        // A pad reports every few ms, so without this the lock goes straight back
        // to this loop and a writer can wait for seconds (seen on Windows).
        while shared.writers.load(Ordering::Acquire) > 0 {
            std::thread::yield_now();
        }
        let n = match shared.device.lock().unwrap().read_timeout(&mut buf, READ_TIMEOUT_MS) {
            Ok(n) => n,
            Err(_) => return, // unplugged
        };
        if n == 0 {
            continue;
        }
        if let Some(state) = ds4::parse_input(&buf[..n]) {
            if state != sent {
                sent = state;
                if !sink(Event::PadState(id.to_string(), state)) {
                    return;
                }
            }
        }
    }
}

/// One pad, read by its thread and written by the engine.
struct Shared {
    device: Mutex<HidDevice>,
    /// Writers waiting for or holding the lock; the reader steps aside for them.
    writers: AtomicUsize,
}

/// Rumble and light bar share one report, so the last values of both are kept.
struct Output {
    device: Arc<Shared>,
    bluetooth: bool,
    low: f32,
    high: f32,
    bar: LightBar,
}

impl Output {
    fn send(&mut self) {
        let report = ds4::output_report(self.bluetooth, self.low, self.high, self.bar);
        self.device.writers.fetch_add(1, Ordering::AcqRel);
        let _ = self.device.device.lock().unwrap().write(&report);
        self.device.writers.fetch_sub(1, Ordering::AcqRel);
    }
}

impl PadOutput for Output {
    fn set_rumble(&mut self, low: f32, high: f32) {
        self.low = low;
        self.high = high;
        self.send();
    }

    fn set_indicator(&mut self, indicator: PlayerIndicator) {
        self.bar = LightBar::from_indicator(indicator);
        self.send();
    }
}
