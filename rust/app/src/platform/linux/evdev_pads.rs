//! Physical game pads through evdev. The kernel driver (xpad for 360 pads,
//! hid-* for others) already decodes the hardware, so any pad Linux knows
//! works: we only map evdev codes onto the canonical controls.
//!
//! Each pad is opened twice: one handle is read by its own thread, the other
//! does output (rumble upload, LED via sysfs) from the engine thread.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use evdev::{
    AbsoluteAxisType as Abs, Device, FFEffect, FFEffectData, FFEffectKind, FFEffectType, FFReplay, FFTrigger,
    InputEventKind, Key,
};
use padbridge_core::{xbox360, Capabilities, ControllerInfo, ControllerState, PadAxis, PadButton, PlayerIndicator};

use crate::platform::{Event, PadOutput};

const SCAN_INTERVAL: Duration = Duration::from_secs(2);

const BUTTONS: [(Key, PadButton); 15] = [
    (Key::BTN_SOUTH, PadButton::A),
    (Key::BTN_EAST, PadButton::B),
    (Key::BTN_NORTH, PadButton::X), // xpad's BTN_X
    (Key::BTN_WEST, PadButton::Y),  // xpad's BTN_Y
    (Key::BTN_TL, PadButton::LeftShoulder),
    (Key::BTN_TR, PadButton::RightShoulder),
    (Key::BTN_THUMBL, PadButton::LeftStick),
    (Key::BTN_THUMBR, PadButton::RightStick),
    (Key::BTN_SELECT, PadButton::Back),
    (Key::BTN_START, PadButton::Start),
    (Key::BTN_MODE, PadButton::Guide),
    // Pads whose driver reports the D-pad as buttons instead of a hat.
    (Key::BTN_DPAD_UP, PadButton::DpadUp),
    (Key::BTN_DPAD_DOWN, PadButton::DpadDown),
    (Key::BTN_DPAD_LEFT, PadButton::DpadLeft),
    (Key::BTN_DPAD_RIGHT, PadButton::DpadRight),
];

pub fn start(sink: impl Fn(Event) -> bool + Send + Clone + 'static) {
    let open: Arc<Mutex<HashSet<PathBuf>>> = Arc::default();
    std::thread::Builder::new()
        .name("evdev-scan".into())
        .spawn(move || loop {
            for path in event_nodes() {
                if open.lock().unwrap().contains(&path) || !is_physical_pad(&path) {
                    continue;
                }
                match connect(&path) {
                    Ok((info, output, reader, state)) => {
                        open.lock().unwrap().insert(path.clone());
                        let id = info.id.clone();
                        if !sink(Event::PadConnected(info, Box::new(output))) {
                            return;
                        }
                        // Pads report only on change: publish the current state once.
                        sink(Event::PadState(id.clone(), state));
                        let (sink, open, path) = (sink.clone(), open.clone(), path.clone());
                        let _ =
                            std::thread::Builder::new().name(format!("evdev-{}", path.display())).spawn(move || {
                                read_loop(reader, &id, state, &sink);
                                open.lock().unwrap().remove(&path);
                                sink(Event::PadGone(id));
                            });
                    }
                    Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                        // Remember it so the warning is not repeated every scan.
                        open.lock().unwrap().insert(path.clone());
                        sink(Event::Error(format!("Kein Zugriff auf {} (Gruppe „input“ fehlt?)", path.display())));
                    }
                    Err(_) => {}
                }
            }
            std::thread::sleep(SCAN_INTERVAL);
        })
        .expect("evdev scan thread");
}

fn event_nodes() -> Vec<PathBuf> {
    let Ok(dir) = fs::read_dir("/dev/input") else { return Vec::new() };
    let mut nodes: Vec<PathBuf> = dir
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("event")))
        .collect();
    nodes.sort();
    nodes
}

fn sysfs_dir(node: &Path) -> Option<PathBuf> {
    let name = node.file_name()?;
    fs::canonicalize(Path::new("/sys/class/input").join(name)).ok()
}

/// A real pad: has a south button and a stick, and is not a uinput device
/// (our own virtual pads look exactly like a 360 pad).
fn is_physical_pad(node: &Path) -> bool {
    let Some(sys) = sysfs_dir(node) else { return false };
    if sys.to_string_lossy().contains("/virtual/") {
        return false;
    }
    // Cheap check via sysfs before opening: key and abs capability bitmaps.
    let caps = |file: &str| fs::read_to_string(sys.join("device/capabilities").join(file)).unwrap_or_default();
    has_bit(&caps("key"), Key::BTN_SOUTH.code() as usize) && has_bit(&caps("abs"), Abs::ABS_X.0 as usize)
}

/// sysfs capability bitmaps are space-separated hex words, most significant first.
fn has_bit(bitmap: &str, bit: usize) -> bool {
    let words: Vec<u64> = bitmap.split_whitespace().rev().filter_map(|w| u64::from_str_radix(w, 16).ok()).collect();
    let bits_per_word = if cfg!(target_pointer_width = "64") { 64 } else { 32 };
    words.get(bit / bits_per_word).is_some_and(|w| w >> (bit % bits_per_word) & 1 == 1)
}

/// Axis ranges from the driver, to normalise values.
#[derive(Clone, Copy)]
struct Range {
    min: i32,
    max: i32,
}

impl Range {
    fn stick(self, v: i32) -> f32 {
        let centre = (self.min as f32 + self.max as f32) / 2.0;
        let half = (self.max as f32 - self.min as f32) / 2.0;
        if half <= 0.0 {
            return 0.0;
        }
        ((v as f32 - centre) / half).clamp(-1.0, 1.0)
    }

    fn trigger(self, v: i32) -> f32 {
        if self.max <= self.min {
            return 0.0;
        }
        ((v - self.min) as f32 / (self.max - self.min) as f32).clamp(0.0, 1.0)
    }
}

struct Reader {
    device: Device,
    ranges: Vec<Range>,
}

fn connect(node: &Path) -> io::Result<(ControllerInfo, LinuxPad, Reader, ControllerState)> {
    let reader = Device::open(node)?;
    let output = Device::open(node)?;
    let abs = reader.get_abs_state()?;
    let ranges: Vec<Range> = abs.iter().map(|a| Range { min: a.minimum, max: a.maximum }).collect();

    // Initial state, so outputs start right away.
    let mut state = ControllerState::default();
    let keys = reader.get_key_state()?;
    for (key, button) in BUTTONS {
        state.set_pressed(button, keys.contains(key));
    }
    let supported = reader.supported_absolute_axes();
    for (i, a) in abs.iter().enumerate() {
        if supported.is_some_and(|s| s.contains(Abs(i as u16))) {
            apply_abs(&mut state, &ranges, Abs(i as u16), a.value);
        }
    }

    let rumble = output.supported_ff().is_some_and(|ff| ff.contains(FFEffectType::FF_RUMBLE));
    let led = led_path(node);
    let id = node.display().to_string();
    let input_id = reader.input_id();
    let info = ControllerInfo {
        id: id.clone(),
        name: reader.name().unwrap_or("Gamepad").to_string(),
        vendor_id: input_id.vendor(),
        product_id: input_id.product(),
        backend: "evdev".into(),
        capabilities: Capabilities { rumble, player_indicator: led.is_some() },
        buttons: PadButton::ALL.to_vec(),
        axes: PadAxis::ALL.to_vec(),
    };
    let pad = LinuxPad { device: output, effect: None, led, led_warned: false };
    Ok((info, pad, Reader { device: reader, ranges }, state))
}

fn read_loop(mut reader: Reader, id: &str, mut state: ControllerState, sink: &impl Fn(Event) -> bool) {
    let mut sent = state;
    loop {
        let events: Vec<_> = match reader.device.fetch_events() {
            Ok(events) => events.collect(),
            Err(_) => return, // unplugged (ENODEV) or similar
        };
        for event in events {
            match event.kind() {
                InputEventKind::Key(key) => {
                    if let Some((_, button)) = BUTTONS.iter().find(|(k, _)| *k == key) {
                        state.set_pressed(*button, event.value() != 0);
                    }
                }
                InputEventKind::AbsAxis(axis) => apply_abs(&mut state, &reader.ranges, axis, event.value()),
                InputEventKind::Synchronization(_) if state != sent => {
                    sent = state;
                    if !sink(Event::PadState(id.to_string(), state)) {
                        return;
                    }
                }
                _ => {}
            }
        }
    }
}

/// evdev axes -> canonical (Linux Y axes grow downwards, ours upwards).
fn apply_abs(state: &mut ControllerState, ranges: &[Range], axis: Abs, value: i32) {
    let range = ranges.get(axis.0 as usize).copied().unwrap_or(Range { min: -1, max: 1 });
    match axis {
        Abs::ABS_X => state.set_axis(PadAxis::LeftX, range.stick(value)),
        Abs::ABS_Y => state.set_axis(PadAxis::LeftY, -range.stick(value)),
        Abs::ABS_RX => state.set_axis(PadAxis::RightX, range.stick(value)),
        Abs::ABS_RY => state.set_axis(PadAxis::RightY, -range.stick(value)),
        Abs::ABS_Z | Abs::ABS_BRAKE => state.set_axis(PadAxis::LeftTrigger, range.trigger(value)),
        Abs::ABS_RZ | Abs::ABS_GAS => state.set_axis(PadAxis::RightTrigger, range.trigger(value)),
        Abs::ABS_HAT0X => {
            state.set_pressed(PadButton::DpadLeft, value < 0);
            state.set_pressed(PadButton::DpadRight, value > 0);
        }
        Abs::ABS_HAT0Y => {
            state.set_pressed(PadButton::DpadUp, value < 0);
            state.set_pressed(PadButton::DpadDown, value > 0);
        }
        _ => {}
    }
}

/// xpad registers the ring as an LED class device ("xpad0") on the USB
/// device, a few levels above the input node; writing a pattern number
/// (0..=15) to its `brightness` sets it.
fn led_path(node: &Path) -> Option<PathBuf> {
    sysfs_dir(node)?.ancestors().take(6).find_map(|dir| {
        fs::read_dir(dir.join("leds"))
            .ok()?
            .filter_map(|e| e.ok())
            .find(|e| e.file_name().to_string_lossy().starts_with("xpad"))
            .map(|e| e.path().join("brightness"))
    })
}

struct LinuxPad {
    device: Device,
    effect: Option<FFEffect>,
    led: Option<PathBuf>,
    led_warned: bool,
}

impl PadOutput for LinuxPad {
    fn set_rumble(&mut self, low: f32, high: f32) {
        let strong = (low.clamp(0.0, 1.0) * 65535.0) as u16;
        let weak = (high.clamp(0.0, 1.0) * 65535.0) as u16;
        if strong == 0 && weak == 0 {
            if let Some(effect) = self.effect.as_mut() {
                let _ = effect.stop();
            }
            return;
        }
        let data = FFEffectData {
            direction: 0,
            trigger: FFTrigger::default(),
            // Length 0: plays until stopped; the engine stops it.
            replay: FFReplay { length: 0, delay: 0 },
            kind: FFEffectKind::Rumble { strong_magnitude: strong, weak_magnitude: weak },
        };
        let result = match self.effect.as_mut() {
            Some(effect) => effect.update(data),
            None => self.device.upload_ff_effect(data).map(|e| {
                self.effect = Some(e);
            }),
        };
        if result.is_ok() {
            if let Some(effect) = self.effect.as_mut() {
                let _ = effect.play(1);
            }
        }
    }

    fn set_indicator(&mut self, indicator: PlayerIndicator) {
        let Some(path) = &self.led else { return };
        let pattern = xbox360::led_report(indicator)[2];
        if let Err(e) = fs::write(path, pattern.to_string()) {
            if !self.led_warned {
                self.led_warned = true;
                eprintln!(
                    "padbridge: LED-Ring: {} nicht beschreibbar ({e}); siehe DOC.md (udev-Regel)",
                    path.display()
                );
            }
        }
    }
}
