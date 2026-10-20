//! Keyboard/mouse on Windows via SendInput. Keys go out as scan codes, so
//! they hit the same key positions as on macOS and Linux whatever the layout,
//! and games reading raw input see them too. Input into elevated windows
//! (run as administrator) is blocked by Windows unless PadBridge runs so too.

use std::collections::HashSet;
use std::io;

use padbridge_core::keymouse::{KeyCode, KmEvent, MouseButton, WireInput};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    KEYEVENTF_SCANCODE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
    MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEINPUT,
};

use crate::platform::{KeyMouseOut, VirtualKeyMouse};

#[derive(Default)]
pub struct Poster {
    held_buttons: HashSet<MouseButton>,
    held_keys: HashSet<KeyCode>,
}

impl Poster {
    fn post_one(&mut self, event: KmEvent) {
        match event {
            KmEvent::Key(key, down) => self.key(key, down),
            KmEvent::Button(button, down) => self.button(button, down),
            KmEvent::Move { dx, dy } => mouse(MOUSEEVENTF_MOVE, dx, dy, 0),
            KmEvent::Scroll { dx, dy } => self.scroll(dx, dy),
        }
    }

    fn key(&mut self, key: KeyCode, down: bool) {
        if down {
            self.held_keys.insert(key);
        } else {
            self.held_keys.remove(&key);
        }
        let (scan, extended) = scan_code(key.linux_code());
        let mut flags = KEYEVENTF_SCANCODE;
        if extended {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        if !down {
            flags |= KEYEVENTF_KEYUP;
        }
        let ki = KEYBDINPUT { wVk: 0, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 };
        send(INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki } });
    }

    fn button(&mut self, button: MouseButton, down: bool) {
        if down {
            self.held_buttons.insert(button);
        } else {
            self.held_buttons.remove(&button);
        }
        let flags = match (button, down) {
            (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
            (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
            (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
            (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
            (MouseButton::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
            (MouseButton::Middle, false) => MOUSEEVENTF_MIDDLEUP,
        };
        mouse(flags, 0, 0, 0);
    }

    /// 120 units per wheel detent, the same unit Windows uses.
    fn scroll(&mut self, dx: i32, dy: i32) {
        if dy != 0 {
            mouse(MOUSEEVENTF_WHEEL, 0, 0, dy);
        }
        if dx != 0 {
            mouse(MOUSEEVENTF_HWHEEL, 0, 0, dx);
        }
    }
}

fn mouse(flags: u32, dx: i32, dy: i32, wheel: i32) {
    let mi = MOUSEINPUT { dx, dy, mouseData: wheel as u32, dwFlags: flags, time: 0, dwExtraInfo: 0 };
    send(INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi } });
}

fn send(input: INPUT) {
    unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) };
}

/// Linux KEY_* -> PC set 1 scan code. Codes up to KEY_F12 (88) are the scan
/// codes themselves; the keys of the navigation block carry the E0 prefix.
fn scan_code(linux: u16) -> (u16, bool) {
    match linux {
        102 => (0x47, true), // Home
        103 => (0x48, true), // Up
        104 => (0x49, true), // Page Up
        105 => (0x4B, true), // Left
        106 => (0x4D, true), // Right
        107 => (0x4F, true), // End
        108 => (0x50, true), // Down
        109 => (0x51, true), // Page Down
        111 => (0x53, true), // Delete
        125 => (0x5B, true), // left Windows key
        code => (code, false),
    }
}

impl KeyMouseOut for Poster {
    fn post(&mut self, events: &[KmEvent]) {
        for &e in events {
            self.post_one(e);
        }
    }
}

/// A peer's keyboard/mouse (wire format uses Linux codes) on this PC.
impl VirtualKeyMouse for Poster {
    fn apply(&mut self, events: &[WireInput]) -> io::Result<()> {
        for e in events {
            match e.kind {
                1 => {
                    let down = e.b != 0;
                    let code = e.a as u16;
                    if let Some(button) = MouseButton::ALL.into_iter().find(|b| b.linux_code() == code) {
                        if down != self.held_buttons.contains(&button) {
                            self.button(button, down);
                        }
                    } else if let Some(key) = KeyCode::ALL.into_iter().find(|k| k.linux_code() == code) {
                        if down != self.held_keys.contains(&key) {
                            self.key(key, down);
                        }
                    }
                }
                2 => mouse(MOUSEEVENTF_MOVE, e.a as i32, e.b as i32, 0),
                3 => self.scroll(e.b as i32, e.a as i32),
                _ => {}
            }
        }
        Ok(())
    }

    fn holding(&self) -> bool {
        !self.held_keys.is_empty() || !self.held_buttons.is_empty()
    }

    fn release_all(&mut self) -> io::Result<()> {
        for key in self.held_keys.clone() {
            self.key(key, false);
        }
        for button in self.held_buttons.clone() {
            self.button(button, false);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::scan_code;
    use padbridge_core::keymouse::KeyCode;

    #[test]
    fn plain_keys_use_their_linux_code() {
        assert_eq!(scan_code(KeyCode::A.linux_code()), (0x1E, false));
        assert_eq!(scan_code(KeyCode::F12.linux_code()), (0x58, false));
        assert_eq!(scan_code(KeyCode::Up.linux_code()), (0x48, true));
    }
}
