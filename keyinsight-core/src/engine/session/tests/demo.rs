//! Ports of the scripted `--demo` DemoDriver acts that exercise the
//! engine end to end (the survival act lives in `survival`).

use super::{engine, play_through, Phase};
use crate::score::{RepertoireLibrary, RepertoirePiece};

fn minuet_in_g() -> RepertoirePiece {
    RepertoireLibrary::bundled()
        .into_iter()
        .find(|p| p.slug == "minuet-in-g")
        .expect("bundled piece")
}

/// DemoDriver Act 3.5: practice-from-here (partial replay) on the piece
/// the repertoire act just played.
#[test]
fn demo_practice_from_here_act() {
    const PARTIAL_START_EVENT: usize = 4;
    let (mut engine, time) = engine();
    // Act 3 (repertoire) leaves one full play on the books.
    engine.start_piece(minuet_in_g());
    let full_note_count = engine.note_count();
    play_through(&mut engine, &time);
    let plays_before = engine.piece_stats("minuet-in-g").map(|s| s.0).unwrap_or(0);
    assert_eq!(plays_before, 1);

    engine.practice_from(PARTIAL_START_EVENT);
    assert_eq!(engine.note_count(), full_note_count - PARTIAL_START_EVENT);
    assert_eq!(engine.current_note_number(), 1);
    assert!(engine.replay_start_measure() >= 1);

    let summary = play_through(&mut engine, &time);
    // Clean pass over the tail only; section practice must not count as a
    // play of the piece.
    assert_eq!(summary.note_count, full_note_count - PARTIAL_START_EVENT);
    assert_eq!(summary.first_try_correct, summary.note_count);
    assert_eq!(
        engine.piece_stats("minuet-in-g").map(|s| s.0).unwrap_or(0),
        plays_before
    );

    engine.clear_replay_start();
    assert_eq!(engine.replay_start_event(), 0);
    assert_eq!(engine.note_count(), full_note_count);
    assert_eq!(*engine.phase(), Phase::Playing);
}
