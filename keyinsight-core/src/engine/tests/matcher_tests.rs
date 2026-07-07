//! Ports `Tests/KeyInSightTests/MatcherTests.swift`.

use crate::engine::{SelfPacedMatcher, SelfPacedOutcome};
use std::collections::HashSet;

fn sets(sets: &[&[u8]]) -> Vec<HashSet<u8>> {
    sets.iter().map(|s| s.iter().copied().collect()).collect()
}

fn matched(index: usize, set_complete: bool, exercise_complete: bool) -> SelfPacedOutcome {
    SelfPacedOutcome::Matched {
        index,
        set_complete,
        exercise_complete,
    }
}

#[test]
fn correct_sequence_advances_and_completes() {
    let mut matcher = SelfPacedMatcher::new(sets(&[&[60], &[62], &[64]]));
    assert_eq!(matcher.consume_note_on(60), matched(0, true, false));
    assert_eq!(matcher.consume_note_on(62), matched(1, true, false));
    assert_eq!(matcher.consume_note_on(64), matched(2, true, true));
    assert!(matcher.is_complete());
}

#[test]
fn wrong_note_does_not_advance() {
    let mut matcher = SelfPacedMatcher::new(sets(&[&[60], &[62]]));
    assert_eq!(
        matcher.consume_note_on(65),
        SelfPacedOutcome::Wrong {
            index: 0,
            played: 65
        }
    );
    assert_eq!(matcher.index(), 0);
    assert_eq!(matcher.consume_note_on(60), matched(0, true, false));
}

#[test]
fn repeated_pitch_across_consecutive_notes() {
    let mut matcher = SelfPacedMatcher::new(sets(&[&[60], &[60]]));
    assert_eq!(matcher.consume_note_on(60), matched(0, true, false));
    assert_eq!(matcher.consume_note_on(60), matched(1, true, true));
}

#[test]
fn input_after_completion_is_ignored() {
    let mut matcher = SelfPacedMatcher::new(sets(&[&[60]]));
    let _ = matcher.consume_note_on(60);
    assert_eq!(matcher.consume_note_on(60), SelfPacedOutcome::Ignored);
    assert_eq!(matcher.consume_note_on(99), SelfPacedOutcome::Ignored);
}

#[test]
fn chord_set_requires_all_members() {
    let mut matcher = SelfPacedMatcher::new(sets(&[&[60, 64, 67], &[72]]));
    assert_eq!(matcher.consume_note_on(64), matched(0, false, false));
    // Re-strike of an already-marked member: ignored, not wrong.
    assert_eq!(matcher.consume_note_on(64), SelfPacedOutcome::Ignored);
    assert_eq!(matcher.consume_note_on(60), matched(0, false, false));
    assert_eq!(matcher.consume_note_on(67), matched(0, true, false));
    assert_eq!(matcher.index(), 1);
}

#[test]
fn empty_exercise_is_immediately_complete() {
    let mut matcher = SelfPacedMatcher::new(Vec::new());
    assert!(matcher.is_complete());
    assert_eq!(matcher.consume_note_on(60), SelfPacedOutcome::Ignored);
}
