/// MIDI real-time messages relevant to following a clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockMessage {
    Tick,
    Start,
    Continue,
    Stop,
    /// Song Position Pointer, in MIDI beats (16th notes).
    SongPosition(i32),
}

impl ClockMessage {
    /// Raw MIDI bytes -> clock message, None for anything else.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        match bytes.first()? {
            0xF8 => Some(ClockMessage::Tick),
            0xFA => Some(ClockMessage::Start),
            0xFB => Some(ClockMessage::Continue),
            0xFC => Some(ClockMessage::Stop),
            0xF2 if bytes.len() >= 3 => {
                Some(ClockMessage::SongPosition(((bytes[2] & 0x7F) as i32) << 7 | (bytes[1] & 0x7F) as i32))
            }
            _ => None,
        }
    }
}

/// What the LED ring should do in response to the clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingCue {
    /// Start of a quarter note. Both 0-based: `beat` within the bar (0..=3),
    /// `bar` counted from song start.
    Beat { beat: i32, bar: i32 },
    /// Half a beat later: dark, so the ring blinks at the tempo.
    Off,
    /// Transport stopped.
    Stopped,
}

/// Follows 24 PPQN MIDI clock in 4/4: counts beats and bars and estimates BPM.
#[derive(Debug, Clone)]
pub struct MidiClockTracker {
    running: bool,
    /// Ticks since song start; -1 means "the next tick is position 0".
    tick: i64,
    bpm: Option<f64>,
    last_tick_time: Option<f64>,
    average_interval: Option<f64>,
}

impl Default for MidiClockTracker {
    fn default() -> Self {
        MidiClockTracker { running: false, tick: -1, bpm: None, last_tick_time: None, average_interval: None }
    }
}

impl MidiClockTracker {
    pub const TICKS_PER_BEAT: i64 = 24;
    pub const BEATS_PER_BAR: i64 = 4;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn bpm(&self) -> Option<f64> {
        self.bpm
    }

    /// 1-based (bar, beat) of the current position, for display.
    pub fn position(&self) -> (i64, i64) {
        let beats = self.tick.max(0) / Self::TICKS_PER_BEAT;
        (beats / Self::BEATS_PER_BAR + 1, beats % Self::BEATS_PER_BAR + 1)
    }

    /// `time` is in seconds on any monotonic clock.
    pub fn handle(&mut self, message: ClockMessage, time: f64) -> Option<RingCue> {
        match message {
            ClockMessage::Start => {
                // Per the MIDI spec the first tick after Start is position 0.
                self.running = true;
                self.tick = -1;
                None
            }
            ClockMessage::Continue => {
                self.running = true;
                None
            }
            ClockMessage::Stop => {
                self.running = false;
                Some(RingCue::Stopped)
            }
            ClockMessage::SongPosition(sixteenths) => {
                self.tick = sixteenths as i64 * (Self::TICKS_PER_BEAT / 4) - 1;
                None
            }
            ClockMessage::Tick => {
                self.update_tempo(time);
                // Many hosts send clock while stopped; only the transport moves the position.
                if !self.running {
                    return None;
                }
                self.tick += 1;
                match self.tick % Self::TICKS_PER_BEAT {
                    0 => {
                        let beats = self.tick / Self::TICKS_PER_BEAT;
                        Some(RingCue::Beat {
                            beat: (beats % Self::BEATS_PER_BAR) as i32,
                            bar: (beats / Self::BEATS_PER_BAR) as i32,
                        })
                    }
                    t if t == Self::TICKS_PER_BEAT / 2 => Some(RingCue::Off),
                    _ => None,
                }
            }
        }
    }

    fn update_tempo(&mut self, time: f64) {
        let last = self.last_tick_time.replace(time);
        let Some(last) = last else { return };
        let interval = time - last;
        // Ignore gaps (clock restarted) and garbage; 20..=400 BPM.
        if !(interval > 60.0 / (400.0 * 24.0) && interval < 60.0 / (20.0 * 24.0)) {
            self.average_interval = None;
            return;
        }
        let avg = match self.average_interval {
            Some(a) => a + (interval - a) * 0.08,
            None => interval,
        };
        self.average_interval = Some(avg);
        self.bpm = Some(60.0 / (avg * Self::TICKS_PER_BEAT as f64));
    }
}
