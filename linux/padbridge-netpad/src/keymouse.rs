//! Virtual keyboard + mouse ("PadBridge Keyboard/Mouse") fed by TYPE_INPUT
//! packets. Tracks held keys so they can all be released when the Mac goes
//! silent; a stuck key on the host is worse than a dropped one.

use std::collections::HashSet;

use evdev::uinput::{VirtualDevice, VirtualDeviceBuilder};
use evdev::{AttributeSet, BusType, EventType, InputEvent, InputId, Key, RelativeAxisType as Rel};

/// One wheel detent in hi-res units (linux/input.h).
const HI_RES_PER_DETENT: i32 = 120;

pub struct KeyMouse {
    device: VirtualDevice,
    held: HashSet<u16>,
    /// Hi-res scroll not yet worth a whole classic detent (vertical, horizontal).
    wheel_rest: (i32, i32),
}

impl KeyMouse {
    pub fn create() -> std::io::Result<Self> {
        let mut keys = AttributeSet::<Key>::new();
        // All regular keyboard keys plus the three mouse buttons.
        for code in 1..=248u16 {
            keys.insert(Key::new(code));
        }
        for code in [0x110u16, 0x111, 0x112] {
            keys.insert(Key::new(code));
        }
        let mut rel = AttributeSet::<Rel>::new();
        for axis in [Rel::REL_X, Rel::REL_Y, Rel::REL_WHEEL, Rel::REL_HWHEEL, Rel::REL_WHEEL_HI_RES, Rel::REL_HWHEEL_HI_RES] {
            rel.insert(axis);
        }
        let device = VirtualDeviceBuilder::new()?
            .name("PadBridge Keyboard/Mouse")
            .input_id(InputId::new(BusType::BUS_VIRTUAL, 0x1209, 0x0b01, 1))
            .with_keys(&keys)?
            .with_relative_axes(&rel)?
            .build()?;
        Ok(Self { device, held: HashSet::new(), wheel_rest: (0, 0) })
    }

    /// Applies (type, a, b) events from one packet; see protocol.rs.
    pub fn apply(&mut self, events: &[(u8, i16, i16)]) -> std::io::Result<()> {
        let mut out = Vec::new();
        for &(kind, a, b) in events {
            match kind {
                1 => {
                    let code = a as u16;
                    let down = b != 0;
                    // Ignore repeats of the same state (packets are sent twice upstream too).
                    if down == self.held.contains(&code) {
                        continue;
                    }
                    if down { self.held.insert(code) } else { self.held.remove(&code) };
                    out.push(InputEvent::new(EventType::KEY, code, down as i32));
                }
                2 => {
                    out.push(InputEvent::new(EventType::RELATIVE, Rel::REL_X.0, a as i32));
                    out.push(InputEvent::new(EventType::RELATIVE, Rel::REL_Y.0, b as i32));
                }
                3 => {
                    // Hi-res for smooth scrolling apps, classic detents for everything else.
                    let (v, h) = (a as i32, b as i32);
                    self.wheel_rest.0 += v;
                    self.wheel_rest.1 += h;
                    let (dv, dh) = (self.wheel_rest.0 / HI_RES_PER_DETENT, self.wheel_rest.1 / HI_RES_PER_DETENT);
                    self.wheel_rest.0 -= dv * HI_RES_PER_DETENT;
                    self.wheel_rest.1 -= dh * HI_RES_PER_DETENT;
                    if v != 0 { out.push(InputEvent::new(EventType::RELATIVE, Rel::REL_WHEEL_HI_RES.0, v)); }
                    if h != 0 { out.push(InputEvent::new(EventType::RELATIVE, Rel::REL_HWHEEL_HI_RES.0, h)); }
                    if dv != 0 { out.push(InputEvent::new(EventType::RELATIVE, Rel::REL_WHEEL.0, dv)); }
                    if dh != 0 { out.push(InputEvent::new(EventType::RELATIVE, Rel::REL_HWHEEL.0, dh)); }
                }
                _ => {}
            }
        }
        if out.is_empty() { Ok(()) } else { self.device.emit(&out) }
    }

    pub fn holding(&self) -> bool {
        !self.held.is_empty()
    }

    pub fn release_all(&mut self) -> std::io::Result<()> {
        let events: Vec<_> = self.held.drain().map(|code| InputEvent::new(EventType::KEY, code, 0)).collect();
        if events.is_empty() { Ok(()) } else { self.device.emit(&events) }
    }
}
