//! Ports `Tests/KeyInSightTests/TempoTests.swift` (TempoMatcherTests,
//! TempoPolicyTests, and the TimelineTests that exercise the score model;
//! `CalibrationSheet.median` is covered with the UI port).

use crate::engine::{
    RhythmPolicy, TempoExpected, TempoMatcher, TempoOutcome, TempoPolicy, TempoReport,
    TempoResolution, Timing,
};
use crate::score::{Exercise, NoteDuration, ScoreNote};
use std::collections::HashSet;

fn matcher(expected: &[(u8, f64)]) -> TempoMatcher {
    TempoMatcher::new(
        expected
            .iter()
            .map(|&(midi, target_ms)| TempoExpected { midi, target_ms })
            .collect(),
    )
}

fn hit(index: usize, timing: Timing, offset_ms: f64, exercise_complete: bool) -> TempoOutcome {
    TempoOutcome::Hit {
        index,
        timing,
        offset_ms,
        exercise_complete,
    }
}

#[test]
fn hit_classification() {
    // On time (within ±45).
    let mut m = matcher(&[(60, 1000.0)]);
    assert_eq!(
        m.consume_note_on(60, 1010.0),
        hit(0, Timing::OnTime, 10.0, true)
    );

    // Early (past 45, within 120).
    let mut m = matcher(&[(60, 1000.0)]);
    assert_eq!(
        m.consume_note_on(60, 900.0),
        hit(0, Timing::Early, -100.0, true)
    );

    // Late.
    let mut m = matcher(&[(60, 1000.0)]);
    assert_eq!(
        m.consume_note_on(60, 1100.0),
        hit(0, Timing::Late, 100.0, true)
    );
}

#[test]
fn outside_window_is_wrong() {
    let mut m = matcher(&[(60, 1000.0), (62, 2000.0)]);
    // Right pitch, way too early: wrong, anchored on the nearest event.
    assert_eq!(
        m.consume_note_on(60, 700.0),
        TempoOutcome::Wrong {
            nearest_index: 0,
            played: 60
        }
    );
    // Wrong pitch at the right time.
    assert_eq!(
        m.consume_note_on(64, 1000.0),
        TempoOutcome::Wrong {
            nearest_index: 0,
            played: 64
        }
    );
    // Nothing got consumed.
    assert_eq!(m.first_unresolved_index(), Some(0));
}

#[test]
fn nearest_same_pitch_event_wins() {
    let mut m = matcher(&[(60, 1000.0), (60, 2000.0)]);
    assert_eq!(
        m.consume_note_on(60, 1950.0),
        hit(1, Timing::Early, -50.0, false)
    );
    // The first event is still pending.
    assert_eq!(m.first_unresolved_index(), Some(0));
}

#[test]
fn double_strike_ignored() {
    let mut m = matcher(&[(60, 1000.0), (62, 2000.0)]);
    let _ = m.consume_note_on(60, 1000.0);
    assert_eq!(m.consume_note_on(60, 1080.0), TempoOutcome::Ignored);
}

#[test]
fn sweep_marks_missed() {
    let mut m = matcher(&[(60, 1000.0), (62, 2000.0)]);
    assert!(m.sweep(1100.0).is_empty()); // window still open
    assert_eq!(m.sweep(1121.0), [0]); // closed
    assert!(m.sweep(1121.0).is_empty()); // only reported once
    assert_eq!(m.resolutions[0], Some(TempoResolution::Missed));
    assert_eq!(m.first_unresolved_index(), Some(1));
    assert_eq!(m.sweep(3000.0), [1]);
    assert!(m.is_complete());
}

#[test]
fn missed_event_cannot_be_hit_later() {
    let mut m = matcher(&[(60, 1000.0)]);
    let _ = m.sweep(1500.0);
    // The strike matches nothing unresolved, and it isn't a double strike
    // of a *hit* — but there is no unresolved event left: ignored.
    assert_eq!(m.consume_note_on(60, 1500.0), TempoOutcome::Ignored);
}

#[test]
fn report_aggregates() {
    let mut m = matcher(&[(60, 1000.0), (62, 2000.0), (64, 3000.0), (65, 4000.0)]);
    let _ = m.consume_note_on(60, 1010.0); // on time
    let _ = m.consume_note_on(62, 1910.0); // early
    let _ = m.consume_note_on(64, 3100.0); // late
    let _ = m.sweep(5000.0); // 65 missed
    let report = m.report();
    assert_eq!(
        report,
        TempoReport {
            expected_count: 4,
            on_time: 1,
            early: 1,
            late: 1,
            missed: 1,
            mean_abs_offset_ms: Some((10.0 + 90.0 + 100.0) / 3.0),
        }
    );
    assert_eq!(report.hit_count(), 3);
    assert!((report.hit_rate() - 0.75).abs() < 1e-9);
}

// --- TempoPolicyTests ---

fn report(hit_rate: f64, missed: usize) -> TempoReport {
    let count = 10;
    let hits = (hit_rate * count as f64) as usize;
    TempoReport {
        expected_count: count,
        on_time: hits,
        early: 0,
        late: 0,
        missed,
        mean_abs_offset_ms: Some(20.0),
    }
}

#[test]
fn speeds_up_on_clean_exercise() {
    assert_eq!(TempoPolicy::next(60.0, &report(1.0, 0)), 66.0);
}

#[test]
fn caps_at_max() {
    assert_eq!(TempoPolicy::next(132.0, &report(1.0, 0)), 132.0);
}

#[test]
fn slows_down_when_struggling() {
    assert_eq!(TempoPolicy::next(60.0, &report(0.5, 5)), 54.0);
    assert_eq!(TempoPolicy::next(48.0, &report(0.0, 10)), 48.0);
}

#[test]
fn holds_in_the_middle() {
    assert_eq!(TempoPolicy::next(90.0, &report(0.8, 1)), 90.0);
}

#[test]
fn miss_blocks_speed_up() {
    assert_eq!(TempoPolicy::next(60.0, &report(0.9, 1)), 60.0);
}

#[test]
fn rhythm_advance() {
    assert!(RhythmPolicy::should_advance(0, &report(1.0, 0), 80.0));
    // Too slow a tempo doesn't earn new rhythm.
    assert!(!RhythmPolicy::should_advance(0, &report(1.0, 0), 66.0));
    // Max level.
    assert!(!RhythmPolicy::should_advance(3, &report(1.0, 0), 132.0));
    assert_eq!(RhythmPolicy::unlock_name(2), Some("eighth notes"));
}

// --- TimelineTests (score-model portion) ---

#[test]
fn start_units_skip_rests() {
    let exercise = Exercise::new(
        vec![
            ScoreNote::note(60, NoteDuration::Quarter), // unit 0
            ScoreNote::rest(NoteDuration::Quarter),     // unit 2
            ScoreNote::note(62, NoteDuration::Eighth),  // unit 4
            ScoreNote::note(64, NoteDuration::Eighth),  // unit 5
            ScoreNote::note(65, NoteDuration::Half),    // unit 6
        ],
        4,
    );
    assert_eq!(exercise.sounded_note_start_units(), [0, 4, 5, 6]);
    assert_eq!(exercise.sounded_notes().len(), 4);
    let expected: Vec<HashSet<u8>> = [[60], [62], [64], [65]]
        .iter()
        .map(|s| s.iter().copied().collect())
        .collect();
    assert_eq!(exercise.expected_sets(), expected);
}

#[test]
fn measures_chunk_by_units() {
    let exercise = Exercise::new(
        vec![
            ScoreNote::note(60, NoteDuration::DottedHalf), // 6
            ScoreNote::note(62, NoteDuration::Quarter),    // 2 → bar
            ScoreNote::note(64, NoteDuration::Whole),      // 8 → bar
        ],
        4,
    );
    assert_eq!(exercise.measures().len(), 2);
    assert_eq!(exercise.measures()[0].len(), 2);
}
