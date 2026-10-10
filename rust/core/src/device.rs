//! What backends report about a pad, and what can be sent back to it.

use crate::controls::{PadAxis, PadButton};

/// Features beyond input; the UI greys out what a device does not have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Capabilities {
    pub rumble: bool,
    /// Player indicator such as the Xbox 360 ring of four LEDs.
    pub player_indicator: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ControllerInfo {
    /// Stable for as long as the device stays plugged in (e.g. USB bus/port path).
    pub id: String,
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    /// Which backend found it, e.g. "usb" or "evdev".
    pub backend: String,
    pub capabilities: Capabilities,
    pub buttons: Vec<PadButton>,
    pub axes: Vec<PadAxis>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerIndicator {
    Off,
    /// Player 1..=4: briefly flashing, then steady.
    Player(u8),
    /// Quadrant 1..=4 on immediately, without the flash. For animations.
    Quadrant(u8),
    BlinkAll,
    Rotate,
}
