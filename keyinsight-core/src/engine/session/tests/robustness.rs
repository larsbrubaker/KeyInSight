//! Mastery robustness (the mash guard and latency outliers), the chord
//! window's broken-attempt path, and rhythm advancement from self-paced
//! play.

use super::{engine, note_on, play_through};
use crate::engine::session::{Phase, LATENCY_OUTLIER_MS};
use crate::engine::InputStormDetector;
use crate::score::{Exercise, NoteDuration, ScoreNote, Staff};

fn total_attempts(engine: &super::SessionEngine) -> i64 {
    engine
        .db
        .as_ref()
        .unwrap()
        .item_stats()
        .iter()
        .map(|s| s.attempts)
        .sum()
}

/// A burst of wrong strikes suspends attempt recording; the storm clears
/// on the next clean (first-try) event — the errored note itself still
/// goes unrecorded.
#[test]
fn storm_suppresses_stats_until_a_clean_event() {
    let (mut engine, time) = engine();
    let expected = engine.current_expected_midi().unwrap();
    let wrong = expected + 1; // a different pitch class: the anchor leaves it
    let before = total_attempts(&engine);
    for _ in 0..InputStormDetector::STRIKE_THRESHOLD {
        *time.borrow_mut() += 0.1;
        let at = *time.borrow();
        engine.handle(note_on(wrong, at));
    }
    assert!(engine.stats_suppressed(), "8 wrongs inside 3 s is a storm");
    assert_eq!(engine.errors_this_exercise(), InputStormDetector::STRIKE_THRESHOLD);

    // Resolving the errored note: still suppressed, nothing recorded.
    *time.borrow_mut() += 0.1;
    let at = *time.borrow();
    engine.handle(note_on(expected, at));
    assert!(engine.stats_suppressed());
    assert_eq!(total_attempts(&engine), before, "gated while suppressed");
    assert_eq!(engine.current_note_index(), 1);

    // The next clean event resumes recording (itself ungated only after).
    let next = engine.current_expected_midi().unwrap();
    *time.borrow_mut() += 0.3;
    let at = *time.borrow();
    engine.handle(note_on(next, at));
    assert!(!engine.stats_suppressed(), "a clean note ends the storm");
    if *engine.phase() == Phase::Playing {
        let third = engine.current_expected_midi().unwrap();
        *time.borrow_mut() += 0.3;
        let at = *time.borrow();
        engine.handle(note_on(third, at));
        assert!(total_attempts(&engine) > before, "recording resumed");
    }
}

/// Wrong strikes spread past the window never trip the guard.
#[test]
fn slow_wrong_notes_are_not_a_storm() {
    let (mut engine, time) = engine();
    let expected = engine.current_expected_midi().unwrap();
    for _ in 0..InputStormDetector::STRIKE_THRESHOLD {
        *time.borrow_mut() += 0.5; // 8 strikes span 3.5 s
        let at = *time.borrow();
        engine.handle(note_on(expected + 1, at));
    }
    assert!(!engine.stats_suppressed());
}

/// A latency past the outlier bar is a break, not slowness.
#[test]
fn latency_outliers_are_not_recorded() {
    let (mut engine, time) = engine();
    let expected = engine.current_expected_midi().unwrap();
    *time.borrow_mut() += LATENCY_OUTLIER_MS / 1000.0 + 5.0;
    let at = *time.borrow();
    engine.handle(note_on(expected, at));
    assert_eq!(engine.errors_this_exercise(), 0);
    assert_eq!(engine.current_note_index(), 1);
    assert!(engine.latencies_ms.is_empty(), "outlier skipped");
    let item = crate::skill::SkillModel::item_name_on(expected, Staff::Treble);
    let stats = engine.db.as_ref().unwrap().item_stats();
    let stat = stats.iter().find(|s| s.item == item).expect("attempt still recorded");
    assert_eq!(stat.attempts, 1);
    assert_eq!(stat.ewma_latency_ms, None, "no latency for the outlier");

    // A normal latency records as usual.
    let next = engine.current_expected_midi().unwrap();
    *time.borrow_mut() += 0.5;
    let at = *time.borrow();
    engine.handle(note_on(next, at));
    assert_eq!(engine.latencies_ms.len(), 1);
    assert!((engine.latencies_ms[0] - 500.0).abs() < 1e-6);
}

/// A chord member landing after the window breaks the attempt: an error,
/// only the late strike kept, the rest back to "play me".
#[test]
fn broken_chord_attempt_counts_an_error_and_resets_marks() {
    let (mut engine, time) = engine();
    let treble: Vec<ScoreNote> = (0..4)
        .map(|_| ScoreNote::note(60, NoteDuration::Quarter))
        .collect();
    let bass: Vec<ScoreNote> = (0..4)
        .map(|_| ScoreNote::note(48, NoteDuration::Quarter).with_staff(Staff::Bass))
        .collect();
    let spec = serde_json::to_string(&Exercise::new(treble, 4).with_bass(bass)).unwrap();
    engine.practice_exercise(&spec);
    assert_eq!(engine.note_count(), 4);
    assert_eq!(engine.current_expected_midis().len(), 2);

    let at = *time.borrow();
    engine.handle(note_on(60, at));
    assert_eq!(engine.errors_this_exercise(), 0);
    assert!(engine.consumed_positions[0].contains(&0));
    assert_eq!(engine.current_expected_midis().len(), 1);
    assert!(engine.current_expected_midis().contains(&48));

    // Late second member: restart with 48 kept.
    engine.handle(note_on(48, at + 0.5));
    assert_eq!(engine.errors_this_exercise(), 1);
    assert_eq!(engine.streak(), 0);
    assert_eq!(engine.current_note_index(), 0);
    assert_eq!(engine.consumed_positions[0].len(), 1);
    assert!(engine.consumed_positions[0].contains(&1), "the bass position stays marked");
    assert_eq!(engine.current_expected_midis().len(), 1);
    assert!(engine.current_expected_midis().contains(&60), "C4 is wanted again");
    // The kept member shows correct, the other back to current.
    let ids = engine.event_ids[0].clone();
    let notation = engine.notation.borrow();
    assert_eq!(notation.state_of(&ids[1]), Some(crate::notation::NoteState::Correct));
    assert_eq!(notation.state_of(&ids[0]), Some(crate::notation::NoteState::Current));
    drop(notation);
    // The bass-staff item took the error.
    let stats = engine.db.as_ref().unwrap().item_stats();
    let bass_item = crate::skill::SkillModel::item_name_on(48, Staff::Bass);
    assert!(stats.iter().any(|s| s.item == bass_item && s.errors == 1), "{stats:?}");

    // Completing the set inside the window advances; the chord shape is
    // recorded once (an errored attempt).
    engine.handle(note_on(60, at + 0.55));
    assert_eq!(engine.current_note_index(), 1);
    assert_eq!(engine.errors_this_exercise(), 1);
    let stats = engine.db.as_ref().unwrap().item_stats();
    let octave = stats.iter().find(|s| s.item == "chord:harm-octave").expect("shape recorded");
    assert_eq!(octave.attempts, 1);
    assert_eq!(octave.errors, 1);
}

/// Cross-staff unison: one key press satisfies every notehead with that
/// pitch in the event.
#[test]
fn unison_across_staves_is_one_press() {
    let (mut engine, time) = engine();
    let treble = vec![ScoreNote::note(60, NoteDuration::Whole)];
    let bass = vec![ScoreNote::note(60, NoteDuration::Whole).with_staff(Staff::Bass)];
    let spec = serde_json::to_string(&Exercise::new(treble, 4).with_bass(bass)).unwrap();
    engine.practice_exercise(&spec);
    assert_eq!(engine.note_count(), 1);
    let at = *time.borrow() + 0.1;
    engine.handle(note_on(60, at));
    assert!(matches!(engine.phase(), Phase::Summary(s) if s.error_count == 0));
}

/// Five clean self-paced training exercises earn the next rhythm rung.
#[test]
fn rhythm_level_advances_after_five_clean_self_paced_exercises() {
    let (mut engine, time) = engine();
    assert_eq!(engine.rhythm_level(), 0);
    for n in 1..=4 {
        let summary = play_through(&mut engine, &time);
        assert_eq!(summary.error_count, 0);
        assert_eq!(summary.rhythm_unlocked, None);
        assert_eq!(engine.rhythm_level(), 0);
        assert_eq!(
            engine.db.as_ref().unwrap().setting("rhythm_clean_streak").as_deref(),
            Some(n.to_string().as_str())
        );
        engine.next_exercise();
    }
    let summary = play_through(&mut engine, &time);
    assert_eq!(engine.rhythm_level(), 1);
    assert_eq!(summary.rhythm_unlocked.as_deref(), Some("dotted half notes"));
    let db = engine.db.as_ref().unwrap();
    assert_eq!(db.setting("rhythm_level").as_deref(), Some("1"));
    assert_eq!(db.setting("rhythm_clean_streak").as_deref(), Some("0"));
}

/// An error resets the clean streak.
#[test]
fn a_wrong_note_resets_the_rhythm_clean_streak() {
    let (mut engine, time) = engine();
    play_through(&mut engine, &time);
    assert_eq!(
        engine.db.as_ref().unwrap().setting("rhythm_clean_streak").as_deref(),
        Some("1")
    );
    engine.next_exercise();
    let expected = engine.current_expected_midi().unwrap();
    *time.borrow_mut() += 0.2;
    let at = *time.borrow();
    engine.handle(note_on(expected + 1, at));
    play_through(&mut engine, &time);
    assert_eq!(
        engine.db.as_ref().unwrap().setting("rhythm_clean_streak").as_deref(),
        Some("0")
    );
}
