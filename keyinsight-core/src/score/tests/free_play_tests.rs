//! Ports `Tests/KeyInSightTests/FreePlayAndDrillTests.swift`
//! (FreePlayScoreTests, DrillNoteTests, NotationVocabularyTests).

use std::collections::HashSet;

use crate::core::SplitMix64;
use crate::notation::NotationVocabulary;
use crate::score::{
    ExerciseGenerator, FreePlayScore, NoteDuration, PitchOption, ScoreNote, Staff,
};

// --- FreePlayScoreTests ---

#[test]
fn empty_buffer_renders_a_whole_rest() {
    let mirror = FreePlayScore::build(&[]);
    assert_eq!(mirror.notes, vec![ScoreNote::rest(NoteDuration::Whole)]);
}

#[test]
fn renders_quarters_in_play_order() {
    let mirror = FreePlayScore::build(&[vec![60], vec![64], vec![67]]);
    assert_eq!(
        mirror.notes.iter().map(|n| n.midi.unwrap()).collect::<Vec<_>>(),
        [60, 64, 67]
    );
    assert!(mirror.notes.iter().all(|n| !n.chord_with_previous));
    assert_eq!(mirror.sounded_notes().len(), 3);
}

#[test]
fn chords_render_stacked_low_to_high() {
    let mirror = FreePlayScore::build(&[vec![64, 48, 60]]);
    assert_eq!(
        mirror.notes.iter().map(|n| n.midi.unwrap()).collect::<Vec<_>>(),
        [48, 60, 64]
    );
    assert_eq!(
        mirror.notes.iter().map(|n| n.chord_with_previous).collect::<Vec<_>>(),
        [false, true, true]
    );
    let expected: Vec<HashSet<u8>> = vec![[48, 60, 64].into_iter().collect()];
    assert_eq!(mirror.expected_sets(), expected);
}

#[test]
fn notes_below_middle_c_land_on_bass_staff() {
    let mirror = FreePlayScore::build(&[vec![48], vec![59], vec![60], vec![72]]);
    assert_eq!(
        mirror.notes.iter().map(|n| n.staff).collect::<Vec<_>>(),
        [Staff::Bass, Staff::Bass, Staff::Treble, Staff::Treble]
    );
}

#[test]
fn sliding_window_keeps_the_most_recent_events() {
    let chords: Vec<Vec<u8>> = (0..25).map(|i| vec![48 + (i % 24) as u8]).collect();
    let mirror = FreePlayScore::build(&chords);
    assert_eq!(mirror.sounded_notes().len(), FreePlayScore::WINDOW_SIZE);
    assert_eq!(
        mirror.notes.first().and_then(|n| n.midi),
        chords[chords.len() - FreePlayScore::WINDOW_SIZE].first().copied()
    );
}

// --- DrillNoteTests ---

#[test]
fn drill_is_a_single_whole_note() {
    let mut rng = SplitMix64::new(3);
    let drill = ExerciseGenerator::drill_note(
        &[60, 62, 64].iter().map(|&m| PitchOption::new(m)).collect::<Vec<_>>(),
        Staff::Treble,
        None,
        &mut rng,
    );
    assert_eq!(drill.notes.len(), 1);
    assert_eq!(drill.notes[0].duration, NoteDuration::Whole);
    assert!([60u8, 62, 64].contains(&drill.notes[0].midi.unwrap()));
}

#[test]
fn drill_sampling_favors_weak_items() {
    let mut rng = SplitMix64::new(11);
    let pitches = [
        PitchOption::weighted(60, 1.0),
        PitchOption::weighted(62, 1.0),
        PitchOption::weighted(64, 4.0), // weak item
    ];
    let mut weak_count = 0;
    for _ in 0..600 {
        if ExerciseGenerator::drill_note(&pitches, Staff::Treble, None, &mut rng).notes[0].midi
            == Some(64)
        {
            weak_count += 1;
        }
    }
    // weight^1.5: 8 vs 1 vs 1 → expected share 0.8; uniform would be 0.33.
    assert!(weak_count > 360, "weak item drawn {weak_count}/600");
}

// --- NotationVocabularyTests ---

#[test]
fn describes_notes() {
    assert_eq!(
        NotationVocabulary::describe_note(&ScoreNote::note(64, NoteDuration::Quarter)),
        "E4 — quarter note"
    );
    assert_eq!(
        NotationVocabulary::describe_note(&ScoreNote::note(66, NoteDuration::DottedHalf)),
        "F#4 — dotted half note"
    );
}

#[test]
fn describes_elements() {
    assert!(NotationVocabulary::describe("clef", 0, 4)
        .unwrap()
        .contains("Treble clef"));
    assert!(NotationVocabulary::describe("keySig", 1, 4)
        .unwrap()
        .contains("G major"));
    assert!(NotationVocabulary::describe("keySig", 2, 4)
        .unwrap()
        .contains("D major"));
    assert!(NotationVocabulary::describe("meterSig", 0, 3)
        .unwrap()
        .contains("3 beats"));
    assert!(NotationVocabulary::describe("barLine", 0, 4)
        .unwrap()
        .contains("measure"));
    assert_eq!(NotationVocabulary::describe("stem", 0, 4), None);
}
