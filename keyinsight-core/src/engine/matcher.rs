//! Self-paced matcher: "the expected set right now is {notes of current
//! event}." A note-on matching a member marks it; when the full set is
//! played the cursor advances. Non-members are wrong-note feedback and
//! never advance. Set-based so chords need no change.
//!
//! Ports `Engine/Matcher.swift`.

use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfPacedOutcome {
    /// A member of the current expected set was played.
    Matched {
        index: usize,
        set_complete: bool,
        exercise_complete: bool,
    },
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
}

impl SelfPacedMatcher {
    pub fn new(expected: Vec<HashSet<u8>>) -> Self {
        let remaining = expected.first().cloned().unwrap_or_default();
        Self {
            expected,
            index: 0,
            remaining,
        }
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn is_complete(&self) -> bool {
        self.index >= self.expected.len()
    }

    pub fn consume_note_on(&mut self, midi: u8) -> SelfPacedOutcome {
        if self.is_complete() {
            return SelfPacedOutcome::Ignored;
        }
        if !self.expected[self.index].contains(&midi) {
            return SelfPacedOutcome::Wrong {
                index: self.index,
                played: midi,
            };
        }
        if !self.remaining.contains(&midi) {
            return SelfPacedOutcome::Ignored;
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
        }
        SelfPacedOutcome::Matched {
            index: current_index,
            set_complete,
            exercise_complete: self.is_complete(),
        }
    }
}
