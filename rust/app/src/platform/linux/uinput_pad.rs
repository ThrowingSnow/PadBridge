//! A uinput device that looks exactly like a wired Xbox 360 pad driven by the
//! kernel's xpad driver: same name, IDs, button codes and axis ranges. SDL,
//! Steam and Wine/Proton therefore treat it as a known controller.

use std::collections::HashMap;
use std::os::unix::io::{AsRawFd, RawFd};
use std::time::{Duration, Instant};

use evdev::uinput::{VirtualDevice, VirtualDeviceBuilder};
use evdev::{
    AbsInfo, AbsoluteAxisType as Abs, AttributeSet, BusType, EventType, FFEffectKind, FFEffectType, InputEvent,
    InputEventKind, InputId, Key, UInputEventType, UinputAbsSetup,
};

use padbridge_core::protocol::WireState as PadState;

use crate::platform::VirtualPad;

/// Bit in the 360 report layout -> key. D-pad bits are handled as a hat.
const BUTTONS: [(u16, Key); 11] = [
    (0x0010, Key::BTN_START),
    (0x0020, Key::BTN_SELECT),
    (0x0040, Key::BTN_THUMBL),
    (0x0080, Key::BTN_THUMBR),
    (0x0100, Key::BTN_TL),
    (0x0200, Key::BTN_TR),
    (0x0400, Key::BTN_MODE),
    (0x1000, Key::BTN_SOUTH), // A
    (0x2000, Key::BTN_EAST),  // B
    (0x4000, Key::BTN_NORTH), // X (xpad's BTN_X)
    (0x8000, Key::BTN_WEST),  // Y (xpad's BTN_Y)
];
const DPAD_UP: u16 = 0x0001;
const DPAD_DOWN: u16 = 0x0002;
const DPAD_LEFT: u16 = 0x0004;
const DPAD_RIGHT: u16 = 0x0008;

/// FF code for setting the overall gain (linux/input.h FF_GAIN).
const FF_GAIN: u16 = 0x60;

/// A rumble effect as uploaded by the game.
#[derive(Clone, Copy)]
struct Effect {
    strong: u16,
    weak: u16,
    /// 0 = until stopped.
    length: Duration,
}

pub struct UinputPad {
    device: VirtualDevice,
    current: PadState,
    effects: HashMap<i16, Effect>,
    /// Effect id -> when it stops on its own (None = until stopped).
    playing: HashMap<i16, Option<Instant>>,
    gain: u32,
}

impl UinputPad {
    pub fn create() -> std::io::Result<Self> {
        let mut keys = AttributeSet::<Key>::new();
        for (_, key) in BUTTONS {
            keys.insert(key);
        }
        let stick = |axis| UinputAbsSetup::new(axis, AbsInfo::new(0, -32768, 32767, 16, 128, 0));
        let trigger = |axis| UinputAbsSetup::new(axis, AbsInfo::new(0, 0, 255, 0, 0, 0));
        let hat = |axis| UinputAbsSetup::new(axis, AbsInfo::new(0, -1, 1, 0, 0, 0));

        let mut ff = AttributeSet::<FFEffectType>::new();
        ff.insert(FFEffectType::FF_RUMBLE);

        let device = VirtualDeviceBuilder::new()?
            .name("Microsoft X-Box 360 pad")
            .input_id(InputId::new(BusType::BUS_USB, 0x045e, 0x028e, 0x0110))
            .with_keys(&keys)?
            .with_absolute_axis(&stick(Abs::ABS_X))?
            .with_absolute_axis(&stick(Abs::ABS_Y))?
            .with_absolute_axis(&stick(Abs::ABS_RX))?
            .with_absolute_axis(&stick(Abs::ABS_RY))?
            .with_absolute_axis(&trigger(Abs::ABS_Z))?
            .with_absolute_axis(&trigger(Abs::ABS_RZ))?
            .with_absolute_axis(&hat(Abs::ABS_HAT0X))?
            .with_absolute_axis(&hat(Abs::ABS_HAT0Y))?
            .with_ff(&ff)?
            .with_ff_effects_max(16)
            .build()?;
        // Reads of FF requests must never stall the receive loop.
        let fd = device.as_raw_fd();
        unsafe { libc::fcntl(fd, libc::F_SETFL, libc::fcntl(fd, libc::F_GETFL) | libc::O_NONBLOCK) };
        Ok(Self {
            device,
            current: PadState::default(),
            effects: HashMap::new(),
            playing: HashMap::new(),
            gain: 0xFFFF,
        })
    }

    /// Answers effect uploads/erases and tracks play/stop/gain requests from
    /// games. Call when the fd is readable. Uploads must be answered promptly:
    /// the game's ioctl blocks until we do.
    pub fn process_force_feedback(&mut self) {
        let events: Vec<_> = match self.device.fetch_events() {
            Ok(events) => events.collect(),
            Err(_) => return, // WouldBlock: nothing pending
        };
        for event in events {
            match event.kind() {
                InputEventKind::UInput(code) if code == UInputEventType::UI_FF_UPLOAD.0 => {
                    if let Ok(mut upload) = self.device.process_ff_upload(event) {
                        let data = upload.effect();
                        if let FFEffectKind::Rumble { strong_magnitude, weak_magnitude } = data.kind {
                            let effect = Effect {
                                strong: strong_magnitude,
                                weak: weak_magnitude,
                                length: Duration::from_millis(data.replay.length as u64),
                            };
                            self.effects.insert(upload.effect_id(), effect);
                            upload.set_retval(0);
                        } else {
                            upload.set_retval(-libc::EINVAL);
                        }
                    }
                }
                InputEventKind::UInput(code) if code == UInputEventType::UI_FF_ERASE.0 => {
                    if let Ok(mut erase) = self.device.process_ff_erase(event) {
                        let id = erase.effect_id() as i16;
                        self.effects.remove(&id);
                        self.playing.remove(&id);
                        erase.set_retval(0);
                    }
                }
                InputEventKind::ForceFeedback(FF_GAIN) => self.gain = (event.value().max(0) as u32).min(0xFFFF),
                InputEventKind::ForceFeedback(code) => {
                    let id = code as i16;
                    if event.value() > 0 {
                        if let Some(effect) = self.effects.get(&id) {
                            let until = (!effect.length.is_zero()).then(|| Instant::now() + effect.length);
                            self.playing.insert(id, until);
                        }
                    } else {
                        self.playing.remove(&id);
                    }
                }
                _ => {}
            }
        }
    }

    /// Current motor levels (heavy, light) 0..=255 from all playing effects.
    pub fn rumble(&mut self) -> (u8, u8) {
        let now = Instant::now();
        self.playing.retain(|_, until| until.is_none_or(|t| t > now));
        let (mut strong, mut weak) = (0u32, 0u32);
        for id in self.playing.keys() {
            if let Some(e) = self.effects.get(id) {
                strong = strong.max(e.strong as u32);
                weak = weak.max(e.weak as u32);
            }
        }
        let scale = |m: u32| ((m * self.gain / 0xFFFF) >> 8) as u8;
        (scale(strong), scale(weak))
    }

    pub fn device_path(&mut self) -> Option<String> {
        self.device.enumerate_dev_nodes_blocking().ok()?.flatten().next().map(|p| p.display().to_string())
    }

    pub fn apply(&mut self, state: PadState) -> std::io::Result<()> {
        if state == self.current {
            return Ok(());
        }
        let b = state.buttons;
        let pressed = |mask: u16| (b & mask != 0) as i32;
        let mut events: Vec<InputEvent> =
            BUTTONS.iter().map(|(mask, key)| InputEvent::new(EventType::KEY, key.code(), pressed(*mask))).collect();
        let abs = |axis: Abs, value: i32| InputEvent::new(EventType::ABSOLUTE, axis.0, value);
        // Linux convention (as xpad reports it): Y axes and hat grow downwards.
        events.extend([
            abs(Abs::ABS_X, state.lx as i32),
            abs(Abs::ABS_Y, flip(state.ly)),
            abs(Abs::ABS_RX, state.rx as i32),
            abs(Abs::ABS_RY, flip(state.ry)),
            abs(Abs::ABS_Z, state.lt as i32),
            abs(Abs::ABS_RZ, state.rt as i32),
            abs(Abs::ABS_HAT0X, pressed(DPAD_RIGHT) - pressed(DPAD_LEFT)),
            abs(Abs::ABS_HAT0Y, pressed(DPAD_DOWN) - pressed(DPAD_UP)),
        ]);
        // emit() appends SYN_REPORT; the kernel drops values that did not change.
        self.device.emit(&events)?;
        self.current = state;
        Ok(())
    }
}

impl VirtualPad for UinputPad {
    fn apply(&mut self, state: PadState) -> std::io::Result<()> {
        UinputPad::apply(self, state)
    }

    fn poll_fd(&self) -> Option<RawFd> {
        Some(self.device.as_raw_fd())
    }

    fn process_force_feedback(&mut self) {
        UinputPad::process_force_feedback(self)
    }

    fn rumble(&mut self) -> (u8, u8) {
        UinputPad::rumble(self)
    }

    fn describe(&mut self) -> String {
        self.device_path().unwrap_or_else(|| "uinput".into())
    }
}

/// Up-positive i16 -> down-positive, without overflowing on -32768.
fn flip(v: i16) -> i32 {
    (-(v as i32)).clamp(-32768, 32767)
}
