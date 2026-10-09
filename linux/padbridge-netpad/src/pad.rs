//! A uinput device that looks exactly like a wired Xbox 360 pad driven by the
//! kernel's xpad driver: same name, IDs, button codes and axis ranges. SDL,
//! Steam and Wine/Proton therefore treat it as a known controller.

use evdev::uinput::{VirtualDevice, VirtualDeviceBuilder};
use evdev::{AbsInfo, AbsoluteAxisType as Abs, AttributeSet, BusType, EventType, InputEvent, InputId, Key, UinputAbsSetup};

use crate::protocol::PadState;

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

pub struct VirtualPad {
    device: VirtualDevice,
    current: PadState,
}

impl VirtualPad {
    pub fn create() -> std::io::Result<Self> {
        let mut keys = AttributeSet::<Key>::new();
        for (_, key) in BUTTONS {
            keys.insert(key);
        }
        let stick = |axis| UinputAbsSetup::new(axis, AbsInfo::new(0, -32768, 32767, 16, 128, 0));
        let trigger = |axis| UinputAbsSetup::new(axis, AbsInfo::new(0, 0, 255, 0, 0, 0));
        let hat = |axis| UinputAbsSetup::new(axis, AbsInfo::new(0, -1, 1, 0, 0, 0));

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
            .build()?;
        Ok(Self { device, current: PadState::default() })
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
        let mut events: Vec<InputEvent> = BUTTONS
            .iter()
            .map(|(mask, key)| InputEvent::new(EventType::KEY, key.code(), pressed(*mask)))
            .collect();
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

    /// Everything released, sticks centred. Used when the sender goes quiet.
    pub fn neutral(&mut self) -> std::io::Result<()> {
        self.apply(PadState::default())
    }
}

/// Up-positive i16 -> down-positive, without overflowing on -32768.
fn flip(v: i16) -> i32 {
    (-(v as i32)).clamp(-32768, 32767)
}
