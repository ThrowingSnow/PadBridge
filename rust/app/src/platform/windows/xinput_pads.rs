//! Xbox pads (360, One, Series and clones) via XInput, the four slots
//! Windows assigns. XInput reports the 360 button layout, so the bits are
//! read like the wired 360 report. The Guide button is only in the
//! undocumented XInputGetStateEx (ordinal 100 of xinput1_4.dll); without it
//! everything else still works. Windows drives the ring LEDs itself.
//! Our own virtual pads (vigem_pad) show up here too and are skipped.

use std::time::{Duration, Instant};

use padbridge_core::xbox360::{buttons_from_bits, stick_from_i16, trigger_to_u8};
use padbridge_core::{Capabilities, ControllerInfo, ControllerState, PadAxis, PadButton, PlayerIndicator};
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
use windows_sys::Win32::UI::Input::XboxController::{
    XInputGetState, XInputSetState, XINPUT_GAMEPAD, XINPUT_STATE, XINPUT_VIBRATION, XUSER_MAX_COUNT,
};

use super::vigem_pad;
use crate::platform::{Event, PadOutput};

const POLL: Duration = Duration::from_millis(4);
/// Asking an empty slot is slow, so empty slots are only looked at this often.
const SCAN_INTERVAL: Duration = Duration::from_secs(2);

/// XINPUT_STATE plus the padding XInputGetStateEx writes.
#[repr(C)]
#[derive(Default)]
struct StateEx {
    state: XINPUT_STATE,
    _reserved: u32,
}

type GetStateEx = unsafe extern "system" fn(u32, *mut StateEx) -> u32;

fn get_state_ex() -> Option<GetStateEx> {
    unsafe {
        let dll = LoadLibraryA(c"xinput1_4.dll".as_ptr().cast());
        if dll.is_null() {
            return None;
        }
        GetProcAddress(dll, 100 as _).map(|f| std::mem::transmute::<_, GetStateEx>(f))
    }
}

pub fn start(sink: impl Fn(Event) -> bool + Send + 'static) {
    std::thread::Builder::new()
        .name("xinput".into())
        .spawn(move || {
            let ex = get_state_ex();
            let read = |slot: u32| -> Option<XINPUT_GAMEPAD> {
                let mut s = StateEx::default();
                let status = unsafe {
                    match ex {
                        Some(f) => f(slot, &mut s),
                        None => XInputGetState(slot, &mut s.state),
                    }
                };
                (status == ERROR_SUCCESS).then_some(s.state.Gamepad)
            };
            let mut sent: [Option<ControllerState>; XUSER_MAX_COUNT as usize] = Default::default();
            let mut last_scan = Instant::now() - SCAN_INTERVAL;
            loop {
                let scan = last_scan.elapsed() >= SCAN_INTERVAL;
                if scan {
                    last_scan = Instant::now();
                }
                for slot in 0..XUSER_MAX_COUNT {
                    let held = &mut sent[slot as usize];
                    let id = format!("xinput-{slot}");
                    if vigem_pad::is_virtual(slot) {
                        if held.take().is_some() && !sink(Event::PadGone(id)) {
                            return;
                        }
                        continue;
                    }
                    if held.is_none() && (!scan || vigem_pad::plugging()) {
                        continue;
                    }
                    match (read(slot), held.is_some()) {
                        (Some(pad), _) => {
                            if held.is_none() && !sink(Event::PadConnected(info(slot, &id), Box::new(Output(slot)))) {
                                return;
                            }
                            let state = to_state(&pad);
                            if *held != Some(state) {
                                *held = Some(state);
                                if !sink(Event::PadState(id, state)) {
                                    return;
                                }
                            }
                        }
                        (None, true) => {
                            *held = None;
                            if !sink(Event::PadGone(id)) {
                                return;
                            }
                        }
                        (None, false) => {}
                    }
                }
                std::thread::sleep(POLL);
            }
        })
        .expect("xinput thread");
}

fn info(slot: u32, id: &str) -> ControllerInfo {
    ControllerInfo {
        id: id.to_string(),
        name: format!("Xbox-Controller {}", slot + 1),
        // XInput does not tell; the 360 pad's ids pick the matching picture.
        vendor_id: 0x045e,
        product_id: 0x028e,
        backend: "xinput".into(),
        capabilities: Capabilities { rumble: true, player_indicator: false },
        buttons: PadButton::ALL.to_vec(),
        axes: PadAxis::ALL.to_vec(),
    }
}

fn to_state(pad: &XINPUT_GAMEPAD) -> ControllerState {
    let mut state = ControllerState::default();
    buttons_from_bits(pad.wButtons as u16, &mut state);
    state.set_axis(PadAxis::LeftTrigger, pad.bLeftTrigger as f32 / 255.0);
    state.set_axis(PadAxis::RightTrigger, pad.bRightTrigger as f32 / 255.0);
    state.set_axis(PadAxis::LeftX, stick_from_i16(pad.sThumbLX));
    state.set_axis(PadAxis::LeftY, stick_from_i16(pad.sThumbLY));
    state.set_axis(PadAxis::RightX, stick_from_i16(pad.sThumbRX));
    state.set_axis(PadAxis::RightY, stick_from_i16(pad.sThumbRY));
    state
}

struct Output(u32);

impl PadOutput for Output {
    fn set_rumble(&mut self, low: f32, high: f32) {
        let speed = |v: f32| trigger_to_u8(v) as u16 * 257;
        let vibration = XINPUT_VIBRATION { wLeftMotorSpeed: speed(low), wRightMotorSpeed: speed(high) };
        unsafe { XInputSetState(self.0, &vibration) };
    }

    fn set_indicator(&mut self, _indicator: PlayerIndicator) {}
}
