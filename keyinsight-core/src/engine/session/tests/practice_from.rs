//! Practice-from-here (repertoire): a replay restarted at a match event
//! counts, locks, and scores from that spot; partial replays are section
//! practice, not plays of the piece.

use super::{engine, note_on, Phase};
use crate::engine::session::{PacingMode, SessionEngine};
use crate::engine::TempoResolution;
use crate::notation::NoteState;
use crate::score::{RepertoireLibrary, RepertoirePiece};

fn twinkle() -> RepertoirePiece {
    RepertoireLibrary::bundled()
        .into_iter()
        .find(|p| p.slug == "twinkle-twinkle")
        .expect("bundled piece")
}

/// Play the current (monophonic) content cleanly to its summary.
fn play_tail(engine: &mut SessionEngine, time: &std::rc::Rc<std::cell::RefCell<f64>>) {
    let mut guard = 0;
    while *engine.phase() == Phase::Playing {
        let expected = engine.current_expected_midi().expect("monophonic piece");
        *time.borrow_mut() += 0.3;
        let at = *time.borrow();
        engine.handle(note_on(expected, at));
        guard += 1;
        assert!(guard < 400, "piece should complete");
    }
}

#[test]
fn practice_from_counts_and_locks_from_the_start_spot() {
    let (mut engine, _time) = engine();
    engine.start_piece(twinkle());
    let full = engine.note_count();
    assert!(full > 8);

    engine.practice_from(4);
    assert_eq!(engine.replay_start_event(), 4);
    assert_eq!(engine.note_count(), full - 4);
    assert_eq!(engine.current_note_number(), 1);
    assert_eq!(engine.current_note_index(), 4);
    assert_eq!(
        engine.replay_start_measure(),
        engine.measure_by_event[4] + 1,
        "the start chip names the start spot's measure"
    );

    // Everything before the spot is grayed out; the spot is current.
    let notation = engine.notation.borrow();
    for ids in engine.event_ids.iter().take(4) {
        for id in ids {
            assert_eq!(notation.state_of(id), Some(NoteState::Locked));
        }
    }
    for id in &engine.event_ids[4] {
        assert_eq!(notation.state_of(id), Some(NoteState::Current));
    }
    drop(notation);
    // The natural state (playback restore) agrees.
    assert_eq!(engine.natural_state(0), Some(NoteState::Locked));
    assert_eq!(engine.natural_state(4), Some(NoteState::Current));
}

#[test]
fn practice_from_tail_is_not_a_piece_play() {
    let (mut engine, time) = engine();
    engine.start_piece(twinkle());
    let full = engine.note_count();
    engine.practice_from(4);

    play_tail(&mut engine, &time);
    let Phase::Summary(summary) = engine.phase() else {
        panic!("expected a summary, got {:?}", engine.phase());
    };
    assert_eq!(summary.note_count, full - 4);
    assert_eq!(summary.first_try_correct, summary.note_count);
    assert_eq!(summary.error_count, 0);
    assert_eq!(
        engine.piece_stats("twinkle-twinkle"),
        None,
        "section practice does not count as a play"
    );

    // Back to the whole piece.
    engine.clear_replay_start();
    assert_eq!(engine.replay_start_event(), 0);
    assert_eq!(engine.note_count(), full);
    assert_eq!(engine.current_note_number(), 1);
    play_tail(&mut engine, &time);
    assert_eq!(engine.piece_stats("twinkle-twinkle").map(|s| s.0), Some(1));
}

#[test]
fn practice_from_shifts_the_tempo_grid_to_the_start_spot() {
    let (mut engine, _time) = engine();
    engine.set_mode(PacingMode::Tempo);
    engine.start_piece(twinkle());
    assert_eq!(engine.active_pacing(), PacingMode::Tempo);
    engine.practice_from(4);
    assert_eq!(engine.active_pacing(), PacingMode::Tempo);

    let count_in_ms = 4.0 * (60_000.0 / engine.tempo_bpm());
    let tempo_matcher = engine.tempo_matcher.as_ref().expect("tempo run");
    // The first note of the tail lands right after the count-in; earlier
    // targets go negative and are pre-resolved as skipped.
    assert!((tempo_matcher.expected[4].target_ms - count_in_ms).abs() < 1e-9);
    assert!(tempo_matcher.expected[0].target_ms < count_in_ms);
    for resolution in tempo_matcher.resolutions.iter().take(4) {
        assert_eq!(*resolution, Some(TempoResolution::Skipped));
    }
    assert_eq!(tempo_matcher.first_unresolved_index(), Some(4));
    assert_eq!(engine.natural_state(0), Some(NoteState::Locked));
    assert_eq!(engine.note_count(), engine.events.len() - 4);
}

#[test]
fn practice_from_is_repertoire_only() {
    let (mut engine, _time) = engine();
    let count = engine.note_count();
    engine.practice_from(3);
    assert_eq!(engine.replay_start_event(), 0);
    assert_eq!(engine.note_count(), count);
    assert_eq!(engine.current_note_number(), 1);
}

#[test]
fn note_clicked_restarts_from_that_events_spot() {
    let (mut engine, _time) = engine();
    engine.start_piece(twinkle());
    let id = engine.event_ids[3][0].clone();
    engine.note_clicked(&id);
    assert_eq!(engine.replay_start_event(), 3);
    assert_eq!(engine.current_note_index(), 3);
    engine.note_clicked("not-a-note");
    assert_eq!(engine.replay_start_event(), 3);
    // Leaving repertoire forgets the spot.
    engine.exit_repertoire();
    assert_eq!(engine.replay_start_event(), 0);
}
