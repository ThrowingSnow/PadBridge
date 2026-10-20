//! A peer's pad as a virtual Xbox 360 controller via the ViGEmBus driver
//! (https://github.com/nefarius/ViGEmBus, installed separately). Games see it
//! as an ordinary XInput pad; their rumble comes back as a notification.
//!
//! The virtual pad also shows up in XInput, so xinput_pads would read it as a
//! local pad and could send it straight back to the peer. Its XInput slot is
//! therefore recorded here and skipped there.

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU16, AtomicUsize, Ordering};

use padbridge_core::protocol::WireState;
use vigem_client::{Client, Error, TargetId, XButtons, XGamepad, Xbox360Wired};

use crate::platform::VirtualPad;

/// Bit n set: XInput slot n is one of our virtual pads.
static VIRTUAL_SLOTS: AtomicU8 = AtomicU8::new(0);
/// Virtual pads being plugged in whose slot is not known yet.
static PLUGGING: AtomicUsize = AtomicUsize::new(0);

/// True if xinput_pads should leave this slot alone.
pub fn is_virtual(slot: u32) -> bool {
    VIRTUAL_SLOTS.load(Ordering::Acquire) & (1 << slot) != 0
}

/// True while a virtual pad is appearing; new slots are not taken meanwhile.
pub fn plugging() -> bool {
    PLUGGING.load(Ordering::Acquire) > 0
}

pub struct VigemPad {
    target: Xbox360Wired<Client>,
    slot: Option<u32>,
    /// Large motor << 8 | small motor, as the last game asked.
    rumble: Arc<AtomicU16>,
}

impl VigemPad {
    pub fn create() -> io::Result<Self> {
        let client = Client::connect().map_err(|e| match e {
            Error::BusNotFound => io::Error::new(
                io::ErrorKind::NotFound,
                "ViGEmBus ist nicht installiert (github.com/nefarius/ViGEmBus/releases, danach neu starten)",
            ),
            e => io::Error::other(format!("ViGEmBus: {e}")),
        })?;
        let mut target = Xbox360Wired::new(client, TargetId::XBOX360_WIRED);
        PLUGGING.fetch_add(1, Ordering::AcqRel);
        let plugged = target.plugin().and_then(|_| target.wait_ready());
        let slot = plugged.as_ref().ok().and_then(|_| target.get_user_index().ok());
        if let Some(slot) = slot {
            VIRTUAL_SLOTS.fetch_or(1 << slot, Ordering::AcqRel);
        }
        PLUGGING.fetch_sub(1, Ordering::AcqRel);
        plugged.map_err(|e| io::Error::other(format!("ViGEmBus: Pad nicht angelegt ({e})")))?;

        let rumble = Arc::new(AtomicU16::new(0));
        if let Ok(request) = target.request_notification() {
            let level = rumble.clone();
            // Ends by itself when the target is dropped.
            request.spawn_thread(move |_, n| {
                level.store((n.large_motor as u16) << 8 | n.small_motor as u16, Ordering::Relaxed);
            });
        }
        Ok(VigemPad { target, slot, rumble })
    }
}

impl Drop for VigemPad {
    fn drop(&mut self) {
        let _ = self.target.unplug();
        if let Some(slot) = self.slot {
            VIRTUAL_SLOTS.fetch_and(!(1 << slot), Ordering::AcqRel);
        }
    }
}

impl VirtualPad for VigemPad {
    fn apply(&mut self, state: WireState) -> io::Result<()> {
        // The wire format uses the XInput button bits.
        let gamepad = XGamepad {
            buttons: XButtons(state.buttons),
            left_trigger: state.lt,
            right_trigger: state.rt,
            thumb_lx: state.lx,
            thumb_ly: state.ly,
            thumb_rx: state.rx,
            thumb_ry: state.ry,
        };
        match self.target.update(&gamepad) {
            Ok(()) | Err(Error::TargetNotReady) => Ok(()),
            Err(e) => Err(io::Error::other(format!("ViGEmBus: {e}"))),
        }
    }

    fn poll_fd(&self) -> Option<i32> {
        None
    }

    /// Notifications arrive on their own thread.
    fn process_force_feedback(&mut self) {}

    fn rumble(&mut self) -> (u8, u8) {
        let level = self.rumble.load(Ordering::Relaxed);
        ((level >> 8) as u8, level as u8)
    }

    fn describe(&mut self) -> String {
        match self.slot {
            Some(slot) => format!("Xbox 360 Controller über ViGEmBus, XInput {}", slot + 1),
            None => "Xbox 360 Controller über ViGEmBus".into(),
        }
    }
}
