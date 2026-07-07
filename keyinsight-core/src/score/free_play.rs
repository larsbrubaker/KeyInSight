//! Free Play mirror score: pitch-accurate, rhythm simplified to quarters,
//! a sliding window of the most recent events so engraving stays fast and
//! the staff reads like a scroll.
//!
//! Grand-staff aware: each event is a chord of one or more pitches (grouped
//! upstream by arrival time); pitches below middle C land on the bass staff.
//!
//! Ports `Score/FreePlayScore.swift`.

use crate::score::{Exercise, NoteDuration, ScoreNote, Staff};

pub struct FreePlayScore;

impl FreePlayScore {
    /// Window size in events (a chord counts as one event).
    pub const WINDOW_SIZE: usize = 16;
    /// Note-ons within this many seconds of an event's start sound together.
    pub const CHORD_WINDOW_SECONDS: f64 = 0.08;
    /// Pitches below middle C render on the bass staff.
    pub const BASS_SPLIT_MIDI: u8 = 60;

    pub fn build(chords: &[Vec<u8>]) -> Exercise {
        let recent: Vec<&Vec<u8>> = chords
            .iter()
            .rev()
            .take(Self::WINDOW_SIZE)
            .rev()
            .filter(|c| !c.is_empty())
            .collect();
        if recent.is_empty() {
            return Exercise::new(vec![ScoreNote::rest(NoteDuration::Whole)], 4);
        }
        let notes: Vec<ScoreNote> = recent
            .iter()
            .flat_map(|chord| {
                // Low-to-high reads naturally and keeps chord stems tidy.
                let mut sorted = (*chord).clone();
                sorted.sort_unstable();
                sorted
                    .into_iter()
                    .enumerate()
                    .map(|(offset, midi)| {
                        ScoreNote::note(midi, NoteDuration::Quarter)
                            .with_staff(if midi < Self::BASS_SPLIT_MIDI {
                                Staff::Bass
                            } else {
                                Staff::Treble
                            })
                            .with_chord(offset > 0)
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        Exercise::new(notes, 4)
    }
}
