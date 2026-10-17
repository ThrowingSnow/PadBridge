//! Elektron Digitakt II as a MIDI source: which parameter a CC is.
//!
//! The data encoders A-H send a different CC on every page, on the channel
//! of the active track (tracks 1-16 on channels 1-16 by default). Numbers
//! after the Digitakt II manual (OS 1.03, appendix B); parameters that only
//! have an NRPN are left out.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    Src,
    Fltr,
    Amp,
    Fx,
    Track,
}

impl Page {
    pub const ALL: [Page; 4] = [Page::Src, Page::Fltr, Page::Amp, Page::Fx];

    pub fn name(self) -> &'static str {
        match self {
            Page::Src => "SRC",
            Page::Fltr => "FLTR",
            Page::Amp => "AMP",
            Page::Fx => "FX",
            Page::Track => "TRACK",
        }
    }

    /// Encoders A-H of the page: (CC, short name); None where the parameter
    /// has no CC.
    pub fn encoders(self) -> [Option<(u8, &'static str)>; 8] {
        match self {
            Page::Src => [
                Some((16, "TUNE")),
                Some((17, "PLAY")),
                None,
                Some((19, "SAMP")),
                Some((20, "STRT")),
                Some((21, "LEN")),
                Some((22, "LOOP")),
                Some((23, "LEV")),
            ],
            Page::Fltr => [
                Some((70, "ATK")),
                Some((71, "DEC")),
                Some((72, "SUS")),
                Some((73, "REL")),
                Some((74, "FREQ")),
                None,
                None,
                Some((77, "ENV")),
            ],
            Page::Amp => [
                Some((79, "ATK")),
                Some((80, "HOLD")),
                Some((81, "DEC")),
                Some((82, "SUS")),
                Some((83, "REL")),
                None,
                Some((90, "PAN")),
                Some((89, "VOL")),
            ],
            Page::Fx => [
                Some((57, "OVER")),
                Some((12, "CHOR")),
                Some((84, "DEL")),
                Some((85, "REV")),
                None,
                None,
                None,
                None,
            ],
            Page::Track => [None; 8],
        }
    }
}

/// The auto channel (channel 10 here, 0-based 9): pressing trig key n also
/// sends note n-1 on it, besides the note on the track's own channel.
pub const AUTO_CHANNEL: u8 = 9;

/// The trig key (0..16) a note stands for: auto-channel notes 0-15 are the
/// key pressed, any other note lights the key of its channel's track.
pub fn trig_key(channel: u8, note: u8) -> Option<u8> {
    if channel == AUTO_CHANNEL {
        (note < 16).then_some(note)
    } else {
        (channel < 16).then_some(channel)
    }
}

pub const TRACK_MUTE: u8 = 94;
pub const TRACK_LEVEL: u8 = 95;

pub fn is_digitakt(device_name: &str) -> bool {
    device_name.to_lowercase().contains("digitakt")
}

/// Page, encoder index (None for track level/mute) and full name of a CC.
pub fn param(cc: u8) -> Option<(Page, Option<usize>, &'static str)> {
    match cc {
        TRACK_MUTE => return Some((Page::Track, None, "Track Mute")),
        TRACK_LEVEL => return Some((Page::Track, None, "Track Level")),
        _ => {}
    }
    for page in Page::ALL {
        if let Some(i) = page.encoders().iter().position(|e| e.is_some_and(|(n, _)| n == cc)) {
            return Some((page, Some(i), long_name(page, cc)));
        }
    }
    None
}

fn long_name(page: Page, cc: u8) -> &'static str {
    match (page, cc) {
        (Page::Src, 16) => "Tune",
        (Page::Src, 17) => "Play Mode",
        (Page::Src, 19) => "Sample Slot",
        (Page::Src, 20) => "Sample Start",
        (Page::Src, 21) => "Sample Length",
        (Page::Src, 22) => "Sample Loop",
        (Page::Src, 23) => "Sample Level",
        (Page::Fltr, 70) => "Filter Attack",
        (Page::Fltr, 71) => "Filter Decay",
        (Page::Fltr, 72) => "Filter Sustain",
        (Page::Fltr, 73) => "Filter Release",
        (Page::Fltr, 74) => "Filter Frequency",
        (Page::Fltr, 77) => "Filter Env Depth",
        (Page::Amp, 79) => "Amp Attack",
        (Page::Amp, 80) => "Amp Hold",
        (Page::Amp, 81) => "Amp Decay",
        (Page::Amp, 82) => "Amp Sustain",
        (Page::Amp, 83) => "Amp Release",
        (Page::Amp, 89) => "Amp Volume",
        (Page::Amp, 90) => "Amp Pan",
        (Page::Fx, 57) => "Overdrive",
        (Page::Fx, 12) => "Chorus Send",
        (Page::Fx, 84) => "Delay Send",
        (Page::Fx, 85) => "Reverb Send",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_parameters_by_cc() {
        assert_eq!(param(74), Some((Page::Fltr, Some(4), "Filter Frequency")));
        assert_eq!(param(89), Some((Page::Amp, Some(7), "Amp Volume")));
        assert_eq!(param(95), Some((Page::Track, None, "Track Level")));
        assert_eq!(param(1), None);
    }

    #[test]
    fn auto_channel_notes_are_keys() {
        assert_eq!(trig_key(AUTO_CHANNEL, 15), Some(15));
        assert_eq!(trig_key(AUTO_CHANNEL, 60), None);
        assert_eq!(trig_key(2, 60), Some(2));
    }

    #[test]
    fn every_encoder_has_a_long_name() {
        for page in Page::ALL {
            for (cc, _) in page.encoders().into_iter().flatten() {
                assert!(!long_name(page, cc).is_empty(), "{cc}");
            }
        }
    }
}
