//! Wired Xbox 360 pads (and clones) read directly over USB with libusb.
//! Same approach as the Swift app's IOUSBHost backend: claim the gamepad
//! interface (class FF, subclass 5D, protocol 01), read its interrupt IN
//! endpoint, write rumble and LED reports to its interrupt OUT endpoint.
//! Endpoint addresses differ between pads, so they come from the descriptors.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use padbridge_core::xbox360::{self, INTERFACE_CLASS, INTERFACE_PROTOCOL, INTERFACE_SUBCLASS};
use padbridge_core::{Capabilities, ControllerInfo, ControllerState, PadAxis, PadButton, PlayerIndicator};
use rusb::{Device, DeviceHandle, Direction, GlobalContext, TransferType};

use crate::platform::{Event, PadOutput};

const SCAN_INTERVAL: Duration = Duration::from_secs(2);
const READ_TIMEOUT: Duration = Duration::from_millis(500);
const WRITE_TIMEOUT: Duration = Duration::from_millis(100);

pub fn start(sink: impl Fn(Event) -> bool + Send + Clone + 'static) {
    let open: Arc<Mutex<HashSet<String>>> = Arc::default();
    // Report each failure once per device, not every scan.
    let mut failed: HashSet<String> = HashSet::new();
    std::thread::Builder::new()
        .name("usb-scan".into())
        .spawn(move || loop {
            let devices = rusb::devices().map(|l| l.iter().collect::<Vec<_>>()).unwrap_or_default();
            let present: HashSet<String> = devices.iter().map(device_id).collect();
            failed.retain(|id| present.contains(id));
            for device in devices {
                let Ok(desc) = device.device_descriptor() else { continue };
                let Some(&(_, _, name)) = xbox360::KNOWN_DEVICES
                    .iter()
                    .find(|(v, p, _)| *v == desc.vendor_id() && *p == desc.product_id())
                else {
                    continue;
                };
                let id = device_id(&device);
                if open.lock().unwrap().contains(&id) {
                    continue;
                }
                match Pad::open(&device) {
                    Ok(pad) => {
                        failed.remove(&id);
                        open.lock().unwrap().insert(id.clone());
                        let info = ControllerInfo {
                            id: id.clone(),
                            name: name.to_string(),
                            vendor_id: desc.vendor_id(),
                            product_id: desc.product_id(),
                            backend: "usb".into(),
                            capabilities: Capabilities { rumble: true, player_indicator: true },
                            buttons: PadButton::ALL.to_vec(),
                            axes: PadAxis::ALL.to_vec(),
                        };
                        let pad = Arc::new(pad);
                        if !sink(Event::PadConnected(info, Box::new(Output(pad.clone())))) {
                            return;
                        }
                        // 360 pads report only on change: publish a neutral state right away.
                        sink(Event::PadState(id.clone(), ControllerState::default()));
                        let (sink, open) = (sink.clone(), open.clone());
                        let _ = std::thread::Builder::new().name(format!("usb-{id}")).spawn(move || {
                            pad.read_loop(&id, &sink);
                            open.lock().unwrap().remove(&id);
                            sink(Event::PadGone(id));
                        });
                    }
                    Err(e) => {
                        if failed.insert(id.clone()) {
                            sink(Event::Error(format!("{name}: USB nicht geöffnet ({e})")));
                        }
                    }
                }
            }
            std::thread::sleep(SCAN_INTERVAL);
        })
        .expect("usb scan thread");
}

/// Bus and port path: stable while the pad stays in the same socket.
fn device_id(device: &Device<GlobalContext>) -> String {
    let ports = device.port_numbers().unwrap_or_default();
    let path: Vec<String> = ports.iter().map(u8::to_string).collect();
    format!("usb-{}-{}", device.bus_number(), path.join("."))
}

struct Pad {
    handle: DeviceHandle<GlobalContext>,
    ep_in: u8,
    ep_out: Option<u8>,
}

impl Pad {
    fn open(device: &Device<GlobalContext>) -> rusb::Result<Pad> {
        let handle = device.open()?;
        if handle.active_configuration().unwrap_or(0) == 0 {
            handle.set_active_configuration(1)?;
        }
        let config = device.active_config_descriptor().or_else(|_| device.config_descriptor(0))?;
        for interface in config.interfaces() {
            for alt in interface.descriptors() {
                if (alt.class_code(), alt.sub_class_code(), alt.protocol_code())
                    != (INTERFACE_CLASS, INTERFACE_SUBCLASS, INTERFACE_PROTOCOL)
                {
                    continue;
                }
                let interrupt = |dir| {
                    alt.endpoint_descriptors()
                        .find(|e| e.transfer_type() == TransferType::Interrupt && e.direction() == dir)
                        .map(|e| e.address())
                };
                let Some(ep_in) = interrupt(Direction::In) else { continue };
                let ep_out = interrupt(Direction::Out);
                let _ = handle.set_auto_detach_kernel_driver(true);
                handle.claim_interface(alt.interface_number())?;
                return Ok(Pad { handle, ep_in, ep_out });
            }
        }
        Err(rusb::Error::NotFound)
    }

    fn read_loop(&self, id: &str, sink: &impl Fn(Event) -> bool) {
        let mut buf = [0u8; 64];
        let mut sent = ControllerState::default();
        loop {
            match self.handle.read_interrupt(self.ep_in, &mut buf, READ_TIMEOUT) {
                Ok(n) => {
                    if let Some(state) = xbox360::parse_input(&buf[..n]) {
                        if state != sent {
                            sent = state;
                            if !sink(Event::PadState(id.to_string(), state)) {
                                return;
                            }
                        }
                    }
                }
                Err(rusb::Error::Timeout) | Err(rusb::Error::Overflow) => {}
                Err(_) => return, // unplugged
            }
        }
    }

    fn write(&self, report: &[u8]) {
        if let Some(ep) = self.ep_out {
            let _ = self.handle.write_interrupt(ep, report, WRITE_TIMEOUT);
        }
    }
}

/// libusb handles are thread-safe: the reader thread and the engine share one.
struct Output(Arc<Pad>);

impl PadOutput for Output {
    fn set_rumble(&mut self, low: f32, high: f32) {
        self.0.write(&xbox360::rumble_report(low, high));
    }

    fn set_indicator(&mut self, indicator: PlayerIndicator) {
        self.0.write(&xbox360::led_report(indicator));
    }
}
