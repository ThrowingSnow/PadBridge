//! Keyboard/mouse on macOS via CGEvent (port of MacEventPoster.swift).
//! Needs System Settings -> Privacy & Security -> Accessibility for the app.

use std::collections::HashSet;
use std::io;

use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::display::{CGDisplay, CGPoint};
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, CGMouseButton, EventField, ScrollEventUnit,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use padbridge_core::keymouse::{KeyCode, KmEvent, MouseButton, WireInput};

use crate::platform::{KeyMouseOut, VirtualKeyMouse};

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: core_foundation::dictionary::CFDictionaryRef) -> bool;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
}

/// One wheel detent in pixels (synthetic pixel scrolls have no detents).
const PIXELS_PER_DETENT: f64 = 40.0;

pub struct Poster {
    held_buttons: HashSet<MouseButton>,
    held_keys: HashSet<KeyCode>,
    flags: CGEventFlags,
    scroll_rest: (f64, f64),
}

impl Poster {
    /// Fails (and shows the system prompt once) while Accessibility is not granted.
    pub fn open() -> io::Result<Self> {
        if !unsafe { AXIsProcessTrusted() } {
            unsafe {
                let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
                let options = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);
                AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef());
            }
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Bedienungshilfen-Freigabe fehlt (Systemeinstellungen → Sicherheit → Bedienungshilfen → PadBridge erlauben)",
            ));
        }
        Ok(Poster {
            held_buttons: HashSet::new(),
            held_keys: HashSet::new(),
            flags: CGEventFlags::empty(),
            scroll_rest: (0.0, 0.0),
        })
    }

    fn source() -> Option<CGEventSource> {
        CGEventSource::new(CGEventSourceStateID::HIDSystemState).ok()
    }

    fn post_one(&mut self, event: KmEvent) {
        match event {
            KmEvent::Key(key, down) => self.key(key, down),
            KmEvent::Button(button, down) => self.button(button, down),
            KmEvent::Move { dx, dy } => self.move_by(dx, dy),
            KmEvent::Scroll { dx, dy } => self.scroll(dx, dy),
        }
    }

    fn key(&mut self, key: KeyCode, down: bool) {
        if down {
            self.held_keys.insert(key);
        } else {
            self.held_keys.remove(&key);
        }
        if let Some(flag) = flag_for(key) {
            self.flags.set(flag, down);
        }
        let Some(source) = Self::source() else { return };
        if let Ok(event) = CGEvent::new_keyboard_event(source, key.mac_code(), down) {
            event.set_flags(self.flags);
            event.post(CGEventTapLocation::HID);
        }
    }

    fn button(&mut self, button: MouseButton, down: bool) {
        if down {
            self.held_buttons.insert(button);
        } else {
            self.held_buttons.remove(&button);
        }
        let (kind, cg) = match (button, down) {
            (MouseButton::Left, true) => (CGEventType::LeftMouseDown, CGMouseButton::Left),
            (MouseButton::Left, false) => (CGEventType::LeftMouseUp, CGMouseButton::Left),
            (MouseButton::Right, true) => (CGEventType::RightMouseDown, CGMouseButton::Right),
            (MouseButton::Right, false) => (CGEventType::RightMouseUp, CGMouseButton::Right),
            (MouseButton::Middle, true) => (CGEventType::OtherMouseDown, CGMouseButton::Center),
            (MouseButton::Middle, false) => (CGEventType::OtherMouseUp, CGMouseButton::Center),
        };
        let Some(source) = Self::source() else { return };
        if let Ok(event) = CGEvent::new_mouse_event(source, kind, current_location(), cg) {
            event.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, 1);
            event.set_flags(self.flags);
            event.post(CGEventTapLocation::HID);
        }
    }

    fn move_by(&mut self, dx: i32, dy: i32) {
        let from = current_location();
        let to = clamp_to_displays(CGPoint::new(from.x + dx as f64, from.y + dy as f64));
        // While a button is held, macOS expects drag events (e.g. selecting text).
        let (kind, button) = if self.held_buttons.contains(&MouseButton::Left) {
            (CGEventType::LeftMouseDragged, CGMouseButton::Left)
        } else if self.held_buttons.contains(&MouseButton::Right) {
            (CGEventType::RightMouseDragged, CGMouseButton::Right)
        } else if self.held_buttons.contains(&MouseButton::Middle) {
            (CGEventType::OtherMouseDragged, CGMouseButton::Center)
        } else {
            (CGEventType::MouseMoved, CGMouseButton::Left)
        };
        let Some(source) = Self::source() else { return };
        if let Ok(event) = CGEvent::new_mouse_event(source, kind, to, button) {
            // Relative deltas: apps with a captured pointer (Moonlight in game mode) read only these.
            event.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_X, dx as i64);
            event.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_Y, dy as i64);
            event.set_flags(self.flags);
            event.post(CGEventTapLocation::HID);
        }
    }

    fn scroll(&mut self, dx: i32, dy: i32) {
        self.scroll_rest.0 += dx as f64 / 120.0 * PIXELS_PER_DETENT;
        self.scroll_rest.1 += dy as f64 / 120.0 * PIXELS_PER_DETENT;
        let (px, py) = (self.scroll_rest.0 as i32, self.scroll_rest.1 as i32);
        self.scroll_rest.0 -= px as f64;
        self.scroll_rest.1 -= py as f64;
        if px == 0 && py == 0 {
            return;
        }
        let Some(source) = Self::source() else { return };
        // wheel2 positive scrolls left, ours means right.
        if let Ok(event) = CGEvent::new_scroll_event(source, ScrollEventUnit::PIXEL, 2, py, -px, 0) {
            event.post(CGEventTapLocation::HID);
        }
    }
}

fn flag_for(key: KeyCode) -> Option<CGEventFlags> {
    match key {
        KeyCode::LeftShift => Some(CGEventFlags::CGEventFlagShift),
        KeyCode::LeftControl => Some(CGEventFlags::CGEventFlagControl),
        KeyCode::LeftOption => Some(CGEventFlags::CGEventFlagAlternate),
        KeyCode::LeftCommand => Some(CGEventFlags::CGEventFlagCommand),
        _ => None,
    }
}

fn current_location() -> CGPoint {
    Poster::source().and_then(|s| CGEvent::new(s).ok()).map_or(CGPoint::new(0.0, 0.0), |e| e.location())
}

/// Keeps the pointer on some display (global coordinates, top-left origin).
fn clamp_to_displays(p: CGPoint) -> CGPoint {
    let bounds: Vec<_> =
        CGDisplay::active_displays().unwrap_or_default().into_iter().map(|id| CGDisplay::new(id).bounds()).collect();
    let inside = |r: &core_graphics::geometry::CGRect| {
        p.x >= r.origin.x && p.x < r.origin.x + r.size.width && p.y >= r.origin.y && p.y < r.origin.y + r.size.height
    };
    if bounds.is_empty() || bounds.iter().any(inside) {
        return p;
    }
    let distance = |r: &core_graphics::geometry::CGRect| {
        let dx = (r.origin.x - p.x).max(0.0).max(p.x - (r.origin.x + r.size.width));
        let dy = (r.origin.y - p.y).max(0.0).max(p.y - (r.origin.y + r.size.height));
        dx * dx + dy * dy
    };
    let nearest = bounds.iter().min_by(|a, b| distance(a).total_cmp(&distance(b))).unwrap();
    CGPoint::new(
        p.x.clamp(nearest.origin.x, nearest.origin.x + nearest.size.width - 1.0),
        p.y.clamp(nearest.origin.y, nearest.origin.y + nearest.size.height - 1.0),
    )
}

impl KeyMouseOut for Poster {
    fn post(&mut self, events: &[KmEvent]) {
        for &e in events {
            self.post_one(e);
        }
    }
}

/// A peer's keyboard/mouse (wire format uses Linux codes) on this Mac.
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
                2 => self.move_by(e.a as i32, e.b as i32),
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
