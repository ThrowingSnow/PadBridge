//! What the LED ring shows, independent of threads and timers: the platform
//! layer feeds it clock cues and random ticks and forwards the indicator it
//! returns to every connected pad.

use crate::device::PlayerIndicator;
use crate::midi::RingCue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingMode {
    Manual,
    Random,
    /// Clock: LED = bar within 4 bars (bar 1 -> LED 1 ... bar 4 -> LED 4), blinking on every beat.
    ClockBar,
    /// Clock: every beat the next LED blinks, going round the ring clockwise.
    ClockBeat,
}

impl RingMode {
    pub fn is_clock(self) -> bool {
        matches!(self, RingMode::ClockBar | RingMode::ClockBeat)
    }
}

/// LED numbers are 1 top-left, 2 top-right, 3 bottom-left, 4 bottom-right,
/// so going round the ring is 1, 2, 4, 3.
pub const CLOCKWISE: [u8; 4] = [1, 2, 4, 3];

#[derive(Debug, Clone)]
pub struct Ring {
    mode: RingMode,
    manual: PlayerIndicator,
    last_random: u8,
    rng: u64,
}

impl Default for Ring {
    fn default() -> Self {
        let mut seed = [0u8; 8];
        let _ = getrandom::getrandom(&mut seed);
        Ring {
            mode: RingMode::Manual,
            manual: PlayerIndicator::Player(1),
            last_random: 0,
            rng: u64::from_le_bytes(seed) | 1,
        }
    }
}

impl Ring {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mode(&self) -> RingMode {
        self.mode
    }

    pub fn manual(&self) -> PlayerIndicator {
        self.manual
    }

    /// Switches to manual mode; returns what to show.
    pub fn set_manual(&mut self, indicator: PlayerIndicator) -> PlayerIndicator {
        self.mode = RingMode::Manual;
        self.manual = indicator;
        indicator
    }

    /// Returns what to show right away (None for random: the first `random_step` decides).
    pub fn set_mode(&mut self, mode: RingMode) -> Option<PlayerIndicator> {
        self.mode = mode;
        self.current()
    }

    /// What a newly connected pad should show.
    pub fn current(&self) -> Option<PlayerIndicator> {
        match self.mode {
            RingMode::Manual => Some(self.manual),
            // Until the first beat arrives, show the ring is waiting for clock.
            RingMode::ClockBar | RingMode::ClockBeat => Some(PlayerIndicator::Quadrant(1)),
            RingMode::Random => None,
        }
    }

    pub fn clock_cue(&self, cue: RingCue) -> Option<PlayerIndicator> {
        if !self.mode.is_clock() {
            return None;
        }
        Some(match cue {
            RingCue::Beat { beat, bar } => PlayerIndicator::Quadrant(if self.mode == RingMode::ClockBar {
                (bar.rem_euclid(4) + 1) as u8
            } else {
                CLOCKWISE[beat.rem_euclid(4) as usize]
            }),
            RingCue::Off => PlayerIndicator::Off,
            RingCue::Stopped => PlayerIndicator::Quadrant(1),
        })
    }

    /// One step of the random animation: a different quadrant (sometimes dark)
    /// and how long to wait before the next step (60..=280 ms).
    pub fn random_step(&mut self) -> (PlayerIndicator, std::time::Duration) {
        let indicator = if self.next(5) == 0 {
            self.last_random = 0;
            PlayerIndicator::Off
        } else {
            let mut q = self.next(4) as u8 + 1;
            while q == self.last_random {
                q = self.next(4) as u8 + 1;
            }
            self.last_random = q;
            PlayerIndicator::Quadrant(q)
        };
        (indicator, std::time::Duration::from_millis(60 + self.next(221)))
    }

    /// xorshift64*: plenty for blinking LEDs, no rand dependency.
    fn next(&mut self, below: u64) -> u64 {
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        (self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 33) % below
    }
}
