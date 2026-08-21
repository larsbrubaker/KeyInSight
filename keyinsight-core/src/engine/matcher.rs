//! Self-paced matcher: "the expected set right now is {notes of current
//! event}." A note-on matching a member marks it; when the full set is
//! played the cursor advances. Non-members are wrong-note feedback and
//! never advance. Set-based so chords need no change.
//!
//! Multi-pitch events (chords, hands together) must be struck as one:
//! every member has to land within the chord window of the first. A
//! member arriving late breaks the attempt — that's an error, and the
//! late strike opens a fresh attempt so the chord must be re-struck
//! together.
//!
//! Ports `Engine/Matcher.swift`.

use std::collections::HashSet;

use crate::score::FreePlayScore;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfPacedOutcome {
    /// A member of the current expected set was played.
    Matched {
        index: usize,
        set_complete: bool,
        exercise_complete: bool,
    },
    /// A member landed after the chord window: the attempt at `index`
    /// resets (an error), with `played` as the new attempt's first
    /// member.
    Restarted { index: usize, played: u8 },
    /// A non-member was played; the cursor stays at `index`.
    Wrong { index: usize, played: u8 },
    /// Re-strike of an already-marked chord member, or input after
    /// completion — no feedback change.
    Ignored,
}

pub struct SelfPacedMatcher {
    pub expected: Vec<HashSet<u8>>,
    index: usize,
    remaining: HashSet<u8>,
    /// Timestamp of the current attempt's first strike (`None` = none yet).
    attempt_start: Option<f64>,
}

impl SelfPacedMatcher {
    /// Members landing within this window sound "together" — matches the
    /// free-play chord grouping (both hands landing "simultaneously"
    /// spread over tens of ms on real input).
    pub const CHORD_WINDOW_SECONDS: f64 = FreePlayScore::CHORD_WINDOW_SECONDS;

    pub fn new(expected: Vec<HashSet<u8>>) -> Self {
        Self::with_start_index(expected, 0)
    }

    /// `start_index` begins the exercise mid-list (repertoire
    /// practice-from-here); earlier events are simply never expected.
    pub fn with_start_index(expected: Vec<HashSet<u8>>, start_index: usize) -> Self {
        let index = start_index.min(expected.len());
        let remaining = expected.get(index).cloned().unwrap_or_default();
        Self {
            expected,
            index,
            remaining,
            attempt_start: None,
        }
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn is_complete(&self) -> bool {
        self.index >= self.expected.len()
    }

    /// Consume a note-on with no timing information (the chord window
    /// never elapses, so multi-pitch sets can be struck at leisure).
    pub fn consume_note_on(&mut self, midi: u8) -> SelfPacedOutcome {
        self.consume_note_on_at(midi, 0.0)
    }

    pub fn consume_note_on_at(&mut self, midi: u8, time: f64) -> SelfPacedOutcome {
        if self.is_complete() {
            return SelfPacedOutcome::Ignored;
        }
        let set = &self.expected[self.index];
        if !set.contains(&midi) {
            return SelfPacedOutcome::Wrong {
                index: self.index,
                played: midi,
            };
        }

        // A partial chord attempt is in flight and this member is late:
        // break it and start over with this strike.
        if let Some(start) = self.attempt_start {
            if self.remaining.len() < set.len() && time - start > Self::CHORD_WINDOW_SECONDS {
                self.remaining = set.clone();
                self.remaining.remove(&midi);
                self.attempt_start = Some(time);
                return SelfPacedOutcome::Restarted {
                    index: self.index,
                    played: midi,
                };
            }
        }

        if !self.remaining.contains(&midi) {
            return SelfPacedOutcome::Ignored;
        }
        if self.remaining.len() == set.len() {
            self.attempt_start = Some(time);
        }
        self.remaining.remove(&midi);
        let current_index = self.index;
        let set_complete = self.remaining.is_empty();
        if set_complete {
            self.index += 1;
            self.remaining = if self.is_complete() {
                HashSet::new()
            } else {
                self.expected[self.index].clone()
            };
            self.attempt_start = None;
        }
        SelfPacedOutcome::Matched {
            index: current_index,
            set_complete,
            exercise_complete: self.is_complete(),
        }
    }
}
