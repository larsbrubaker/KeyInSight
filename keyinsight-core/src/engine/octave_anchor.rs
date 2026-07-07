//! Octave anchor: single-note practice follows the octave the user starts
//! in. A player sitting an octave off middle C reads and matches pitch
//! classes perfectly — without this they'd see nothing but wrong marks and
//! no way to understand why.
//!
//! Rules: until anchored, any note-on whose pitch class matches the
//! expected note (within ±2 octaves) locks the shift for the rest of the
//! exercise — including an exact match, which locks a zero shift so later
//! octave slips count as real errors. Wrong pitch classes never anchor.
//!
//! Ports `Engine/OctaveAnchor.swift`.

#[derive(Debug, Clone, Copy, Default)]
pub struct OctaveAnchor {
    /// Semitones added to incoming notes once anchored (None = not yet).
    shift: Option<i32>,
}

impl OctaveAnchor {
    pub const MAX_OCTAVES: i32 = 2;

    pub fn shift(&self) -> Option<i32> {
        self.shift
    }

    /// The user's octave offset relative to the score (negative = playing
    /// below the written octave). 0 until anchored.
    pub fn user_octaves(&self) -> i32 {
        match self.shift {
            Some(shift) => -shift / 12,
            None => 0,
        }
    }

    /// Feed a note-on with the currently expected pitch; returns the
    /// effective pitch to match with.
    pub fn process_note_on(&mut self, midi: u8, expected: Option<u8>) -> u8 {
        if self.shift.is_none() {
            if let Some(expected) = expected {
                let delta = midi as i32 - expected as i32;
                if delta % 12 == 0 && delta.abs() <= Self::MAX_OCTAVES * 12 {
                    self.shift = Some(-delta);
                }
            }
        }
        self.apply(midi)
    }

    /// Shift any event (note-offs too, so on/off pairs stay consistent).
    pub fn apply(&self, midi: u8) -> u8 {
        match self.shift {
            Some(shift) if shift != 0 => (midi as i32 + shift).clamp(0, 127) as u8,
            _ => midi,
        }
    }
}
