//! Pitch-spelling helpers over raw MIDI numbers.
//!
//! Phase 1 spells all black keys as sharps (no key-signature awareness yet).
//! Key signatures arrive with the Phase 2 skill model; this type is the
//! single place that mapping will change.
//!
//! Ports `Core/PitchSpelling.swift`.

/// Namespace struct mirroring the Swift caseless `enum PitchSpelling`.
pub struct PitchSpelling;

const STEP_NAMES: [&str; 12] = ["C", "C", "D", "D", "E", "F", "F", "G", "G", "A", "A", "B"];
const ALTERS: [i32; 12] = [0, 1, 0, 1, 0, 0, 1, 0, 1, 0, 1, 0];
/// Flat spelling of each pitch class (D♭ E♭ G♭ A♭ B♭).
const FLAT_STEP_NAMES: [&str; 12] = ["C", "D", "D", "E", "E", "F", "G", "G", "A", "A", "B", "B"];
const FLAT_ALTERS: [i32; 12] = [0, -1, 0, -1, 0, 0, -1, 0, -1, 0, -1, 0];
/// Diatonic step (0=C … 6=B) of the spelled natural for each pitch class.
const STEP_IN_OCTAVE: [i32; 12] = [0, 0, 1, 1, 2, 3, 3, 4, 4, 5, 5, 6];

/// MusicXML-style spelling of a MIDI number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spelling {
    pub step: &'static str,
    pub alter: i32,
    /// Scientific octave (C4 = middle C).
    pub octave: i32,
}

impl PitchSpelling {
    /// MusicXML-style spelling: step letter, alter, scientific octave.
    /// Flat keys (fifths < 0) spell black keys as flats.
    pub fn spell(midi: u8, prefer_flats: bool) -> Spelling {
        let pc = (midi as usize) % 12;
        if prefer_flats {
            // A flat spelling can cross the octave boundary (B♭ stays; C♭ n/a).
            return Spelling {
                step: FLAT_STEP_NAMES[pc],
                alter: FLAT_ALTERS[pc],
                octave: (midi as i32) / 12 - 1,
            };
        }
        Spelling {
            step: STEP_NAMES[pc],
            alter: ALTERS[pc],
            octave: (midi as i32) / 12 - 1,
        }
    }

    /// Display / skill-item name, e.g. "C4", "F#4".
    pub fn name(midi: u8) -> String {
        let s = Self::spell(midi, false);
        format!(
            "{}{}{}",
            s.step,
            if s.alter == 1 { "#" } else { "" },
            s.octave
        )
    }

    /// Major-key name for a key signature, e.g. −2 → "B♭ major".
    pub fn key_name(fifths: i32) -> String {
        const NAMES: [&str; 15] = [
            "C♭", "G♭", "D♭", "A♭", "E♭", "B♭", "F", "C", "G", "D", "A", "E", "B", "F♯", "C♯",
        ];
        let index = (fifths.clamp(-7, 7) + 7) as usize;
        format!("{} major", NAMES[index])
    }

    /// Absolute diatonic index (C-1 = 0, one per staff position). Sharps
    /// share the index of their natural — exactly what ghost-note
    /// positioning needs.
    pub fn diatonic_index(midi: u8) -> i32 {
        let pc = (midi as usize) % 12;
        (midi as i32 / 12) * 7 + STEP_IN_OCTAVE[pc]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spells_naturals_and_sharps() {
        assert_eq!(PitchSpelling::name(60), "C4");
        assert_eq!(PitchSpelling::name(61), "C#4");
        assert_eq!(PitchSpelling::name(69), "A4");
        assert_eq!(PitchSpelling::name(59), "B3");
    }

    #[test]
    fn flat_spelling_uses_flats() {
        let s = PitchSpelling::spell(61, true);
        assert_eq!((s.step, s.alter, s.octave), ("D", -1, 4));
        let s = PitchSpelling::spell(70, true);
        assert_eq!((s.step, s.alter, s.octave), ("B", -1, 4));
    }

    #[test]
    fn key_names() {
        assert_eq!(PitchSpelling::key_name(0), "C major");
        assert_eq!(PitchSpelling::key_name(-2), "B♭ major");
        assert_eq!(PitchSpelling::key_name(2), "D major");
    }

    #[test]
    fn diatonic_index_shares_sharp_with_natural() {
        assert_eq!(
            PitchSpelling::diatonic_index(60),
            PitchSpelling::diatonic_index(61)
        );
        // C4 to D4 is one staff position.
        assert_eq!(
            PitchSpelling::diatonic_index(62) - PitchSpelling::diatonic_index(60),
            1
        );
        // One octave is seven staff positions.
        assert_eq!(
            PitchSpelling::diatonic_index(72) - PitchSpelling::diatonic_index(60),
            7
        );
    }
}
