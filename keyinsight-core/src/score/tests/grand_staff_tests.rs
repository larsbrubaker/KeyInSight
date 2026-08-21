//! Ports `Tests/KeyInSightTests/GrandStaffTests.swift` (GrandStaffTests,
//! TwoVoiceTests, FlatsAndTiesTests).

use std::collections::HashSet;

use crate::core::{PitchSpelling, SplitMix64};
use crate::engine::{SelfPacedMatcher, SelfPacedOutcome};
use crate::audio::MidiFileEncoder;
use crate::notation::NotationRenderer;
use crate::score::{
    Exercise, ExerciseGenerator, Hands, MusicXmlEncoder, MusicXmlImporter, NoteDuration,
    PitchOption, ScoreNote, Staff,
};

fn matched(index: usize, set_complete: bool, exercise_complete: bool) -> SelfPacedOutcome {
    SelfPacedOutcome::Matched {
        index,
        set_complete,
        exercise_complete,
    }
}

fn sets(sets: &[&[u8]]) -> Vec<HashSet<u8>> {
    sets.iter().map(|s| s.iter().copied().collect()).collect()
}

// --- GrandStaffTests ---

#[test]
fn chord_members_share_onset_and_matcher_set() {
    let exercise = Exercise::new(
        vec![
            ScoreNote::note(60, NoteDuration::Quarter),
            ScoreNote::note(64, NoteDuration::Quarter).with_chord(true),
            ScoreNote::note(67, NoteDuration::Quarter),
        ],
        4,
    );
    assert_eq!(exercise.expected_sets(), sets(&[&[60, 64], &[67]]));
    assert_eq!(exercise.sounded_note_start_units(), [0, 0, 2]);
    assert_eq!(exercise.measures().len(), 1);
}

#[test]
fn chord_never_splits_across_the_barline() {
    // The anchor fills the measure; its chord member must stay with it.
    let exercise = Exercise::new(
        vec![
            ScoreNote::note(60, NoteDuration::Half),
            ScoreNote::note(62, NoteDuration::Half),
            ScoreNote::note(65, NoteDuration::Half).with_chord(true),
        ],
        4,
    );
    assert_eq!(exercise.measures().len(), 1);
    assert_eq!(exercise.measures()[0].len(), 3);
}

#[test]
fn self_paced_matcher_handles_chords() {
    let exercise = Exercise::new(
        vec![
            ScoreNote::note(60, NoteDuration::Quarter),
            ScoreNote::note(64, NoteDuration::Quarter).with_chord(true),
        ],
        4,
    );
    let mut matcher = SelfPacedMatcher::new(exercise.expected_sets());
    assert_eq!(matcher.consume_note_on(64), matched(0, false, false));
    assert_eq!(
        matcher.consume_note_on(62),
        SelfPacedOutcome::Wrong {
            index: 0,
            played: 62
        }
    );
    assert_eq!(matcher.consume_note_on(60), matched(0, true, true));
}

#[test]
fn bass_note_promotes_encoding_to_grand_staff() {
    let exercise = Exercise::new(
        vec![
            ScoreNote::note(48, NoteDuration::Quarter).with_staff(Staff::Bass),
            ScoreNote::note(64, NoteDuration::Quarter).with_chord(true),
        ],
        4,
    );
    let xml = MusicXmlEncoder::encode(&exercise);
    assert!(xml.contains("<staves>2</staves>"));
    assert!(xml.contains("<staff>2</staff>"));
    assert!(xml.contains("<staff>1</staff>"));
    assert!(xml.contains("<chord/>"));
}

#[test]
fn treble_only_scores_stay_single_staff() {
    let exercise = Exercise::new(vec![ScoreNote::note(60, NoteDuration::Quarter)], 4);
    let xml = MusicXmlEncoder::encode(&exercise);
    assert!(!xml.contains("<staves>"));
    assert!(!xml.contains("<staff>"));
}

#[test]
fn legacy_stored_specs_decode_to_treble_non_chord() {
    let legacy = r#"{"notes":[{"midi":60,"duration":"quarter"}],"beatsPerMeasure":4}"#;
    let exercise: Exercise = serde_json::from_str(legacy).unwrap();
    assert_eq!(exercise.notes[0].staff, Staff::Treble);
    assert!(!exercise.notes[0].chord_with_previous);
}

#[test]
fn engraver_renders_grand_staff_chord_with_all_note_ids() {
    let mut renderer = NotationRenderer::new();
    let exercise = Exercise::new(
        vec![
            ScoreNote::note(48, NoteDuration::Quarter).with_staff(Staff::Bass),
            ScoreNote::note(52, NoteDuration::Quarter)
                .with_staff(Staff::Bass)
                .with_chord(true),
            ScoreNote::note(64, NoteDuration::Quarter).with_chord(true),
            ScoreNote::note(67, NoteDuration::Quarter),
            ScoreNote::rest(NoteDuration::Half),
        ],
        4,
    );
    let rendered = renderer
        .render(&MusicXmlEncoder::encode(&exercise))
        .expect("grand staff engraves");
    // Every chord member gets its own element id (timemap flattening).
    assert_eq!(rendered.note_ids.len(), exercise.sounded_notes().len());
}

// --- TwoVoiceTests ---

fn two_voice_exercise() -> Exercise {
    Exercise::new(
        vec![
            ScoreNote::note(64, NoteDuration::Quarter),
            ScoreNote::note(65, NoteDuration::Quarter),
            ScoreNote::note(67, NoteDuration::Half),
        ],
        4,
    )
    .with_bass(vec![
        ScoreNote::note(48, NoteDuration::Whole).with_staff(Staff::Bass)
    ])
}

#[test]
fn events_merge_voices_by_onset() {
    let exercise = two_voice_exercise();
    let events = exercise.match_events();
    assert_eq!(events.len(), 3);
    // Beat 1: E4 (treble first) + C3 sound together.
    assert_eq!(events[0].pitches, [64, 48]);
    assert_eq!(events[0].staves, [Staff::Treble, Staff::Bass]);
    // Later beats: melody alone.
    assert_eq!(events[1].pitches, [65]);
    assert_eq!(events[2].pitches, [67]);
    assert_eq!(
        exercise.expected_sets(),
        sets(&[&[64, 48], &[65], &[67]])
    );
    assert_eq!(exercise.event_measure_indices(), [0, 0, 0]);
}

#[test]
fn matcher_requires_both_hands_at_shared_onsets() {
    let exercise = two_voice_exercise();
    let mut matcher = SelfPacedMatcher::new(exercise.expected_sets());
    assert_eq!(matcher.consume_note_on(48), matched(0, false, false));
    assert_eq!(matcher.consume_note_on(64), matched(0, true, false));
    assert_eq!(matcher.consume_note_on(65), matched(1, true, false));
}

#[test]
fn encoder_emits_backup_and_two_voices() {
    let xml = MusicXmlEncoder::encode(&two_voice_exercise());
    assert!(xml.contains("<backup><duration>8</duration></backup>"));
    assert!(xml.contains("<voice>1</voice>"));
    assert!(xml.contains("<voice>2</voice>"));
    assert!(xml.contains("<staves>2</staves>"));
}

#[test]
fn import_round_trip_preserves_both_voices() {
    let exercise = two_voice_exercise();
    let xml = MusicXmlEncoder::encode(&exercise);
    let piece = MusicXmlImporter::parse(xml.as_bytes(), "rt").unwrap();
    assert_eq!(piece.exercise, exercise);
}

/// The load-bearing assumption for note-id alignment: the engraver's
/// timemap emits ids in document order — treble voice before bass at a
/// shared onset. Verified via note_midi per id.
#[test]
fn engraver_timemap_orders_treble_before_bass_at_shared_onsets() {
    let exercise = two_voice_exercise();
    let mut renderer = NotationRenderer::new();
    let rendered = renderer
        .render(&MusicXmlEncoder::encode(&exercise))
        .expect("two-voice engraves");
    let events = exercise.match_events();
    assert_eq!(rendered.note_ids.len(), 4);
    let mut cursor = 0;
    for event in &events {
        for &pitch in &event.pitches {
            let id = &rendered.note_ids[cursor];
            assert_eq!(
                renderer.midi_pitch(id),
                Some(pitch),
                "id {cursor} expected pitch {pitch}"
            );
            cursor += 1;
        }
    }
}

#[test]
fn playback_covers_both_voices_and_duration_is_longest_voice() {
    let short = Exercise::new(vec![ScoreNote::note(64, NoteDuration::Quarter)], 4)
        .with_bass(vec![
            ScoreNote::note(48, NoteDuration::Whole).with_staff(Staff::Bass)
        ]);
    // Whole-note bass outlasts the quarter-note melody.
    assert_eq!(MidiFileEncoder::duration(&short, 120.0), 2.0);
    assert_eq!(MidiFileEncoder::event_start_seconds(&short, 120.0), [0.0]);
}

#[test]
fn generator_two_handed_adds_alternating_bass_ending_on_tonic() {
    let mut generator = ExerciseGenerator::default();
    generator.config.hands = Hands::Both;
    generator.config.measures = 3;
    let mut rng = SplitMix64::new(7);
    let ex = generator.generate(
        &[60, 62, 64]
            .iter()
            .map(|&m| PitchOption::new(m))
            .collect::<Vec<_>>(),
        &mut rng,
    );
    assert!(ex.is_two_voice());
    assert_eq!(ex.bass_notes.len(), 3);
    assert_eq!(
        ex.bass_notes.iter().map(|n| n.midi.unwrap()).collect::<Vec<_>>(),
        [48, 43, 48]
    ); // I V I
    assert!(ex
        .bass_notes
        .iter()
        .all(|n| n.staff == Staff::Bass && n.duration == NoteDuration::Whole));
}

// --- FlatsAndTiesTests ---

#[test]
fn flat_keys_spell_and_round_trip() {
    // B-flat major: F4 Bb4 Eb5 D5
    let exercise = Exercise::new(
        vec![
            ScoreNote::note(65, NoteDuration::Quarter),
            ScoreNote::note(70, NoteDuration::Quarter),
            ScoreNote::note(75, NoteDuration::Quarter),
            ScoreNote::note(74, NoteDuration::Quarter),
        ],
        4,
    )
    .with_fifths(-2);
    let xml = MusicXmlEncoder::encode(&exercise);
    // Bb spelled as B alter -1, and in key: no accidental glyph.
    assert!(xml.contains("<step>B</step><alter>-1</alter>"));
    assert!(!xml.contains("<accidental>"));
    assert!(xml.contains("<fifths>-2</fifths>"));
    let piece = MusicXmlImporter::parse(xml.as_bytes(), "rt").unwrap();
    assert_eq!(piece.exercise, exercise);
}

#[test]
fn out_of_key_notes_get_accidentals_both_directions() {
    // F major (1 flat): B natural needs a natural; C# needs a sharp.
    let exercise = Exercise::new(
        vec![
            ScoreNote::note(71, NoteDuration::Quarter),
            ScoreNote::note(61, NoteDuration::Quarter),
            ScoreNote::note(70, NoteDuration::Half),
        ],
        4,
    )
    .with_fifths(-1);
    let xml = MusicXmlEncoder::encode(&exercise);
    assert!(xml.contains("<accidental>natural</accidental>"));
    // In a flat key black keys spell flat: C# renders as Db with a flat glyph.
    assert!(
        xml.contains("<accidental>flat</accidental>")
            || xml.contains("<accidental>sharp</accidental>")
    );
}

#[test]
fn key_names() {
    assert_eq!(PitchSpelling::key_name(-2), "B♭ major");
    assert_eq!(PitchSpelling::key_name(0), "C major");
    assert_eq!(PitchSpelling::key_name(4), "E major");
}

#[test]
fn tie_continuation_is_not_a_new_onset() {
    // C4 half tied to C4 half: ONE event, playback length = whole.
    let exercise = Exercise::new(
        vec![
            ScoreNote::note(60, NoteDuration::Half),
            ScoreNote::note(60, NoteDuration::Half).with_tie(true),
            ScoreNote::note(62, NoteDuration::Whole),
        ],
        4,
    );
    assert_eq!(exercise.expected_sets(), sets(&[&[60], &[62]]));
    let spans = Exercise::voice_note_spans(&exercise.notes);
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].length_units, 8); // extended across the tie
    assert_eq!(spans[1].start_units, 8); // D4 starts at bar 2
    assert_eq!(
        exercise
            .match_events()
            .iter()
            .map(|e| e.start_units)
            .collect::<Vec<_>>(),
        [0, 8]
    );
}

#[test]
fn tie_roles_mark_start_and_stop() {
    let notes = vec![
        ScoreNote::note(60, NoteDuration::Half),
        ScoreNote::note(60, NoteDuration::Half).with_tie(true),
    ];
    let roles = Exercise::tie_roles(&notes);
    assert!(roles[0].start && !roles[0].stop);
    assert!(!roles[1].start && roles[1].stop);
}

#[test]
fn ties_encode_and_round_trip() {
    let exercise = Exercise::new(
        vec![
            ScoreNote::note(72, NoteDuration::Half),
            ScoreNote::note(72, NoteDuration::Half).with_tie(true),
            ScoreNote::note(74, NoteDuration::Whole),
        ],
        4,
    )
    .with_bass(vec![
        ScoreNote::note(48, NoteDuration::Whole).with_staff(Staff::Bass),
        ScoreNote::note(48, NoteDuration::Whole)
            .with_staff(Staff::Bass)
            .with_tie(true),
    ]);
    let xml = MusicXmlEncoder::encode(&exercise);
    assert!(xml.contains("<tie type=\"start\"/>"));
    assert!(xml.contains("<tied type=\"stop\"/>"));
    let piece = MusicXmlImporter::parse(xml.as_bytes(), "rt").unwrap();
    assert_eq!(piece.exercise, exercise);
}

/// Load-bearing for note-id binding. C++ Verovio listed tie continuations
/// as timemap onsets (the Swift test pinned that quirk and binding filtered
/// by pitch); verovio-rust's timemap **omits continuations by design**, so
/// this port pins the cleaner behavior: expected onsets carry exactly the
/// match pitches, and no extra onset appears at the continuation.
#[test]
fn engraver_timemap_skips_tie_continuations() {
    let exercise = Exercise::new(
        vec![
            ScoreNote::note(72, NoteDuration::Half),
            ScoreNote::note(72, NoteDuration::Half).with_tie(true),
            ScoreNote::note(74, NoteDuration::Whole),
        ],
        4,
    )
    .with_bass(vec![
        ScoreNote::note(48, NoteDuration::Whole).with_staff(Staff::Bass),
        ScoreNote::note(48, NoteDuration::Whole)
            .with_staff(Staff::Bass)
            .with_tie(true),
    ]);
    let mut renderer = NotationRenderer::new();
    let rendered = renderer
        .render(&MusicXmlEncoder::encode(&exercise))
        .expect("tied score engraves");
    let mut groups: std::collections::HashMap<u64, Vec<String>> = std::collections::HashMap::new();
    for (qstamp, ids) in &rendered.note_groups {
        groups
            .entry(qstamp.to_bits())
            .or_default()
            .extend(ids.iter().cloned());
    }
    for event in exercise.match_events() {
        let q = event.start_units as f64 / 2.0;
        let ids = groups.get(&q.to_bits()).expect("timemap group exists");
        let pitches: Vec<u8> = ids.iter().filter_map(|id| renderer.midi_pitch(id)).collect();
        for pitch in &event.pitches {
            assert!(pitches.contains(pitch), "pitch {pitch} missing at qstamp {q}");
        }
    }
    // No onset appears at the tie continuation (qstamp 2.0) — the divergence
    // from C++ Verovio this port standardizes on.
    assert!(!groups.contains_key(&2.0f64.to_bits()));
}
