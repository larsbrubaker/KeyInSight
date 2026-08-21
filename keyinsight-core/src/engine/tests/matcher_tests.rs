//! Ports `Tests/KeyInSightTests/MatcherTests.swift`.

use crate::engine::{InputStormDetector, SelfPacedMatcher, SelfPacedOutcome};
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

// --- Chord window (`LeftHandTests.swift` SynchronousChordTests; the
// window-edge and re-strike details follow Matcher.swift) ---

#[test]
fn late_chord_member_restarts_the_attempt() {
    let mut matcher = SelfPacedMatcher::new(sets(&[&[60, 64], &[72]]));
    assert_eq!(matcher.consume_note_on_at(60, 1.0), matched(0, false, false));
    // The second member lands past the window: the attempt breaks and the
    // late strike opens a new one.
    assert_eq!(
        matcher.consume_note_on_at(64, 1.0 + SelfPacedMatcher::CHORD_WINDOW_SECONDS + 0.01),
        SelfPacedOutcome::Restarted {
            index: 0,
            played: 64
        }
    );
    assert_eq!(matcher.index(), 0);
    // 60 is needed again (it was dropped from the attempt); 64 carries.
    assert_eq!(matcher.consume_note_on_at(64, 1.1), SelfPacedOutcome::Ignored);
    assert_eq!(matcher.consume_note_on_at(60, 1.12), matched(0, true, false));
    assert_eq!(matcher.index(), 1);
}

#[test]
fn chord_members_within_the_window_match() {
    let mut matcher = SelfPacedMatcher::new(sets(&[&[60, 64, 67]]));
    assert_eq!(matcher.consume_note_on_at(60, 0.0), matched(0, false, false));
    assert_eq!(matcher.consume_note_on_at(64, 0.05), matched(0, false, false));
    // Exactly at the window edge still counts (strict `>`).
    assert_eq!(
        matcher.consume_note_on_at(67, SelfPacedMatcher::CHORD_WINDOW_SECONDS),
        matched(0, true, true)
    );
}

#[test]
fn late_restrike_of_marked_member_restarts() {
    // A re-strike of an already-marked member after the window is a
    // restart, not an ignore — the late-member check runs first.
    let mut matcher = SelfPacedMatcher::new(sets(&[&[60, 64]]));
    assert_eq!(matcher.consume_note_on_at(60, 0.0), matched(0, false, false));
    assert_eq!(
        matcher.consume_note_on_at(60, 0.5),
        SelfPacedOutcome::Restarted {
            index: 0,
            played: 60
        }
    );
    assert_eq!(matcher.consume_note_on_at(64, 0.52), matched(0, true, true));
}

#[test]
fn single_pitch_events_never_restart() {
    let mut matcher = SelfPacedMatcher::new(sets(&[&[60], &[62]]));
    assert_eq!(matcher.consume_note_on_at(60, 0.0), matched(0, true, false));
    assert_eq!(matcher.consume_note_on_at(62, 10.0), matched(1, true, true));
}

#[test]
fn wrong_note_does_not_break_the_attempt() {
    let mut matcher = SelfPacedMatcher::new(sets(&[&[60, 64]]));
    assert_eq!(matcher.consume_note_on_at(60, 0.0), matched(0, false, false));
    assert_eq!(
        matcher.consume_note_on_at(99, 0.02),
        SelfPacedOutcome::Wrong {
            index: 0,
            played: 99
        }
    );
    assert_eq!(matcher.consume_note_on_at(64, 0.04), matched(0, true, true));
}

#[test]
fn start_index_skips_earlier_events_and_clamps() {
    let mut matcher = SelfPacedMatcher::with_start_index(sets(&[&[60], &[62], &[64]]), 1);
    assert_eq!(matcher.index(), 1);
    assert_eq!(
        matcher.consume_note_on(60),
        SelfPacedOutcome::Wrong {
            index: 1,
            played: 60
        }
    );
    assert_eq!(matcher.consume_note_on(62), matched(1, true, false));
    assert_eq!(matcher.consume_note_on(64), matched(2, true, true));

    let mut clamped = SelfPacedMatcher::with_start_index(sets(&[&[60]]), 5);
    assert_eq!(clamped.index(), 1);
    assert!(clamped.is_complete());
    assert_eq!(clamped.consume_note_on(60), SelfPacedOutcome::Ignored);
}

// --- InputStormTests (mastery robustness: mashing is noise, not practice) ---

#[test]
fn storm_requires_a_burst() {
    let mut detector = InputStormDetector::default();
    // Seven scattered wrongs across a long stretch: never a storm.
    for i in 0..7 {
        assert!(!detector.record_wrong(i as f64 * 2.0));
    }
    detector.reset();
    // Eight wrongs inside three seconds: storm.
    let mut storm = false;
    for i in 0..8 {
        storm = detector.record_wrong(10.0 + i as f64 * 0.3);
    }
    assert!(storm);
}

#[test]
fn old_strikes_age_out() {
    let mut detector = InputStormDetector::default();
    for i in 0..7 {
        let _ = detector.record_wrong(i as f64 * 0.1);
    }
    // 4 seconds later a single wrong is not a storm — the burst aged out.
    assert!(!detector.record_wrong(5.0));
}

#[test]
fn reset_clears_the_window() {
    let mut detector = InputStormDetector::default();
    for i in 0..7 {
        let _ = detector.record_wrong(i as f64 * 0.1);
    }
    detector.reset();
    assert!(!detector.record_wrong(0.8));
}
